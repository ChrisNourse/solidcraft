//! Standard parts: ISO metric and ASME/ANSI inch fasteners, dowel pins and ball bearings,
//! generated in code from the dimensions in the public standards tables (no vendor geometry).
//! Inch parts are tabled in inches and built in millimetres like the rest. Each part is inserted as
//! its own component, built by ordinary features (so it stays parametric and editable), with a
//! cosmetic thread where it has one; at a hole it gets a rigid joint that puts its axis on the
//! hole's and seats it on the face. Inserted at several holes it is still one component, placed
//! once per hole, each placement with its own joint.
//!
//! Part frame: the seating plane is z = 0 (the underside of a head, the bottom of a nut or
//! washer, the top of a set screw or pin); the shank runs down −z. A countersunk screw is the
//! exception: its head sinks into the part, so z = 0 is the top of the head, level with the face.
//! Seated at a hole, its cone lies in the hole's 90 degree countersink: pick the rim where the
//! countersink meets the bore.

use serde_json::{Value, json};
use solidcore_record::Vec3;
use solidcraft_geom::prelude::*;

use super::CommandSpec;
use crate::params::{bad, num, str_, vec3};
use crate::schema::parts::{InsertedPart, PartFamily, PartsInsertParams, PartsLibrary, PartsLibraryParams, PartsResizeParams, ResizedPart};
use crate::schema::{TypedContract, output_schema_for};
use crate::{EngineError, Result, Session};

const LIBRARY_CONTRACT: TypedContract<PartsLibraryParams> = TypedContract::new();
const INSERT_CONTRACT: TypedContract<PartsInsertParams> = TypedContract::new();
const RESIZE_CONTRACT: TypedContract<PartsResizeParams> = TypedContract::new();

pub static COMMANDS: &[CommandSpec] = &[
    CommandSpec::new("parts.library", "Standard Parts", crate::checked!("parts.library", library))
        .icon("library")
        .noundo()
        .contract(&LIBRARY_CONTRACT)
        .output(output_schema_for::<PartsLibrary>)
        .params("family? → families (name, standard, system: metric|inch, sizes, lengths) and, with family, each size's dimensions (mm)"),
    CommandSpec::new("parts.fastener", "Insert Fastener", crate::checked!("parts.fastener", insert_atomic))
        .at("SOLID", "INSERT")
        .icon("fastener")
        .contract(&INSERT_CONTRACT)
        .output(output_schema_for::<InsertedPart>)
        .params("the Insert Part dialog's command (same as parts.insert): family, size, length?, at?|point?+direction? (from the shank's tip toward the head), flip?, name?"),
    CommandSpec::new("parts.insert", "Insert Part", crate::checked!("parts.insert", insert_atomic))
        .icon("fastener")
        .contract(&INSERT_CONTRACT)
        .output(output_schema_for::<InsertedPart>)
        .params(
            "family (socket_head_cap_screw | button_head_cap_screw | countersunk_socket_screw | hex_bolt | hex_nut | washer | set_screw | dowel_pin | bearing | \
         low_head_cap_screw | pan_head_screw | cheese_head_screw | countersunk_screw | hex_cap_screw_inch | hex_nut_inch | washer_sae | washer_uss); size (\"M6\", \"608\" for bearings, \"1/2-13 UNC\" for inch screws and nuts, \
         \"1/2\" for inch washers); length? (mm, inch parts too; screws, bolts, set screws, pins; default a standard length near 2.5 d); \
         at?: [x,y,z] on a hole's rim or wall (the part seats on the hole's face with a rigid joint, head on the picked face and \
         shank into the hole, whichever way the face points) or on a flat face (seated at that point, head out of the face), or a list of \
         them (one component, placed and seated once per pick) | point?: [x,y,z] with direction?: [x,y,z] (no joint: the part's seating plane, under a head or \
         the bottom of a nut, washer or bearing, the top of a set screw, pin or countersunk head, goes to point, and direction is its +z, from the shank's tip toward the head; default [0,0,1], \
         head up, shank down -z); flip?: bool (default false: turned over, head into the material with at, +z against direction \
         with point); name?; for parts with a metric thread: thread?: cosmetic (default: recorded on the face, the body unchanged) | modeled (cut into the body, \
         its standard thread length from the tip, the start chamfered; slower, and refused past 80 turns) | none; pitch?: a fine pitch in mm, as parts.library lists \
         them for the size (default coarse); left_hand?: bool; print_fit?: bool (made to be 3D printed: the thread under size and the drive recess wider by \
         clearance?: mm per side, default 0.15; a nut's thread over size); drive?: hex | hexalobular | cross | slot, where the family comes with more than one \
         (parts.library: drives); tip?: flat | cone | cup (a set screw's point); joint?: with at, the joint each hole gets: rigid (default) | revolute (turns in place) | \
         cylindrical (turns and slides along the hole) | screw (cylindrical, its turn driving its travel by the pitch: drive it with joint.drive or a motion study to \
         show it screwing in) | planar (seated on the face and free to slide over it, as in a slot) → component, occurrence, part_number, thread, pitch, extent \
         ([lowest z, top z] of the part in its own frame, the seat at 0), joint?, directions (each placement's head direction in the world, the one \
         chosen), and with at: occurrences, joints (one per pick, in pick order)",
        ),
    CommandSpec::new("parts.resize", "Change Part Size", crate::checked!("parts.resize", resize_atomic))
        .icon("scale")
        .contract(&RESIZE_CONTRACT)
        .output(output_schema_for::<ResizedPart>)
        .params(
            "component: id|name of an inserted part; size?; length?; thread?, pitch? (0: coarse), left_hand?, print_fit?, clearance?, drive?, tip? as \
             parts.insert takes them (each left out keeps what the part has) — rebuilds it (joints keep it seated)",
        ),
];

/// A family: (id, name, standard).
const FAMILIES: &[(&str, &str, &str)] = &[
    ("socket_head_cap_screw", "Socket head cap screw", "ISO 4762"),
    ("button_head_cap_screw", "Button head cap screw", "ISO 7380-1"),
    ("countersunk_socket_screw", "Countersunk socket screw", "ISO 10642"),
    ("low_head_cap_screw", "Low head socket cap screw", "DIN 7984"),
    ("pan_head_screw", "Pan head screw", "ISO 7045 / ISO 14583 / ISO 1580"),
    ("cheese_head_screw", "Slotted cheese head screw", "ISO 1207"),
    ("countersunk_screw", "Countersunk flat head screw", "ISO 7046 / ISO 14581 / ISO 2009"),
    ("hex_bolt", "Hex head bolt / screw", "ISO 4014 / ISO 4017"),
    ("hex_nut", "Hex nut", "ISO 4032"),
    ("washer", "Plain washer", "ISO 7089"),
    ("set_screw", "Hex socket set screw, flat point", "ISO 4026 / ISO 4027 / ISO 4029"),
    ("dowel_pin", "Parallel pin (dowel)", "ISO 8734"),
    ("bearing", "Deep groove ball bearing (envelope)", "ISO 15 (6000/6200/600 series)"),
    ("hex_cap_screw_inch", "Hex cap screw (inch)", "ASME B18.2.1"),
    ("hex_nut_inch", "Hex nut (inch)", "ASME B18.2.2"),
    ("washer_sae", "Plain washer, SAE (narrow)", "ANSI B18.22.1 Type A"),
    ("washer_uss", "Plain washer, USS (wide)", "ANSI B18.22.1 Type A"),
];

/// ISO metric coarse: (size, d, pitch).
const METRIC: [(&str, f64, f64); 14] = [
    ("M2", 2.0, 0.4),
    ("M2.5", 2.5, 0.45),
    ("M3", 3.0, 0.5),
    ("M4", 4.0, 0.7),
    ("M5", 5.0, 0.8),
    ("M6", 6.0, 1.0),
    ("M8", 8.0, 1.25),
    ("M10", 10.0, 1.5),
    ("M12", 12.0, 1.75),
    ("M14", 14.0, 2.0),
    ("M16", 16.0, 2.0),
    ("M20", 20.0, 2.5),
    ("M24", 24.0, 3.0),
    ("M30", 30.0, 3.5),
];

/// ISO 4762: (size, head diameter dk, head height k, socket s, socket depth t).
const SHCS: [(&str, f64, f64, f64, f64); 14] = [
    ("M2", 3.8, 2.0, 1.5, 1.0),
    ("M2.5", 4.5, 2.5, 2.0, 1.1),
    ("M3", 5.5, 3.0, 2.5, 1.3),
    ("M4", 7.0, 4.0, 3.0, 2.0),
    ("M5", 8.5, 5.0, 4.0, 2.5),
    ("M6", 10.0, 6.0, 5.0, 3.0),
    ("M8", 13.0, 8.0, 6.0, 4.0),
    ("M10", 16.0, 10.0, 8.0, 5.0),
    ("M12", 18.0, 12.0, 10.0, 6.0),
    ("M14", 21.0, 14.0, 12.0, 7.0),
    ("M16", 24.0, 16.0, 14.0, 8.0),
    ("M20", 30.0, 20.0, 17.0, 10.0),
    ("M24", 36.0, 24.0, 19.0, 12.0),
    ("M30", 45.0, 30.0, 22.0, 15.5),
];

/// ISO 7380-1: (size, head diameter dk, head height k, socket s, socket depth t, the last being
/// the key engagement depth, which is 0.59 to 0.63 k throughout). The standard runs M3 to M16:
/// the M2.5 and M14 some suppliers list are not in it.
const BHCS: [(&str, f64, f64, f64, f64); 8] = [
    ("M3", 5.7, 1.65, 2.0, 1.04),
    ("M4", 7.6, 2.2, 2.5, 1.3),
    ("M5", 9.5, 2.75, 3.0, 1.56),
    ("M6", 10.5, 3.3, 4.0, 2.08),
    ("M8", 14.0, 4.4, 5.0, 2.6),
    ("M10", 17.5, 5.5, 6.0, 3.12),
    ("M12", 21.0, 6.6, 8.0, 4.16),
    ("M16", 28.0, 8.8, 10.0, 5.2),
];

/// A button head's cylindrical flank, as a fraction of the head height k, and the flat top it
/// keeps around its socket, as a fraction of the socket's across-flats s. The flat clears the
/// socket's corners (0.577 s) at every size.
///
/// ISO 7380-1 draws neither, and a real button head has no flat at all: its crown curves on and
/// the socket's corners break through it. The flat is ours, so that the socket is a plain cut on
/// a plane. It costs us the standard's crown radius r1, which the head's own dk and k then fix:
/// the result is inside r1's tolerance at six of the eight sizes, 0.08 under it at M3 and 0.12
/// over it at M12. No one pair of fractions is inside it at all eight, and taking r1 as given
/// instead leaves no flat clear of the socket below M10, so dk, k, s and t are what we keep exact.
const BUTTON_FLANK: f64 = 0.15;
const BUTTON_FLAT: f64 = 0.7;

/// ISO 10642: (size, theoretical head diameter dk, head height k, socket s, key engagement t).
/// dk is the diameter of the 90° cone where it meets the top plane, so k = (dk − d) / 2 at every
/// size; the standard's real rim is rounded and a little smaller (dk max 5.81 at M3), which the
/// sharp cone does not draw. t is the standard's minimum engagement depth: the 2013 edition has it
/// larger at M4, M5, M8 and M10 (1.5, 1.9, 3 and 3.6), and these are the newer edition's. M14 is
/// the standard's non-preferred size. The standard runs M2 to M20; M2 and M2.5 are left out here
/// because only one of the tables we compared lists them.
const CSK: [(&str, f64, f64, f64, f64); 10] = [
    ("M3", 6.72, 1.86, 2.0, 1.1),
    ("M4", 8.96, 2.48, 2.5, 1.4),
    ("M5", 11.2, 3.1, 3.0, 1.75),
    ("M6", 13.44, 3.72, 4.0, 2.2),
    ("M8", 17.92, 4.96, 5.0, 2.9),
    ("M10", 22.4, 6.2, 6.0, 3.5),
    ("M12", 26.88, 7.44, 8.0, 4.3),
    ("M14", 30.8, 8.4, 10.0, 4.5),
    ("M16", 33.6, 8.8, 10.0, 4.8),
    ("M20", 40.32, 10.16, 12.0, 5.6),
];

/// ISO 4014/4017 hex head: (size, across flats s, head height k).
const HEX_BOLT: [(&str, f64, f64); 14] = [
    ("M2", 4.0, 1.4),
    ("M2.5", 5.0, 1.7),
    ("M3", 5.5, 2.0),
    ("M4", 7.0, 2.8),
    ("M5", 8.0, 3.5),
    ("M6", 10.0, 4.0),
    ("M8", 13.0, 5.3),
    ("M10", 16.0, 6.4),
    ("M12", 18.0, 7.5),
    ("M14", 21.0, 8.8),
    ("M16", 24.0, 10.0),
    ("M20", 30.0, 12.5),
    ("M24", 36.0, 15.0),
    ("M30", 46.0, 18.7),
];

/// ISO 4032: (size, across flats s, height m).
const HEX_NUT: [(&str, f64, f64); 14] = [
    ("M2", 4.0, 1.6),
    ("M2.5", 5.0, 2.0),
    ("M3", 5.5, 2.4),
    ("M4", 7.0, 3.2),
    ("M5", 8.0, 4.7),
    ("M6", 10.0, 5.2),
    ("M8", 13.0, 6.8),
    ("M10", 16.0, 8.4),
    ("M12", 18.0, 10.8),
    ("M14", 21.0, 12.8),
    ("M16", 24.0, 14.8),
    ("M20", 30.0, 18.0),
    ("M24", 36.0, 21.5),
    ("M30", 46.0, 25.6),
];

/// ISO 7089: (size, inner d1, outer d2, thickness h).
const WASHER: [(&str, f64, f64, f64); 14] = [
    ("M2", 2.2, 5.0, 0.3),
    ("M2.5", 2.7, 6.0, 0.5),
    ("M3", 3.2, 7.0, 0.5),
    ("M4", 4.3, 9.0, 0.8),
    ("M5", 5.3, 10.0, 1.0),
    ("M6", 6.4, 12.0, 1.6),
    ("M8", 8.4, 16.0, 1.6),
    ("M10", 10.5, 20.0, 2.0),
    ("M12", 13.0, 24.0, 2.5),
    ("M14", 15.0, 28.0, 2.5),
    ("M16", 17.0, 30.0, 3.0),
    ("M20", 21.0, 37.0, 3.0),
    ("M24", 25.0, 44.0, 4.0),
    ("M30", 31.0, 56.0, 4.0),
];

/// ISO 4026: (size, socket s, socket depth t).
const SET_SCREW: [(&str, f64, f64); 7] =
    [("M3", 1.5, 1.2), ("M4", 2.0, 1.5), ("M5", 2.5, 2.0), ("M6", 3.0, 2.0), ("M8", 4.0, 3.0), ("M10", 5.0, 4.0), ("M12", 6.0, 4.8)];

/// Set screw points: (size, flat point dp of ISO 4026, cup dz of ISO 4029, cone's flat dt of
/// ISO 4027, which is a sharp point below M6).
const SET_SCREW_POINT: [(&str, f64, f64, f64); 7] = [
    ("M3", 2.0, 1.4, 0.0),
    ("M4", 2.5, 2.0, 0.0),
    ("M5", 3.5, 2.5, 0.0),
    ("M6", 4.0, 3.0, 1.5),
    ("M8", 5.5, 5.0, 2.0),
    ("M10", 7.0, 6.0, 2.5),
    ("M12", 8.5, 8.0, 3.0),
];

/// ISO 261 fine pitches offered per size, the first choice first: (size, pitches).
const FINE: [(&str, &[f64]); 8] = [
    ("M8", &[1.0]),
    ("M10", &[1.25, 1.0]),
    ("M12", &[1.25, 1.5]),
    ("M14", &[1.5]),
    ("M16", &[1.5]),
    ("M20", &[1.5, 2.0]),
    ("M24", &[2.0]),
    ("M30", &[2.0]),
];

/// DIN 7984: (size, head diameter dk, head height k, socket s, socket depth t).
const LOW_HEAD: [(&str, f64, f64, f64, f64); 7] = [
    ("M3", 5.5, 2.0, 2.0, 1.5),
    ("M4", 7.0, 2.8, 2.5, 2.3),
    ("M5", 8.5, 3.5, 3.0, 2.7),
    ("M6", 10.0, 4.0, 4.0, 3.0),
    ("M8", 13.0, 5.0, 5.0, 3.8),
    ("M10", 16.0, 6.0, 7.0, 4.5),
    ("M12", 18.0, 7.0, 8.0, 5.0),
];

/// Pan head: ISO 7045 (cross recess) and ISO 14583 (hexalobular) share dk and k, ISO 1580
/// (slotted) is lower. (size, dk, k recessed, k slotted, top radius rf, cross recess number,
/// cross recess m, hexalobular number, slot n, slot t).
#[allow(clippy::type_complexity)]
const PAN: [(&str, f64, f64, f64, f64, u8, f64, u8, f64, f64); 8] = [
    ("M2", 4.0, 1.6, 1.3, 3.2, 0, 1.9, 6, 0.5, 0.5),
    ("M2.5", 5.0, 2.1, 1.5, 4.0, 1, 2.7, 8, 0.6, 0.6),
    ("M3", 5.6, 2.4, 1.8, 5.0, 1, 3.0, 10, 0.8, 0.7),
    ("M4", 8.0, 3.1, 2.4, 6.5, 2, 4.4, 20, 1.2, 1.0),
    ("M5", 9.5, 3.7, 3.0, 8.0, 2, 4.9, 25, 1.2, 1.2),
    ("M6", 12.0, 4.6, 3.6, 10.0, 3, 6.9, 30, 1.6, 1.4),
    ("M8", 16.0, 6.0, 4.8, 13.0, 4, 9.0, 45, 2.0, 1.9),
    ("M10", 20.0, 7.5, 6.0, 16.0, 4, 10.1, 50, 2.5, 2.4),
];

/// ISO 1207: (size, head diameter dk, head height k, slot n, slot t).
const CHEESE: [(&str, f64, f64, f64, f64); 8] = [
    ("M2", 3.8, 1.4, 0.5, 0.6),
    ("M2.5", 4.5, 1.8, 0.6, 0.7),
    ("M3", 5.5, 2.0, 0.8, 0.85),
    ("M4", 7.0, 2.6, 1.2, 1.1),
    ("M5", 8.5, 3.3, 1.2, 1.3),
    ("M6", 10.0, 3.9, 1.6, 1.6),
    ("M8", 13.0, 5.0, 2.0, 2.0),
    ("M10", 16.0, 6.0, 2.5, 2.4),
];

/// Countersunk flat head, 90°: ISO 7046 (cross recess), ISO 14581 (hexalobular), ISO 2009
/// (slotted). (size, theoretical head diameter dk, cross recess number, cross recess m,
/// hexalobular number, slot n, slot t). As for ISO 10642, k = (dk − d) / 2 and the cone is drawn
/// sharp to dk.
const CSK_FLAT: [(&str, f64, u8, f64, u8, f64, f64); 8] = [
    ("M2", 4.4, 0, 1.9, 6, 0.5, 0.6),
    ("M2.5", 5.5, 1, 2.9, 8, 0.6, 0.73),
    ("M3", 6.3, 1, 3.2, 10, 0.8, 0.85),
    ("M4", 9.4, 2, 4.6, 20, 1.2, 1.3),
    ("M5", 10.4, 2, 5.2, 25, 1.2, 1.4),
    ("M6", 12.6, 3, 6.8, 30, 1.6, 1.6),
    ("M8", 17.3, 4, 8.9, 45, 2.0, 2.3),
    ("M10", 20.0, 4, 10.0, 50, 2.5, 2.6),
];

/// ISO 14579, a socket head cap screw with a hexalobular socket: (size, hexalobular number).
const SHCS_HEXALOBULAR: [(&str, u8); 9] =
    [("M2", 6), ("M2.5", 8), ("M3", 10), ("M4", 20), ("M5", 25), ("M6", 30), ("M8", 45), ("M10", 50), ("M12", 55)];

/// ISO 10664 hexalobular internal driving feature: (number, A across the lobes).
const HEXALOBULAR: [(u8, f64); 11] =
    [(6, 1.75), (8, 2.4), (10, 2.8), (15, 3.35), (20, 3.95), (25, 4.5), (30, 5.6), (40, 6.75), (45, 7.93), (50, 8.95), (55, 11.35)];

/// A cross recess (type H) by number 0…4: (the flat g across its bottom, the width of a wing).
/// With the wing ends at 26° 30' to the axis these give its depth from m, the span across the
/// wings at the head's surface. The recess drawn is two such wings, not the gauge form of
/// ISO 4757: right for a picture, a drawing and a driver's size, not for cutting one.
const CROSS: [(f64, f64); 5] = [(0.81, 0.6), (1.27, 1.0), (2.29, 1.5), (3.81, 2.4), (5.08, 3.5)];

/// The most turns a modelled thread is cut with. Cutting one takes longer than in proportion to
/// its length, and a part with more is asked for with a cosmetic thread instead.
const MAX_MODELED_TURNS: f64 = 80.0;

/// The clearance (mm per side) a part made to be 3D printed gets when none is given.
const PRINT_CLEARANCE: f64 = 0.15;

/// ISO 8734 diameters.
const DOWEL: [f64; 9] = [2.0, 3.0, 4.0, 5.0, 6.0, 8.0, 10.0, 12.0, 16.0];

