//! Edges on faces and flush edges between boxes: a T, a lap and a butt joint, each found where it
//! is with its faces, run and gap checked by hand; a gap, an overlap and the reach; the order of
//! the bodies changing nothing but the order of the fields.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use super::*;
use crate::body::make_box;

const TOL: f64 = 1e-6;
const REACH: f64 = 5.0;

fn order(b: &Body) -> Order {
    b.to_record().unwrap().1
}

/// The length of a run along an axis (every run in these tests is): its largest coordinate change.
fn length(run: [[f64; 3]; 2]) -> f64 {
    let [a, b] = run;
    (0..3).map(|i| (b[i] - a[i]).abs()).fold(0.0, f64::max)
}

fn sorted_lengths(runs: impl Iterator<Item = [[f64; 3]; 2]>) -> Vec<f64> {
    let mut l: Vec<f64> = runs.map(length).collect();
    l.sort_by(f64::total_cmp);
    l
}

fn close(got: f64, want: f64) {
    assert!((got - want).abs() < 1e-6, "{got} against {want}");
}

/// The edges of `edges` lying on faces of `faces`.
fn on_faces(edges: &Body, faces: &Body, reach: f64) -> Vec<EdgeOnFace> {
    let sides = edges.side_edges(&order(edges), TOL).unwrap();
    faces
        .edges_on_faces(&order(faces), &sides, reach, TOL)
        .unwrap()
}

fn flush(a: &Body, b: &Body, reach: f64) -> Vec<FlushEdges> {
    let (sa, sb) = (
        a.side_edges(&order(a), TOL).unwrap(),
        b.side_edges(&order(b), TOL).unwrap(),
    );
    flush_edges(&sa, &sb, reach, TOL)
}

/// A 60 x 60 x 5 base and an 8 mm plate standing on it, its foot at `z`.
fn t_joint(z: f64) -> (Body, Body) {
    let base = make_box([-30.0, -30.0, -5.0], [60.0, 60.0, 5.0]).unwrap();
    let plate = make_box([-4.0, -20.0, z], [8.0, 40.0, 40.0]).unwrap();
    (base, plate)
}

#[test]
fn a_standing_plate_s_foot_edges_lie_on_the_base_in_four_inside_corners() {
    let (base, plate) = t_joint(0.0);
    let found = on_faces(&plate, &base, REACH);
    assert_eq!(found.len(), 4, "{found:?}");
    for (got, want) in sorted_lengths(found.iter().map(|j| j.run))
        .into_iter()
        .zip([8.0, 8.0, 40.0, 40.0])
    {
        close(got, want);
    }
    assert!(
        found.iter().all(|j| j.face == found[0].face),
        "all on the base's top face"
    );
    found.iter().flat_map(|j| j.gap).for_each(|g| close(g, 0.0));
    assert!(
        on_faces(&base, &plate, REACH).is_empty(),
        "the base's edges lie on no face of the plate"
    );
    assert!(flush(&base, &plate, REACH).is_empty());
}

#[test]
fn a_gap_and_an_overlap_are_measured_and_the_reach_is_kept() {
    for (z, want) in [(1.0, 1.0), (-0.3, -0.3)] {
        let (base, plate) = t_joint(z);
        let found = on_faces(&plate, &base, REACH);
        assert_eq!(found.len(), 4, "foot at {z}: {found:?}");
        found
            .iter()
            .flat_map(|j| j.gap)
            .for_each(|g| close(g, want));
    }
    let (base, plate) = t_joint(1.0);
    assert!(on_faces(&plate, &base, 0.5).is_empty());
    let (base, plate) = t_joint(10.0);
    assert!(on_faces(&plate, &base, REACH).is_empty());
}

#[test]
fn a_lap_has_an_inside_corner_each_way_and_two_flush_sides() {
    let lower = make_box([0.0, 0.0, 0.0], [60.0, 40.0, 3.0]).unwrap();
    let upper = make_box([40.0, 0.0, 3.0], [60.0, 40.0, 3.0]).unwrap();
    let down = on_faces(&upper, &lower, REACH);
    let up = on_faces(&lower, &upper, REACH);
    assert_eq!((down.len(), up.len()), (1, 1), "{down:?} {up:?}");
    close(length(down[0].run), 40.0);
    close(length(up[0].run), 40.0);
    let sides = flush(&lower, &upper, REACH);
    assert_eq!(sides.len(), 2, "{sides:?}");
    for j in &sides {
        close(length(j.run), 20.0);
    }
    let swapped = flush(&upper, &lower, REACH);
    assert_eq!(swapped.len(), 2);
    for j in &sides {
        assert!(swapped
            .iter()
            .any(|k| k.edges == [j.edges[1], j.edges[0]] && k.flush == [j.flush[1], j.flush[0]]));
    }
}

#[test]
fn a_butt_joint_is_flush_on_both_faces_across_its_gap() {
    let left = make_box([0.0, 0.0, 0.0], [50.0, 40.0, 5.0]).unwrap();
    let right = make_box([52.0, 0.0, 0.0], [48.0, 40.0, 5.0]).unwrap();
    let found = flush(&left, &right, REACH);
    for (got, want) in sorted_lengths(found.iter().map(|j| j.run))
        .into_iter()
        .zip([5.0, 5.0, 40.0, 40.0])
    {
        close(got, want);
    }
    assert_eq!(found.len(), 4, "{found:?}");
    found.iter().flat_map(|j| j.gap).for_each(|g| close(g, 2.0));
    assert!(flush(&left, &right, 1.5).is_empty());
    assert!(on_faces(&left, &right, REACH).is_empty() && on_faces(&right, &left, REACH).is_empty());
}

#[test]
fn a_reach_or_tolerance_that_is_not_usable_is_refused() {
    let (base, plate) = t_joint(0.0);
    assert!(matches!(
        plate.side_edges(&order(&plate), 0.0),
        Err(Error::Argument)
    ));
    let sides = plate.side_edges(&order(&plate), TOL).unwrap();
    assert!(matches!(
        base.edges_on_faces(&order(&base), &sides, f64::NAN, TOL),
        Err(Error::Argument)
    ));
    assert!(flush_edges(&sides, &sides, f64::NAN, TOL).is_empty());
}
