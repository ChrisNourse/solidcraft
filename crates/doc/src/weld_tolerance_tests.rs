//! Weld tolerances: each value against its formula at two sizes, every pair of joint and
//! deviation either not applying or a finite band with its source, the refusal points (Q227), the
//! user's own value, and the document's family.

use super::*;

const JOINTS: [Joint; 9] =
    [Joint::Fillet, Joint::Butt, Joint::Edge, Joint::Plug, Joint::Resistance, Joint::Fusion, Joint::Stake, Joint::Stud, Joint::Surfacing];

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-12
}

fn fillet(family: Family, level: Level, t: f64, z: f64) -> Band {
    tolerance(family, Joint::Fillet, Deviation::Gap, Sizes { t, reach: Some(z), ..Sizes::default() }, level).unwrap()
}

#[test]
fn a_fillet_s_gap_follows_iso_5817_row_3_2_by_level_and_aws_d1_1() {
    let a = 6.0 / std::f64::consts::SQRT_2;
    // t > 3: B 0.5 + 0.1a max 2, C 0.5 + 0.2a max 3, D 1 + 0.3a max 4; outside past D
    for (level, want) in [(Level::B, 0.5 + 0.1 * a), (Level::C, 0.5 + 0.2 * a), (Level::D, 1.0 + 0.3 * a)] {
        let b = fillet(Family::Iso, level, 10.0, 6.0);
        assert!(close(b.silent, want), "{level:?}: {}", b.silent);
        assert!(close(b.outside.unwrap(), 1.0 + 0.3 * a));
        assert_eq!(b.refused_past, Some(6.0), "refused only past the leg (Q227)");
        assert!(!b.proposed);
    }
    // the caps: a 30 mm leg's throat 21.2 gives 2.62 / 4.74 / 7.36 before them
    assert!(close(fillet(Family::Iso, Level::B, 10.0, 30.0).silent, 2.0));
    assert!(close(fillet(Family::Iso, Level::C, 10.0, 30.0).silent, 3.0));
    assert!(close(fillet(Family::Iso, Level::D, 10.0, 30.0).outside.unwrap(), 4.0));
    // t <= 3: B 0.2 + 0.1a, C 0.3 + 0.1a, D 0.5 + 0.1a, no cap
    let a = 2.0;
    let thin = |level| tolerance(Family::Iso, Joint::Fillet, Deviation::Gap, Sizes { t: 2.0, a: Some(a), ..Sizes::default() }, level).unwrap();
    assert!(close(thin(Level::B).silent, 0.4));
    assert!(close(thin(Level::C).silent, 0.5));
    assert!(close(thin(Level::D).silent, 0.7));
    assert_eq!(thin(Level::C).refused_past, None, "no reach given");
    // AWS: 2 mm, outside past 5, whatever the size
    let b = fillet(Family::Aws, Level::C, 10.0, 4.0);
    assert_eq!((b.silent, b.outside, b.refused_past), (2.0, Some(5.0), Some(4.0)));
    // no size to read: nothing to say under ISO; AWS's 2 and 5 mm need none, and no refusal point
    let none = Sizes { t: 5.0, ..Sizes::default() };
    assert_eq!(tolerance(Family::Iso, Joint::Fillet, Deviation::Gap, none, Level::C), None);
    let b = tolerance(Family::Aws, Joint::Fillet, Deviation::Gap, none, Level::C).unwrap();
    assert_eq!((b.silent, b.outside, b.refused_past), (2.0, Some(5.0), None));
}

#[test]
fn a_butt_s_high_low_follows_iso_5817_row_3_1_and_aws_d1_1() {
    let at = |family, t, tube, level| tolerance(family, Joint::Butt, Deviation::Misalignment, Sizes { t, tube, ..Sizes::default() }, level).unwrap();
    assert!(close(at(Family::Iso, 2.0, false, Level::C).silent, 0.2 + 0.15 * 2.0));
    assert!(close(at(Family::Iso, 10.0, false, Level::C).silent, 1.5));
    assert!(close(at(Family::Iso, 40.0, false, Level::C).silent, 4.0));
    assert!(close(at(Family::Iso, 10.0, false, Level::C).outside.unwrap(), 2.5));
    assert!(close(at(Family::Iso, 10.0, true, Level::C).silent, 3.0));
    assert!(close(at(Family::Iso, 4.0, true, Level::B).silent, 2.0));
    assert!(close(at(Family::Aws, 10.0, false, Level::C).silent, 1.0));
    assert!(close(at(Family::Aws, 40.0, false, Level::C).silent, 3.0));
    assert!(close(at(Family::Aws, 20.0, true, Level::C).silent, 4.0));
    assert!(!at(Family::Iso, 10.0, false, Level::C).proposed);
    // an edge stack takes it by analogy
    let edge = tolerance(Family::Iso, Joint::Edge, Deviation::Misalignment, Sizes { t: 10.0, ..Sizes::default() }, Level::C).unwrap();
    assert!(close(edge.silent, 1.5) && edge.proposed);
}