/// Bearings: (designation, bore, outside diameter, width).
const BEARINGS: [(&str, f64, f64, f64); 13] = [
    ("625", 5.0, 16.0, 5.0),
    ("626", 6.0, 19.0, 6.0),
    ("608", 8.0, 22.0, 7.0),
    ("6000", 10.0, 26.0, 8.0),
    ("6001", 12.0, 28.0, 8.0),
    ("6002", 15.0, 32.0, 9.0),
    ("6003", 17.0, 35.0, 10.0),
    ("6004", 20.0, 42.0, 12.0),
    ("6200", 10.0, 30.0, 9.0),
    ("6201", 12.0, 32.0, 10.0),
    ("6202", 15.0, 35.0, 11.0),
    ("6203", 17.0, 40.0, 12.0),
    ("6204", 20.0, 47.0, 14.0),
];

/// Standard lengths for screws, bolts and pins (mm).
const LENGTHS: [f64; 22] =
    [4.0, 5.0, 6.0, 8.0, 10.0, 12.0, 16.0, 20.0, 25.0, 30.0, 35.0, 40.0, 45.0, 50.0, 55.0, 60.0, 65.0, 70.0, 80.0, 90.0, 100.0, 120.0];

/// Millimetres per inch.
const INCH: f64 = 25.4;

/// ASME B18.2.1 hex cap screw, in inches: (nominal size, width across flats F, head height H).
/// 9/16 is left out until the thread catalogue has its UNC and UNF threads.
const INCH_HEX_CAP: [(&str, f64, f64); 9] = [
    ("1/4", 0.4375, 0.156),
    ("5/16", 0.5, 0.203),
    ("3/8", 0.5625, 0.234),
    ("7/16", 0.625, 0.281),
    ("1/2", 0.75, 0.312),
    ("5/8", 0.9375, 0.391),
    ("3/4", 1.125, 0.469),
    ("7/8", 1.3125, 0.547),
    ("1", 1.5, 0.609),
];

/// ASME B18.2.2 hex nut, in inches: (nominal size, width across flats F, thickness H).
const INCH_HEX_NUT: [(&str, f64, f64); 9] = [
    ("1/4", 0.4375, 0.219),
    ("5/16", 0.5, 0.266),
    ("3/8", 0.5625, 0.328),
    ("7/16", 0.6875, 0.375),
    ("1/2", 0.75, 0.438),
    ("5/8", 0.9375, 0.547),
    ("3/4", 1.125, 0.641),
    ("7/8", 1.3125, 0.75),
    ("1", 1.5, 0.859),
];

/// ANSI B18.22.1 Type A plain washers, in inches: (nominal size, inside diameter, outside
/// diameter, thickness). SAE is the standard's narrow (N) series, USS its wide (W) one.
const WASHER_SAE: [(&str, f64, f64, f64); 9] = [
    ("1/4", 0.281, 0.625, 0.065),
    ("5/16", 0.344, 0.688, 0.065),
    ("3/8", 0.406, 0.812, 0.065),
    ("7/16", 0.469, 0.922, 0.065),
    ("1/2", 0.531, 1.062, 0.095),
    ("5/8", 0.656, 1.312, 0.095),
    ("3/4", 0.812, 1.469, 0.134),
    ("7/8", 0.938, 1.75, 0.134),
    ("1", 1.062, 2.0, 0.134),
];
const WASHER_USS: [(&str, f64, f64, f64); 9] = [
    ("1/4", 0.312, 0.734, 0.065),
    ("5/16", 0.375, 0.875, 0.083),
    ("3/8", 0.438, 1.0, 0.083),
    ("7/16", 0.5, 1.25, 0.083),
    ("1/2", 0.562, 1.375, 0.109),
    ("5/8", 0.688, 1.75, 0.134),
    ("3/4", 0.812, 2.0, 0.148),
    ("7/8", 0.938, 2.25, 0.165),
    ("1", 1.062, 2.5, 0.165),
];

/// Standard lengths for inch cap screws (inches).
const INCH_LENGTHS: [f64; 19] = [0.5, 0.75, 1.0, 1.25, 1.5, 1.75, 2.0, 2.25, 2.5, 2.75, 3.0, 3.5, 4.0, 4.5, 5.0, 5.5, 6.0, 7.0, 8.0];

fn inch_lengths() -> Vec<f64> {
    INCH_LENGTHS.iter().map(|l| l * INCH).collect()
}

/// The first standard length at or above 2.5 d. Both sides are inches turned into mm, so the
/// comparison allows for rounding: a 1/2 screw's 2.5 d is exactly 1-1/4.
fn default_inch_length(d: f64) -> f64 {
    inch_lengths().into_iter().find(|l| *l >= 2.5 * d - 1e-9).unwrap_or(8.0 * INCH)
}

fn is_inch(family: &str) -> bool {
    matches!(family, "hex_cap_screw_inch" | "hex_nut_inch" | "washer_sae" | "washer_uss")
}

/// A unified inch thread from the catalogue (`1/2-13 UNC`): its nominal size (`1/2`), and its
/// major diameter and pitch (mm).
fn unified(size: &str) -> Option<(String, f64, f64)> {
    use solidcraft_doc::TappedThreadFamily::{Unc, Unf};
    let t = solidcraft_doc::parse_tapped_thread(size).filter(|t| matches!(t.family, Unc | Unf))?;
    let nominal = t.designation.split('-').next()?.to_string();
    Some((nominal, t.major_diameter, t.pitch))
}

/// The catalogue's threads for a nominal inch size, UNC before UNF.
fn unified_sizes(nominal: &str) -> Vec<solidcraft_doc::TappedThread> {
    use solidcraft_doc::TappedThreadFamily::{Unc, Unf};
    [Unc, Unf].into_iter().flat_map(solidcraft_doc::tapped_thread_sizes).filter(|t| t.designation.split('-').next() == Some(nominal)).collect()
}

/// A length (mm) in fractional inches to the nearest 1/64 (`1-1/2`), or none off that grid.
fn inch_fraction(mm: f64) -> Option<String> {
    let n = mm / INCH * 64.0;
    let r = n.round();
    if !r.is_finite() || r <= 0.0 || r > 1e6 || (n - r).abs() > 1e-6 {
        return None;
    }
    let r = r as u64;
    let (whole, mut num, mut den) = (r / 64, r % 64, 64);
    while num > 0 && num % 2 == 0 {
        num /= 2;
        den /= 2;
    }
    Some(match (whole, num) {
        (w, 0) => w.to_string(),
        (0, n) => format!("{n}/{den}"),
        (w, n) => format!("{w}-{n}/{den}"),
    })
}

/// How a part's length (mm) is shown where its family has a unit of its own: `1-1/2 in` for an
/// inch family, none for a metric one (shown in the design's units).
pub fn part_length_label(family: &str, mm: f64) -> Option<String> {
    is_inch(family).then(|| inch_fraction(mm)).flatten().map(|f| format!("{f} in"))
}

/// A part's size with its length, as names and part numbers write it: `M6x20`, `1/2-13 UNC x 2`.
fn size_with_length(family: &str, size: &str, len: f64) -> String {
    match is_inch(family).then(|| inch_fraction(len)).flatten() {
        Some(l) => format!("{size} x {l}"),
        None => format!("{size}x{len}"),
    }
}

fn metric(size: &str) -> Option<(f64, f64)> {
    METRIC.iter().find(|(s, _, _)| s.eq_ignore_ascii_case(size)).map(|(_, d, p)| (*d, *p))
}

/// The head a counterbore seats, for `solid.hole`'s `head`: (head diameter dk, head height k)
/// of a metric cap screw, by its standard (`ISO 4762`, `ISO 7380-1`) or the family name
/// `parts.insert` takes. The size is written as `clearance` takes it (`M4`, or `M4x0.7` with the
/// pitch). The error names what is missing: the standard, or the size in it.
pub(crate) fn cap_screw_head(standard: &str, size: &str) -> std::result::Result<(f64, f64), String> {
    let (name, table): (&str, &[(&str, f64, f64, f64, f64)]) = match standard.trim() {
        s if s.eq_ignore_ascii_case("ISO 4762") || s.eq_ignore_ascii_case("socket_head_cap_screw") => ("ISO 4762", &SHCS),
        s if s.eq_ignore_ascii_case("ISO 7380-1") || s.eq_ignore_ascii_case("button_head_cap_screw") => ("ISO 7380-1", &BHCS),
        other => {
            return Err(format!("no head standard `{}`: ISO 4762 (socket head) or ISO 7380-1 (button head)", super::features::brief(other)));
        }
    };
    let size = size.trim().split(['x', 'X']).next().unwrap_or("").trim();
    let (first, last) = (table.first().map_or("", |r| r.0), table.last().map_or("", |r| r.0));
    table
        .iter()
        .find(|r| r.0.eq_ignore_ascii_case(size))
        .map(|r| (r.1, r.2))
        .ok_or_else(|| format!("no {name} head for `{}`: {first} to {last}", super::features::brief(size)))
}

fn default_length(d: f64) -> f64 {
    LENGTHS.iter().copied().find(|l| *l >= 2.5 * d).unwrap_or(100.0)
}

/// How a threaded part's thread is made.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
enum ThreadMode {
    /// Recorded on the face, the body unchanged: the default, and the cheapest.
    #[default]
    Cosmetic,
    /// Cut into the body.
    Modeled,
    None,
}

impl ThreadMode {
    fn id(self) -> &'static str {
        match self {
            ThreadMode::Cosmetic => "cosmetic",
            ThreadMode::Modeled => "modeled",
            ThreadMode::None => "none",
        }
    }
}

/// What a part is asked for beyond its family, size and length. The default is the part as it
/// has always been made.
#[derive(Clone, Debug, Default, PartialEq)]
struct Options {
    thread: ThreadMode,
    /// `hex`, `hexalobular`, `cross` or `slot`; none: the family's first.
    drive: Option<String>,
    /// A fine pitch; none: coarse.
    pitch: Option<f64>,
    left_hand: bool,
    /// Made to be 3D printed, with this clearance per side on the thread and the drive recess.
    print_clearance: Option<f64>,
    /// A set screw's `cone` or `cup` point; none: flat.
    point: Option<String>,
}

impl Options {
    fn of(part: &solidcraft_doc::StandardPart) -> Self {
        Options {
            thread: match part.thread.as_deref() {
                Some("modeled") => ThreadMode::Modeled,
                Some("none") => ThreadMode::None,
                _ => ThreadMode::Cosmetic,
            },
            drive: part.drive.clone(),
            pitch: part.pitch,
            left_hand: part.left_hand,
            print_clearance: part.print_clearance,
            point: part.point.clone(),
        }
    }

    fn save(&self, family: &str, size: &str, length: Option<f64>) -> solidcraft_doc::StandardPart {
        let mut part = solidcraft_doc::StandardPart::new(family, size, length);
        part.thread = (self.thread != ThreadMode::Cosmetic).then(|| self.thread.id().to_string());
        part.drive = self.drive.clone();
        part.pitch = self.pitch;
        part.left_hand = self.left_hand;
        part.print_clearance = self.print_clearance;
        part.point = self.point.clone();
        part
    }

    fn clearance(&self) -> f64 {
        self.print_clearance.unwrap_or(0.0)
    }
}

/// A drive recess that is not a hex socket, cut down from the top of a head.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Recess {
    /// Six lobes, `a` across them, `t` deep.
    Hexalobular { a: f64, t: f64 },
    /// Two wings at right angles: `m` across at the surface, `g` across the bottom, `wing` wide.
    Cross { m: f64, g: f64, wing: f64, t: f64 },
    /// A slot `n` wide and `t` deep, across the whole head.
    Slot { n: f64, t: f64 },
}

/// The end of a set screw: (the point's end diameter, whether it is a cup).
type Point = Option<(f64, bool)>;

/// One part's shape, in its own frame.
#[derive(Clone, Debug, PartialEq)]
enum Shape {
    /// A head (round, domed or hex), a shank down −z, an optional hex socket in the head's top,
    /// or another recess there. `pitch` is that of a metric thread; inch parts have none, as
    /// `solid.thread` takes only metric designations (#9571).
    Screw { d: f64, pitch: Option<f64>, len: f64, head: Head, socket: Option<(f64, f64)>, recess: Option<Recess> },
    /// A hex prism with a through hole (nut), z 0…m.
    Nut { d: f64, pitch: Option<f64>, s: f64, m: f64 },
    /// A ring (washer, bearing envelope), z 0…h.
    Ring { d_in: f64, d_out: f64, h: f64 },
    /// A cylinder down −z with a socket in its top (set screw), or plain (pin). `point` shapes
    /// a set screw's end: a cone or a cup.
    Rod { d: f64, pitch: Option<f64>, len: f64, socket: Option<(f64, f64)>, point: Point },
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Head {
    Round {
        dk: f64,
        k: f64,
    },
    /// A button head: a short cylindrical flank, a crown domed over it, and a flat top of
    /// radius `flat` around the socket, at z = k.
    Dome {
        dk: f64,
        k: f64,
        flat: f64,
    },
    /// A countersunk head: a 90° cone from the shank, k deep, opening to dk at the top plane
    /// z = 0. The screw's length is measured from that plane, head included.
    Countersunk {
        dk: f64,
        k: f64,
    },
    Hex {
        s: f64,
        k: f64,
    },
    /// A pan head: a cylindrical flank under a spherical top of radius `rf`, `k` high at the axis.
    Pan {
        dk: f64,
        k: f64,
        rf: f64,
    },
}

/// The drives a family comes with, the first being the one it gets when none is asked for.
fn drives(family: &str) -> &'static [&'static str] {
    match family {
        "socket_head_cap_screw" => &["hex", "hexalobular"],
        "pan_head_screw" | "countersunk_screw" => &["cross", "hexalobular", "slot"],
        _ => &[],
    }
}

/// Whether a family's parts are threaded with a metric thread, which the thread options act on.
fn threaded(family: &str) -> bool {
    matches!(
        family,
        "socket_head_cap_screw"
            | "button_head_cap_screw"
            | "countersunk_socket_screw"
            | "low_head_cap_screw"
            | "pan_head_screw"
            | "cheese_head_screw"
            | "countersunk_screw"
            | "hex_bolt"
            | "hex_nut"
            | "set_screw"
    )
}

fn fine_pitches(size: &str) -> &'static [f64] {
    FINE.iter().find(|f| f.0.eq_ignore_ascii_case(size)).map_or(&[], |f| f.1)
}

/// A number as a designation writes it: 5.7, not 5.699999999999999.
fn tidy(x: f64) -> f64 {
    (x * 1000.0).round() / 1000.0
}

/// A hexalobular recess of a number: its size across the lobes.
fn hexalobular_size(number: u8) -> Option<f64> {
    HEXALOBULAR.iter().find(|h| h.0 == number).map(|h| h.1)
}

/// A cross recess `m` across the wings, of a number.
fn cross_recess(number: u8, m: f64) -> Recess {
    let (g, wing) = CROSS.get(usize::from(number)).copied().unwrap_or((0.81, 0.6));
    Recess::Cross { m, g, wing, t: (m - g) / 2.0 / 26.5_f64.to_radians().tan() }
}

/// The length of thread a modelled screw is cut with, from its tip, where its standard leaves a
/// plain shank under the head (none: threaded to the head). ISO 4762 and its kin thread
/// b = 2d + 12 once the plain part would be over about 7 pitches; ISO 4014 threads b = 2d + 6
/// (2d + 12 over 125 long) and shorter bolts are ISO 4017, threaded to the head.
fn thread_length(family: &str, d: f64, pitch: f64, shank: f64) -> Option<f64> {
    match family {
        "socket_head_cap_screw" | "low_head_cap_screw" | "countersunk_socket_screw" => {
            let b = 2.0 * d + 12.0;
            (shank - b > 7.0 * pitch).then_some(b)
        }
        "hex_bolt" => {
            let b = if shank <= 125.0 { 2.0 * d + 6.0 } else { 2.0 * d + 12.0 };
            (shank - b > 10.0 * pitch).then_some(b)
        }
        "pan_head_screw" | "cheese_head_screw" | "countersunk_screw" => (shank > 40.0).then_some(38.0),
        _ => None,
    }
}

/// The shape of one part of a family, from its standard's table: the size and, for screws, bolts,
/// set screws and pins, a length in mm (a standard one near 2.5 d when none is given). The error
/// says which family, size or length does not exist.
fn shape(family: &str, size: &str, length: Option<f64>) -> std::result::Result<Shape, String> {
    shape_with(family, size, length, &Options::default())
}

