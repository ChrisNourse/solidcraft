//! A solid cut through along several axes, such as a block bored on two: the intersection of its
//! straight extrusions, one along each axis whose two end caps match.
//!
//! The extrusion of a solid's end caps holds the solid when every point of it projects into the
//! caps' section, and the intersection of such extrusions is the solid's visual hull from those
//! directions (A. Laurentini, "The visual hull concept for silhouette-based image understanding",
//! IEEE PAMI 16(2), 1994). The hull is the solid when it holds no more material. The solid built
//! is the first extrusion less the holes of the others' sections, which holds their
//! intersection. It is taken only when every hole of every section is one circle and the first
//! section is one region with a convex outline; every source vertex lies inside every
//! extrusion; every source triangle is a cap triangle of the first extrusion or lies whole
//! within the tolerance of one of its walls or of a hole's wall of a later one (the chord
//! certificate of [`super::curves`]), and keeps out of every hole but for the tolerance along
//! its wall, which a triangle on one wall can cross with all its corners outside: so the
//! source lies on the built solid's boundary; and the volumes agree within the tolerance times
//! the source's area. A kernel failure status while building or measuring it keeps the
//! source's facets, which the step only replaces ([`Refused::Failed`]).
//!
//! Deviations: no upstream counterpart. No recursion.

use std::f64::consts::{FRAC_PI_2, PI, TAU};

use crate::kernel::Kernel;
use crate::math::{det, Vec2, Vec3};
use crate::prism::profile::{Loop2, Region2, Seg2};
use crate::{Brep, Result, SolidError};

use super::curves::certified;
use super::geometry::{distance_to_segment, MeshPlane, PlanarVector};
use super::parts::{self, MAX_FACES};
use super::{invalid, native, prism, Prism, Sides, TriMesh, MAX_WALL_WORK};

