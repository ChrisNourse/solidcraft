//! Weld tolerances (Q135, Q190): per kind of joint and kind of deviation, how far a model may be
//! off before a weld says so, past which value the notice adds "outside <standard>", and past which
//! no sensible weld is left. Geometry is always built on the real faces; these values decide only
//! the notice, the size compensation and the refusal.
//!
//! Each value comes from a standard, named in [`Band::source`], or is proposed where none gives one
//! ([`Band::proposed`]). ISO 2553 documents take ISO 5817:2023 (Table 1 rows 3.1, 3.2), ISO 9692-1
//! and ISO 13920; AWS A2.4 documents take AWS D1.1 (2000 edition clause 5.22, 2020 edition 7.21).
//! Where only one family gives a value, both use it. The user can replace a silent band
//! ([`Band::with_override`]).

/// Lengths are millimetres, tilts degrees.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    Iso,
    Aws,
}

impl Family {
    /// The family of a document's symbol standard (`WeldSettings::symbol_standard`).
    pub fn of(symbol_standard: &str) -> Self {
        if symbol_standard.starts_with("aws") { Self::Aws } else { Self::Iso }
    }
}

/// ISO 5817 / ISO 13919-1 quality level; C when the weld names none.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Level {
    B,
    #[default]
    C,
    D,
}

/// The kinds of joint the values differ by. A tack takes its joint's own kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Joint {
    Fillet,
    Butt,
    Edge,
    Plug,
    /// Resistance spot, seam and projection welds.
    Resistance,
    /// Laser or electron-beam spot or seam on lapped sheets.
    Fusion,
    /// Laser through a skin into a web.
    Stake,
    Stud,
    Surfacing,
}

/// What is off.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Deviation {
    /// The parts apart where they should touch.
    Gap,
    /// One part reaching into the other.
    Overlap,
    /// Faces that should be square or parallel, in degrees.
    Tilt,
    /// The joint's section changing along the weld.
    Drift,
    /// A leg, cap, footprint or collar past its face's edge.
    Toe,
    /// Sideways offset: a butt's high-low, a stake line off the web's middle.
    Misalignment,
}

impl Deviation {
    pub const ALL: [Self; 6] = [Self::Gap, Self::Overlap, Self::Tilt, Self::Drift, Self::Toe, Self::Misalignment];

    /// The key a user's value is saved under (`WeldSettings::tolerances`).
    pub fn key(self) -> &'static str {
        match self {
            Self::Gap => "gap",
            Self::Overlap => "overlap",
            Self::Tilt => "tilt",
            Self::Drift => "drift",
            Self::Toe => "toe",
            Self::Misalignment => "misalignment",
        }
    }
}

/// The sizes a value is read from, in mm.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Sizes {
    /// The thinner part's thickness (fusion: the top sheet; stake: the skin).
    pub t: f64,
    /// A fillet's throat a; from its leg when not given.
    pub a: Option<f64>,
    /// The weld's own size across a gap, past which it is refused (Q227): a fillet's leg, an edge
    /// cap's height, a fusion zone's depth, a stud collar's height, a projection's height.
    pub reach: Option<f64>,
    /// The shorter leg of an angle (ISO 13920 Table 2); 400 mm or less when not given.
    pub l: Option<f64>,
    /// A circumferential joint of tubes (ISO 5817 5072, AWS girth welds).
    pub tube: bool,
}

/// How far a deviation may go.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Band {
    /// Up to this the weld counts as perfect: no notice. Callers raise it to the zone tolerance.
    pub silent: f64,
    /// Past this the notice adds "outside" the standard named in `source`.
    pub outside: Option<f64>,
    /// Past this no sensible weld is left: refused, naming the fix.
    pub refused_past: Option<f64>,
    /// Where the band's values come from; kept when the user sets `silent`, so the "outside"
    /// notice still names it.
    pub source: &'static str,
    pub proposed: bool,
    /// `silent` is the user's own value, not `source`'s.
    pub user: bool,
}