/// [`shape`] of a part asked for with options. Each option is checked against the family and the
/// size here, before anything is built: the error says what the family or size comes with.
fn shape_with(family: &str, size: &str, length: Option<f64>, o: &Options) -> std::result::Result<Shape, String> {
    let need = |what: &str| format!("no {what} size `{size}` (parts.library lists them)");
    let len = |d: f64| length.unwrap_or_else(|| default_length(d));
    let name = FAMILIES.iter().find(|f| f.0 == family).map_or(family, |f| f.1);
    // The thread options act on a metric thread.
    if !threaded(family) && (o.thread != ThreadMode::Cosmetic || o.pitch.is_some() || o.left_hand) {
        return Err(format!("`thread`, `pitch` and `left_hand` are for parts with a metric thread; a {name} has none"));
    }
    if let Some(p) = o.pitch {
        let fine = fine_pitches(size);
        if !fine.iter().any(|f| (f - p).abs() < 1e-9) {
            let list = if fine.is_empty() {
                "no fine pitch".to_string()
            } else {
                format!("fine pitch {}", fine.iter().map(f64::to_string).collect::<Vec<_>>().join(" or "))
            };
            return Err(format!("`pitch` {p} is not a fine pitch of {size}: it comes with {list} (leave `pitch` out for coarse)"));
        }
    }
    let drive = match o.drive.as_deref() {
        None => drives(family).first().copied().unwrap_or("hex"),
        Some(dr) => match drives(family).iter().find(|x| x.eq_ignore_ascii_case(dr)) {
            Some(x) => x,
            None if drives(family).is_empty() => return Err(format!("a {name} comes with one drive: leave `drive` out")),
            None => return Err(format!("a {name} has no `{}` drive: {}", super::features::brief(dr), drives(family).join(", "))),
        },
    };
    if o.point.is_some() && family != "set_screw" {
        return Err("`tip` is a set screw's point: flat, cone or cup".into());
    }
    if o.print_clearance.is_some() && !threaded(family) {
        return Err(format!("`print_fit` loosens a thread and a drive recess; a {name} has neither"));
    }
    // A fine pitch replaces the size's coarse one.
    let metric = |size: &str| metric(size).map(|(d, p)| (d, o.pitch.unwrap_or(p)));
    let hexalobular = |number: u8, t: f64| -> std::result::Result<Recess, String> {
        let a = hexalobular_size(number).ok_or_else(|| format!("a {size} {name} has no hexalobular size"))?;
        Ok(Recess::Hexalobular { a, t: t.min(0.5 * a) })
    };
    let sh = match family {
        "socket_head_cap_screw" => {
            let (d, pitch) = metric(size).ok_or_else(|| need("metric"))?;
            let (_, dk, k, s, t) = SHCS.iter().find(|r| r.0.eq_ignore_ascii_case(size)).copied().ok_or_else(|| need("ISO 4762"))?;
            if drive == "hexalobular" {
                let number = SHCS_HEXALOBULAR.iter().find(|r| r.0.eq_ignore_ascii_case(size)).map(|r| r.1).ok_or_else(|| need("ISO 14579"))?;
                Shape::Screw { d, pitch: Some(pitch), len: len(d), head: Head::Round { dk, k }, socket: None, recess: Some(hexalobular(number, t)?) }
            } else {
                Shape::Screw { d, pitch: Some(pitch), len: len(d), head: Head::Round { dk, k }, socket: Some((s, t)), recess: None }
            }
        }
        "low_head_cap_screw" => {
            let (d, pitch) = metric(size).ok_or_else(|| need("metric"))?;
            let (_, dk, k, s, t) = LOW_HEAD.iter().find(|r| r.0.eq_ignore_ascii_case(size)).copied().ok_or_else(|| need("DIN 7984"))?;
            Shape::Screw { d, pitch: Some(pitch), len: len(d), head: Head::Round { dk, k }, socket: Some((s, t)), recess: None }
        }
        "cheese_head_screw" => {
            let (d, pitch) = metric(size).ok_or_else(|| need("metric"))?;
            let (_, dk, k, n, t) = CHEESE.iter().find(|r| r.0.eq_ignore_ascii_case(size)).copied().ok_or_else(|| need("ISO 1207"))?;
            Shape::Screw { d, pitch: Some(pitch), len: len(d), head: Head::Round { dk, k }, socket: None, recess: Some(Recess::Slot { n, t }) }
        }
        "pan_head_screw" => {
            let (d, pitch) = metric(size).ok_or_else(|| need("metric"))?;
            let (_, dk, k, k_slot, rf, cross, m, lobes, n, t) =
                PAN.iter().find(|r| r.0.eq_ignore_ascii_case(size)).copied().ok_or_else(|| need("ISO 7045"))?;
            let (k, recess) = match drive {
                "hexalobular" => (k, hexalobular(lobes, 0.65 * k)?),
                "slot" => (k_slot, Recess::Slot { n, t }),
                _ => (k, cross_recess(cross, m)),
            };
            Shape::Screw { d, pitch: Some(pitch), len: len(d), head: Head::Pan { dk, k, rf }, socket: None, recess: Some(recess) }
        }
        "countersunk_screw" => {
            let (d, pitch) = metric(size).ok_or_else(|| need("metric"))?;
            let (_, dk, cross, m, lobes, n, t) = CSK_FLAT.iter().find(|r| r.0.eq_ignore_ascii_case(size)).copied().ok_or_else(|| need("ISO 7046"))?;
            let k = (dk - d) / 2.0;
            let len = len(d);
            if len <= k {
                return Err(format!("a {size} countersunk screw's length counts its head, so it must be over {k} mm (it is {len} mm)"));
            }
            let recess = match drive {
                "hexalobular" => hexalobular(lobes, 0.65 * k)?,
                "slot" => Recess::Slot { n, t },
                _ => cross_recess(cross, m),
            };
            Shape::Screw { d, pitch: Some(pitch), len, head: Head::Countersunk { dk, k }, socket: None, recess: Some(recess) }
        }
        "button_head_cap_screw" => {
            let (d, pitch) = metric(size).ok_or_else(|| need("metric"))?;
            let (_, dk, k, s, t) = BHCS.iter().find(|r| r.0.eq_ignore_ascii_case(size)).copied().ok_or_else(|| need("ISO 7380-1"))?;
            Shape::Screw { d, pitch: Some(pitch), len: len(d), head: Head::Dome { dk, k, flat: BUTTON_FLAT * s }, socket: Some((s, t)), recess: None }
        }
        "countersunk_socket_screw" => {
            let (d, pitch) = metric(size).ok_or_else(|| need("metric"))?;
            let (_, dk, k, s, t) = CSK.iter().find(|r| r.0.eq_ignore_ascii_case(size)).copied().ok_or_else(|| need("ISO 10642"))?;
            let len = len(d);
            if len <= k {
                return Err(format!("a {size} countersunk screw's length counts its head, so it must be over {k} mm (it is {len} mm)"));
            }
            Shape::Screw { d, pitch: Some(pitch), len, head: Head::Countersunk { dk, k }, socket: Some((s, t)), recess: None }
        }
        "hex_bolt" => {
            let (d, pitch) = metric(size).ok_or_else(|| need("metric"))?;
            let (_, s, k) = HEX_BOLT.iter().find(|r| r.0.eq_ignore_ascii_case(size)).copied().ok_or_else(|| need("ISO 4014"))?;
            Shape::Screw { d, pitch: Some(pitch), len: len(d), head: Head::Hex { s, k }, socket: None, recess: None }
        }
        "hex_nut" => {
            let (d, pitch) = metric(size).ok_or_else(|| need("metric"))?;
            let (_, s, m) = HEX_NUT.iter().find(|r| r.0.eq_ignore_ascii_case(size)).copied().ok_or_else(|| need("ISO 4032"))?;
            Shape::Nut { d, pitch: Some(pitch), s, m }
        }
        "hex_cap_screw_inch" => {
            let (nominal, d, _) = unified(size).ok_or_else(|| need("UNC/UNF"))?;
            let (_, f, h) = INCH_HEX_CAP.iter().find(|r| r.0 == nominal).copied().ok_or_else(|| need("ASME B18.2.1"))?;
            Shape::Screw {
                d,
                pitch: None,
                len: length.unwrap_or_else(|| default_inch_length(d)),
                head: Head::Hex { s: f * INCH, k: h * INCH },
                socket: None,
                recess: None,
            }
        }
        "hex_nut_inch" => {
            let (nominal, d, _) = unified(size).ok_or_else(|| need("UNC/UNF"))?;
            let (_, f, h) = INCH_HEX_NUT.iter().find(|r| r.0 == nominal).copied().ok_or_else(|| need("ASME B18.2.2"))?;
            Shape::Nut { d, pitch: None, s: f * INCH, m: h * INCH }
        }
        "washer_sae" | "washer_uss" => {
            let table = if family == "washer_sae" { &WASHER_SAE } else { &WASHER_USS };
            let (_, d1, d2, h) = table.iter().find(|r| r.0 == size.trim()).copied().ok_or_else(|| need("ANSI B18.22.1"))?;
            Shape::Ring { d_in: d1 * INCH, d_out: d2 * INCH, h: h * INCH }
        }
        "washer" => {
            let (_, d1, d2, h) = WASHER.iter().find(|r| r.0.eq_ignore_ascii_case(size)).copied().ok_or_else(|| need("ISO 7089"))?;
            Shape::Ring { d_in: d1, d_out: d2, h }
        }
        "set_screw" => {
            let (d, pitch) = metric(size).ok_or_else(|| need("metric"))?;
            let (_, s, t) = SET_SCREW.iter().find(|r| r.0.eq_ignore_ascii_case(size)).copied().ok_or_else(|| need("ISO 4026"))?;
            let point: Point = match o.point.as_deref().map(str::to_ascii_lowercase).as_deref() {
                None | Some("flat") => None,
                Some(kind @ ("cone" | "cup")) => {
                    let (_, _, dz, dt) =
                        SET_SCREW_POINT.iter().find(|r| r.0.eq_ignore_ascii_case(size)).copied().ok_or_else(|| need("set screw point"))?;
                    Some(if kind == "cup" { (dz, true) } else { (dt, false) })
                }
                Some(other) => return Err(format!("no set screw point `{}`: flat, cone or cup", super::features::brief(other))),
            };
            let len = length.unwrap_or_else(|| LENGTHS.iter().copied().find(|l| *l >= d).unwrap_or(d));
            // The point is cut from the end: what is left must still hold the socket.
            if let Some((end, _)) = point
                && len < t + (d - end) / 2.0 + pitch
            {
                return Err(format!(
                    "a {size} set screw with that point must be at least {} mm long (it is {len} mm)",
                    (t + (d - end) / 2.0 + pitch).ceil()
                ));
            }
            Shape::Rod { d, pitch: Some(pitch), len, socket: Some((s, t)), point }
        }
        "dowel_pin" => {
            let d: f64 = size.trim_start_matches(['d', 'D', 'Ø']).trim().parse().map_err(|_| need("dowel pin"))?;
            if !DOWEL.contains(&d) {
                return Err(need("ISO 8734"));
            }
            Shape::Rod { d, pitch: None, len: len(d * 1.6), socket: None, point: None }
        }
        "bearing" => {
            let (_, bore, od, w) = BEARINGS.iter().find(|r| r.0 == size.trim()).copied().ok_or_else(|| need("bearing"))?;
            Shape::Ring { d_in: bore, d_out: od, h: w }
        }
        o => return Err(format!("unknown part family `{o}` (parts.library lists them)")),
    };
    // A modelled thread is cut turn by turn: refuse one too long to cut in reasonable time.
    let cut = match sh {
        Shape::Screw { d, pitch: Some(p), len, head, .. } => {
            let shank = if let Head::Countersunk { k, .. } = head { len - k } else { len };
            Some((p, thread_length(family, d, p, shank).unwrap_or(shank)))
        }
        Shape::Rod { pitch: Some(p), len, .. } => Some((p, len)),
        Shape::Nut { pitch: Some(p), m, .. } => Some((p, m)),
        _ => None,
    };
    if o.thread == ThreadMode::Modeled
        && let Some((pitch, length)) = cut
        && length / pitch > MAX_MODELED_TURNS
    {
        return Err(format!(
            "a modelled thread of {:.0} turns takes too long to cut (the limit is {MAX_MODELED_TURNS:.0}): use `thread: cosmetic` for this part, or a length of {} mm or less",
            length / pitch,
            (MAX_MODELED_TURNS * pitch).floor()
        ));
    }
    if let Some(c) = o.print_clearance
        && let Shape::Screw { d, .. } | Shape::Rod { d, .. } | Shape::Nut { d, .. } = sh
        && c >= d / 6.0
    {
        return Err(format!("`clearance` {c} mm is too much for a {size} thread: under {} mm", tidy(d / 6.0)));
    }
    Ok(sh)
}

fn run(s: &mut Session, id: &str, p: Value) -> Result<Value> {
    s.execute(id, &p)
}

/// A hex prism (across flats `af`) from z0 to z1, joined or cut.
fn hex(s: &mut Session, af: f64, z0: f64, z1: f64, op: &str, body_name: Option<&str>) -> Result<()> {
    run(s, "sketch.create", json!({"plane": "XY", "offset": format!("{z0} mm")}))?;
    run(s, "sketch.polygon.circumscribed", json!({"center": [0, 0], "radius": af / 2.0, "sides": 6}))?;
    fix_all(s)?;
    run(s, "sketch.finish", json!({}))?;
    let (dist, dir) = if z1 >= z0 { (z1 - z0, "positive") } else { (z0 - z1, "negative") };
    run(s, "solid.extrude", json!({"distance": format!("{dist} mm"), "direction": dir, "operation": op, "body_name": body_name}))?;
    Ok(())
}

fn cyl(s: &mut Session, d: f64, z0: f64, z1: f64, op: &str) -> Result<()> {
    run(
        s,
        "solid.cylinder",
        json!({"diameter": format!("{d} mm"), "height": format!("{} mm", z1 - z0), "base": [0, 0, z0], "axis": [0, 0, 1], "operation": op}),
    )?;
    Ok(())
}

/// A solid of revolution about z from a closed (r, z) outline, as one body or joined.
fn revolve(s: &mut Session, pts: &[(f64, f64)], op: &str, body_name: Option<&str>) -> Result<()> {
    revolve_outline(s, pts, None, op, body_name)
}

/// The same, with the outline closed from its last point back to its first by an arc through
/// `through`: a button head's domed crown.
fn revolve_domed(s: &mut Session, pts: &[(f64, f64)], through: (f64, f64), op: &str, body_name: Option<&str>) -> Result<()> {
    revolve_outline(s, pts, Some(through), op, body_name)
}

fn revolve_outline(s: &mut Session, pts: &[(f64, f64)], arc: Option<(f64, f64)>, op: &str, body_name: Option<&str>) -> Result<()> {
    run(s, "sketch.create", json!({"plane": "XZ"}))?;
    let xz = |(r, z): &(f64, f64)| [*r, -*z];
    let points: Vec<[f64; 2]> = pts.iter().map(xz).collect();
    run(s, "sketch.line", json!({"points": points, "closed": arc.is_none()}))?;
    if let (Some(through), Some(first), Some(last)) = (arc, pts.first(), pts.last()) {
        run(s, "sketch.arc.three_point", json!({"start": xz(last), "through": xz(&through), "end": xz(first)}))?;
    }
    fix_all(s)?;
    run(s, "sketch.finish", json!({}))?;
    run(s, "solid.revolve", json!({"axis": "y", "operation": op, "body_name": body_name}))?;
    Ok(())
}

/// A point halfway along a button head's crown: on the circle centred on the axis that runs
/// from the top of the flank (a, c) to the rim of the flat top (b, k).
fn crown(a: f64, c: f64, b: f64, k: f64) -> (f64, f64) {
    // The centre on the axis, equidistant from both, and the radius from it.
    let zc = (b * b + k * k - a * a - c * c) / (2.0 * (k - c));
    let r = (b * b + (k - zc) * (k - zc)).sqrt();
    // Halfway between the directions to the two ends.
    let (x, z) = ((a + b) / r, (c - zc + k - zc) / r);
    let n = x.hypot(z);
    if n.is_finite() && n > 0.0 { (r * x / n, zc + r * z / n) } else { ((a + b) / 2.0, (c + k) / 2.0) }
}

/// Fix every point and curve of the sketch being drawn: a standard part's outline is its size, so
/// its sketches are fully defined and a check of the design finds nothing left free in them.
fn fix_all(s: &mut Session) -> Result<()> {
    let sketch = super::sketch_store::active(s).and_then(|id| super::sketch_store::read(&s.doc, id).ok());
    let entities: Vec<String> =
        sketch.map(|sk| sk.points.iter().map(|p| p.id.clone()).chain(sk.curves.iter().map(|c| c.id.clone())).collect()).unwrap_or_default();
    if !entities.is_empty() {
        run(s, "sketch.constraint.fix", json!({"entities": entities, "fixed": true}))?;
    }
    Ok(())
}

/// The thread of a part: on the cylindrical face at `at`, of the thread diameter `d` (the
/// part's, less or plus its print clearance). `start` is a point near the end the thread starts
/// from and `length` how far it runs from there (none: the whole face); `external` threads get
/// their start chamfered when modelled, as a real screw's end is.
fn thread(s: &mut Session, o: &Options, (d, pitch): (f64, f64), at: Vec3, start: Vec3, length: Option<f64>, external: bool) -> Result<()> {
    let mut p = json!({"face": at, "designation": format!("M{}x{}", tidy(d), pitch)});
    if o.left_hand {
        p["hand"] = json!("left");
    }
    match o.thread {
        ThreadMode::None => Ok(()),
        // A cosmetic thread a part can't take is left off, as it always was.
        ThreadMode::Cosmetic => {
            let _ = run(s, "solid.thread", p);
            Ok(())
        }
        ThreadMode::Modeled => {
            p["modeled"] = json!(true);
            p["from"] = json!(start);
            if external {
                p["tip"] = json!("start");
            }
            if let Some(l) = length {
                p["length"] = json!(format!("{l} mm"));
            }
            run(s, "solid.thread", p).map(|_| ())
        }
    }
}

/// A recess cut down from the top of a head at z = `top`. `c` widens it per side (a printed
/// part); `reach` is past the head's rim, for a slot.
fn recess(s: &mut Session, r: Recess, top: f64, c: f64, reach: f64) -> Result<()> {
    // A slab through the axis: its outline (across, z) drawn on a plane holding the axis, and
    // extruded both ways to `width`.
    let slab = |s: &mut Session, plane: &str, outline: [(f64, f64); 4], width: f64| -> Result<()> {
        run(s, "sketch.create", json!({"plane": plane}))?;
        // On XZ a sketch's y runs down −z; on YZ it runs up +z.
        let flip = if plane == "XZ" { -1.0 } else { 1.0 };
        let points: Vec<[f64; 2]> = outline.iter().map(|(x, z)| [*x, flip * *z]).collect();
        run(s, "sketch.line", json!({"points": points, "closed": true}))?;
        fix_all(s)?;
        run(s, "sketch.finish", json!({}))?;
        run(s, "solid.extrude", json!({"distance": format!("{width} mm"), "direction": "symmetric", "operation": "cut"}))?;
        Ok(())
    };
    match r {
        Recess::Hexalobular { a, t } => {
            run(s, "sketch.create", json!({"plane": "XY", "offset": format!("{top} mm")}))?;
            for (start, through, end) in hexalobular_arcs(a + 2.0 * c) {
                run(s, "sketch.arc.three_point", json!({"start": start, "through": through, "end": end}))?;
            }
            fix_all(s)?;
            run(s, "sketch.finish", json!({}))?;
            run(s, "solid.extrude", json!({"distance": format!("{t} mm"), "direction": "negative", "operation": "cut"}))?;
        }
        Recess::Cross { m, g, wing, t } => {
            let (m, wing) = (m + 2.0 * c, wing + 2.0 * c);
            let outline = [(-m / 2.0, top), (m / 2.0, top), (g / 2.0, top - t), (-g / 2.0, top - t)];
            slab(s, "XZ", outline, wing)?;
            slab(s, "YZ", outline, wing)?;
        }
        Recess::Slot { n, t } => {
            let outline = [(-reach, top + 1.0), (reach, top + 1.0), (reach, top - t), (-reach, top - t)];
            slab(s, "XZ", outline, n + 2.0 * c)?;
        }
    }
    Ok(())
}

/// The outline of a hexalobular recess `a` across its lobes, as twelve arcs (start, through,
/// end) running round it: six lobes bulging out, joined by six flutes bulging in, each tangent
/// to its neighbours. ISO 10664 draws it in proportion to A: across the flutes B ≈ 0.72 A, the
/// lobes' radius ≈ 0.1 A and the flutes' ≈ 0.175 A. The first two are taken as given and the
/// flutes' radius follows from the arcs being tangent, which comes to 0.175 A.
fn hexalobular_arcs(a: f64) -> Vec<([f64; 2], [f64; 2], [f64; 2])> {
    let (re, b) = (0.1 * a, 0.72 * a);
    // The circle the lobes' centres lie on.
    let la = a / 2.0 - re;
    let s3 = 3f64.sqrt();
    // A flute's circle (radius ri, centred b/2 + ri out, 30° on from a lobe) touches the lobe's:
    // la² + c² − √3 la c = (re + ri)², with c = b/2 + ri.
    let ri = (re * re - la * la - b * b / 4.0 + s3 * la * b / 2.0) / (b - s3 * la - 2.0 * re);
    let fc = b / 2.0 + ri;
    let polar = |r: f64, deg: f64| [r * deg.to_radians().cos(), r * deg.to_radians().sin()];
    let lobe = |i: i32| polar(la, 60.0 * f64::from(i));
    let flute = |i: i32| polar(fc, 30.0 + 60.0 * f64::from(i));
    // Where lobe i's circle touches flute j's.
    let touch = |i: i32, j: i32| {
        let (l, f) = (lobe(i), flute(j));
        let n = (f[0] - l[0]).hypot(f[1] - l[1]);
        if n > 0.0 { [l[0] + re * (f[0] - l[0]) / n, l[1] + re * (f[1] - l[1]) / n] } else { l }
    };
    let mut arcs = Vec::with_capacity(12);
    for i in 0..6 {
        arcs.push((touch(i, i - 1), polar(la + re, 60.0 * f64::from(i)), touch(i, i)));
        arcs.push((touch(i, i), polar(b / 2.0, 30.0 + 60.0 * f64::from(i)), touch(i + 1, i)));
    }
    arcs
}

/// Build a part in the active component.
fn build(s: &mut Session, family: &str, sh: &Shape, o: &Options, body_name: Option<&str>) -> Result<()> {
    let _frame = crate::frames::local_geometry();
    // A part made to be printed: its external thread and shank drawn under size, an internal
    // thread over size, and its recess wider, each by the clearance per side.
    let c = o.clearance();
    match *sh {
        Shape::Screw { d: nominal, pitch, len, head, socket, recess: cut } => {
            let d = nominal - 2.0 * c;
            let k = match head {
                // One revolved outline: shank and round head.
                Head::Round { dk, k } => {
                    revolve(s, &[(0.0, -len), (d / 2.0, -len), (d / 2.0, 0.0), (dk / 2.0, 0.0), (dk / 2.0, k), (0.0, k)], "new", body_name)?;
                    k
                }
                // The same outline up to the head's rim, then a short flank, and the arc of the
                // crown closing it back to the flat top.
                Head::Dome { dk, k, flat } => {
                    let (a, c) = (dk / 2.0, BUTTON_FLANK * k);
                    let outline = [(flat, k), (0.0, k), (0.0, -len), (d / 2.0, -len), (d / 2.0, 0.0), (a, 0.0), (a, c)];
                    revolve_domed(s, &outline, crown(a, c, flat, k), "new", body_name)?;
                    k
                }
                // One revolved outline again, but the head hangs below z = 0: the shank, the cone
                // opening up to dk at the top, and the top's flat, which the socket is cut into.
                Head::Countersunk { dk, k } => {
                    revolve(s, &[(0.0, -len), (d / 2.0, -len), (d / 2.0, -k), (dk / 2.0, 0.0), (0.0, 0.0)], "new", body_name)?;
                    0.0
                }
                // A hex head joined over a shank that runs up into it.
                Head::Hex { s: af, k } => {
                    hex(s, af, 0.0, k, "new", body_name)?;
                    revolve(s, &[(0.0, -len), (d / 2.0, -len), (d / 2.0, k / 2.0), (0.0, k / 2.0)], "join", None)?;
                    k
                }
                // The shank and the head's flank, closed over the top by the arc of its sphere,
                // which meets the flank where the sphere has the head's radius.
                Head::Pan { dk, k, rf } => {
                    let a = dk / 2.0;
                    let rf = rf.max(a);
                    let zc = k - rf;
                    let at = |r: f64| zc + (rf * rf - r * r).max(0.0).sqrt();
                    let outline = [(0.0, k), (0.0, -len), (d / 2.0, -len), (d / 2.0, 0.0), (a, 0.0), (a, at(a))];
                    revolve_domed(s, &outline, (a / 2.0, at(a / 2.0)), "new", body_name)?;
                    k
                }
            };
            // The z of the top the recess is cut from, and how far out a slot must reach.
            let (top, reach) = match head {
                Head::Countersunk { dk, .. } => (0.0, dk),
                Head::Round { dk, .. } | Head::Dome { dk, .. } | Head::Pan { dk, .. } => (k, dk),
                Head::Hex { s, .. } => (k, 2.0 * s),
            };
            if let Some((af, t)) = socket {
                hex(s, af + 2.0 * c, top, top - t, "cut", None)?;
            }
            if let Some(r) = cut {
                recess(s, r, top, c, reach)?;
            }
            if let Some(p) = pitch {
                // The shank, which a countersunk head leaves shorter than the length.
                let shank = match head {
                    Head::Countersunk { k, .. } => len - k,
                    _ => len,
                };
                let length = if o.thread == ThreadMode::Modeled { thread_length(family, nominal, p, shank) } else { None };
                thread(s, o, (d, p), Vec3::new(d / 2.0, 0.0, shank / 2.0 - len), Vec3::new(d / 2.0, 0.0, -len + 0.1 * shank), length, true)?;
            }
        }
        Shape::Nut { d: nominal, pitch, s: af, m } => {
            let d = nominal + 2.0 * c;
            hex(s, af, 0.0, m, "new", body_name)?;
            cyl(s, d, -1.0, m + 1.0, "cut")?;
            if let Some(p) = pitch {
                thread(s, o, (d, p), Vec3::new(d / 2.0, 0.0, m / 2.0), Vec3::new(d / 2.0, 0.0, 0.1 * m), None, false)?;
            }
        }
        Shape::Ring { d_in, d_out, h } => {
            revolve(s, &[(d_in / 2.0, 0.0), (d_out / 2.0, 0.0), (d_out / 2.0, h), (d_in / 2.0, h)], "new", body_name)?;
        }
        Shape::Rod { d: nominal, pitch, len, socket, point } => {
            let d = nominal - 2.0 * c;
            let r = d / 2.0;
            // Where the cylinder ends above the point.
            let foot = match point {
                None => {
                    revolve(s, &[(0.0, -len), (r, -len), (r, 0.0), (0.0, 0.0)], "new", body_name)?;
                    -len
                }
                // A cup: chamfered at 45° to its rim, then a 120° cone back up inside it.
                Some((dz, true)) => {
                    let rz = (dz / 2.0).min(r);
                    let foot = -len + (r - rz);
                    revolve(s, &[(0.0, -len + rz * 30f64.to_radians().tan()), (rz, -len), (r, foot), (r, 0.0), (0.0, 0.0)], "new", body_name)?;
                    foot
                }
                // A cone: 90°, down to its flat end, or to a point where it has none.
                Some((dt, false)) => {
                    let rt = (dt / 2.0).min(r);
                    let foot = -len + (r - rt);
                    let outline: Vec<(f64, f64)> = if rt > 0.0 {
                        vec![(0.0, -len), (rt, -len), (r, foot), (r, 0.0), (0.0, 0.0)]
                    } else {
                        vec![(0.0, -len), (r, foot), (r, 0.0), (0.0, 0.0)]
                    };
                    revolve(s, &outline, "new", body_name)?;
                    foot
                }
            };
            if let Some((af, t)) = socket {
                hex(s, af + 2.0 * c, 0.0, -t, "cut", None)?;
            }
            if let Some(p) = pitch {
                thread(s, o, (d, p), Vec3::new(r, 0.0, foot / 2.0), Vec3::new(r, 0.0, 0.9 * foot), None, true)?;
            }
        }
    }
    Ok(())
}