/// Why [`reconstruct`] keeps the source's facets.
#[derive(Debug)]
pub(super) enum Refused {
    /// The source is not the boundary of such an intersection, or it exceeds a work bound.
    NotAnIntersection,
    /// The kernel failed reading, building or measuring the intersection: a defect of the kernel,
    /// not of the source, which keeps its facets as it would without this step (such as the
    /// B-rep Boolean failure the seam note in [`candidate`] avoids). The error names the defect;
    /// only the tests read it.
    Failed(#[cfg_attr(not(test), allow(dead_code))] SolidError),
}

/// Intersect the extrusions of every axis with matching caps, at least two, when the source is
/// the boundary of that intersection and has its volume. Cancellation is the only error.
pub(super) fn reconstruct(
    k: &mut Kernel,
    mesh: &TriMesh,
    tolerance: f64,
) -> Result<std::result::Result<Brep, Refused>> {
    refusal(candidate(k, mesh, tolerance))
}

/// The answer of [`reconstruct`] from the candidate's. An error that may stand for a kernel
/// failure status is [`Refused::Failed`]; one that no status stands for (a stop the caller asked
/// for, an unported call, a port invariant), which [`crate::error::native_failure`] passes on,
/// stays an error.
pub(super) fn refusal(
    candidate: Result<Option<Brep>>,
) -> Result<std::result::Result<Brep, Refused>> {
    match candidate {
        Ok(Some(body)) => Ok(Ok(body)),
        Ok(None) => Ok(Err(Refused::NotAnIntersection)),
        Err(error) => match crate::error::native_failure::<()>(Err(error.clone())) {
            Ok(_) => Ok(Err(Refused::Failed(error))),
            Err(_) => Err(error),
        },
    }
}

/// The certified intersection; `None` when a certificate fails.
fn candidate(k: &mut Kernel, mesh: &TriMesh, tolerance: f64) -> Result<Option<Brep>> {
    let size = mesh.size();
    let mut prisms: Vec<Prism> = Vec::new();
    for (axis, _) in parts::axes(mesh) {
        k.check_cancelled("reconstruct mesh")?;
        // A normal within the source's rounding of an axis already taken is that axis.
        if prisms
            .iter()
            .any(|p| size * p.axis.cross(axis).length() <= p.epsilon * 3.)
        {
            continue;
        }
        if let Some(found) = prism(k, mesh, axis, tolerance, Sides::Any)? {
            prisms.push(found);
        }
    }
    let Some(epsilon) = prisms.get(1).map(|p| p.epsilon) else {
        return Ok(None);
    };
    let segments: usize = prisms
        .iter()
        .flat_map(|p| &p.loops)
        .map(|lp| lp.segs.len())
        .sum();
    let samples = mesh.positions.len() + mesh.triangles.len();
    if samples.saturating_mul(segments) > MAX_WALL_WORK {
        return Ok(None);
    }
    // The tests below hold a triangle in the built solid for these sections only: a convex
    // first outline holds every triangle whose corners it holds, and a circle's inside less the
    // tolerance is a disk a triangle can be measured against.
    let straight = epsilon / size.max(1.);
    let sections_fit = prisms.first().is_some_and(
        |first| matches!(first.regions.as_slice(), [region] if convex(&region.outer, straight)),
    ) && prisms
        .iter()
        .flat_map(|p| &p.regions)
        .flat_map(|region| &region.holes)
        .all(|hole| circle(hole).is_some());
    if !sections_fit {
        return Ok(None);
    }
    k.check_cancelled("reconstruct mesh")?;
    for &p in &mesh.positions {
        k.check_cancelled("reconstruct mesh")?;
        for prism in &prisms {
            if !inside(prism, p, tolerance) {
                return Ok(None);
            }
        }
    }
    let mut area = 0.;
    for &[a, b, c] in &mesh.triangles {
        k.check_cancelled("reconstruct mesh")?;
        let (Some(&a), Some(&b), Some(&c)) = (
            mesh.positions.get(a as usize),
            mesh.positions.get(b as usize),
            mesh.positions.get(c as usize),
        ) else {
            return Err(invalid("missing triangle vertex"));
        };
        area += (b - a).cross(c - a).length() * 0.5;
        // A face of the solid built below lies on the first extrusion or on a later one's
        // hole: a triangle on a later extrusion's outline or cap would be material it keeps.
        let corners = [a, b, c];
        let on_first = prisms
            .first()
            .is_some_and(|first| bounds(first, corners, tolerance, Bound::Whole));
        if !on_first
            && !prisms
                .iter()
                .skip(1)
                .any(|prism| bounds(prism, corners, tolerance, Bound::Holes))
        {
            return Ok(None);
        }
        // A wall triangle runs along its own axis and can cross another extrusion's hole with
        // its corners outside it, as the wall of a bore over a web the next bore does not cut:
        // the solid built below has no material there.
        if !prisms
            .iter()
            .all(|prism| clear_of_holes(prism, corners, tolerance))
        {
            return Ok(None);
        }
    }
    k.check_cancelled("reconstruct mesh")?;
    // The first extrusion less each hole of the others' sections, swept past the solid at both
    // ends: no face of a tool lies on a face of the first, as the faces of two extrusions of one
    // block would. It holds their intersection, and every source triangle lies on its boundary.
    let mut prisms = prisms.into_iter();
    let Some(first) = prisms.next() else {
        return Ok(None);
    };
    let mut body = first.body;
    for prism in prisms {
        let normal = prism.axis;
        let Some(start) = MeshPlane::from_normal(normal * (prism.lo - size), normal) else {
            return Ok(None);
        };
        for hole in prism.regions.iter().flat_map(|region| &region.holes) {
            k.check_cancelled("reconstruct mesh")?;
            // A fitted circle starts on its frame's x direction, which is horizontal for every
            // horizontal axis: the seams of two bores at one height then lie in one plane and
            // meet where the bores cross, and the Boolean fails when rounding moves them apart
            // (NoRadialEdgeUse at a 1e-7 miss, for the f32 X and Y bores of the tests). A way
            // round that Boolean defect, not a certificate: a tool's seam starts a quarter turn
            // on, on the frame's y direction, which is up for a horizontal axis. Where it still
            // meets a seam (the X and Z bores of the tests, through one point of their curve)
            // the Boolean builds; where one fails, [`Refused::Failed`] keeps the facets.
            let mut outer = hole.reversed();
            for seg in &mut outer.segs {
                if let Seg2::Arc { start, sweep, .. } = seg {
                    if sweep.abs() >= TAU {
                        *start += FRAC_PI_2;
                    }
                }
            }
            let tool = Region2 {
                outer,
                holes: Vec::new(),
            };
            let length = prism.hi - prism.lo + 2. * size;
            let Some(tool) = native::extrude(k, &start.frame(), &[tool], length)? else {
                return Ok(None);
            };
            let Some(rest) = native::difference(k, body, tool)? else {
                return Ok(None);
            };
            body = rest;
        }
    }
    if body.face_list().len() > MAX_FACES || !native::is_valid(k, &mut body)? {
        return Ok(None);
    }
    // Where the built boundary follows the source within the tolerance, the two solids differ
    // by at most a band that wide along the source's surface, its area times the tolerance, and
    // the cap triangles' rounding across a section, epsilon times size squared. A larger
    // difference is boundary the source does not have, such as a part of the first extrusion
    // that no source triangle lies on.
    let original = mesh.volume();
    let Some(volume) = native::volume(k, &mut body)? else {
        return Ok(None);
    };
    if !original.is_finite()
        || original <= 0.
        || (volume - original).abs() > area * tolerance + epsilon * det::powi(size, 2)
    {
        return Ok(None);
    }
    Ok(Some(body))
}

/// Whether the point is inside the extrusion or within `tolerance` of its boundary.
fn inside(prism: &Prism, p: Vec3, tolerance: f64) -> bool {
    let h = p.dot(prism.axis);
    if !(prism.lo - tolerance..=prism.hi + tolerance).contains(&h) {
        return false;
    }
    let q = prism.plane.to_local(p);
    let mut held = false;
    for lp in &prism.loops {
        if lp
            .segs
            .iter()
            .any(|s| distance_to_segment(s, q) <= tolerance)
        {
            return true;
        }
        match lp.holds(q) {
            Some(holds) => held ^= holds,
            None => return false,
        }
    }
    held
}

/// The centre and radius of a hole that is one full circle, as [`super::prism`] fits a bore.
pub(super) fn circle(hole: &Loop2) -> Option<(Vec2, f64)> {
    match hole.segs.as_slice() {
        [Seg2::Arc {
            center,
            radius,
            sweep,
            ..
        }] if sweep.abs() >= TAU => Some((*center, *radius)),
        _ => None,
    }
}

/// Whether the loop turns one way only, once round, a corner by less than `straight` (its angle)
/// not counting: the outline of a convex section, which holds the segment between any two points
/// it holds. A turn back on itself (half a turn at a corner) or a second time round is not.
pub(super) fn convex(outline: &Loop2, straight: f64) -> bool {
    // A segment's direction where it starts and ends, and which way it turns between.
    let tangent =
        |angle: f64, sweep: f64| Vec2::new(-det::sin(angle), det::cos(angle)) * sweep.signum();
    let mut ends = Vec::new();
    for seg in &outline.segs {
        match *seg {
            Seg2::Line { a, b } => {
                let Some(along) = (b - a).normalized() else {
                    return false;
                };
                ends.push((along, along, 0.));
            }
            Seg2::Arc { start, sweep, .. } => {
                ends.push((tangent(start, sweep), tangent(start + sweep, sweep), sweep));
            }
            _ => return false,
        }
    }
    let mut turns = ends.iter().map(|end| end.2).collect::<Vec<_>>();
    for (before, after) in ends.iter().zip(ends.iter().cycle().skip(1)) {
        let turn = det::atan2(before.1.cross(after.0), before.1.dot(after.0));
        if turn.abs() >= PI - straight {
            return false;
        }
        if turn.abs() > straight {
            turns.push(turn);
        }
    }
    let total: f64 = turns.iter().sum();
    let one_way = turns.iter().all(|&t| t >= 0.) || turns.iter().all(|&t| t <= 0.);
    // The corners left out turn by at most `straight` each.
    one_way && (total.abs() - TAU).abs() <= straight * (turns.len() + ends.len()) as f64
}

/// Whether the triangle, seen along the extrusion's axis, keeps out of each of its section's
/// holes (one circle each) but for `tolerance` inside the circle.
fn clear_of_holes(prism: &Prism, corners: [Vec3; 3], tolerance: f64) -> bool {
    let projected = corners.map(|p| prism.plane.to_local(p));
    prism
        .regions
        .iter()
        .flat_map(|region| &region.holes)
        .all(|hole| {
            circle(hole).is_some_and(|(center, radius)| {
                distance_to_triangle(&projected, center) >= radius - tolerance
            })
        })
}

/// The distance from `point` to the triangle `corners`, zero on or inside it.
fn distance_to_triangle(corners: &[Vec2; 3], point: Vec2) -> f64 {
    let [a, b, c] = *corners;
    let sides = [(a, b), (b, c), (c, a)];
    let turns = sides.map(|(from, to)| (to - from).cross(point - from));
    if turns.iter().all(|&t| t >= 0.) || turns.iter().all(|&t| t <= 0.) {
        return 0.;
    }
    sides
        .iter()
        .map(|&(a, b)| distance_to_segment(&Seg2::Line { a, b }, point))
        .fold(f64::INFINITY, f64::min)
}

/// Which part of an extrusion's boundary a triangle may lie on.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Bound {
    /// Its caps and all its walls: the extrusion the solid is cut from.
    Whole,
    /// The walls of its sections' holes only: the tools cut from it.
    Holes,
}

/// Whether the triangle is one of the extrusion's cap triangles (within its rounding of a cap's
/// plane, as [`prism`] reads them) or lies whole within `tolerance` of one of its walls, for
/// the part of the boundary `bound` allows.
fn bounds(prism: &Prism, corners: [Vec3; 3], tolerance: f64, bound: Bound) -> bool {
    let heights = corners.map(|p| p.dot(prism.axis));
    if bound == Bound::Whole
        && [prism.lo, prism.hi]
            .iter()
            .any(|level| heights.iter().all(|h| (h - level).abs() <= prism.epsilon))
    {
        return true;
    }
    if !heights
        .iter()
        .all(|h| (prism.lo - tolerance..=prism.hi + tolerance).contains(h))
    {
        return false;
    }
    let projected = corners.map(|p| prism.plane.to_local(p));
    let walls = |lp: &Loop2| certified(&projected, &lp.segs, tolerance);
    match bound {
        Bound::Whole => prism.loops.iter().any(walls),
        Bound::Holes => prism
            .regions
            .iter()
            .flat_map(|region| &region.holes)
            .any(walls),
    }
}
