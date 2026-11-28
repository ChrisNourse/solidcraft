//! Where one body's straight edges meet another body, read-only: an edge lying on the other's
//! planar face in an open inside corner ([`edges_on_faces`]), and edges of the two lying side by
//! side with the faces beside them flush ([`flush_edges`]). For an application that offers the
//! places two parts could be joined (a weld's joint picking): it computes nothing itself
//! (solidcore/docs/layers.md §4). Nothing here is named after what the application joins.
//!
//! Each body is read on its own: [`side_edges`] reads one body's straight edges with the planes
//! and inward directions of their faces, and [`edges_on_faces`] tests them against the other
//! body's faces, so neither body is copied and every id stays the body's own.
//!
//! Planar faces and straight edges only. Any other face or edge, and a seam, is no candidate
//! and is passed over; a query that fails is the caller's error, never taken for no contact.

use crate::intersect::api_intersectors::curve_plane_parameters;
use crate::kernel::Kernel;
use crate::query::api_queries::{edge_inward, face_normal_at_edge, get_edges, get_faces};
use crate::query::face::p1::point_on_face;
use crate::topology::edgeuse::p1 as eu;
use crate::topology::face::p2::outer_faceuse;
use crate::topology::{edge, face, vertex, Brep, EdgeId, FaceId};
use crate::{Point3, Result, SolidError, Vec3};

const OP: &str = "query::edge_contacts";

/// A planar face's plane: a point of it and its outward unit normal.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FacePlane {
    /// A point of the face (a vertex).
    pub point: Point3,
    /// The face's outward unit normal.
    pub normal: Vec3,
}

impl FacePlane {
    /// The signed distance of `p` from the plane along the normal.
    #[must_use]
    pub fn height(self, p: Point3) -> f64 {
        self.normal.dot(Vec3::between(self.point, p))
    }

    fn foot(self, p: Point3) -> Point3 {
        p - self.normal * self.height(p)
    }
}

/// A straight edge between two vertices with exactly two faces, both planar, neither on both
/// sides of it (not a seam), and the unit direction into each face from the edge's middle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SideEdge {
    /// The edge.
    pub edge: EdgeId,
    /// Its start and end points.
    pub ends: [Point3; 2],
    /// Its two faces with their planes.
    pub faces: [(FaceId, FacePlane); 2],
    /// The direction into each of `faces`.
    pub inward: [Vec3; 2],
}

/// A straight edge of one body lying on a planar face of the other in an open inside corner.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EdgeOnFace {
    /// The edge (an id of the edges' body).
    pub edge: EdgeId,
    /// The face beside the edge that faces `face` (the edges' body).
    pub against: FaceId,
    /// The other face beside the edge, rising from `face` (the edges' body).
    pub rising: FaceId,
    /// The face the edge lies on (the faces' body).
    pub face: FaceId,
    /// The stretch of the edge over `face`, its two ends on the edge.
    pub run: [Point3; 2],
    /// The distance from `face`'s plane at the run's ends, along its outward normal; negative
    /// where the edge lies inside the faces' body.
    pub gap: [f64; 2],
}

/// An edge of each body side by side: a face beside each facing the other, the other faces
/// beside them flush (in one plane, within the tolerance).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FlushEdges {
    /// The edge of the first body, then of the second.
    pub edges: [EdgeId; 2],
    /// The flush faces beside them, the first body's first.
    pub flush: [FaceId; 2],
    /// The stretch the two edges share, its ends on the first body's edge.
    pub run: [Point3; 2],
    /// The gap across the facing faces at the run's ends, from the first body's facing face;
    /// negative where the bodies overlap.
    pub gap: [f64; 2],
}