/// `parts.library`: the families, or with `family` one family's sizes (the rows of its table, in
/// mm) and its standard lengths.
fn library(_s: &mut Session, p: &Value) -> Result<Value> {
    let fam = |id: &str| -> PartFamily {
        let mut sizes: Vec<Value> = match id {
            "socket_head_cap_screw" => SHCS.iter().map(|r| json!({"size": r.0, "d": metric(r.0).map(|m| m.0), "pitch": metric(r.0).map(|m| m.1), "head_diameter": r.1, "head_height": r.2, "socket": r.3, "socket_depth": r.4})).collect(),
            "button_head_cap_screw" => BHCS.iter().map(|r| json!({"size": r.0, "d": metric(r.0).map(|m| m.0), "pitch": metric(r.0).map(|m| m.1), "head_diameter": r.1, "head_height": r.2, "socket": r.3, "socket_depth": r.4})).collect(),
            "countersunk_socket_screw" => CSK.iter().map(|r| json!({"size": r.0, "d": metric(r.0).map(|m| m.0), "pitch": metric(r.0).map(|m| m.1), "head_diameter": r.1, "head_height": r.2, "socket": r.3, "socket_depth": r.4})).collect(),
            "low_head_cap_screw" => LOW_HEAD.iter().map(|r| json!({"size": r.0, "d": metric(r.0).map(|m| m.0), "pitch": metric(r.0).map(|m| m.1), "head_diameter": r.1, "head_height": r.2, "socket": r.3, "socket_depth": r.4})).collect(),
            "pan_head_screw" => PAN.iter().map(|r| json!({"size": r.0, "d": metric(r.0).map(|m| m.0), "pitch": metric(r.0).map(|m| m.1), "head_diameter": r.1, "head_height": r.2})).collect(),
            "cheese_head_screw" => CHEESE.iter().map(|r| json!({"size": r.0, "d": metric(r.0).map(|m| m.0), "pitch": metric(r.0).map(|m| m.1), "head_diameter": r.1, "head_height": r.2})).collect(),
            "countersunk_screw" => CSK_FLAT.iter().map(|r| json!({"size": r.0, "d": metric(r.0).map(|m| m.0), "pitch": metric(r.0).map(|m| m.1), "head_diameter": r.1, "head_height": metric(r.0).map(|m| tidy((r.1 - m.0) / 2.0))})).collect(),
            "hex_bolt" => HEX_BOLT.iter().map(|r| json!({"size": r.0, "d": metric(r.0).map(|m| m.0), "pitch": metric(r.0).map(|m| m.1), "across_flats": r.1, "head_height": r.2})).collect(),
            "hex_nut" => HEX_NUT.iter().map(|r| json!({"size": r.0, "d": metric(r.0).map(|m| m.0), "pitch": metric(r.0).map(|m| m.1), "across_flats": r.1, "height": r.2})).collect(),
            "washer" => WASHER.iter().map(|r| json!({"size": r.0, "inner": r.1, "outer": r.2, "thickness": r.3})).collect(),
            "set_screw" => SET_SCREW.iter().map(|r| json!({"size": r.0, "d": metric(r.0).map(|m| m.0), "pitch": metric(r.0).map(|m| m.1), "socket": r.1, "socket_depth": r.2})).collect(),
            "dowel_pin" => DOWEL.iter().map(|d| json!({"size": format!("{d}"), "d": d})).collect(),
            "bearing" => BEARINGS.iter().map(|r| json!({"size": r.0, "bore": r.1, "outer": r.2, "width": r.3})).collect(),
            "hex_cap_screw_inch" => INCH_HEX_CAP
                .iter()
                .flat_map(|r| unified_sizes(r.0).into_iter().map(move |t| json!({"size": t.designation, "d": t.major_diameter, "pitch": t.pitch, "across_flats": r.1 * INCH, "head_height": r.2 * INCH})))
                .collect(),
            "hex_nut_inch" => INCH_HEX_NUT
                .iter()
                .flat_map(|r| unified_sizes(r.0).into_iter().map(move |t| json!({"size": t.designation, "d": t.major_diameter, "pitch": t.pitch, "across_flats": r.1 * INCH, "height": r.2 * INCH})))
                .collect(),
            "washer_sae" | "washer_uss" => {
                let table = if id == "washer_sae" { &WASHER_SAE } else { &WASHER_USS };
                table.iter().map(|r| json!({"size": r.0, "inner": r.1 * INCH, "outer": r.2 * INCH, "thickness": r.3 * INCH})).collect()
            }
            _ => Vec::new(),
        };
        let (_, name, standard) = FAMILIES.iter().find(|f| f.0 == id).copied().unwrap_or(("", "", ""));
        let lengths = match id {
            "socket_head_cap_screw"
            | "button_head_cap_screw"
            | "countersunk_socket_screw"
            | "low_head_cap_screw"
            | "pan_head_screw"
            | "cheese_head_screw"
            | "countersunk_screw"
            | "hex_bolt"
            | "set_screw"
            | "dowel_pin" => Some(LENGTHS.to_vec()),
            "hex_cap_screw_inch" => Some(inch_lengths()),
            _ => None,
        };
        if lengths.is_some() {
            for row in &mut sizes {
                let Some(size) = row.get("size").and_then(Value::as_str).map(str::to_string) else { continue };
                let Some(l) = shape(id, &size, None).ok().as_ref().and_then(shape_len) else { continue };
                if let Value::Object(m) = row {
                    m.insert("default_length".into(), json!(l));
                }
            }
        }
        let system = if is_inch(id) { "inch" } else { "metric" };
        let list = |items: &[&str]| (!items.is_empty()).then(|| items.iter().map(|x| x.to_string()).collect::<Vec<_>>());
        let fine: std::collections::BTreeMap<String, Vec<f64>> = if threaded(id) {
            sizes
                .iter()
                .filter_map(|row| row.get("size").and_then(Value::as_str))
                .filter(|size| !fine_pitches(size).is_empty())
                .map(|size| (size.to_string(), fine_pitches(size).to_vec()))
                .collect()
        } else {
            Default::default()
        };
        PartFamily {
            family: id.into(),
            name: name.into(),
            standard: standard.into(),
            system: system.into(),
            sizes,
            lengths,
            drives: list(drives(id)),
            threads: if threaded(id) { list(&["cosmetic", "modeled", "none"]) } else { None },
            fine_pitches: (!fine.is_empty()).then_some(fine),
            points: if id == "set_screw" { list(&["flat", "cone", "cup"]) } else { None },
        }
    };
    match str_(p, "family") {
        Some(f) if FAMILIES.iter().any(|x| x.0 == f) => {
            reply("parts.library", PartsLibrary { families: None, family: Some(fam(f)), param_warnings: Vec::new() })
        }
        Some(f) => Err(bad("parts.library", format!("unknown part family `{f}`"))),
        None => reply(
            "parts.library",
            PartsLibrary { families: Some(FAMILIES.iter().map(|f| fam(f.0)).collect()), family: None, param_warnings: Vec::new() },
        ),
    }
}

fn reply(cmd: &str, reply: impl serde::Serialize) -> Result<Value> {
    serde_json::to_value(reply).map_err(|e| bad(cmd, e.to_string()))
}

/// A part's size as its name and part number write it: with a fine pitch (`M8x1`), and `LH`
/// after it for a left-hand thread.
fn size_text(size: &str, o: &Options) -> String {
    let size = match o.pitch {
        Some(p) => format!("{size}x{p}"),
        None => size.to_string(),
    };
    if o.left_hand { format!("{size} LH") } else { size }
}

/// The standard a part is made to: a family's, by its drive and a set screw's point.
fn standard(family: &str, o: &Options) -> &'static str {
    let drive = o.drive.as_deref().unwrap_or("");
    match (family, drive, o.point.as_deref()) {
        ("socket_head_cap_screw", "hexalobular", _) => "ISO 14579",
        ("socket_head_cap_screw", _, _) => "ISO 4762",
        ("button_head_cap_screw", _, _) => "ISO 7380-1",
        ("countersunk_socket_screw", _, _) => "ISO 10642",
        ("low_head_cap_screw", _, _) => "DIN 7984",
        ("pan_head_screw", "hexalobular", _) => "ISO 14583",
        ("pan_head_screw", "slot", _) => "ISO 1580",
        ("pan_head_screw", _, _) => "ISO 7045",
        ("cheese_head_screw", _, _) => "ISO 1207",
        ("countersunk_screw", "hexalobular", _) => "ISO 14581",
        ("countersunk_screw", "slot", _) => "ISO 2009",
        ("countersunk_screw", _, _) => "ISO 7046",
        ("hex_bolt", _, _) => "ISO 4017",
        ("hex_nut", _, _) => "ISO 4032",
        ("washer", _, _) => "ISO 7089",
        ("set_screw", _, Some("cone")) => "ISO 4027",
        ("set_screw", _, Some("cup")) => "ISO 4029",
        ("set_screw", _, _) => "ISO 4026",
        ("dowel_pin", _, _) => "ISO 8734",
        ("hex_cap_screw_inch", _, _) => "ASME B18.2.1",
        ("hex_nut_inch", _, _) => "ASME B18.2.2",
        ("washer_sae" | "washer_uss", _, _) => "ANSI B18.22.1",
        _ => "",
    }
}

fn part_name(family: &str, size: &str, sh: &Shape) -> String {
    match sh {
        Shape::Screw { len, .. } | Shape::Rod { len, .. } => format!("{} {}", label(family), size_with_length(family, size, *len)),
        _ => format!("{} {size}", label(family)),
    }
}

/// A standard part's part number and description: its standard and size (`ISO 4762 M6x20`,
/// `ASME B18.2.1 1/2-13 UNC x 2`, an inch washer with its series: `ANSI B18.22.1 1/2 N`), or a
/// bearing's designation, and the family's name with the size.
fn identity(family: &str, size: &str, sh: &Shape, o: &Options) -> (String, String) {
    let size = match sh {
        Shape::Screw { len, .. } | Shape::Rod { len, .. } => size_with_length(family, size, *len),
        _ => size.to_string(),
    };
    let standard = standard(family, o);
    let series = match family {
        "washer_sae" => " N",
        "washer_uss" => " W",
        _ => "",
    };
    let name = FAMILIES.iter().find(|f| f.0 == family).map_or("Part", |f| f.1);
    let part_number = if standard.is_empty() { size.clone() } else { format!("{standard} {size}{series}") };
    (part_number, format!("{name} {size}"))
}

/// Give a standard part its part number and description; a field someone changed since the last
/// size (`was`) is theirs and stays.
fn set_identity(s: &mut Session, comp: u64, now: (String, String), was: Option<(String, String)>) {
    use solidcraft_doc::metadata::{DESCRIPTION, Owner, PART_NUMBER};
    let Some(c) = s.doc.components.iter().find(|c| c.id == comp) else { return };
    let mut m = c.metadata.clone();
    for (k, new, old) in [(PART_NUMBER, now.0, was.as_ref().map(|w| w.0.as_str())), (DESCRIPTION, now.1, was.as_ref().map(|w| w.1.as_str()))] {
        if m.text(k).is_none_or(|cur| Some(cur) == old) {
            let _ = m.set(Owner::Component, k, Value::String(new));
        }
    }
    if m != c.metadata
        && let Some(c) = s.doc_mut().components.iter_mut().find(|c| c.id == comp)
    {
        c.metadata = m;
    }
}

/// The short label a family's parts are named with: `SHCS` in `SHCS M6x20`, `CSK` in `CSK M6x20`.
fn label(family: &str) -> &'static str {
    match family {
        "socket_head_cap_screw" => "SHCS",
        "button_head_cap_screw" => "BHCS",
        "countersunk_socket_screw" => "CSK",
        "low_head_cap_screw" => "Low Head SHCS",
        "pan_head_screw" => "Pan Head Screw",
        "cheese_head_screw" => "Cheese Head Screw",
        "countersunk_screw" => "CSK Flat Head Screw",
        "hex_bolt" => "Hex Bolt",
        "hex_nut" => "Hex Nut",
        "washer" => "Washer",
        "set_screw" => "Set Screw",
        "dowel_pin" => "Dowel Pin",
        "hex_cap_screw_inch" => "Hex Cap Screw",
        "hex_nut_inch" => "Hex Nut",
        "washer_sae" => "SAE Washer",
        "washer_uss" => "USS Washer",
        _ => "Bearing",
    }
}

fn length_param(p: &Value, cmd: &str) -> Result<Option<f64>> {
    match p.get("length") {
        None | Some(Value::Null) => Ok(None),
        Some(_) => {
            let l = num(p, "length").filter(|l| l.is_finite() && *l > 0.0 && *l <= 2000.0).ok_or_else(|| bad(cmd, "`length` must be 0…2000 mm"))?;
            Ok(Some(l))
        }
    }
}

/// The options a call gives, over `base` (a new part's defaults, or what a part being resized
/// was made with): each key present replaces its option, and a `null` puts it back to default.
fn options(p: &Value, cmd: &str, base: Options) -> Result<Options> {
    let mut o = base;
    let text = |key: &str| -> Result<Option<Option<String>>> {
        match p.get(key) {
            None => Ok(None),
            Some(Value::Null) => Ok(Some(None)),
            Some(Value::String(t)) => Ok(Some(Some(t.trim().to_ascii_lowercase()))),
            Some(_) => Err(bad(cmd, format!("`{key}` must be text"))),
        }
    };
    let flag = |key: &str| -> Result<Option<bool>> {
        match p.get(key) {
            None => Ok(None),
            Some(Value::Null) => Ok(Some(false)),
            Some(Value::Bool(b)) => Ok(Some(*b)),
            Some(_) => Err(bad(cmd, format!("`{key}` must be true or false"))),
        }
    };
    if let Some(t) = text("thread")? {
        o.thread = match t.as_deref() {
            None | Some("cosmetic") => ThreadMode::Cosmetic,
            Some("modeled" | "modelled") => ThreadMode::Modeled,
            Some("none") => ThreadMode::None,
            Some(other) => return Err(bad(cmd, format!("`thread` is cosmetic, modeled or none, not `{}`", super::features::brief(other)))),
        };
    }
    if let Some(d) = text("drive")? {
        // Names people write for the same recess.
        o.drive = d.map(|d| match d.as_str() {
            "torx" | "star" | "6-lobe" | "six-lobe" => "hexalobular".to_string(),
            "phillips" | "cross recess" => "cross".to_string(),
            "slotted" => "slot".to_string(),
            "hex socket" | "socket" | "allen" => "hex".to_string(),
            _ => d,
        });
    }
    if let Some(t) = text("tip")? {
        o.point = t.filter(|t| t != "flat");
    }
    match p.get("pitch") {
        None => {}
        Some(Value::Null) => o.pitch = None,
        Some(_) => {
            let pitch = num(p, "pitch")
                .filter(|x| x.is_finite() && *x >= 0.0 && *x <= 10.0)
                .ok_or_else(|| bad(cmd, "`pitch` must be a fine pitch in mm, or 0 for coarse"))?;
            o.pitch = (pitch > 0.0).then_some(pitch);
        }
    }
    if let Some(b) = flag("left_hand")? {
        o.left_hand = b;
    }
    // `clearance` alone asks for a printed part; `print_fit: false` makes it a nominal one again.
    let clearance = match p.get("clearance") {
        None | Some(Value::Null) => None,
        Some(_) => Some(
            num(p, "clearance")
                .filter(|c| c.is_finite() && *c > 0.0 && *c <= 2.0)
                .ok_or_else(|| bad(cmd, "`clearance` must be above 0 and at most 2 mm per side"))?,
        ),
    };
    match (flag("print_fit")?, clearance) {
        (Some(false), Some(_)) => return Err(bad(cmd, "`clearance` is the print fit's: leave it out with `print_fit: false`")),
        (Some(false), None) => o.print_clearance = None,
        (_, Some(c)) => o.print_clearance = Some(c),
        (Some(true), None) => o.print_clearance = Some(o.print_clearance.unwrap_or(PRINT_CLEARANCE)),
        (None, None) => {}
    }
    Ok(o)
}

/// The joint an insert seats a part in a hole with: rigid, or one that leaves it the motion its
/// name says, about and along the hole's axis.
fn joint_kind(p: &Value, cmd: &str) -> Result<&'static str> {
    match p.get("joint") {
        None | Some(Value::Null) => Ok("rigid"),
        Some(Value::String(t)) => match t.trim().to_ascii_lowercase().as_str() {
            "rigid" => Ok("rigid"),
            "revolute" => Ok("revolute"),
            "cylindrical" => Ok("cylindrical"),
            "screw" => Ok("screw"),
            "planar" => Ok("planar"),
            other => Err(bad(cmd, format!("`joint` is rigid, revolute, cylindrical, screw or planar, not `{}`", super::features::brief(other)))),
        },
        Some(_) => Err(bad(cmd, "`joint` must be text")),
    }
}

/// Building the part and seating it runs many feature and joint commands: as in
/// [`resize_atomic`], they run on a scratch session and the insert is one undo step.
fn insert_atomic(s: &mut Session, p: &Value) -> Result<Value> {
    let mut computed = s.scratch();
    let result = insert(&mut computed, p)?;
    computed.undo.clear();
    computed.active_component = s.active_component;
    computed.active_occurrence_path = s.active_occurrence_path.clone();
    computed.active_sketch = s.active_sketch.filter(|id| computed.doc.feature(*id).is_some());
    s.accept_computed(computed);
    Ok(result)
}