impl Band {
    /// The band with the user's own silent value, when given.
    pub fn with_override(self, silent: Option<f64>) -> Self {
        match silent {
            Some(silent) => Self { silent, user: true, ..self },
            None => self,
        }
    }
}

/// The accuracy of a built bead (Q190), proposed where no standard gives a value.
pub const ACCURACY: f64 = 0.1;
/// A fillet joint's angles in ISO 9692-1 Table 3: a T welded from one side, a corner joint.
pub const FILLET_T_DEGREES: [f64; 2] = [70.0, 100.0];
pub const FILLET_CORNER_DEGREES: [f64; 2] = [60.0, 120.0];

const ISO_5817_GAP: &str = "ISO 5817:2023 Table 1 row 3.2 (617)";
const AWS_GAP: &str = "AWS D1.1 7.21.1 (2000: 5.22.1)";
const ISO_13920: &str = "ISO 13920:2023 Table 2, class B";
const ISO_13919: &str = "ISO 13919-1:2019 Table 1 row 3.2 (508)";

fn band(silent: f64, outside: Option<f64>, source: &'static str, proposed: bool) -> Band {
    Band { silent, outside, refused_past: None, source, proposed, user: false }
}

fn accuracy() -> Band {
    band(ACCURACY, None, "proposed: the accuracy of a built bead", true)
}

/// ISO 13920 Table 2, class B, by the shorter leg: ±45′ to 400 mm, ±30′ to 1000, ±20′ above.
fn angle(l: Option<f64>) -> Band {
    let minutes = match l.unwrap_or(0.0) {
        l if l <= 400.0 => 45.0,
        l if l <= 1000.0 => 30.0,
        _ => 20.0,
    };
    band(minutes / 60.0, None, ISO_13920, true)
}

/// ISO 13919-1 Table 1 row 3.2: angular misalignment of beam welds.
fn beam_angle(level: Level) -> Band {
    band(
        match level {
            Level::B => 1.0,
            Level::C => 2.0,
            Level::D => 4.0,
        },
        Some(4.0),
        ISO_13919,
        false,
    )
}

/// ISO 5817 row 3.2 (incorrect root gap of a fillet) at `level`.
fn fillet_gap_iso(level: Level, t: f64, a: f64) -> f64 {
    match (level, t <= 3.0) {
        (Level::B, true) => 0.2 + 0.1 * a,
        (Level::C, true) => 0.3 + 0.1 * a,
        (Level::D, true) => 0.5 + 0.1 * a,
        (Level::B, false) => (0.5 + 0.1 * a).min(2.0),
        (Level::C, false) => (0.5 + 0.2 * a).min(3.0),
        (Level::D, false) => (1.0 + 0.3 * a).min(4.0),
    }
}

/// ISO 5817 row 3.1 (linear misalignment) at `level`: plates (5071) or tubes (5072).
fn high_low_iso(level: Level, t: f64, tube: bool) -> f64 {
    let (k, max, tube_max) = match level {
        Level::B => (0.1, 3.0, 2.0),
        Level::C => (0.15, 4.0, 3.0),
        Level::D => (0.25, 5.0, 4.0),
    };
    if tube {
        (0.5 * t).min(tube_max)
    } else if t <= 3.0 {
        0.2 + k * t
    } else {
        (k * t).min(max)
    }
}