/// The straight edges of `b` longer than `tol` between two vertices, with exactly two faces, both
/// planar, neither on both sides of the edge. Any other edge (curved, open-ended, a seam, a
/// sheet's border, an edge of three faces) is no candidate and is passed over.
///
/// # Errors
/// Those of reading the body's topology, and of the line, planarity, normal and inward-direction
/// queries: a failure there is returned, not taken for a noncandidate.
pub fn side_edges(k: &Kernel, b: &mut Brep, tol: f64) -> Result<Vec<SideEdge>> {
    let mut out = Vec::new();
    if !(tol.is_finite() && tol > 0.0) {
        return Ok(out);
    }
    for e in get_edges(b)? {
        let Some(curve) = edge::curve(b, e)? else {
            continue;
        };
        if curve.is_line(k, 9, tol)?.is_none() {
            continue;
        }
        let vertices = edge::vertices(b, e, None)?;
        let &[Some(v0), Some(v1)] = vertices.as_slice() else {
            continue;
        };
        let ends = [vertex::point(b, v0)?, vertex::point(b, v1)?];
        if Vec3::between(ends[0], ends[1]).length() <= tol {
            continue;
        }
        // Two faces, neither on both sides of the edge: a seam is no candidate.
        let mut faces = Vec::new();
        let mut uses = Vec::new();
        let mut seam = false;
        for u in edge::edgeuses(b, e, None, None)? {
            let Some(f) = eu::face(b, u)? else {
                continue;
            };
            let fu = eu::faceuse(b, u)?;
            seam |= uses.contains(&(f, fu));
            uses.push((f, fu));
            if !faces.contains(&f) {
                faces.push(f);
            }
        }
        let &[f0, f1] = faces.as_slice() else {
            continue;
        };
        if seam {
            continue;
        }
        let (Some(p0), Some(p1)) = (plane_of(k, b, f0)?, plane_of(k, b, f1)?) else {
            continue;
        };
        let inward = [edge_inward(k, b, e, f0)?, edge_inward(k, b, e, f1)?];
        out.push(SideEdge {
            edge: e,
            ends,
            faces: [(f0, p0), (f1, p1)],
            inward,
        });
    }
    Ok(out)
}

/// The edges among `edges` (another body's, from [`side_edges`]) lying on planar faces of
/// `faces_body`, no farther from the face's plane than `reach` at either end of the stretch over
/// the face, in an open inside corner: the edge's face that faces the face most faces it, the
/// other rises off it, and the face goes on past the edge on the air side. Each stretch of an
/// edge over a face is its own entry.
///
/// # Errors
/// Those of reading the body's topology and of the planarity, normal, boundary-crossing and
/// point-on-face queries.
pub fn edges_on_faces(
    k: &Kernel,
    faces_body: &mut Brep,
    edges: &[SideEdge],
    reach: f64,
    tol: f64,
) -> Result<Vec<EdgeOnFace>> {
    let mut out = Vec::new();
    if !(reach.is_finite() && reach >= 0.0 && tol.is_finite() && tol > 0.0) {
        return Ok(out);
    }
    let mut planes = Vec::new();
    for f in get_faces(faces_body)? {
        if let Some(p) = plane_of(k, faces_body, f)? {
            planes.push((f, p));
        }
    }
    for side in edges {
        let [p0, p1] = side.ends;
        for &(f, plane) in &planes {
            let n = plane.normal;
            // The face beside the edge that faces `f` most lies against it; the other rises.
            let (c, r) = if side.faces[0].1.normal.dot(n) <= side.faces[1].1.normal.dot(n) {
                (0, 1)
            } else {
                (1, 0)
            };
            let (Some(&(against, pc)), Some(&(rising, pr))) =
                (side.faces.get(c), side.faces.get(r))
            else {
                continue;
            };
            let (nc, nr) = (pc.normal, pr.normal);
            if nc.dot(n) >= 0.0 || nr.dot(n) <= nc.dot(n) {
                continue;
            }
            // An edge wholly beyond reach of the plane on one side is no candidate.
            let (h0, h1) = (plane.height(p0), plane.height(p1));
            if (h0 > reach && h1 > reach) || (h0 < -reach && h1 < -reach) {
                continue;
            }
            let (Some(&rise), Some(&into_against)) = (side.inward.get(r), side.inward.get(c))
            else {
                continue;
            };
            // Convex at the edge and rising off the face: open air between `rising` and the face.
            if rise.dot(nc) >= 0.0 || rise.dot(n) <= 0.0 {
                continue;
            }
            for [t0, t1] in runs_on_face(k, faces_body, f, plane, [p0, p1], tol)? {
                let run = [lerp(p0, p1, t0), lerp(p0, p1, t1)];
                let gap = run.map(|p| plane.height(p));
                if gap.iter().any(|g| g.abs() > reach) {
                    continue;
                }
                // Off the edge on the air side the face must go on, or there is no corner. The
                // step is taken in the face's plane, so a plate leaning on its edge is probed on
                // the face too.
                let Ok(away) = (into_against - n * into_against.dot(n)).unitize() else {
                    continue;
                };
                let mid = plane.foot(lerp(run[0], run[1], 0.5));
                let probe = mid - away * (10.0 * tol);
                if point_on_face(k, faces_body, f, probe, tol)?.is_none() {
                    continue;
                }
                out.push(EdgeOnFace {
                    edge: side.edge,
                    against,
                    rising,
                    face: f,
                    run,
                    gap,
                });
            }
        }
    }
    Ok(out)
}

