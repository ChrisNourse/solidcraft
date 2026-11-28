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
//! Planar faces and straight edges only; any other face or edge is passed over.

use crate::kernel::Kernel;
use crate::query::api_queries::{edge_inward, face_normal_at_edge, get_edges, get_faces};
use crate::query::face::p1::point_on_face;
use crate::topology::edgeuse::p1 as eu;
use crate::topology::face::p2::outer_faceuse;
use crate::topology::{edge, face, vertex, Brep, EdgeId, FaceId};
use crate::{Point3, Result, SolidError, Vec3};

/// How many even pieces an edge is tested in before the ends of its runs on a face are bisected
/// to the tolerance.
// shortcut: a stretch of a face narrower than 1/32 of an edge between two test points can be missed; refine adaptively if a case shows up.
const SAMPLES: usize = 32;

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

/// A straight edge between two vertices with exactly two faces, both planar, and the unit
/// direction into each face from the edge's middle (`None` where the kernel refuses it as input,
/// a seam).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SideEdge {
    /// The edge.
    pub edge: EdgeId,
    /// Its start and end points.
    pub ends: [Point3; 2],
    /// Its two faces with their planes.
    pub faces: [(FaceId, FacePlane); 2],
    /// The direction into each of `faces`.
    pub inward: [Option<Vec3>; 2],
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
/// beside them pointing the same way.
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