/// `parts.insert`: build the part as its own component, then seat it with a rigid joint at each
/// hole picked in `at`, or place it at `point`, or leave it at the origin.
fn insert(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "parts.insert (Insert Fastener)";
    let family = str_(p, "family").ok_or_else(|| bad(cmd, "`family` is required (parts.library)"))?.to_string();
    let size = str_(p, "size").ok_or_else(|| bad(cmd, "`size` is required"))?.to_string();
    let o = options(p, cmd, Options::default())?;
    let sh = shape_with(&family, &size, length_param(p, cmd)?, &o).map_err(|e| bad(cmd, e))?;
    let joint = joint_kind(p, cmd)?;
    let pitch = match sh {
        Shape::Screw { pitch, .. } | Shape::Rod { pitch, .. } | Shape::Nut { pitch, .. } => pitch,
        Shape::Ring { .. } => None,
    };
    if joint == "screw" && pitch.is_none() {
        return Err(bad(cmd, "`joint: screw` turns a part along its thread's pitch, and this part has no metric thread: use cylindrical"));
    }
    // A pick on a hole's rim also touches its end face: step onto the wall just inside the rim.
    // Each hole's occurrence is found before the part exists, so a pick off every body fails the
    // insert before anything is made.
    let flip = p.get("flip").and_then(Value::as_bool).unwrap_or(false);
    let mut holes: Vec<(Vec3, u64)> = Vec::new();
    let mut seats: Vec<(u64, HoleWall)> = Vec::new();
    // Picks on a flat face, off every hole: the point and the face's outward normal (world).
    let mut faces: Vec<Option<Vec3>> = Vec::new();
    for a in hole_picks(p, cmd)? {
        let found = hole_wall(s, a);
        let face = if found.is_none() { outward_normal(s, a) } else { None };
        let wall = found.unwrap_or(HoleWall { at: a, seat: a, axis: None, ends: None, open: true, radius: 0.0 });
        let host = super::joints::occurrence_at(s, wall.at).ok_or_else(|| bad(cmd, "no body at `at` (pick a hole's rim or wall)"))?;
        // A rim is drawn as more than one edge, and one click on it picks them all; a hole can
        // also be picked at both its ends (#12191). The picks of one hole seat one part, at an
        // open end: seated at a blind hole's floor it would stand on its head in the material.
        match seats.iter().position(|(h, w)| *h == host && w.same_hole(&wall)) {
            Some(i) => {
                if !seats.get(i).is_some_and(|(_, w)| w.open)
                    && wall.open
                    && let (Some(seat), Some(hole)) = (seats.get_mut(i), holes.get_mut(i))
                {
                    *hole = (wall.at, host);
                    seat.1 = wall;
                }
            }
            None => {
                holes.push((wall.at, host));
                seats.push((host, wall));
                faces.push(face);
            }
        }
    }
    let shown = size_text(&size, &o);
    let name = str_(p, "name").map(str::to_string).unwrap_or_else(|| part_name(&family, &shown, &sh));
    let active = s.active_component;
    // Hardware is fitted to other parts: it starts free even as the design's first part (#2804).
    let c = run(s, "component.create", json!({"name": name, "grounded": false}))?;
    let (comp, occ) = (c["component"].as_u64().unwrap_or(0), c["occurrence"].as_u64().unwrap_or(0));
    build(s, &family, &sh, &o, None)?;
    s.doc_mut().parts.insert(comp, o.save(&family, &size, shape_len(&sh)));
    let (part_number, _) = identity(&family, &shown, &sh, &o);
    set_identity(s, comp, identity(&family, &shown, &sh, &o), None);
    run(s, "component.activate", json!({"component": active}))?;
    let mut out = InsertedPart {
        component: comp,
        occurrence: occ,
        name: name.clone(),
        part_number,
        thread: pitch.map(|_| o.thread.id().to_string()),
        pitch,
        extent: extent(&sh),
        joint: None,
        occurrences: None,
        joints: None,
        directions: Vec::new(),
        param_warnings: Vec::new(),
    };
    let left_hand = o.left_hand;
    // The part's −z (its shank) mates with the seat's outward z; flipped, its +z does.
    let part_z = if flip { [0.0, 0.0, 1.0] } else { [0.0, 0.0, -1.0] };
    if !holes.is_empty() {
        // The first hole has the placement the component came with; each further hole places the
        // same part again. All are made before any is seated, while each is still at the part's
        // own origin, where the joint below takes the part's seating point to be.
        let mut occurrences = vec![occ];
        for _ in 1..holes.len() {
            occurrences.push(
                run(s, "occurrence.copy", json!({"component": comp}))?["occurrence"]
                    .as_u64()
                    .ok_or_else(|| bad(cmd, "the copy has no occurrence"))?,
            );
        }
        let mut joints = Vec::new();
        for (i, (((a, h), o), face)) in holes.into_iter().zip(&occurrences).zip(faces).enumerate() {
            // Seat it: its axis on the hole's, its seating plane on the hole's face, the shank
            // going into the hole (the joint mates the part's −z with the face's outward z).
            let seat = if i == 0 { format!("{name} seat") } else { format!("{name} seat {}", i + 1) };
            // A countersunk head sinks into the part: the foot of its cone, not its top, goes on
            // the rim picked (where the hole's cylinder meets its countersink), moved along the
            // axis by the bore's radius over the shank's, so that its 90 degree cone lies on the
            // countersink's, whose cone is as wide as the head's at the face. The difference is
            // signed: a clearance hole is wider than the shank and the cone goes deeper, a tap
            // drill is narrower and the cone goes higher. With no bore known, there is no move.
            let at_rim = match sh {
                Shape::Screw { d, head: Head::Countersunk { k, .. }, .. } => {
                    let clearance = seats.get(i).map_or(0.0, |(_, w)| if w.radius > 0.0 { w.radius - d / 2.0 } else { 0.0 });
                    [0.0, 0.0, -k + clearance]
                }
                _ => [0.0, 0.0, 0.0],
            };
            // On a flat face: seated at the picked point, its z the face's outward normal.
            let host = match face {
                Some(n) => json!({"occurrence": h, "point": [a.x, a.y, a.z], "z": [n.x, n.y, n.z]}),
                None => json!({"occurrence": h, "circle": a}),
            };
            // A screw joint is a cylindrical one whose turn drives its travel by the thread's pitch.
            let kind = if joint == "screw" { "cylindrical" } else { joint };
            let j = run(s, "joint.create", json!({"type": kind, "a": host, "b": {"occurrence": o, "point": at_rim, "z": part_z}, "name": seat}))?;
            let j = j["joint"].as_u64().ok_or_else(|| bad(cmd, "the joint has no id"))?;
            if let (true, Some(pitch)) = (joint == "screw", pitch) {
                run(s, "joint.screw", json!({"joint": j, "pitch": pitch, "reverse": left_hand}))?;
            }
            joints.push(j);
        }
        out.joint = joints.first().copied();
        out.directions = occurrences.iter().map(|o| head_direction(s, *o)).collect();
        out.occurrences = Some(occurrences);
        out.joints = Some(joints);
    } else if let Some(pt) = p.get("point").and_then(vec3) {
        // Placed at a point, its +z along `direction`.
        let dir = p.get("direction").and_then(vec3).and_then(|d| d.normalized()).unwrap_or(Vec3::Z);
        let dir = if flip { dir * -1.0 } else { dir };
        let m = solidcraft_doc::rigid(
            Vec3::ZERO,
            Vec3::ZERO,
            Vec3::Z.cross(dir).normalized().unwrap_or(Vec3::X),
            Vec3::Z.dot(dir).clamp(-1.0, 1.0).acos(),
        );
        let mut m = m;
        m[3] = [pt.x, pt.y, pt.z, 1.0];
        if !solidcraft_doc::is_placement(&m) {
            return Err(bad(cmd, "`point` must be within 1e9 mm and `direction` finite"));
        }
        if let Some(o) = s.doc_mut().occurrences.iter_mut().find(|o| o.id == occ) {
            o.transform = m;
        }
    }
    if out.directions.is_empty() {
        out.directions = vec![head_direction(s, occ)];
    }
    reply(cmd, out)
}

/// The way a placed part's head points in the world: its +z.
fn head_direction(s: &Session, occ: u64) -> [f64; 3] {
    let z = solidcraft_doc::apply_vector(&world_of(s, occ), Vec3::Z);
    let z = z.normalized().unwrap_or(z);
    // Rounding noise reads as zero.
    let tidy = |v: f64| if v.abs() < 1e-12 { 0.0 } else { v };
    [tidy(z.x), tidy(z.y), tidy(z.z)]
}

/// An occurrence's placement in the world (the design itself: none).
fn world_of(s: &Session, occ: u64) -> solidcraft_doc::Mat {
    if occ == 0 { solidcraft_doc::IDENTITY } else { solidcraft_doc::joints::occurrence_world(&s.doc, occ) }
}

/// The outward normal (world) of the flat face at `a`, on the body the pick is on.
fn outward_normal(s: &Session, a: Vec3) -> Option<Vec3> {
    use solidcraft_doc::joints::{JointOrigin, Snap};
    let occurrence = super::joints::occurrence_at(s, a)?;
    let t = world_of(s, occurrence);
    let inv = solidcraft_doc::mat_inverse(&t)?;
    let pick = solidcraft_doc::apply_point(&inv, a);
    let st = s.model.state();
    let o = JointOrigin::new(occurrence, Snap::FaceCenter { pick, face: None, x: None });
    let m = solidcraft_doc::joints::resolve_origin(&s.doc, &st, &o).ok()?;
    let (c, n) = (Vec3::new(m[3][0], m[3][1], m[3][2]), Vec3::new(m[2][0], m[2][1], m[2][2]));
    // The nearest plane must be the one picked: the point lies on it.
    if (pick - c).dot(n).abs() > 1e-6 {
        return None;
    }
    solidcraft_doc::apply_vector(&t, n).normalized()
}

/// The most holes one insert seats a part in.
const MAX_HOLES: usize = 1000;

/// The hole picks in `at`: one point, or a list of them.
fn hole_picks(p: &Value, cmd: &str) -> Result<Vec<Vec3>> {
    let finite = |a: Vec3| a.x.is_finite() && a.y.is_finite() && a.z.is_finite();
    let one = |v: &Value| vec3(v).filter(|a| finite(*a)).ok_or_else(|| bad(cmd, "`at` must be [x,y,z] or a list of them"));
    match p.get("at") {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::Array(list)) if list.first().is_some_and(Value::is_array) || list.is_empty() => {
            if list.len() > MAX_HOLES {
                return Err(bad(cmd, format!("`at` takes at most {MAX_HOLES} holes")));
            }
            list.iter().map(one).collect()
        }
        Some(v) => Ok(vec![one(v)?]),
    }
}

/// Where a pick on a hole's rim seats a part.
#[derive(Clone, Copy, Debug)]
struct HoleWall {
    /// A point on the hole's cylindrical wall, a little in from the picked rim.
    at: Vec3,
    /// The point on the hole's axis level with `at` (the same for every pick on one rim).
    seat: Vec3,
    /// The hole's axis, when the pick is on a hole.
    axis: Option<Vec3>,
    /// The two ends of the picked cylindrical wall, on the axis.
    ends: Option<[Vec3; 2]>,
    /// The picked rim opens onto air, not onto the floor of a blind hole.
    open: bool,
    /// The wall's radius (0 when the pick is not on a hole).
    radius: f64,
}

impl HoleWall {
    /// The two picks seat on one hole: at one point, or on one axis line with walls that meet.
    /// Two blind holes drilled towards each other from opposite faces share an axis, but solid
    /// material between their walls keeps them two holes.
    fn same_hole(&self, other: &HoleWall) -> bool {
        if (self.seat - other.seat).len() < 1e-3 {
            return true;
        }
        match (self.axis, other.axis, self.ends, other.ends) {
            (Some(a), Some(b), Some(e), Some(f)) if a.cross(b).len() < 1e-6 && (other.seat - self.seat).cross(a).len() < 1e-3 => {
                let along = |p: Vec3| (p - self.seat).dot(a);
                let ([e0, e1], [f0, f1]) = (e.map(along), f.map(along));
                e0.min(e1) <= f0.max(f1) + 1e-3 && f0.min(f1) <= e0.max(e1) + 1e-3
            }
            _ => false,
        }
    }
}

/// The wall of the hole at `a`, a little in from the picked rim, and whether that rim is an open
/// end of the hole: just past it along the axis is not the body's material.
fn hole_wall(s: &Session, a: Vec3) -> Option<HoleWall> {
    // Picks are in world coordinates, including rims on already placed standard parts.
    let st = s.world_state().ok()?;
    let dirs = [Vec3::Z * -1.0, Vec3::Z, Vec3::X, Vec3::X * -1.0, Vec3::Y, Vec3::Y * -1.0];
    for step in [0.2, 0.5, 1.0, 2.0] {
        for b in &st.bodies {
            for d in dirs {
                let q = a + d * step;
                if let Some(c) = solidcraft_doc::kernel::cylinder_face_at(&b.body, q) {
                    let v = q - c.axis_point;
                    let along = v.dot(c.axis);
                    let on_wall = ((v - c.axis * along).len() - c.radius).abs() < 1e-3 && along > c.start && along < c.end;
                    if on_wall {
                        let axis: Vec3 = c.axis * 1.0;
                        // The end of the wall nearer the pick, and a point on the axis just past it.
                        let (end, out) = if along - c.start < c.end - along { (c.start, -1.0) } else { (c.end, 1.0) };
                        let past = c.axis_point + axis * (end + out * (c.end - c.start).min(1.0) * 0.25);
                        let open = !matches!(b.body.classify_point(past), Ok(solidcraft_doc::kernel::PointClass::Inside));
                        let ends = [c.axis_point + axis * c.start, c.axis_point + axis * c.end];
                        return Some(HoleWall {
                            at: q,
                            seat: c.axis_point + axis * along,
                            axis: Some(axis),
                            ends: Some(ends),
                            open,
                            radius: c.radius,
                        });
                    }
                }
            }
        }
    }
    None
}

/// How far a part reaches along its own z: [its lowest point, its top].
fn extent(sh: &Shape) -> [f64; 2] {
    match *sh {
        Shape::Screw { len, head, .. } => match head {
            Head::Countersunk { .. } => [-len, 0.0],
            Head::Round { k, .. } | Head::Dome { k, .. } | Head::Hex { k, .. } | Head::Pan { k, .. } => [-len, k],
        },
        Shape::Nut { m, .. } => [0.0, m],
        Shape::Ring { h, .. } => [0.0, h],
        Shape::Rod { len, .. } => [-len, 0.0],
    }
}

fn shape_len(sh: &Shape) -> Option<f64> {
    match sh {
        Shape::Screw { len, .. } | Shape::Rod { len, .. } => Some(*len),
        _ => None,
    }
}

/// Building the replacement runs several feature commands. Keep their intermediate history
/// and any failed build on a scratch session; the outer command records one complete undo.
fn resize_atomic(s: &mut Session, p: &Value) -> Result<Value> {
    let mut computed = s.scratch();
    let result = resize(&mut computed, p)?;
    computed.undo.clear();
    computed.active_component = s.active_component;
    computed.active_occurrence_path = s.active_occurrence_path.clone();
    computed.active_sketch = s.active_sketch.filter(|id| computed.doc.feature(*id).is_some());
    s.accept_computed(computed);
    Ok(result)
}