/// Edges of two bodies (each from [`side_edges`]) side by side within `reach` over the stretch
/// they share, a face beside each facing the other within `reach`, and the other faces beside
/// them flush: the other edge in their plane within `tol`. A pair is kept only when it holds
/// measured from either edge, so swapping the two lists gives the same pairs, each entry's fields
/// swapped (its run and gap then measured from the other edge).
#[must_use]
pub fn flush_edges(a: &[SideEdge], b: &[SideEdge], reach: f64, tol: f64) -> Vec<FlushEdges> {
    let mut out = Vec::new();
    if !(reach.is_finite() && reach >= 0.0 && tol.is_finite() && tol > 0.0) {
        return out;
    }
    for sa in a {
        for sb in b {
            for i in 0..2 {
                for j in 0..2 {
                    let (Some(here), Some(_)) = (
                        side_by_side(sa, sb, [i, j], reach, tol),
                        side_by_side(sb, sa, [j, i], reach, tol),
                    ) else {
                        continue;
                    };
                    let (Some(&(fa, _)), Some(&(fb, _))) =
                        (sa.faces.get(1 - i), sb.faces.get(1 - j))
                    else {
                        continue;
                    };
                    out.push(FlushEdges {
                        edges: [sa.edge, sb.edge],
                        flush: [fa, fb],
                        run: here.0,
                        gap: here.1,
                    });
                }
            }
        }
    }
    out
}

/// Measured from `sa`: the stretch of `sa` that `sb` shares, both edges within `reach` of each
/// other at its ends, `sa`'s face `facing[0]` and `sb`'s face `facing[1]` facing each other with
/// `sb`'s edge within `reach` of the first's plane, and the other faces flush (`sb`'s edge within
/// `tol` of `sa`'s other face's plane). The run's ends on `sa` and the gap from its facing face.
fn side_by_side(
    sa: &SideEdge,
    sb: &SideEdge,
    facing: [usize; 2],
    reach: f64,
    tol: f64,
) -> Option<([Point3; 2], [f64; 2])> {
    let [p0, p1] = sa.ends;
    let along = Vec3::between(p0, p1);
    let length = along.length();
    let dir = along.unitize().ok()?;
    let [q0, q1] = sb.ends;
    let (s0, s1) = (
        dir.dot(Vec3::between(p0, q0)),
        dir.dot(Vec3::between(p0, q1)),
    );
    if (s1 - s0).abs() <= tol {
        return None;
    }
    let (lo, hi) = (s0.min(s1).max(0.0), s0.max(s1).min(length));
    if hi - lo <= tol {
        return None;
    }
    let on_b = |s: f64| lerp(q0, q1, (s - s0) / (s1 - s0));
    let on_a = |s: f64| p0 + dir * s;
    if [lo, hi]
        .iter()
        .any(|&s| Vec3::between(on_a(s), on_b(s)).length() > reach)
    {
        return None;
    }
    let [i, j] = facing;
    let (&(_, fa), &(_, ta)) = (sa.faces.get(i)?, sa.faces.get(1 - i)?);
    let (&(_, fb), &(_, tb)) = (sb.faces.get(j)?, sb.faces.get(1 - j)?);
    if fa.normal.dot(fb.normal) >= 0.0 || ta.normal.dot(tb.normal) <= 0.0 {
        return None;
    }
    if [lo, hi].iter().any(|&s| ta.height(on_b(s)).abs() > tol) {
        return None;
    }
    let gap = [lo, hi].map(|s| fa.height(on_b(s)));
    if gap.iter().any(|g| g.abs() > reach) {
        return None;
    }
    Some(([on_a(lo), on_a(hi)], gap))
}