#[test]
fn angles_follow_iso_13920_class_b_and_beam_welds_iso_13919_1() {
    let tilt = |joint, l, level| tolerance(Family::Iso, joint, Deviation::Tilt, Sizes { t: 5.0, l, ..Sizes::default() }, level).unwrap();
    assert!(close(tilt(Joint::Butt, None, Level::C).silent, 0.75));
    assert!(close(tilt(Joint::Resistance, Some(700.0), Level::C).silent, 0.5));
    assert!(close(tilt(Joint::Stud, Some(1500.0), Level::C).silent, 20.0 / 60.0));
    for (level, want) in [(Level::B, 1.0), (Level::C, 2.0), (Level::D, 4.0)] {
        let b = tilt(Joint::Fusion, None, level);
        assert!(close(b.silent, want) && b.outside == Some(4.0) && !b.proposed);
        assert_eq!(tilt(Joint::Stake, None, level), b);
    }
    assert_eq!(tolerance(Family::Iso, Joint::Fillet, Deviation::Tilt, Sizes::default(), Level::C), None);
}

#[test]
fn the_other_rows_read_their_sizes_and_name_their_refusal_point() {
    let s = |t, reach| Sizes { t, reach, ..Sizes::default() };
    let gap = |joint, sizes| tolerance(Family::Iso, joint, Deviation::Gap, sizes, Level::C).unwrap();
    // a spot or seam: refused past the thinner sheet, a projection past its height
    assert_eq!(gap(Joint::Resistance, s(1.0, None)).refused_past, Some(1.0));
    assert_eq!(gap(Joint::Resistance, s(1.0, Some(0.6))).refused_past, Some(0.6));
    // fusion and stake: 10 % of the top sheet, refused past the zone's depth
    let b = gap(Joint::Fusion, s(2.0, Some(1.5)));
    assert!(close(b.silent, 0.2) && b.refused_past == Some(1.5) && b.proposed);
    // plug: a notice at any gap (Q122, Q222), outside past AWS's 2 mm, never refused
    let b = gap(Joint::Plug, s(5.0, None));
    assert_eq!((b.silent, b.outside, b.refused_past), (0.0, Some(2.0), None));
    // butt: filled as modelled, outside past twice the thinner part or 20 mm
    assert!(close(gap(Joint::Butt, s(6.0, None)).outside.unwrap(), 12.0));
    assert!(close(gap(Joint::Butt, s(15.0, None)).outside.unwrap(), 20.0));
    assert!(gap(Joint::Butt, s(6.0, None)).proposed, "AWS's correction limit is not a fit-up allowance");
    // a groove's drift: AWS Figure 5.3's 1/16 in
    let d = tolerance(Family::Aws, Joint::Butt, Deviation::Drift, s(6.0, None), Level::C).unwrap();
    assert!(close(d.silent, 1.5875));
    // overlaps and the stake's offset: the accuracy of a built bead, proposed
    for joint in [Joint::Fillet, Joint::Butt, Joint::Plug, Joint::Resistance, Joint::Stud] {
        let b = tolerance(Family::Aws, joint, Deviation::Overlap, s(3.0, None), Level::C).unwrap();
        assert!(close(b.silent, ACCURACY) && b.proposed, "{joint:?}");
    }
    let b = tolerance(Family::Iso, Joint::Stake, Deviation::Misalignment, s(1.0, None), Level::C).unwrap();
    assert!(close(b.silent, ACCURACY));
}

#[test]
fn every_band_is_finite_with_a_source() {
    let sizes = Sizes { t: 4.0, a: Some(3.0), reach: Some(4.0), l: Some(300.0), tube: false };
    for family in [Family::Iso, Family::Aws] {
        for joint in JOINTS {
            for deviation in Deviation::ALL {
                for level in [Level::B, Level::C, Level::D] {
                    let Some(b) = tolerance(family, joint, deviation, sizes, level) else { continue };
                    let finite = |v: f64| v.is_finite() && v >= 0.0;
                    assert!(finite(b.silent) && b.outside.is_none_or(finite) && b.refused_past.is_none_or(finite), "{joint:?} {deviation:?}");
                    assert!(b.outside.is_none_or(|o| o >= b.silent), "{joint:?} {deviation:?}: outside below silent");
                    assert!(!b.source.is_empty(), "{joint:?} {deviation:?}");
                }
            }
        }
    }
    // the surfacing's one part has no gap, and the plug's walls no drift
    assert_eq!(tolerance(Family::Iso, Joint::Surfacing, Deviation::Gap, sizes, Level::C), None);
    assert_eq!(tolerance(Family::Iso, Joint::Plug, Deviation::Drift, sizes, Level::C), None);
}

#[test]
fn the_user_s_value_replaces_the_silent_band_only() {
    let b = fillet(Family::Iso, Level::C, 10.0, 6.0);
    assert_eq!(b.with_override(None), b);
    let mine = b.with_override(Some(0.4));
    assert_eq!((mine.silent, mine.outside, mine.refused_past), (0.4, b.outside, b.refused_past));
    assert!(mine.user && !b.user);
    assert_eq!((mine.source, mine.proposed), (b.source, b.proposed), "the outside notice still names the standard");
}

#[test]
fn keys_and_families() {
    let keys: Vec<_> = Deviation::ALL.iter().map(|d| d.key()).collect();
    assert_eq!(keys, ["gap", "overlap", "tilt", "drift", "toe", "misalignment"]);
    assert_eq!(Family::of("aws_a2_4_2020"), Family::Aws);
    assert_eq!(Family::of("iso2553_2019_a"), Family::Iso);
    assert_eq!(Family::of("iso2553_2019_b"), Family::Iso);
    assert_eq!(Level::default(), Level::C);
}