/// The band for `deviation` at a `joint` of these `sizes`, or `None` where it does not apply (a
/// fillet's tilt: the bead follows the faces at any angle, [`FILLET_T_DEGREES`] names the joint's
/// range).
pub fn tolerance(family: Family, joint: Joint, deviation: Deviation, sizes: Sizes, level: Level) -> Option<Band> {
    use Deviation as D;
    use Joint as J;
    let Sizes { t, reach, l, tube, .. } = sizes;
    Some(match (joint, deviation) {
        (J::Fillet, D::Gap) => {
            let b = match family {
                Family::Iso => {
                    let a = sizes.a.or(reach.map(|z| z / std::f64::consts::SQRT_2))?;
                    band(fillet_gap_iso(level, t, a), Some(fillet_gap_iso(Level::D, t, a)), ISO_5817_GAP, false)
                }
                // shortcut: AWS's 8 mm for parts 75 mm or thicker on backing is not read, add it when backing is modelled
                Family::Aws => band(2.0, Some(5.0), AWS_GAP, false),
            };
            Band { refused_past: reach, ..b }
        }
        (J::Fillet, D::Tilt | D::Drift | D::Misalignment) => return None,
        // shortcut: "outside" on the throat a shortened leg loses (ISO 5817 5213, AWS Table 8.1) comes with the leg shortening
        (J::Fillet | J::Butt | J::Edge, D::Toe) => accuracy(),
        (J::Butt, D::Gap) => {
            // the opening is part of the groove and filled as modelled: no notice up to the most
            // that AWS lets be corrected by welding before joining, outside past it
            let past = (2.0 * t).min(20.0);
            band(
                past,
                Some(past),
                "proposed: filled as modelled; outside past AWS D1.1 5.22.4.3's correction limit (2000), also for ISO documents",
                true,
            )
        }
        (J::Butt | J::Edge | J::Plug, D::Tilt) => angle(l),
        (J::Butt, D::Drift) => band(25.4 / 16.0, None, "AWS D1.1 Figure 5.3, root opening as fit-up, also for ISO documents", false),
        (J::Butt | J::Edge, D::Misalignment) => {
            // an edge stack's proud end takes a butt's high-low by analogy
            let edge = joint == J::Edge;
            match family {
                Family::Iso => band(high_low_iso(level, t, tube), Some(high_low_iso(Level::D, t, tube)), "ISO 5817:2023 Table 1 row 3.1", edge),
                Family::Aws => {
                    let v = if tube { (0.2 * t).min(6.0) } else { (0.1 * t).min(3.0) };
                    band(v, Some(v), "AWS D1.1 7.21.3 (2000: 5.22.3)", edge)
                }
            }
        }
        (J::Edge, D::Gap) => Band { refused_past: reach, ..band(0.1 * t, None, "proposed: TWI's 10 % of t for thin-sheet fusion joints", true) },
        (J::Plug, D::Gap) => band(0.0, Some(2.0), "Q122: a notice at any gap; AWS D1.1 7.21.1.1 (2000: 5.22.1.1): 2 mm", false),
        (J::Plug, D::Drift | D::Toe | D::Misalignment) => return None,
        (J::Resistance, D::Gap) => Band {
            refused_past: Some(reach.unwrap_or(t)),
            ..band(ACCURACY, None, "proposed: no standard gives a number (ISO 14373 asks for contact)", true)
        },
        (J::Resistance | J::Stud, D::Tilt) => angle(l),
        (J::Fusion | J::Stake, D::Gap) => {
            Band { refused_past: reach, ..band(0.1 * t, None, "proposed: TWI's 10 % of the top sheet for laser lap welds", true) }
        }
        (J::Fusion | J::Stake, D::Tilt) => beam_angle(level),
        (J::Stake, D::Toe) => return None,
        (J::Stud, D::Gap) => Band { refused_past: reach, ..accuracy() },
        (J::Resistance | J::Fusion, D::Drift | D::Misalignment) | (J::Stud, D::Drift | D::Misalignment) => return None,
        (J::Surfacing, D::Gap | D::Overlap | D::Tilt | D::Misalignment) => return None,
        // overlaps, the stake's drift and offset, the edge's drift, toes of spots, fusion zones,
        // studs and surfacing, the surfacing's drift: the accuracy of a built bead
        _ => accuracy(),
    })
}

#[cfg(test)]
#[path = "weld_tolerance_tests.rs"]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests;