/// The straight edges of `b` longer than `tol` between two vertices, with exactly two faces,
/// both planar. Any other edge is passed over.
///
/// # Errors
/// Those of reading the body's topology and of a face or edge query that is not passed over.
pub fn side_edges(k: &Kernel, b: &mut Brep, tol: f64) -> Result<Vec<SideEdge>> {
    let mut out = Vec::new();
    if !(tol.is_finite() && tol > 0.0) {
        return Ok(out);
    }
    for e in get_edges(b)? {
        let Some(curve) = edge::curve(b, e)? else {
            continue;
        };
        if !matches!(curve.is_line(k, 9, tol), Ok(Some(_))) {
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
        let mut faces = Vec::new();
        for u in edge::edgeuses(b, e, None, None)? {
            if let Some(f) = eu::face(b, u)? {
                if !faces.contains(&f) {
                    faces.push(f);
                }
            }
        }
        let &[f0, f1] = faces.as_slice() else {
            continue;
        };
        let (Some(p0), Some(p1)) = (plane_of(k, b, f0)?, plane_of(k, b, f1)?) else {
            continue;
        };
        let inward = [inward(k, b, e, f0)?, inward(k, b, e, f1)?];
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
/// Those of reading the body's topology and of a face or point query that is not passed over.
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
            let (Some(&Some(rise)), Some(&Some(into_against))) =
                (side.inward.get(r), side.inward.get(c))
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
                // Off the edge on the air side the face must go on, or there is no corner.
                let mid = plane.foot(lerp(run[0], run[1], 0.5));
                let probe = mid - into_against * (10.0 * tol);
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
/// they share, a face beside each facing the other and the other faces beside them pointing the
/// same way. Swapping the two lists swaps each entry's fields and nothing else.
#[must_use]
pub fn flush_edges(a: &[SideEdge], b: &[SideEdge], reach: f64, tol: f64) -> Vec<FlushEdges> {
    let mut out = Vec::new();
    if !(reach.is_finite() && reach >= 0.0 && tol.is_finite() && tol > 0.0) {
        return out;
    }
    for sa in a {
        let [p0, p1] = sa.ends;
        let along = Vec3::between(p0, p1);
        let length = along.length();
        let Ok(dir) = along.unitize() else {
            continue;
        };
        for sb in b {
            let [q0, q1] = sb.ends;
            let (s0, s1) = (
                dir.dot(Vec3::between(p0, q0)),
                dir.dot(Vec3::between(p0, q1)),
            );
            if (s1 - s0).abs() <= tol {
                continue;
            }
            let (lo, hi) = (s0.min(s1).max(0.0), s0.max(s1).min(length));
            if hi - lo <= tol {
                continue;
            }
            // The distance between the edges over the shared stretch only, so neither edge's
            // unused length decides (and the order of the bodies does not).
            let on_b = |s: f64| lerp(q0, q1, (s - s0) / (s1 - s0));
            let on_a = |s: f64| p0 + dir * s;
            if [lo, hi]
                .iter()
                .any(|&s| Vec3::between(on_a(s), on_b(s)).length() > reach)
            {
                continue;
            }
            let [a0, a1] = sa.faces;
            let [b0, b1] = sb.faces;
            for (fa, ta) in [(a0, a1), (a1, a0)] {
                for (fb, tb) in [(b0, b1), (b1, b0)] {
                    if fa.1.normal.dot(fb.1.normal) >= 0.0 || ta.1.normal.dot(tb.1.normal) <= 0.0 {
                        continue;
                    }
                    let gap = [lo, hi].map(|s| fa.1.height(on_b(s)));
                    if gap.iter().any(|g| g.abs() > reach) {
                        continue;
                    }
                    out.push(FlushEdges {
                        edges: [sa.edge, sb.edge],
                        flush: [ta.0, tb.0],
                        run: [on_a(lo), on_a(hi)],
                        gap,
                    });
                }
            }
        }
    }
    out
}

/// The intervals of `t` in [0, 1] where the foot of `p0 + t (p1 - p0)` on `plane` lies on face
/// `f`: [`SAMPLES`] even tests, each change bisected to `tol` along the segment.
fn runs_on_face(
    k: &Kernel,
    b: &mut Brep,
    f: FaceId,
    plane: FacePlane,
    [p0, p1]: [Point3; 2],
    tol: f64,
) -> Result<Vec<[f64; 2]>> {
    let length = Vec3::between(p0, p1).length();
    let mut on = |t: f64| -> Result<bool> {
        Ok(point_on_face(k, b, f, plane.foot(lerp(p0, p1, t)), tol)?.is_some())
    };
    let mut runs = Vec::new();
    let mut start = if on(0.0)? { Some(0.0) } else { None };
    let mut prev = (0.0, start.is_some());
    for i in 1..=SAMPLES {
        let t = i as f64 / SAMPLES as f64;
        let now = on(t)?;
        if now != prev.1 {
            let (mut lo, mut hi) = (prev.0, t);
            while (hi - lo) * length > tol {
                let mid = 0.5 * (lo + hi);
                if on(mid)? == prev.1 {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            match start.take() {
                Some(s) => runs.push([s, lo]),
                None => start = Some(hi),
            }
        }
        prev = (t, now);
    }
    if let Some(s) = start {
        runs.push([s, 1.0]);
    }
    runs.retain(|[t0, t1]| (t1 - t0) * length > tol);
    Ok(runs)
}

/// Face `f`'s plane when it is planar, through a vertex of its first edge; `None` for a face that
/// is not planar or that the kernel cannot describe.
fn plane_of(k: &Kernel, b: &mut Brep, f: FaceId) -> Result<Option<FacePlane>> {
    let Some(surface) = face::surface(b, f)? else {
        return Ok(None);
    };
    let Ok(planarity) = surface.is_planar(k, face::tolerance(b, f)?) else {
        return Ok(None);
    };
    if !planarity.is_planar {
        return Ok(None);
    }
    let Some(&e) = face::edges(b, f, None, false, None)?.first() else {
        return Ok(None);
    };
    let Some(fu) = outer_faceuse(b, f)? else {
        return Ok(None);
    };
    let Ok(frame) = face_normal_at_edge(k, b, e, f, fu) else {
        return Ok(None);
    };
    let Some(Some(v)) = edge::vertices(b, e, None)?.first().copied() else {
        return Ok(None);
    };
    let normal = frame.normal.value;
    let point = vertex::point(b, v)?;
    let finite = |v: Vec3| v.x.is_finite() && v.y.is_finite() && v.z.is_finite();
    Ok((finite(normal) && finite(point)).then_some(FacePlane { point, normal }))
}

/// The unit direction into face `f` from the middle of edge `e` ([`edge_inward`]); `None` where
/// the kernel refuses it as input (a seam, an edge that does not bound the face): those edges are
/// passed over, every other failure is the caller's.
fn inward(k: &Kernel, b: &mut Brep, e: EdgeId, f: FaceId) -> Result<Option<Vec3>> {
    match edge_inward(k, b, e, f) {
        Ok(v) => Ok(Some(v)),
        Err(SolidError::InvalidInput { .. }) => Ok(None),
        Err(err) => Err(err),
    }
}

fn lerp(a: Point3, b: Point3, t: f64) -> Point3 {
    a + Vec3::between(a, b) * t
}
