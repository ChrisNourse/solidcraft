//! Where one body's straight edges meet another body, in record indices
//! ([`solidcore::query::edge_contacts`]): an edge lying on the other's planar face in an open
//! inside corner, and edges of the two side by side with the faces beside them flush. Each body
//! is read on its own and only read; nothing is copied.
//!
//! An application asks this for the places two parts could be joined (a weld's joint picking)
//! and computes nothing itself.

use std::collections::HashMap;

use solidcore::query::edge_contacts::{self as q, SideEdge};
use solidcore::topology::{EdgeId, FaceId};
use solidcore::Vec3;

use crate::body::Body;
use crate::order::Order;
use crate::session::with_body;
use crate::{Error, Result};

/// One body's straight edges with exactly two planar faces ([`Body::side_edges`]), for
/// [`Body::edges_on_faces`] on another body and for [`flush_edges`].
#[derive(Clone, Debug)]
pub struct SideEdges {
    list: Vec<SideEdge>,
    /// Each entry's edge and faces as record indices.
    index: Vec<(usize, [usize; 2])>,
}

/// A straight edge of one body lying on a planar face of another in an open inside corner,
/// in record indices.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EdgeOnFace {
    /// The edge (the edges' body).
    pub edge: usize,
    /// The face beside the edge that faces `face` (the edges' body).
    pub against: usize,
    /// The other face beside the edge, rising from `face` (the edges' body).
    pub rising: usize,
    /// The face the edge lies on (the faces' body).
    pub face: usize,
    /// The stretch of the edge over `face`, its two ends.
    pub run: [[f64; 3]; 2],
    /// The distance from `face`'s plane at the run's ends; negative inside the faces' body.
    pub gap: [f64; 2],
}

/// An edge of each body side by side with the faces beside them flush, in record indices.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FlushEdges {
    /// The first body's edge, then the second's.
    pub edges: [usize; 2],
    /// The flush faces beside them, the first body's first.
    pub flush: [usize; 2],
    /// The stretch the edges share, its ends on the first body's edge.
    pub run: [[f64; 3]; 2],
    /// The gap across the facing faces at the run's ends; negative where the bodies overlap.
    pub gap: [f64; 2],
}

impl Body {
    /// The body's straight edges longer than `tol` with exactly two faces, both planar
    /// ([`q::side_edges`]); an edge or face that `order` does not hold is left out. The body is
    /// only read.
    ///
    /// # Errors
    /// [`Error::Argument`] for a `tol` that is not finite and positive; the kernel's error.
    pub fn side_edges(&self, order: &Order, tol: f64) -> Result<SideEdges> {
        if !(tol.is_finite() && tol > 0.0) {
            return Err(Error::Argument);
        }
        let all = with_body("side_edges", self, |k, b| q::side_edges(k, b, tol).map(Ok))??;
        let (edges, faces) = (places(&order.edges), places(&order.faces));
        let mut out = SideEdges {
            list: Vec::new(),
            index: Vec::new(),
        };
        for s in all {
            let [(f0, _), (f1, _)] = s.faces;
            if let (Some(&e), Some(&a), Some(&b)) =
                (edges.get(&s.edge), faces.get(&f0), faces.get(&f1))
            {
                out.list.push(s);
                out.index.push((e, [a, b]));
            }
        }
        Ok(out)
    }

    /// The edges among `edges` (another body's, from [`Body::side_edges`]) lying on this body's
    /// planar faces within `reach` at both ends of the stretch over the face, in an open inside
    /// corner ([`q::edges_on_faces`]). This body is only read.
    ///
    /// # Errors
    /// [`Error::Argument`] for a `reach` that is not finite and at least 0 or a `tol` that is not
    /// finite and positive; the kernel's error.
    pub fn edges_on_faces(
        &self,
        order: &Order,
        edges: &SideEdges,
        reach: f64,
        tol: f64,
    ) -> Result<Vec<EdgeOnFace>> {
        if !(reach.is_finite() && reach >= 0.0 && tol.is_finite() && tol > 0.0) {
            return Err(Error::Argument);
        }
        let found = with_body("edges_on_faces", self, |k, b| {
            q::edges_on_faces(k, b, &edges.list, reach, tol).map(Ok)
        })??;
        let faces = places(&order.faces);
        let own = |e: EdgeId, f: FaceId| -> Option<(usize, usize)> {
            let i = edges.list.iter().position(|s| s.edge == e)?;
            let (edge, [a, b]) = *edges.index.get(i)?;
            let side = edges.list.get(i)?;
            Some((edge, if side.faces[0].0 == f { a } else { b }))
        };
        Ok(found
            .into_iter()
            .filter_map(|j| {
                let (edge, against) = own(j.edge, j.against)?;
                let (_, rising) = own(j.edge, j.rising)?;
                Some(EdgeOnFace {
                    edge,
                    against,
                    rising,
                    face: *faces.get(&j.face)?,
                    run: j.run.map(array),
                    gap: j.gap,
                })
            })
            .collect())
    }
}

/// Edges of two bodies (each from [`Body::side_edges`]) side by side within `reach` over the
/// stretch they share, the faces beside them flush ([`q::flush_edges`]). Swapping the two swaps
/// each entry's fields and nothing else.
#[must_use]
pub fn flush_edges(a: &SideEdges, b: &SideEdges, reach: f64, tol: f64) -> Vec<FlushEdges> {
    let index = |s: &SideEdges, e: EdgeId, f: FaceId| -> Option<(usize, usize)> {
        let i = s.list.iter().position(|x| x.edge == e)?;
        let (edge, [f0, f1]) = *s.index.get(i)?;
        let side = s.list.get(i)?;
        Some((edge, if side.faces[0].0 == f { f0 } else { f1 }))
    };
    q::flush_edges(&a.list, &b.list, reach, tol)
        .into_iter()
        .filter_map(|j| {
            let (ea, fa) = index(a, j.edges[0], j.flush[0])?;
            let (eb, fb) = index(b, j.edges[1], j.flush[1])?;
            Some(FlushEdges {
                edges: [ea, eb],
                flush: [fa, fb],
                run: j.run.map(array),
                gap: j.gap,
            })
        })
        .collect()
}

/// Each id's first place in `list`.
fn places<T: Copy + Eq + std::hash::Hash>(list: &[T]) -> HashMap<T, usize> {
    let mut out = HashMap::with_capacity(list.len());
    for (i, &x) in list.iter().enumerate() {
        out.entry(x).or_insert(i);
    }
    out
}

fn array(v: Vec3) -> [f64; 3] {
    [v.x, v.y, v.z]
}

#[cfg(test)]
#[path = "edge_contacts_tests.rs"]
mod tests;