fn resize(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "parts.resize";
    let key = match p.get("component") {
        Some(Value::Number(n)) => n.to_string(),
        Some(Value::String(x)) => x.clone(),
        _ => return Err(bad(cmd, "`component` must be an id or a name")),
    };
    let comp =
        s.doc.resolve_component(&key).map_err(|error| bad(cmd, error.to_string()))?.ok_or_else(|| bad(cmd, format!("no component `{key}`")))?;
    let part = s.doc.parts.get(&comp).cloned().ok_or_else(|| bad(cmd, "that component is not a standard part"))?;
    let size = str_(p, "size").map(str::to_string).unwrap_or(part.size.clone());
    let length = length_param(p, cmd)?.or(if size == part.size { part.length } else { None });
    let was_options = Options::of(&part);
    let mut o = options(p, cmd, was_options.clone())?;
    // A fine pitch is a size's own: a new size starts coarse unless the call names a pitch.
    if size != part.size && p.get("pitch").is_none() {
        o.pitch = None;
    }
    let sh = shape_with(&part.family, &size, length, &o).map_err(|e| bad(cmd, e))?;
    // Later features can use the body by name. Build its replacement at the original slot
    // and give the new producer that name before evaluating the rest of the timeline.
    let features: Vec<_> = s.doc.features.iter().enumerate().filter(|(_, f)| f.component == comp).collect();
    let position = features.first().map(|(index, _)| *index);
    let body_name = features.iter().flat_map(|(_, f)| &f.body_names).find(|name| !name.trim().is_empty()).cloned();
    let count = features.len();
    let before_marker = s.doc.marker.map_or(0, |marker| features.partition_point(|(index, _)| *index < marker));
    let ids: Vec<_> = features.into_iter().map(|(_, f)| f.id).collect();
    for id in ids.into_iter().rev() {
        let _ = s.doc_mut().delete_feature(id);
    }
    let marker = s.doc.marker;
    let position = position.or(marker).unwrap_or(s.doc.features.len());
    s.doc_mut().marker = Some(position);
    let active = s.active_component;
    run(s, "component.activate", json!({"component": comp}))?;
    build(s, &part.family, &sh, &o, body_name.as_deref())?;
    let added = s.doc.marker.unwrap_or(position).saturating_sub(position);
    // Keep the rollback boundary among the surviving features. If it was inside the
    // part's feature block, leave the same number of replacement features active.
    s.doc_mut().marker = marker.map(|marker| {
        let active = if marker > position || before_marker == count { added } else { before_marker.min(added) };
        marker.saturating_add(active)
    });
    let shown = size_text(&size, &o);
    let name = part_name(&part.family, &shown, &sh);
    let was = shape_with(&part.family, &part.size, part.length, &was_options)
        .ok()
        .map(|old| identity(&part.family, &size_text(&part.size, &was_options), &old, &was_options));
    set_identity(s, comp, identity(&part.family, &shown, &sh, &o), was);
    s.doc_mut().parts.insert(comp, o.save(&part.family, &size, shape_len(&sh)));
    if let Some(c) = s.doc_mut().components.iter_mut().find(|c| c.id == comp) {
        c.name = name.clone();
    }
    run(s, "component.activate", json!({"component": active}))?;
    s.refresh();
    if let Some(e) = s.model.results.iter().find_map(|r| r.error.clone()) {
        return Err(EngineError::Other(e));
    }
    let threaded = matches!(sh, Shape::Screw { pitch: Some(_), .. } | Shape::Rod { pitch: Some(_), .. } | Shape::Nut { pitch: Some(_), .. });
    reply(
        cmd,
        ResizedPart {
            component: comp,
            name,
            size,
            length: shape_len(&sh),
            thread: threaded.then(|| o.thread.id().to_string()),
            param_warnings: Vec::new(),
        },
    )
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};
    use solidcraft_geom::prelude::*;

    use crate::Session;

    fn run(s: &mut Session, id: &str, p: Value) -> Value {
        match s.execute(id, &p) {
            Ok(v) => v,
            Err(e) => panic!("{id} {p}: {e}"),
        }
    }

    fn bbox(s: &Session, prefix: &str) -> (solidcore_record::Vec3, solidcore_record::Vec3) {
        let st = s.world_state().unwrap();
        let comp = s.doc.components.iter().find(|c| c.name.starts_with(prefix)).map(|c| c.id).unwrap_or(0);
        let b = st
            .bodies
            .iter()
            .filter(|b| s.doc.model_body_component(b) == comp)
            .fold(solidcraft_geom::Aabb3::EMPTY, |a, b| a.union(&b.mesh().bounds()));
        (b.min, b.max)
    }

    #[test]
    fn dimensions_follow_the_standard_tables() {
        let l = run(&mut Session::default(), "parts.library", json!({"family": "socket_head_cap_screw"}));
        let m6 = l["sizes"].as_array().and_then(|a| a.iter().find(|x| x["size"] == "M6")).cloned().unwrap_or_default();
        assert_eq!((m6["head_diameter"].as_f64(), m6["head_height"].as_f64(), m6["socket"].as_f64()), (Some(10.0), Some(6.0), Some(5.0)));
        // An M6 x 20 cap screw: head Ø10 x 6, shank 20 long.
        let mut s = Session::default();
        run(&mut s, "parts.insert", json!({"family": "socket_head_cap_screw", "size": "M6", "length": 20}));
        let (lo, hi) = bbox(&s, "SHCS M6x20");
        assert!((hi.x - lo.x - 10.0).abs() < 1e-6 && (lo.z + 20.0).abs() < 1e-6 && (hi.z - 6.0).abs() < 1e-6, "{lo:?} {hi:?}");
        assert!(s.model.state().threads.iter().any(|t| t.designation.starts_with("M6")), "{:?}", s.model.state().threads);
        // An M8 nut: 13 across flats (14.43 across corners), 6.8 high.
        let mut n = Session::default();
        run(&mut n, "parts.insert", json!({"family": "hex_nut", "size": "M8"}));
        let (lo, hi) = bbox(&n, "Hex Nut M8");
        let (w, h) = ((hi.x - lo.x).max(hi.y - lo.y), hi.z - lo.z);
        assert!((w - 13.0 / (std::f64::consts::PI / 6.0).cos()).abs() < 1e-6 && (h - 6.8).abs() < 1e-6, "{w} {h}");
        // A 608 bearing: 8 x 22 x 7.
        let mut b = Session::default();
        run(&mut b, "parts.insert", json!({"family": "bearing", "size": "608"}));
        let (lo, hi) = bbox(&b, "Bearing 608");
        assert!((hi.x - lo.x - 22.0).abs() < 1e-6 && (hi.z - lo.z - 7.0).abs() < 1e-6);
        assert!(b.execute("parts.insert", &json!({"family": "hex_nut", "size": "M7"})).is_err());
    }

    /// The Length list of a dialog opens on the length an insert gets when none is asked for,
    /// so the library reports that length for every size of every family that has lengths. The
    /// rule differs by family, so the reported value is the one the part is actually built to.
    #[test]
    fn the_library_reports_the_default_length_of_every_size() {
        let all = run(&mut Session::default(), "parts.library", json!({}));
        let families = all["families"].as_array().cloned().unwrap_or_default();
        assert!(!families.is_empty(), "{all}");
        for fam in families {
            let id = fam["family"].as_str().unwrap_or_default().to_string();
            let standard: Vec<f64> = fam["lengths"].as_array().map(|a| a.iter().filter_map(Value::as_f64).collect()).unwrap_or_default();
            for row in fam["sizes"].as_array().cloned().unwrap_or_default() {
                let size = row["size"].as_str().unwrap_or_default().to_string();
                let reported = row["default_length"].as_f64();
                if standard.is_empty() {
                    assert!(reported.is_none(), "{id} {size} has no lengths of its own but reports a default");
                    continue;
                }
                // What an insert that asks for no length is actually built to.
                let mut s = Session::default();
                let out = run(&mut s, "parts.insert", json!({"family": id, "size": size}));
                let built = s.doc.parts.get(&out["component"].as_u64().unwrap_or(0)).and_then(|p| p.length);
                assert_eq!(reported, built, "{id} {size}: reported default is not what an insert builds");
                assert!(standard.iter().any(|l| Some(*l) == reported), "{id} {size}: {reported:?} is not one of the standard lengths");
            }
        }
    }

    /// #10316: ASME/ANSI inch hex cap screws, hex nuts and SAE/USS washers, tabled in inches and
    /// built in millimetres, with sizes from the UNC/UNF thread catalogue and fractional-inch names.
    #[test]
    fn inch_parts_follow_the_asme_and_ansi_tables() {
        const IN: f64 = 25.4;
        let across_corners = |af: f64| af / (std::f64::consts::PI / 6.0).cos();
        let part_number = |s: &Session, out: &Value| {
            let comp = out["component"].as_u64().unwrap_or(0);
            s.doc.components.iter().find(|c| c.id == comp).and_then(|c| c.metadata.text(solidcraft_doc::metadata::PART_NUMBER)).map(str::to_string)
        };
        // Every nominal size 1/4 to 1 but 9/16, coarse and fine.
        let l = run(&mut Session::default(), "parts.library", json!({"family": "hex_cap_screw_inch"}));
        let sizes: Vec<&str> = l["sizes"].as_array().map(|a| a.iter().filter_map(|x| x["size"].as_str()).collect()).unwrap_or_default();
        assert_eq!(sizes.len(), 18, "{sizes:?}");
        assert_eq!(sizes[..2], ["1/4-20 UNC", "1/4-28 UNF"]);
        assert_eq!(sizes[16..], ["1-8 UNC", "1-12 UNF"]);
        assert_eq!((l["standard"].as_str(), l["system"].as_str()), (Some("ASME B18.2.1"), Some("inch")));
        let all = run(&mut Session::default(), "parts.library", json!({}));
        let inch: Vec<&str> = all["families"]
            .as_array()
            .map(|a| a.iter().filter(|f| f["system"] == "inch").filter_map(|f| f["family"].as_str()).collect())
            .unwrap_or_default();
        assert_eq!(inch, ["hex_cap_screw_inch", "hex_nut_inch", "washer_sae", "washer_uss"]);
        let lengths: Vec<f64> = l["lengths"].as_array().map(|a| a.iter().filter_map(Value::as_f64).collect()).unwrap_or_default();
        assert_eq!((lengths.first().copied(), lengths.last().copied(), lengths.len()), (Some(0.5 * IN), Some(8.0 * IN), 19));
        // A 1/2-13 x 2 cap screw: 3/4 across flats, head 0.312 high, shank 2 in long, no thread mark.
        let mut s = Session::default();
        let out = run(&mut s, "parts.insert", json!({"family": "hex_cap_screw_inch", "size": "1/2-13 UNC", "length": 2.0 * IN}));
        assert_eq!(out["name"], "Hex Cap Screw 1/2-13 UNC x 2");
        assert_eq!(part_number(&s, &out).as_deref(), Some("ASME B18.2.1 1/2-13 UNC x 2"));
        let (lo, hi) = bbox(&s, "Hex Cap Screw 1/2-13 UNC x 2");
        let w = (hi.x - lo.x).max(hi.y - lo.y);
        assert!((w - across_corners(0.75 * IN)).abs() < 1e-6 && (lo.z + 2.0 * IN).abs() < 1e-6 && (hi.z - 0.312 * IN).abs() < 1e-6, "{lo:?} {hi:?}");
        assert!(s.model.state().threads.is_empty(), "solid.thread takes only metric designations (#9571)");
        // Its default length is the first standard length at or above 2.5 d: 1-1/4 for a 1/2.
        let mut d = Session::default();
        assert_eq!(
            run(&mut d, "parts.insert", json!({"family": "hex_cap_screw_inch", "size": "1/2-20 UNF"}))["name"],
            "Hex Cap Screw 1/2-20 UNF x 1-1/4"
        );
        // A 1/2-13 nut: 3/4 across flats, 0.438 thick, with the 1/2 in hole through it.
        let mut n = Session::default();
        let out = run(&mut n, "parts.insert", json!({"family": "hex_nut_inch", "size": "1/2-13 UNC"}));
        assert_eq!(part_number(&n, &out).as_deref(), Some("ASME B18.2.2 1/2-13 UNC"));
        let (lo, hi) = bbox(&n, "Hex Nut 1/2-13 UNC");
        let (w, h) = ((hi.x - lo.x).max(hi.y - lo.y), hi.z - lo.z);
        assert!((w - across_corners(0.75 * IN)).abs() < 1e-6 && (h - 0.438 * IN).abs() < 1e-6, "{w} {h}");
        // 1/2 washers: SAE 1.062 x 0.095, USS 1.375 x 0.109, the series in the part number.
        for (family, label, od, t, number) in [
            ("washer_sae", "SAE Washer 1/2", 1.062, 0.095, "ANSI B18.22.1 1/2 N"),
            ("washer_uss", "USS Washer 1/2", 1.375, 0.109, "ANSI B18.22.1 1/2 W"),
        ] {
            let mut w = Session::default();
            let out = run(&mut w, "parts.insert", json!({"family": family, "size": "1/2"}));
            assert_eq!(part_number(&w, &out).as_deref(), Some(number));
            let (lo, hi) = bbox(&w, label);
            assert!((hi.x - lo.x - od * IN).abs() < 1e-6 && (hi.z - lo.z - t * IN).abs() < 1e-6, "{family}: {lo:?} {hi:?}");
        }
        // Resizing keeps it inch, and sizes the tables don't have are refused.
        let r = run(&mut s, "parts.resize", json!({"component": "Hex Cap Screw 1/2-13 UNC x 2", "size": "3/8-16 UNC", "length": 1.0 * IN}));
        assert_eq!(r["name"], "Hex Cap Screw 3/8-16 UNC x 1");
        for (family, size) in [
            ("hex_cap_screw_inch", "9/16-12 UNC"),
            ("hex_cap_screw_inch", "#10-24 UNC"),
            ("hex_cap_screw_inch", "M12"),
            ("hex_cap_screw_inch", "1/2-20 UNC"),
            ("hex_nut_inch", "1/2"),
            ("washer_sae", "9/16"),
            ("washer_uss", "1/2-13 UNC"),
        ] {
            assert!(Session::default().execute("parts.insert", &json!({"family": family, "size": size})).is_err(), "{family} {size}");
        }
    }

    #[test]
    fn inch_lengths_read_as_fractions() {
        assert_eq!(super::part_length_label("hex_cap_screw_inch", 1.5 * 25.4).as_deref(), Some("1-1/2 in"));
        assert_eq!(super::part_length_label("hex_cap_screw_inch", 0.75 * 25.4).as_deref(), Some("3/4 in"));
        assert_eq!(super::part_length_label("hex_cap_screw_inch", 8.0 * 25.4).as_deref(), Some("8 in"));
        // Off the 1/64 grid, or a metric family: shown as millimetres in the design's units.
        assert_eq!(super::part_length_label("hex_cap_screw_inch", 20.0), None);
        assert_eq!(super::part_length_label("socket_head_cap_screw", 25.4), None);
        for bad in [f64::NAN, f64::INFINITY, -25.4, 0.0, 1e300] {
            assert_eq!(super::part_length_label("hex_cap_screw_inch", bad), None, "{bad}");
        }
    }

    /// #7692: a button head cap screw (ISO 7380-1) takes the same parameters as a socket head
    /// cap screw, with a domed crown in place of the cylindrical head.
    #[test]
    fn a_button_head_is_domed_over_the_shank() {
        let l = run(&mut Session::default(), "parts.library", json!({"family": "button_head_cap_screw"}));
        let m6 = l["sizes"].as_array().and_then(|a| a.iter().find(|x| x["size"] == "M6")).cloned().unwrap_or_default();
        assert_eq!((m6["head_diameter"].as_f64(), m6["head_height"].as_f64(), m6["socket"].as_f64()), (Some(10.5), Some(3.3), Some(4.0)));
        // ISO 7380-1 has no M2, M2.5, M14 or larger than M16; the lengths are a screw's.
        let sizes: Vec<&str> = l["sizes"].as_array().map(|a| a.iter().filter_map(|x| x["size"].as_str()).collect()).unwrap_or_default();
        assert_eq!(sizes, ["M3", "M4", "M5", "M6", "M8", "M10", "M12", "M16"]);
        assert!(l["lengths"].as_array().is_some_and(|a| !a.is_empty()), "{l}");
        // An M6 x 20 button head: head Ø10.5, 3.3 high over the seating plane, shank 20 long.
        let mut s = Session::default();
        run(&mut s, "parts.insert", json!({"family": "button_head_cap_screw", "size": "M6", "length": 20}));
        let (lo, hi) = bbox(&s, "BHCS M6x20");
        assert!((hi.x - lo.x - 10.5).abs() < 1e-3 && (lo.z + 20.0).abs() < 1e-6 && (hi.z - 3.3).abs() < 1e-3, "{lo:?} {hi:?}");
        assert!(s.model.state().threads.iter().any(|t| t.designation.starts_with("M6")), "{:?}", s.model.state().threads);
        // The profile: full width at the rim and up the flank (0.15 k = 0.50 high), drawing in
        // over the crown (r = 4.62 halfway up the head), and a flat top of 0.7 s = 2.8, which
        // leaves a ring of material around the socket's corners at r = 2.31.
        let st = s.world_state().unwrap();
        let widest = |above: f64| {
            st.bodies.iter().flat_map(|b| b.mesh().positions.clone()).filter(|p| p.z > above).map(|p| p.x.hypot(p.y)).fold(0.0, f64::max)
        };
        assert!((5.2..5.26).contains(&widest(-0.1)), "the rim: {}", widest(-0.1));
        assert!((5.2..5.26).contains(&widest(0.3)), "the flank: {}", widest(0.3));
        assert!((3.5..4.7).contains(&widest(1.65)), "the crown halfway up: {}", widest(1.65));
        assert!((2.6..2.9).contains(&widest(3.29)), "the flat top: {}", widest(3.29));
        // Its part number is the standard and size, as the other families' are.
        let comp = s.doc.components.iter().find(|c| c.name.starts_with("BHCS")).map(|c| c.id).unwrap_or(0);
        let number = s.doc.components.iter().find(|c| c.id == comp).and_then(|c| c.metadata.text(solidcraft_doc::metadata::PART_NUMBER));
        assert_eq!(number, Some("ISO 7380-1 M6x20"));
    }

    /// #13361: a countersunk socket screw (ISO 10642) takes the same parameters as the other
    /// screws, with a 90° cone sunk below its seating plane: the top of the head is level with
    /// the face, and the length counts the head.
    #[test]
    fn a_countersunk_head_is_a_cone_sunk_below_the_seating_plane() {
        let l = run(&mut Session::default(), "parts.library", json!({"family": "countersunk_socket_screw"}));
        let row = |size: &str| l["sizes"].as_array().and_then(|a| a.iter().find(|x| x["size"] == size)).cloned().unwrap_or_default();
        let m6 = row("M6");
        let dims = [&m6["head_diameter"], &m6["head_height"], &m6["socket"], &m6["socket_depth"]].map(|v| v.as_f64());
        assert_eq!(dims, [Some(13.44), Some(3.72), Some(4.0), Some(2.2)]);
        // M2 and M2.5 are left out (one source only), and the cone is 90° at every size: k is half
        // the way from the shank to dk.
        let sizes: Vec<&str> = l["sizes"].as_array().map(|a| a.iter().filter_map(|x| x["size"].as_str()).collect()).unwrap_or_default();
        assert_eq!(sizes, ["M3", "M4", "M5", "M6", "M8", "M10", "M12", "M14", "M16", "M20"]);
        for size in sizes {
            let (d, dk, k) = (
                row(size)["d"].as_f64().unwrap_or(0.0),
                row(size)["head_diameter"].as_f64().unwrap_or(0.0),
                row(size)["head_height"].as_f64().unwrap_or(0.0),
            );
            assert!(((dk - d) / 2.0 - k).abs() < 0.006, "{size}: dk {dk}, d {d}, k {k}");
        }
        assert!(l["lengths"].as_array().is_some_and(|a| !a.is_empty()), "{l}");
        // An M6 x 20: head Ø13.44 at the top, which is z = 0; the whole 20 hangs below it.
        let mut s = Session::default();
        run(&mut s, "parts.insert", json!({"family": "countersunk_socket_screw", "size": "M6", "length": 20}));
        let (lo, hi) = bbox(&s, "CSK M6x20");
        assert!((hi.x - lo.x - 13.44).abs() < 1e-3 && (lo.z + 20.0).abs() < 1e-6 && hi.z.abs() < 1e-6, "{lo:?} {hi:?}");
        assert!(s.model.state().threads.iter().any(|t| t.designation.starts_with("M6")), "{:?}", s.model.state().threads);
        // The profile: the rim at the top, the shank (Ø6) below the cone, and the cone's foot
        // 3.72 down, where it meets the shank.
        let st = s.world_state().unwrap();
        let points: Vec<_> = st.bodies.iter().flat_map(|b| b.mesh().positions.clone()).collect();
        let radius = |p: &solidcore_record::Vec3| p.x.hypot(p.y);
        let rim = points.iter().filter(|p| p.z > -0.01).map(radius).fold(0.0, f64::max);
        let shank = points.iter().filter(|p| p.z < -3.8).map(radius).fold(0.0, f64::max);
        // The cone's foot is the highest ring of the shank's radius below the top; the mesh
        // splits the cone itself into rings of its own, so the cone is not measured by vertices.
        let foot = points.iter().filter(|p| (radius(p) - 3.0).abs() < 1e-3 && p.z < -0.5).map(|p| p.z).fold(f64::MIN, f64::max);
        assert!((rim - 6.72).abs() < 1e-3, "the rim: {rim}");
        assert!((shank - 3.0).abs() < 1e-3, "the shank: {shank}");
        assert!((foot + 3.72).abs() < 1e-3, "the cone's foot: {foot}");
        let comp = s.doc.components.iter().find(|c| c.name.starts_with("CSK")).map(|c| c.id).unwrap_or(0);
        let number = s.doc.components.iter().find(|c| c.id == comp).and_then(|c| c.metadata.text(solidcraft_doc::metadata::PART_NUMBER));
        assert_eq!(number, Some("ISO 10642 M6x20"));
        // The length counts the head, so one no longer than the head is refused, and nothing is made.
        let mut short = Session::default();
        assert!(short.execute("parts.insert", &json!({"family": "countersunk_socket_screw", "size": "M20", "length": 8})).is_err());
        assert!(short.doc.components.is_empty());
    }

    /// #13361: seated at a countersunk hole (picked where the countersink meets the bore), the
    /// head's cone lies in the countersink and its top is level with the face, not at the bore.
    #[test]
    fn a_countersunk_screw_seats_flush_in_a_countersunk_hole() {
        let mut s = Session::default();
        run(&mut s, "solid.box", json!({"length": 40, "width": 30, "height": 10, "body_name": "Plate"}));
        let hole = json!({"position": [20, 15, 10], "type": "countersink", "diameter": 6.6, "cs_diameter": 13.44, "cs_angle": "90 deg", "depth": 10});
        run(&mut s, "solid.hole", hole);
        let r = run(&mut s, "parts.fastener", json!({"family": "countersunk_socket_screw", "size": "M6", "length": 20, "at": [23.3, 15, 6.4]}));
        assert!(r["joint"].as_u64().is_some(), "{r}");
        // The plate's face is z = 10: the head's top is there, and the 20 mm length hangs from it.
        let (lo, hi) = bbox(&s, "CSK M6x20");
        assert!((hi.z - 10.0).abs() < 0.02 && (lo.z + 10.0).abs() < 0.02, "{lo:?} {hi:?}");
        assert!((hi.x - lo.x - 13.44).abs() < 1e-3, "{lo:?} {hi:?}");
    }

    /// #13361: in a tapped countersink the bore is a tap drill, narrower than the screw's shank, so
    /// the cone sits higher on the rim than in a clearance hole, and the head is still level with
    /// the face (it was 0.6 mm off for an M8 when the difference of the radii was cut off at 0).
    #[test]
    fn a_countersunk_screw_seats_flush_in_a_tapped_countersink() {
        let mut s = Session::default();
        run(&mut s, "solid.box", json!({"length": 40, "width": 30, "height": 10, "body_name": "Plate"}));
        let hole =
            json!({"position": [20, 15, 10], "type": "countersink", "thread": "M8x1.25", "cs_diameter": 17.92, "cs_angle": "90 deg", "depth": 10});
        run(&mut s, "solid.hole", hole);
        // The tap drill is 6.8, so the bore's wall is at r = 3.4 and meets the countersink at z = 4.44.
        let r = run(&mut s, "parts.fastener", json!({"family": "countersunk_socket_screw", "size": "M8", "length": 20, "at": [23.4, 15, 4.3]}));
        assert!(r["joint"].as_u64().is_some(), "{r}");
        let (lo, hi) = bbox(&s, "CSK M8x20");
        assert!((hi.z - 10.0).abs() < 0.02 && (lo.z + 10.0).abs() < 0.02, "{lo:?} {hi:?}");
        assert!((hi.x - lo.x - 17.92).abs() < 1e-3, "{lo:?} {hi:?}");
    }

    /// #2091: the second and larger ISO sizes, M2, M2.5, M14, M24 and M30, are in the screw, bolt,
    /// nut and washer tables (they stopped at M3 and M20).
    #[test]
    fn small_and_large_iso_sizes_insert() {
        for family in ["socket_head_cap_screw", "hex_bolt", "hex_nut", "washer"] {
            let l = run(&mut Session::default(), "parts.library", json!({"family": family}));
            let sizes: Vec<&str> = l["sizes"].as_array().unwrap().iter().filter_map(|x| x["size"].as_str()).collect();
            for size in ["M2", "M2.5", "M14", "M24", "M30"] {
                assert!(sizes.contains(&size), "{family} has {size}: {sizes:?}");
            }
        }
        // An M2.5 x 8 cap screw: head Ø4.5 x 2.5, shank 8 long.
        let mut s = Session::default();
        run(&mut s, "parts.insert", json!({"family": "socket_head_cap_screw", "size": "M2.5", "length": 8}));
        let (lo, hi) = bbox(&s, "SHCS M2.5x8");
        assert!((hi.x - lo.x - 4.5).abs() < 1e-6 && (lo.z + 8.0).abs() < 1e-6 && (hi.z - 2.5).abs() < 1e-6, "{lo:?} {hi:?}");
        // An M24 nut: 36 across flats, 21.5 high.
        let mut n = Session::default();
        run(&mut n, "parts.insert", json!({"family": "hex_nut", "size": "M24"}));
        let (lo, hi) = bbox(&n, "Hex Nut M24");
        let (w, h) = ((hi.x - lo.x).max(hi.y - lo.y), hi.z - lo.z);
        assert!((w - 36.0 / (std::f64::consts::PI / 6.0).cos()).abs() < 1e-6 && (h - 21.5).abs() < 1e-6, "{w} {h}");
        // An M30 washer: Ø56 x 4.
        let mut w = Session::default();
        run(&mut w, "parts.insert", json!({"family": "washer", "size": "M30"}));
        let (lo, hi) = bbox(&w, "Washer M30");
        assert!((hi.x - lo.x - 56.0).abs() < 1e-6 && (hi.z - lo.z - 4.0).abs() < 1e-6, "{lo:?} {hi:?}");
    }

    #[test]
    fn a_screw_seats_in_a_hole_and_resizes() {
        let mut s = Session::default();
        run(&mut s, "solid.box", json!({"length": 40, "width": 30, "height": 10, "body_name": "Plate"}));
        run(&mut s, "solid.hole", json!({"position": [20, 15, 10], "diameter": 6.4}));
        // Pick the hole's rim at the top face.
        let r = run(&mut s, "parts.insert", json!({"family": "socket_head_cap_screw", "size": "M6", "length": 20, "at": [23.2, 15, 10]}));
        assert!(r["joint"].is_number(), "{r}");
        let (lo, hi) = bbox(&s, "SHCS M6x20");
        // The head sits flush on the top face, the shank down the hole's axis.
        assert!((hi.z - 16.0).abs() < 1e-6 && (lo.z + 10.0).abs() < 1e-6, "{lo:?} {hi:?}");
        assert!(((lo.x + hi.x) / 2.0 - 20.0).abs() < 1e-6 && ((lo.y + hi.y) / 2.0 - 15.0).abs() < 1e-6);
        // M8 x 25: rebuilt in place, still seated.
        let comp = r["component"].as_u64().unwrap_or(0);
        run(&mut s, "parts.resize", json!({"component": comp, "size": "M8", "length": 25}));
        let (lo, hi) = bbox(&s, "SHCS M8x25");
        assert!((hi.z - 18.0).abs() < 1e-6 && (lo.z + 15.0).abs() < 1e-6, "{lo:?} {hi:?}");
        assert!(s.model.state().threads.iter().any(|t| t.designation == "M8" && (t.pitch - 1.25).abs() < 1e-9));
    }

    /// #12393: a command that runs other commands is one undo step, and a failed one leaves the
    /// undo and redo lists as they were.
    #[test]
    fn a_command_built_from_commands_is_one_undo_step() {
        let mut s = Session::default();
        run(&mut s, "solid.box", json!({"length":40,"width":40,"height":10,"body_name":"Plate"}));
        let (undo, before) = (s.undo.len(), s.doc.clone());
        run(&mut s, "parts.insert", json!({"family":"socket_head_cap_screw","size":"M6","length":20}));
        assert_eq!(s.undo.len(), undo + 1, "parts.insert");
        run(&mut s, "edit.undo", json!({}));
        assert_eq!(s.doc, before, "one undo removes the whole screw");
        // A combine into a new component is "Combine" alone.
        run(&mut s, "solid.box", json!({"length":10,"width":10,"height":10,"corner":[50,0,0],"operation":"new","body_name":"Block"}));
        let undo = s.undo.len();
        run(&mut s, "solid.combine", json!({"target":"Plate","tools":["Block"],"operation":"join","new_component":true}));
        assert_eq!(s.undo.len(), undo + 1, "solid.combine new_component");
        // A failed command (a pick off every body): no inner step stays, and redo is kept.
        run(&mut s, "edit.undo", json!({}));
        let (undo, redo, doc) = (s.undo.len(), s.redo.len(), s.doc.clone());
        assert!(redo > 0);
        let failed = s.execute("parts.insert", &json!({"family":"socket_head_cap_screw","size":"M6","length":20,"at":[200,200,200]}));
        assert!(failed.is_err());
        assert_eq!((s.undo.len(), s.redo.len()), (undo, redo), "a failed insert leaves the history alone");
        assert_eq!(s.doc, doc);
    }

    #[test]
    fn a_screw_seats_on_a_translated_washer() {
        for z in [11.6, 11.2] {
            let mut s = Session::default();
            run(&mut s, "solid.box", json!({"length":60,"width":60,"height":10,"body_name":"Plate"}));
            run(&mut s, "solid.hole", json!({"position":[19.6,33.948,10],"diameter":8.4}));
            let washer = run(&mut s, "parts.insert", json!({"family":"washer","size":"M8","at":[23.8,33.948,10]}));
            let before = s.doc.clone();
            let undo = s.undo.len();
            // Both the top bore rim and the wall are world picks on the seated washer (#9648).
            let screw = run(&mut s, "parts.fastener", json!({"family":"socket_head_cap_screw","size":"M8","length":20,"at":[23.8,33.948,z]}));
            let joint = s.doc.assembly.joints.iter().find(|j| Some(j.id) == screw["joint"].as_u64()).unwrap();
            assert_eq!(Some(joint.a.occurrence), washer["occurrence"].as_u64());
            let (lo, hi) = bbox(&s, "SHCS M8x20");
            assert!((lo.z + 8.4).abs() < 1e-6 && (hi.z - 19.6).abs() < 1e-6, "{lo:?} {hi:?}");
            assert!(((lo.x + hi.x) / 2.0 - 19.6).abs() < 1e-6 && ((lo.y + hi.y) / 2.0 - 33.948).abs() < 1e-6, "{lo:?} {hi:?}");
            assert_eq!(s.undo.len(), undo + 1);
            run(&mut s, "edit.undo", json!({}));
            assert_eq!(s.doc, before);
        }
    }

    #[test]
    fn a_screw_seats_on_a_rotated_washer() {
        let mut s = Session::default();
        let washer = run(&mut s, "parts.insert", json!({"family":"washer","size":"M8","point":[40,50,60],"direction":[1,0,0]}));
        run(&mut s, "occurrence.ground", json!({"occurrence":washer["occurrence"],"grounded":true}));
        let screw = run(&mut s, "parts.insert", json!({"family":"socket_head_cap_screw","size":"M8","length":20,"at":[41.6,54.2,60]}));
        let joint = s.doc.assembly.joints.iter().find(|j| Some(j.id) == screw["joint"].as_u64()).unwrap();
        assert_eq!(Some(joint.a.occurrence), washer["occurrence"].as_u64());
        // The seating face is X = 41.6: the 20 mm shank runs along -X and the 8 mm head along +X.
        let (lo, hi) = bbox(&s, "SHCS M8x20");
        assert!(lo.dist(solidcore_record::Vec3::new(21.6, 43.5, 53.5)) < 1e-6, "{lo:?}");
        assert!(hi.dist(solidcore_record::Vec3::new(49.6, 56.5, 66.5)) < 1e-6, "{hi:?}");
    }

    /// #11729: placed at a point, the part's seating plane is at `point` and `direction` points
    /// from the shank's tip toward the head, as the help now says: `[0, 0, 1]` puts a screw head
    /// up with its shank down into a plate's top face, `[0, 0, -1]` upside down.
    #[test]
    fn a_part_placed_at_a_point_points_its_head_along_direction() {
        let help = super::COMMANDS.iter().find(|c| c.id == "parts.insert").map(|c| c.params).unwrap_or_default();
        assert!(help.contains("seating plane") && help.contains("from the shank's tip toward the head"), "{help}");
        for (direction, head_up) in [([0, 0, 1], true), ([0, 0, -1], false)] {
            let mut s = Session::default();
            run(
                &mut s,
                "parts.insert",
                json!({"family": "socket_head_cap_screw", "size": "M6", "length": 20, "point": [5, 5, 10], "direction": direction}),
            );
            let (lo, hi) = bbox(&s, "SHCS M6x20");
            // The 6 mm head on one side of z = 10, the 20 mm shank on the other.
            let (head, shank) = if head_up { (hi.z - 10.0, 10.0 - lo.z) } else { (10.0 - lo.z, hi.z - 10.0) };
            assert!((head - 6.0).abs() < 1e-6 && (shank - 20.0).abs() < 1e-6, "{direction:?}: {lo:?} {hi:?}");
        }
    }

    /// #12150: a fastener picked on a flat face, off any hole, seats there with its head out of
    /// the face, whichever way the face points, also on a component that is turned over; the
    /// reply says which way each head points.
    #[test]
    fn a_fastener_picked_on_a_flat_face_seats_head_out_of_it() {
        let screw = |at: Value| json!({"family": "socket_head_cap_screw", "size": "M3", "length": 8, "at": at});
        // (pick, the face normal's axis, the part's extent on it: the 3 mm head outside the face,
        // the 8 mm shank inside, the head's direction)
        let cases: [(Value, usize, (f64, f64), [f64; 3]); 3] = [
            (json!([10, 10, 10]), 2, (2.0, 13.0), [0.0, 0.0, 1.0]),
            (json!([10, 10, 0]), 2, (-3.0, 8.0), [0.0, 0.0, -1.0]),
            (json!([40, 10, 5]), 0, (32.0, 43.0), [1.0, 0.0, 0.0]),
        ];
        for (at, axis, span, direction) in cases {
            let mut s = Session::default();
            run(&mut s, "solid.box", json!({"length": 40, "width": 30, "height": 10, "body_name": "Plate"}));
            let r = run(&mut s, "parts.insert", screw(at.clone()));
            assert!(r["joint"].is_number(), "{at}: {r}");
            assert_eq!(r["directions"], json!([direction]), "{at}: {r}");
            let (lo, hi) = bbox(&s, "SHCS M3x8");
            let (lo, hi) = ([lo.x, lo.y, lo.z][axis], [hi.x, hi.y, hi.z][axis]);
            assert!((lo - span.0).abs() < 1e-6 && (hi - span.1).abs() < 1e-6, "{at}: {lo} {hi}");
        }
        // A plate component turned over: its top face now faces down, and the head goes below it.
        let mut s = Session::default();
        let plate = run(&mut s, "component.create", json!({"name": "P"}))["occurrence"].clone();
        run(&mut s, "solid.box", json!({"length": 40, "width": 30, "height": 10}));
        run(&mut s, "component.activate", json!({"component": 0}));
        run(
            &mut s,
            "occurrence.move",
            json!({"occurrence": plate, "axis": [1, 0, 0], "angle": "180 deg", "origin": [0, 0, 0], "move_grounded": true}),
        );
        let r = run(&mut s, "parts.insert", screw(json!([10, -10, -10])));
        assert_eq!(r["directions"], json!([[0.0, 0.0, -1.0]]), "{r}");
        let (lo, hi) = bbox(&s, "SHCS M3x8");
        assert!((lo.z + 13.0).abs() < 1e-6 && (hi.z + 2.0).abs() < 1e-6, "{lo:?} {hi:?}");
    }

    /// #12150: `flip` turns a seated part over (head into the hole, shank out of the face) and a
    /// part placed at a point against `direction`; the reply reports the direction chosen.
    #[test]
    fn flip_turns_an_inserted_part_over() {
        let mut s = Session::default();
        run(&mut s, "solid.box", json!({"length": 40, "width": 30, "height": 10, "body_name": "Plate"}));
        run(&mut s, "solid.hole", json!({"position": [20, 15, 10], "diameter": 6.4}));
        let seat = |flip: bool| json!({"family": "socket_head_cap_screw", "size": "M6", "length": 20, "at": [23.2, 15, 10], "flip": flip});
        let r = run(&mut s, "parts.insert", seat(false));
        assert_eq!(r["directions"], json!([[0.0, 0.0, 1.0]]), "{r}");
        run(&mut s, "edit.undo", json!({}));
        let r = run(&mut s, "parts.insert", seat(true));
        assert_eq!(r["directions"], json!([[0.0, 0.0, -1.0]]), "{r}");
        // The 6 mm head down in the hole under the top face, the 20 mm shank standing out of it.
        let (lo, hi) = bbox(&s, "SHCS M6x20");
        assert!((lo.z - 4.0).abs() < 1e-6 && (hi.z - 30.0).abs() < 1e-6, "{lo:?} {hi:?}");
        let mut s = Session::default();
        let r = run(
            &mut s,
            "parts.insert",
            json!({"family": "socket_head_cap_screw", "size": "M6", "length": 20, "point": [5, 5, 10], "direction": [0, 0, 1], "flip": true}),
        );
        assert_eq!(r["directions"], json!([[0.0, 0.0, -1.0]]), "{r}");
        let (lo, hi) = bbox(&s, "SHCS M6x20");
        assert!((lo.z - 4.0).abs() < 1e-6 && (hi.z - 30.0).abs() < 1e-6, "{lo:?} {hi:?}");
    }

    /// Several holes in one insert: one component placed and seated once per hole, in one undo
    /// step (#9341); two picks of one rim are one hole; a pick off every body fails the whole
    /// insert.
    #[test]
    fn one_insert_seats_a_screw_in_each_hole() {
        let mut s = Session::default();
        run(&mut s, "solid.box", json!({"length": 40, "width": 30, "height": 10, "body_name": "Plate"}));
        let centres = [(8.0, 8.0), (32.0, 8.0), (32.0, 22.0), (8.0, 22.0)];
        for (x, y) in centres {
            run(&mut s, "solid.hole", json!({"position": [x, y, 10], "diameter": 3.4}));
        }
        let (comps, occs, undo) = (s.doc.components.len(), s.doc.occurrences.len(), s.undo.len());
        // A hole off the plate fails the insert, and nothing is made.
        let off = json!({"family": "socket_head_cap_screw", "size": "M3", "length": 8, "at": [[9.7, 8, 10], [100, 100, 100]]});
        assert!(s.execute("parts.insert", &off).is_err());
        assert_eq!((s.doc.components.len(), s.doc.occurrences.len()), (comps, occs));
        // Each rim picked at the top face.
        // The first hole is picked twice, on both halves of its rim, as one click on a rim picks
        // every edge of it: it still gets one screw.
        let mut at: Vec<_> = centres.iter().map(|(x, y)| json!([x + 1.7, y, 10])).collect();
        at.insert(1, json!([centres[0].0 - 1.7, centres[0].1, 10]));
        let r = run(&mut s, "parts.fastener", json!({"family": "socket_head_cap_screw", "size": "M3", "length": 8, "at": at}));
        assert_eq!(s.doc.components.len(), comps + 1, "one part: {r}");
        let comp = r["component"].as_u64().unwrap_or(0);
        let placed: Vec<u64> = r["occurrences"].as_array().into_iter().flatten().filter_map(Value::as_u64).collect();
        assert_eq!(placed.len(), 4, "{r}");
        assert!(placed.iter().all(|o| s.doc.occurrences.iter().any(|x| x.id == *o && x.component == comp)), "{r}");
        assert_eq!(r["joints"].as_array().map(|j| j.iter().filter(|j| j.is_number()).count()), Some(4), "{r}");
        assert_eq!(r["joint"], r["joints"][0]);
        // Each placement sits in its own hole: the head flush on the top face, the shank down
        // the hole's axis.
        let st = s.world_state().unwrap();
        let mut seated: Vec<(f64, f64)> = Vec::new();
        for b in st.bodies.iter().filter(|b| s.doc.model_body_component(b) == comp) {
            let (lo, hi) = (b.mesh().bounds().min, b.mesh().bounds().max);
            assert!((hi.z - 13.0).abs() < 1e-6 && (lo.z - 2.0).abs() < 1e-6, "{lo:?} {hi:?}");
            seated.push(((lo.x + hi.x) / 2.0, (lo.y + hi.y) / 2.0));
        }
        assert_eq!(seated.len(), 4, "{seated:?}");
        for (x, y) in centres {
            assert!(seated.iter().any(|c| (c.0 - x).abs() < 1e-6 && (c.1 - y).abs() < 1e-6), "no screw at ({x}, {y}): {seated:?}");
        }
        // One undo takes them all away.
        assert_eq!(s.undo.len(), undo + 1);
        run(&mut s, "edit.undo", json!({}));
        assert_eq!((s.doc.components.len(), s.doc.occurrences.len()), (comps, occs));
    }

    /// #12191: a hole picked at both ends (a blind hole's rim and its floor edge, a through
    /// hole's top and bottom rims) gets one fastener, head at the open end, not a second one
    /// turned over.
    #[test]
    fn a_hole_picked_at_both_ends_gets_one_fastener_at_its_open_end() {
        let mut s = Session::default();
        run(&mut s, "solid.box", json!({"length": 40, "width": 30, "height": 20, "body_name": "Block"}));
        let blind = [(8.0, 8.0), (32.0, 8.0), (32.0, 22.0)];
        for (x, y) in blind {
            run(&mut s, "solid.hole", json!({"position": [x, y, 20], "diameter": 3.4, "depth": 10}));
        }
        let through = (8.0, 22.0);
        run(&mut s, "solid.hole", json!({"position": [through.0, through.1, 20], "diameter": 3.4}));
        // The first blind hole is picked at its floor edge first, then at its rim; the second at
        // its rim, then its floor; the third at its rim only; the through hole at both rims.
        let rim = |(x, y): (f64, f64), z: f64| json!([x + 1.7, y, z]);
        let at = vec![
            rim(blind[0], 10.0),
            rim(blind[0], 20.0),
            rim(blind[1], 20.0),
            rim(blind[1], 10.0),
            rim(blind[2], 20.0),
            rim(through, 20.0),
            rim(through, 0.0),
        ];
        let r = run(&mut s, "parts.fastener", json!({"family": "socket_head_cap_screw", "size": "M3", "length": 8, "at": at}));
        let comp = r["component"].as_u64().unwrap_or(0);
        assert_eq!(r["occurrences"].as_array().map(Vec::len), Some(4), "one screw per hole: {r}");
        let st = s.world_state().unwrap();
        let mut seated: Vec<(f64, f64)> = Vec::new();
        for b in st.bodies.iter().filter(|b| s.doc.model_body_component(b) == comp) {
            let (lo, hi) = (b.mesh().bounds().min, b.mesh().bounds().max);
            // Head on the top face (3 mm tall), shank 8 mm down the hole.
            assert!((hi.z - 23.0).abs() < 1e-6 && (lo.z - 12.0).abs() < 1e-6, "{lo:?} {hi:?}");
            seated.push(((lo.x + hi.x) / 2.0, (lo.y + hi.y) / 2.0));
        }
        assert_eq!(seated.len(), 4, "{seated:?}");
        for (x, y) in blind.into_iter().chain([through]) {
            assert!(seated.iter().any(|c| (c.0 - x).abs() < 1e-6 && (c.1 - y).abs() < 1e-6), "no screw at ({x}, {y}): {seated:?}");
        }
    }

    /// #12191: two blind holes drilled towards each other from opposite faces share an axis, but
    /// the material between them keeps them two holes: each rim picked gets its own fastener.
    #[test]
    fn coaxial_blind_holes_from_opposite_faces_get_a_fastener_each() {
        let mut s = Session::default();
        run(&mut s, "solid.box", json!({"length": 40, "width": 30, "height": 20, "body_name": "Block"}));
        run(&mut s, "solid.hole", json!({"position": [20, 15, 20], "diameter": 3.4, "depth": 6}));
        run(&mut s, "solid.hole", json!({"position": [20, 15, 0], "direction": [0, 0, 1], "diameter": 3.4, "depth": 6}));
        let at = json!([[21.7, 15, 20], [21.7, 15, 0]]);
        let r = run(&mut s, "parts.fastener", json!({"family": "socket_head_cap_screw", "size": "M3", "length": 6, "at": at}));
        assert_eq!(r["occurrences"].as_array().map(Vec::len), Some(2), "one screw per hole: {r}");
    }

    /// Every family's sketches are fully defined when the part is inserted and when it is resized.
    #[test]
    fn every_sketch_of_a_standard_part_is_fully_defined() {
        for (family, size, resized) in [
            ("socket_head_cap_screw", "M6", "M8"),
            ("button_head_cap_screw", "M6", "M8"),
            ("countersunk_socket_screw", "M6", "M8"),
            ("hex_bolt", "M6", "M8"),
            ("hex_nut", "M6", "M8"),
            ("washer", "M6", "M8"),
            ("set_screw", "M6", "M8"),
            ("dowel_pin", "6", "8"),
            ("bearing", "608", "6200"),
            ("hex_cap_screw_inch", "1/2-13 UNC", "5/8-11 UNC"),
            ("hex_nut_inch", "1/2-13 UNC", "5/8-18 UNF"),
            ("washer_sae", "1/2", "5/8"),
            ("washer_uss", "1/2", "5/8"),
        ] {
            let mut s = Session::default();
            let part = run(&mut s, "parts.insert", json!({"family": family, "size": size}));
            for step in ["inserted", "resized"] {
                let st = s.model.state();
                let free: Vec<_> =
                    st.sketches.iter().filter(|k| k.report.dof != 0 || !k.report.fully_constrained()).map(|k| (&k.name, k.report.dof)).collect();
                assert!(!st.sketches.is_empty() && free.is_empty(), "{family} {step}: {free:?}");
                assert!(s.model.results.iter().all(|r| r.error.is_none()), "{family} {step}");
                run(&mut s, "parts.resize", json!({"component": part["component"], "size": resized}));
            }
        }
    }

    /// A placed part is resized in its own frame and in millimetres, whatever the design's units.
    #[test]
    fn resizing_placed_parts_uses_local_geometry_and_millimetres_in_every_design_unit() {
        for (family, initial, resized, prefix) in [
            ("socket_head_cap_screw", "M6", "M8", "SHCS"),
            ("button_head_cap_screw", "M6", "M8", "BHCS"),
            ("countersunk_socket_screw", "M6", "M8", "CSK"),
            ("hex_bolt", "M6", "M8", "Hex Bolt"),
            ("hex_nut", "M6", "M8", "Hex Nut"),
            ("washer", "M6", "M8", "Washer"),
            ("set_screw", "M6", "M8", "Set Screw"),
            ("dowel_pin", "6", "8", "Dowel Pin"),
            ("bearing", "608", "6200", "Bearing"),
        ] {
            let mut bounds = Vec::new();
            for units in ["mm", "in"] {
                let mut s = Session::default();
                run(&mut s, "document.units", json!({"units":units}));
                let part = run(&mut s, "parts.insert", json!({"family":family,"size":initial,"length":20,"point":[40,50,60],"direction":[1,0,0]}));
                run(&mut s, "parts.resize", json!({"component":part["component"],"size":resized,"length":30}));
                assert!(s.model.results.iter().all(|r| r.error.is_none()), "{family} {units}");
                let b = bbox(&s, prefix);
                assert!(b.0.x < b.1.x && b.0.y < b.1.y && b.0.z < b.1.z, "{family} {units}: {b:?}");
                if matches!(
                    family,
                    "socket_head_cap_screw" | "button_head_cap_screw" | "countersunk_socket_screw" | "hex_bolt" | "hex_nut" | "set_screw"
                ) {
                    assert!(s.model.state().threads.iter().any(|t| t.designation == "M8" && (t.pitch - 1.25).abs() < 1e-9), "{family} {units}");
                }
                if family == "dowel_pin" {
                    assert!(b.0.dist(solidcore_record::Vec3::new(10., 46., 56.)) < 1e-6, "{b:?}");
                    assert!(b.1.dist(solidcore_record::Vec3::new(40., 54., 64.)) < 1e-6, "{b:?}");
                }
                bounds.push(b);
            }
            assert!(bounds[0].0.dist(bounds[1].0) < 1e-6 && bounds[0].1.dist(bounds[1].1) < 1e-6, "{family}: {bounds:?}");
        }
    }

    #[test]
    fn resizing_a_screw_keeps_its_body_and_timeline_position_for_combine() {
        let mut s = Session::default();
        run(&mut s, "solid.box", json!({"length":20,"width":20,"height":10,"corner":[-10,-10,0]}));
        let part = run(&mut s, "parts.insert", json!({"family":"socket_head_cap_screw","size":"M8","length":20,"point":[0,0,10]}));
        let component = part["component"].as_u64().unwrap();
        let combine = run(&mut s, "solid.combine", json!({"target":"Body1","tools":["Body2"],"operation":"cut","keep_tools":true}));
        let combine = combine["feature"].as_u64().unwrap();
        let combine_position = s.doc.feature_index(combine);
        let part_position = |s: &Session| s.doc.features.iter().position(|f| f.component == component);
        let original_position = part_position(&s);

        run(&mut s, "parts.resize", json!({"component":component,"size":"M8","length":35}));

        assert_eq!(part_position(&s), original_position);
        assert_eq!(s.doc.feature_index(combine), combine_position);
        let cut = s.model.result(combine).unwrap();
        assert!(!cut.skipped && cut.error.is_none(), "{:?}", cut.error);
        let measured = run(&mut s, "inspect.measure", json!({"bodies":["Body2"]}));
        let tip_z = measured["bodies"][0]["bbox"]["min"][2].as_f64().unwrap();
        assert!((tip_z + 25.0).abs() < 1e-6, "{measured}");
    }

    #[test]
    fn resizing_a_part_is_one_undo_and_keeps_the_active_occurrence() {
        let mut s = Session::default();
        let part = run(&mut s, "parts.insert", json!({"family":"socket_head_cap_screw","size":"M6","length":20}));
        let id = part["component"].as_u64().unwrap_or(0);
        let copied = run(&mut s, "occurrence.copy", json!({"component":id,"translate":[40,0,0]}));
        let path = vec![copied["occurrence"].as_u64().unwrap_or(0)];
        run(&mut s, "component.activate", json!({"component":id,"occurrence":copied["occurrence"],"path":path}));
        let before = s.doc.clone();
        let undo = s.undo.len();
        run(&mut s, "parts.resize", json!({"component":id,"size":"M8","length":25}));
        assert_eq!(s.undo.len(), undo + 1);
        assert_eq!(s.active_component, id);
        assert_eq!(s.active_occurrence_path, Some(path.clone()));
        let after = s.doc.clone();
        run(&mut s, "edit.undo", json!({}));
        assert_eq!(s.doc, before);
        let redo = s.redo.len();
        assert!(s.execute("parts.resize", &json!({"component":id,"length":0})).is_err());
        assert_eq!(s.doc, before);
        assert_eq!(s.redo.len(), redo);
        assert_eq!(s.active_occurrence_path, Some(path));
        run(&mut s, "edit.redo", json!({}));
        assert_eq!(s.doc, after);
    }

    #[test]
    fn resizing_a_part_at_the_history_limit_drops_only_one_old_step() {
        let mut s = Session::default();
        let part = run(&mut s, "parts.insert", json!({"family":"washer","size":"M6"}));
        let Some(snapshot) = s.undo.last().cloned() else { panic!("insert did not record history") };
        s.undo = (0..crate::MAX_UNDO)
            .map(|index| {
                let mut old = snapshot.clone();
                old.label = format!("Earlier step {index}");
                old
            })
            .collect();
        run(&mut s, "parts.resize", json!({"component":part["component"],"size":"M8"}));
        assert_eq!(s.undo.len(), crate::MAX_UNDO);
        assert_eq!(s.undo.first().map(|step| step.label.as_str()), Some("Earlier step 1"));
        assert_eq!(s.undo.last().map(|step| step.label.as_str()), Some("Change Part Size"));
    }

    fn volume(s: &mut Session) -> f64 {
        run(s, "inspect.measure", json!({}))["total"]["volume_mm3"].as_f64().unwrap_or(f64::NAN)
    }

    /// The bodies of the component whose name starts with `prefix`.
    fn bodies_of(s: &Session, prefix: &str) -> Vec<String> {
        let comp = s.doc.components.iter().find(|c| c.name.starts_with(prefix)).map(|c| c.id).unwrap_or(0);
        s.model.state().bodies.iter().filter(|b| s.doc.model_body_component(b) == comp).map(|b| b.name.clone()).collect()
    }

    /// Two parts built alike each have a cylindrical face at the same point of their own
    /// coordinates. The second one's thread went on the first one's body: each keeps its own.
    #[test]
    fn two_screws_of_one_size_each_get_their_own_thread() {
        let mut s = Session::default();
        let a = run(&mut s, "parts.insert", json!({"family": "socket_head_cap_screw", "size": "M6", "length": 20, "name": "First"}));
        let b = run(&mut s, "parts.insert", json!({"family": "socket_head_cap_screw", "size": "M6", "length": 20, "name": "Second"}));
        assert_ne!(a["component"], b["component"]);
        let (first, second) = (bodies_of(&s, "First"), bodies_of(&s, "Second"));
        assert_eq!((first.len(), second.len()), (1, 1), "{first:?} {second:?}");
        let on: Vec<String> = s.model.state().threads.iter().map(|t| t.body.clone()).collect();
        assert_eq!(on.len(), 2, "{on:?}");
        assert!(on.contains(&first[0]) && on.contains(&second[0]), "{on:?} on {first:?} and {second:?}");
    }

    /// `thread: modeled` cuts the thread into the shank instead of recording it: the screw loses
    /// less than the whole ring between its major and minor diameters over its thread, and more
    /// than a third of it. `none` leaves a plain shank and records nothing.
    #[test]
    fn a_modeled_thread_is_cut_into_the_shank() {
        let screw = |thread: &str| {
            let mut s = Session::default();
            let r = run(&mut s, "parts.insert", json!({"family": "socket_head_cap_screw", "size": "M6", "length": 20, "thread": thread}));
            (volume(&mut s), s.model.state().threads.len(), r)
        };
        let (plain, recorded, r) = screw("cosmetic");
        assert_eq!((recorded, r["thread"].as_str(), r["pitch"].as_f64()), (1, Some("cosmetic"), Some(1.0)), "{r}");
        let (bare, none, r) = screw("none");
        assert_eq!((none, r["thread"].as_str()), (0, Some("none")), "{r}");
        assert!((bare - plain).abs() < 1e-6, "{bare} {plain}");
        let (cut, modeled, r) = screw("modeled");
        assert_eq!((modeled, r["thread"].as_str()), (0, Some("modeled")), "{r}");
        // M6: the external minor diameter is d - 1.226869 P = 4.773.
        let ring = std::f64::consts::PI / 4.0 * (36.0 - 4.773 * 4.773) * 20.0;
        assert!(plain - cut > ring / 3.0 && plain - cut < ring, "{plain} - {cut} against a ring of {ring}");
        // One closed body still, as long and as wide as before.
        let m = run(&mut Session::default(), "parts.library", json!({"family": "socket_head_cap_screw"}));
        assert_eq!(m["threads"], json!(["cosmetic", "modeled", "none"]), "{m}");
    }

    /// A long screw is threaded over its standard's thread length only (ISO 4762: 2d + 12), so
    /// its plain shank keeps its full diameter and the cut stays short.
    #[test]
    fn a_long_modeled_screw_keeps_a_plain_shank_under_its_head() {
        let mut s = Session::default();
        run(&mut s, "parts.insert", json!({"family": "socket_head_cap_screw", "size": "M6", "length": 60, "thread": "modeled"}));
        let plain = std::f64::consts::PI * 9.0 * 60.0 + std::f64::consts::PI * 25.0 * 6.0 - 3f64.sqrt() / 2.0 * 25.0 * 3.0;
        let ring = std::f64::consts::PI / 4.0 * (36.0 - 4.773 * 4.773);
        let lost = plain - volume(&mut s);
        // Thread over 24 mm of the 60: more than a third of that ring, and less than all of it.
        assert!(lost > ring * 24.0 / 3.0 && lost < ring * 24.0 * 1.05, "lost {lost}; a 24 mm ring is {}", ring * 24.0);
        // Past the limit a modelled thread is refused, with the way out.
        let e = s.execute("parts.insert", &json!({"family": "set_screw", "size": "M3", "length": 60, "thread": "modeled"})).unwrap_err().to_string();
        assert!(e.contains("cosmetic") && e.contains("120 turns"), "{e}");
    }

    /// A fine pitch, a left-hand thread and a print fit are recorded on the thread, named on the
    /// part, and refused where the size or family has none.
    #[test]
    fn thread_options_reach_the_thread_and_the_part_number() {
        let mut s = Session::default();
        let r = run(&mut s, "parts.insert", json!({"family": "hex_bolt", "size": "M8", "length": 30, "pitch": 1, "left_hand": true}));
        assert_eq!(
            (r["part_number"].as_str(), r["name"].as_str(), r["pitch"].as_f64()),
            (Some("ISO 4017 M8x1 LHx30"), Some("Hex Bolt M8x1 LHx30"), Some(1.0)),
            "{r}"
        );
        let t = s.model.state().threads.first().cloned().unwrap();
        assert_eq!((t.designation.as_str(), t.pitch), ("M8x1", 1.0), "{t:?}");
        assert_eq!(format!("{:?}", t.hand), "Left", "{t:?}");
        let e = s.execute("parts.insert", &json!({"family": "hex_bolt", "size": "M6", "pitch": 0.75})).unwrap_err().to_string();
        assert!(e.contains("no fine pitch"), "{e}");
        let e = s.execute("parts.insert", &json!({"family": "hex_bolt", "size": "M10", "pitch": 0.75})).unwrap_err().to_string();
        assert!(e.contains("1.25 or 1"), "{e}");
        for (family, size) in [("washer", "M6"), ("bearing", "608"), ("dowel_pin", "6"), ("hex_cap_screw_inch", "1/4-20 UNC")] {
            let e = s.execute("parts.insert", &json!({"family": family, "size": size, "thread": "modeled"})).unwrap_err().to_string();
            assert!(e.contains("metric thread"), "{family}: {e}");
        }
        let lib = run(&mut s, "parts.library", json!({"family": "hex_bolt"}));
        assert_eq!(lib["fine_pitches"]["M10"], json!([1.25, 1.0]), "{lib}");
    }

    /// A part made to be printed: its shank and thread 0.15 mm under size per side (or the
    /// clearance given), its socket as much wider, and a nut's thread over size.
    #[test]
    fn a_print_fit_loosens_the_thread_and_widens_the_socket() {
        let mut s = Session::default();
        run(&mut s, "parts.insert", json!({"family": "socket_head_cap_screw", "size": "M6", "length": 20, "print_fit": true}));
        let t = s.model.state().threads.first().cloned().unwrap();
        assert!((t.major_diameter - 5.7).abs() < 1e-9 && (t.pitch - 1.0).abs() < 1e-9, "{t:?}");
        // Shank Ø5.7 x 20, head Ø10 x 6, socket 5.3 across flats x 3 deep.
        let expect = std::f64::consts::PI * 2.85 * 2.85 * 20.0 + std::f64::consts::PI * 25.0 * 6.0 - 3f64.sqrt() / 2.0 * 5.3 * 5.3 * 3.0;
        assert!((volume(&mut s) - expect).abs() < 1e-3, "{} against {expect}", volume(&mut s));
        let mut n = Session::default();
        run(&mut n, "parts.insert", json!({"family": "hex_nut", "size": "M6", "clearance": 0.2}));
        let t = n.model.state().threads.first().cloned().unwrap();
        assert!((t.major_diameter - 6.4).abs() < 1e-9 && t.internal, "{t:?}");
        let e = n.execute("parts.insert", &json!({"family": "hex_nut", "size": "M6", "clearance": 3})).unwrap_err().to_string();
        assert!(e.contains("clearance"), "{e}");
        let e = n.execute("parts.insert", &json!({"family": "washer", "size": "M6", "print_fit": true})).unwrap_err().to_string();
        assert!(e.contains("print_fit"), "{e}");
    }

    /// The families with heads of their own, to their tables: each is one closed body as wide as
    /// its head and as tall as its head and length.
    #[test]
    fn the_other_head_families_follow_their_tables() {
        // (family, size, length, head diameter, top z, lowest z)
        for (family, size, len, dk, top, low) in [
            ("low_head_cap_screw", "M6", 16.0, 10.0, 4.0, -16.0),
            ("pan_head_screw", "M4", 10.0, 8.0, 3.1, -10.0),
            ("cheese_head_screw", "M5", 12.0, 8.5, 3.3, -12.0),
            ("countersunk_screw", "M4", 12.0, 9.4, 0.0, -12.0),
        ] {
            let mut s = Session::default();
            let r = run(&mut s, "parts.insert", json!({"family": family, "size": size, "length": len}));
            let m = run(&mut s, "inspect.measure", json!({}));
            let b = &m["bodies"][0];
            assert_eq!((m["body_count"].as_u64(), b["closed"].as_bool()), (Some(1), Some(true)), "{family}: {m}");
            let (lo, hi) = (b["bbox"]["min"].clone(), b["bbox"]["max"].clone());
            let width = hi[0].as_f64().unwrap_or(0.0) - lo[0].as_f64().unwrap_or(0.0);
            assert!((width - dk).abs() < 1e-6, "{family}: {width} wide, not {dk}");
            assert!((lo[2].as_f64().unwrap_or(0.0) - low).abs() < 1e-6, "{family}: {lo}");
            assert_eq!(r["extent"], json!([low, top]), "{family}: {r}");
        }
        // A slotted cheese head M5 x 12: head Ø8.5 x 3.3 less a slot 1.2 wide and 1.3 deep right
        // across it, on a shank Ø5 x 12.
        let mut s = Session::default();
        let r = run(&mut s, "parts.insert", json!({"family": "cheese_head_screw", "size": "M5", "length": 12}));
        assert_eq!(r["part_number"], "ISO 1207 M5x12", "{r}");
        let (rk, n): (f64, f64) = (4.25, 1.2);
        let strip = 2.0 * (n / 2.0 * (rk * rk - n * n / 4.0).sqrt() + rk * rk * (n / (2.0 * rk)).asin());
        let expect = std::f64::consts::PI * 6.25 * 12.0 + std::f64::consts::PI * rk * rk * 3.3 - strip * 1.3;
        assert!((volume(&mut s) - expect).abs() < 1e-3, "{} against {expect}", volume(&mut s));
    }

    /// A family that comes with several drives takes one by name, with the standard that goes
    /// with it; the recess is cut from the head, and a drive the family has not got is refused
    /// with the ones it has.
    #[test]
    fn a_drive_is_chosen_by_name() {
        let vol = |family: &str, drive: Option<&str>| {
            let mut s = Session::default();
            let mut p = json!({"family": family, "size": "M5", "length": 16});
            if let Some(d) = drive {
                p["drive"] = json!(d);
            }
            let r = run(&mut s, "parts.insert", p);
            let m = run(&mut s, "inspect.measure", json!({}));
            assert_eq!((m["body_count"].as_u64(), m["bodies"][0]["closed"].as_bool()), (Some(1), Some(true)), "{family} {drive:?}");
            (m["total"]["volume_mm3"].as_f64().unwrap_or(f64::NAN), r["part_number"].as_str().unwrap_or("").to_string())
        };
        // A hexalobular socket T25 (4.5 across its lobes, 3.24 across its flutes) 2.25 deep in
        // an M5 cap screw: it removes more than the cylinder inside its flutes and less than the
        // one round its lobes.
        let (hex, number) = vol("socket_head_cap_screw", None);
        assert_eq!(number, "ISO 4762 M5x16");
        let (lobed, number) = vol("socket_head_cap_screw", Some("torx"));
        assert_eq!(number, "ISO 14579 M5x16");
        let solid = hex + 3f64.sqrt() / 2.0 * 16.0 * 2.5;
        let (inner, outer) = (std::f64::consts::PI * 1.62 * 1.62 * 2.25, std::f64::consts::PI * 2.25 * 2.25 * 2.25);
        assert!(solid - lobed > inner && solid - lobed < outer, "{} between {inner} and {outer}", solid - lobed);
        // Pan and countersunk heads: cross recess by default, hexalobular or slotted by name.
        for (family, standards) in
            [("pan_head_screw", ["ISO 7045", "ISO 14583", "ISO 1580"]), ("countersunk_screw", ["ISO 7046", "ISO 14581", "ISO 2009"])]
        {
            for (drive, standard) in [None, Some("hexalobular"), Some("slot")].into_iter().zip(standards) {
                assert_eq!(vol(family, drive).1, format!("{standard} M5x16"), "{family} {drive:?}");
            }
        }
        let mut s = Session::default();
        let e = s.execute("parts.insert", &json!({"family": "pan_head_screw", "size": "M5", "drive": "hex"})).unwrap_err().to_string();
        assert!(e.contains("cross, hexalobular, slot"), "{e}");
        let e = s.execute("parts.insert", &json!({"family": "hex_bolt", "size": "M5", "drive": "slot"})).unwrap_err().to_string();
        assert!(e.contains("one drive"), "{e}");
        let lib = run(&mut s, "parts.library", json!({"family": "pan_head_screw"}));
        assert_eq!(lib["drives"], json!(["cross", "hexalobular", "slot"]), "{lib}");
    }

    /// A set screw's point: flat as before, a 90° cone down to its flat end, or a cup.
    #[test]
    fn a_set_screw_takes_a_cone_or_cup_point() {
        let mut s = Session::default();
        let r = run(&mut s, "parts.insert", json!({"family": "set_screw", "size": "M6", "length": 10, "tip": "cone"}));
        assert_eq!(r["part_number"], "ISO 4027 M6x10", "{r}");
        // Ø6: a cylinder 7.75 long, a 90° frustum 2.25 high down to Ø1.5, less a socket 3 across
        // flats and 2 deep.
        let pi = std::f64::consts::PI;
        let frustum = pi * 2.25 / 3.0 * (9.0 + 3.0 * 0.75 + 0.75 * 0.75);
        let expect = pi * 9.0 * 7.75 + frustum - 3f64.sqrt() / 2.0 * 9.0 * 2.0;
        assert!((volume(&mut s) - expect).abs() < 1e-3, "{} against {expect}", volume(&mut s));
        let mut c = Session::default();
        let r = run(&mut c, "parts.insert", json!({"family": "set_screw", "size": "M6", "length": 10, "tip": "cup"}));
        assert_eq!(r["part_number"], "ISO 4029 M6x10", "{r}");
        let m = run(&mut c, "inspect.measure", json!({}));
        assert_eq!(m["bodies"][0]["closed"], true, "{m}");
        let e = c.execute("parts.insert", &json!({"family": "set_screw", "size": "M6", "tip": "dog"})).unwrap_err().to_string();
        assert!(e.contains("flat, cone or cup"), "{e}");
        let e = c.execute("parts.insert", &json!({"family": "hex_bolt", "size": "M6", "tip": "cone"})).unwrap_err().to_string();
        assert!(e.contains("set screw"), "{e}");
    }

    /// `joint: screw` seats the part with a cylindrical joint whose turn drives its travel by the
    /// thread's pitch: one turn of an M6 moves it 1 mm along the hole.
    #[test]
    fn a_screw_joint_travels_one_pitch_a_turn() {
        let mut s = Session::default();
        run(&mut s, "solid.box", json!({"length": 40, "width": 30, "height": 10, "body_name": "Plate"}));
        run(&mut s, "solid.hole", json!({"position": [20, 15, 10], "diameter": 6.4}));
        let r = run(
            &mut s,
            "parts.insert",
            json!({"family": "socket_head_cap_screw", "size": "M6", "length": 20, "at": [23.2, 15, 10], "joint": "screw"}),
        );
        let (_, seated) = bbox(&s, "SHCS M6x20");
        run(&mut s, "joint.drive", json!({"joint": r["joint"], "value": 360}));
        let (_, turned) = bbox(&s, "SHCS M6x20");
        assert!(((turned.z - seated.z).abs() - 1.0).abs() < 1e-6, "{} then {}", seated.z, turned.z);
        // The other joints are made as asked, and one a part can't have is refused.
        for kind in ["revolute", "cylindrical", "planar"] {
            let mut s = Session::default();
            run(&mut s, "solid.box", json!({"length": 40, "width": 30, "height": 10, "body_name": "Plate"}));
            run(&mut s, "solid.hole", json!({"position": [20, 15, 10], "diameter": 6.4}));
            let r = run(&mut s, "parts.insert", json!({"family": "socket_head_cap_screw", "size": "M6", "at": [23.2, 15, 10], "joint": kind}));
            let list = run(&mut s, "joint.list", json!({}));
            assert_eq!(list["joints"][0]["type"], kind, "{list}");
            assert!(r["joint"].is_number(), "{r}");
        }
        let e =
            s.execute("parts.insert", &json!({"family": "washer", "size": "M6", "at": [23.2, 15, 10], "joint": "screw"})).unwrap_err().to_string();
        assert!(e.contains("cylindrical"), "{e}");
    }

    /// Change Part Size keeps how a part was asked for, and changes any of it: the thread, the
    /// drive, the pitch.
    #[test]
    fn resizing_keeps_a_part_s_options_and_changes_them() {
        let mut s = Session::default();
        let r = run(&mut s, "parts.insert", json!({"family": "pan_head_screw", "size": "M5", "length": 16, "drive": "slot", "left_hand": true}));
        let comp = r["component"].as_u64().unwrap_or(0);
        let r = run(&mut s, "parts.resize", json!({"component": comp, "length": 20}));
        assert_eq!((r["name"].as_str(), r["thread"].as_str()), (Some("Pan Head Screw M5 LHx20"), Some("cosmetic")), "{r}");
        let part = s.doc.parts.get(&comp).cloned().unwrap();
        assert_eq!((part.drive.as_deref(), part.left_hand, part.length), (Some("slot"), true, Some(20.0)));
        assert_eq!(
            s.doc
                .components
                .iter()
                .find(|c| c.id == comp)
                .and_then(|c| c.metadata.text(solidcraft_doc::metadata::PART_NUMBER).map(str::to_string))
                .as_deref(),
            Some("ISO 1580 M5 LHx20")
        );
        let before = volume(&mut s);
        let r = run(&mut s, "parts.resize", json!({"component": comp, "thread": "modeled", "left_hand": false, "drive": "cross"}));
        assert_eq!((r["name"].as_str(), r["thread"].as_str()), (Some("Pan Head Screw M5x20"), Some("modeled")), "{r}");
        assert!(volume(&mut s) < before && s.model.state().threads.is_empty());
        let part = s.doc.parts.get(&comp).cloned().unwrap();
        assert_eq!((part.thread.as_deref(), part.drive.as_deref(), part.left_hand), (Some("modeled"), Some("cross"), false));
        // Saved and read back as it is.
        let text = serde_json::to_string(&part).unwrap();
        assert_eq!(serde_json::from_str::<solidcraft_doc::StandardPart>(&text).unwrap(), part, "{text}");
        assert_eq!(
            serde_json::to_value(solidcraft_doc::StandardPart::new("hex_nut", "M6", None)).unwrap(),
            json!({"family": "hex_nut", "size": "M6"})
        );
    }
}