/// The intervals of `t` in [0, 1] where the foot of `p0 + t (p1 - p0)` on `plane` lies on face
/// `f`. The segment is cut where face `f`'s boundary edges cross the plane through it square to
/// the face (the kernel's curve/plane solve on each edge's curve), and each piece is tested at its
/// middle, so a narrow stretch of the face or a narrow hole in it is found wherever it lies.
fn runs_on_face(
    k: &Kernel,
    b: &mut Brep,
    f: FaceId,
    plane: FacePlane,
    [p0, p1]: [Point3; 2],
    tol: f64,
) -> Result<Vec<[f64; 2]>> {
    let (a, z) = (plane.foot(p0), plane.foot(p1));
    let along = Vec3::between(a, z);
    let length = along.length();
    let (Ok(dir), Ok(across)) = (along.unitize(), plane.normal.cross(along).unitize()) else {
        return Ok(Vec::new());
    };
    let mut cuts = vec![0.0, 1.0];
    for e in face::edges(b, f, None, false, None)? {
        // An edge without a curve is a point: it crosses nothing.
        let Some(curve) = edge::curve(b, e)? else {
            continue;
        };
        let interval = edge::interval(b, e)?;
        for s in curve_plane_parameters(k, curve, &interval, a, across)? {
            let &p = curve
                .evaluate(k, s, 0, true, false)?
                .first()
                .ok_or_else(|| SolidError::invalid_input(OP, "an evaluation without its point"))?;
            let t = dir.dot(Vec3::between(a, p)) / length;
            if t > 0.0 && t < 1.0 {
                cuts.push(t);
            }
        }
    }
    cuts.sort_by(f64::total_cmp);
    let mut runs: Vec<[f64; 2]> = Vec::new();
    for w in cuts.windows(2) {
        let &[t0, t1] = w else {
            continue;
        };
        if (t1 - t0) * length <= tol {
            continue;
        }
        let mid = lerp(a, z, 0.5 * (t0 + t1));
        if point_on_face(k, b, f, mid, tol)?.is_none() {
            continue;
        }
        match runs.last_mut() {
            Some(last) if (t0 - last[1]) * length <= tol => last[1] = t1,
            _ => runs.push([t0, t1]),
        }
    }
    Ok(runs)
}

/// Face `f`'s plane when it is planar, through a vertex of its first edge, its normal the face's
/// outward one at that edge; `None` (no candidate) for a face without surface or edges, or that is
/// not planar.
///
/// # Errors
/// Those of the planarity and normal queries, and [`SolidError::InvalidInput`] for a normal or
/// point that is not finite.
fn plane_of(k: &Kernel, b: &mut Brep, f: FaceId) -> Result<Option<FacePlane>> {
    let Some(surface) = face::surface(b, f)? else {
        return Ok(None);
    };
    let planarity = surface.is_planar(k, face::tolerance(b, f)?)?;
    if !planarity.is_planar {
        return Ok(None);
    }
    let Some(&e) = face::edges(b, f, None, false, None)?.first() else {
        return Ok(None);
    };
    let Some(fu) = outer_faceuse(b, f)? else {
        return Ok(None);
    };
    let frame = face_normal_at_edge(k, b, e, f, fu)?;
    let Some(Some(v)) = edge::vertices(b, e, None)?.first().copied() else {
        return Ok(None);
    };
    let normal = frame.normal.value;
    let point = vertex::point(b, v)?;
    let finite = |v: Vec3| v.x.is_finite() && v.y.is_finite() && v.z.is_finite();
    if !(finite(normal) && finite(point)) {
        return Err(SolidError::invalid_input(
            OP,
            "a face plane that is not finite",
        ));
    }
    Ok(Some(FacePlane { point, normal }))
}

fn lerp(a: Point3, b: Point3, t: f64) -> Point3 {
    a + Vec3::between(a, b) * t
}
