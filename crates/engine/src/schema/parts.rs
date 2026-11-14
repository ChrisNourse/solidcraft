//! Contracts and result types for the commands in `cmd/parts.rs`: standard parts (#11003).

use std::borrow::Cow;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use solidcore_record::Vec3;

use super::check::whole;
use super::types::Point3;
use crate::params;

/// List the standard part families, or one family's sizes with their dimensions (mm).
#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct PartsLibraryParams {
    /// One family, as the list names it (default: every family).
    #[serde(default)]
    pub family: Option<String>,
}

/// Insert a standard part as a new component: seated in holes with a rigid joint each (`at`),
/// placed at a point (`point`), or at the origin. One undo step.
#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct PartsInsertParams {
    /// The family, as `parts.library` lists it: socket_head_cap_screw, button_head_cap_screw,
    /// countersunk_socket_screw, low_head_cap_screw, pan_head_screw, cheese_head_screw,
    /// countersunk_screw, hex_bolt, hex_nut, washer, set_screw, dowel_pin, bearing,
    /// hex_cap_screw_inch, hex_nut_inch, washer_sae or washer_uss.
    #[serde(deserialize_with = "family")]
    pub family: String,
    /// The size, as `parts.library` lists it: `M6`, `608` for a bearing, `1/2-13 UNC` for an
    /// inch screw or nut, `1/2` for an inch washer.
    #[serde(deserialize_with = "size")]
    pub size: String,
    #[serde(default, deserialize_with = "length")]
    #[schemars(schema_with = "length_schema")]
    pub length: Option<f64>,
    #[serde(default)]
    pub at: Option<HolePicks>,
    /// Without `at`: where the part's origin goes, with no joint.
    #[serde(default)]
    pub point: Option<Point3>,
    /// With `point`: the direction the part's +Z turns to (default +Z), a non-zero `[x, y, z]`.
    #[serde(default)]
    pub direction: Option<Point3>,
    /// The component's name (default: the part's, such as `Socket Head Cap Screw M6x20`).
    #[serde(default)]
    pub name: Option<String>,
    /// Turn the part over from the way it is placed: with `at`, head into the material and shank
    /// out of the face; with `point`, its +Z against `direction` (default false).
    #[serde(default)]
    pub flip: Option<bool>,
    /// Place a part the design already has once more instead of building it again (default
    /// false): one the same in family, size, length and options, under the same name and with
    /// nothing placed inside it, gets one more occurrence of its component.
    #[serde(default)]
    pub share: Option<bool>,
    /// For a part with a metric thread: `cosmetic` (default: recorded on the face, the body
    /// unchanged), `modeled` (cut into the body: its standard thread length from the tip, the
    /// start chamfered; slower, and refused past 80 turns) or `none`.
    #[serde(default)]
    pub thread: Option<String>,
    /// A fine pitch in mm, as `parts.library` lists them for the size (default, or 0: coarse).
    #[serde(default)]
    pub pitch: Option<f64>,
    /// A left-hand thread (default false).
    #[serde(default)]
    pub left_hand: Option<bool>,
    /// The drive recess, where the family comes with more than one (`parts.library`: `drives`):
    /// `hex`, `hexalobular`, `cross` or `slot` (default: the family's first).
    #[serde(default)]
    pub drive: Option<String>,
    /// A set screw's point: `flat` (default), `cone` or `cup`.
    #[serde(default)]
    pub tip: Option<String>,
    /// With `at`: the joint each hole gets. `rigid` (default), `revolute` (turns in place),
    /// `cylindrical` (turns and slides along the hole), `screw` (cylindrical, its turn driving
    /// its travel by the thread's pitch) or `planar` (seated on the face, free to slide over it).
    #[serde(default)]
    pub joint: Option<String>,
}

/// Change an inserted standard part's size or length, rebuilding it in place; its joints keep it
/// seated. One undo step.
#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct PartsResizeParams {
    /// The part's component, by id or name.
    #[serde(deserialize_with = "component")]
    #[schemars(schema_with = "component_schema")]
    pub component: String,
    /// The new size (default: the part's).
    #[serde(default)]
    pub size: Option<String>,
    #[serde(default, deserialize_with = "length")]
    #[schemars(schema_with = "length_schema")]
    pub length: Option<f64>,
    /// For a part with a metric thread: `cosmetic` (default: recorded on the face, the body
    /// unchanged), `modeled` (cut into the body: its standard thread length from the tip, the
    /// start chamfered; slower, and refused past 80 turns) or `none`.
    #[serde(default)]
    pub thread: Option<String>,
    /// A fine pitch in mm, as `parts.library` lists them for the size (default, or 0: coarse).
    #[serde(default)]
    pub pitch: Option<f64>,
    /// A left-hand thread (default false).
    #[serde(default)]
    pub left_hand: Option<bool>,
    /// The drive recess, where the family comes with more than one (`parts.library`: `drives`):
    /// `hex`, `hexalobular`, `cross` or `slot` (default: the family's first).
    #[serde(default)]
    pub drive: Option<String>,
    /// A set screw's point: `flat` (default), `cone` or `cup`.
    #[serde(default)]
    pub tip: Option<String>,
}

/// The family, with the handler's message for anything that isn't text.
fn family<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    match Value::deserialize(d)? {
        Value::String(s) => Ok(s),
        _ => Err(D::Error::custom(whole("`family` is required (parts.library)"))),
    }
}

/// The size, with the handler's message for anything that isn't text.
fn size<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    match Value::deserialize(d)? {
        Value::String(s) => Ok(s),
        _ => Err(D::Error::custom(whole("`size` is required"))),
    }
}

/// A part's length, as the handler reads it: `null` is the default, else above 0 and at most
/// 2000 mm.
fn length<'de, D: Deserializer<'de>>(d: D) -> Result<Option<f64>, D::Error> {
    match Value::deserialize(d)? {
        Value::Null => Ok(None),
        v => v
            .as_f64()
            .and_then(params::finite)
            .filter(|l| *l > 0.0 && *l <= 2000.0)
            .map(Some)
            .ok_or_else(|| D::Error::custom(whole("`length` must be 0…2000 mm"))),
    }
}

fn length_schema(_: &mut SchemaGenerator) -> Schema {
    json_schema!({
        "description": "For screws, bolts, set screws and pins: the length in mm, inch parts too (default: a standard length near 2.5 diameters).",
        "type": ["number", "null"],
        "exclusiveMinimum": 0,
        "maximum": 2000
    })
}

/// A component's id or name, as the resize reads it, with its message.
fn component<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    match Value::deserialize(d)? {
        Value::Number(n) => Ok(n.to_string()),
        Value::String(s) => Ok(s),
        _ => Err(D::Error::custom("must be an id or a name")),
    }
}

fn component_schema(_: &mut SchemaGenerator) -> Schema {
    json_schema!({"description": "The part's component, by id or name.", "type": ["number", "string"]})
}

/// The most holes one insert seats a part in.
const MAX_HOLES: usize = 1000;

/// Points on the rims or walls of holes, as the insert reads them: one point, or a list of up to
/// 1000 (a list's first item is a list, or it is empty), with its messages. The part seats on
/// each hole's face with a rigid joint, placed once per hole as one component.
#[derive(Clone, Debug, PartialEq)]
pub struct HolePicks(pub Vec<Vec3>);

impl<'de> Deserialize<'de> for HolePicks {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let fail = || D::Error::custom(whole("`at` must be [x,y,z] or a list of them"));
        let one = |v: &Value| params::vec3(v).ok_or_else(fail);
        match Value::deserialize(d)? {
            Value::Array(list) if list.first().is_some_and(Value::is_array) || list.is_empty() => {
                if list.len() > MAX_HOLES {
                    return Err(D::Error::custom(whole(format!("`at` takes at most {MAX_HOLES} holes"))));
                }
                list.iter().map(one).collect::<Result<_, _>>().map(HolePicks)
            }
            v => Ok(HolePicks(vec![one(&v)?])),
        }
    }
}

impl JsonSchema for HolePicks {
    fn schema_name() -> Cow<'static, str> {
        "HolePicks".into()
    }
    fn inline_schema() -> bool {
        true
    }
    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        let point = generator.subschema_for::<Point3>();
        json_schema!({
            "description": "A point on a hole's rim or wall, or a list of them (one per hole, at most 1000): the part seats on each hole's face with a rigid joint.",
            "anyOf": [point.clone(), {"type": "array", "prefixItems": [{"allOf": [{"type": "array"}, point.clone()]}], "items": point, "maxItems": MAX_HOLES}]
        })
    }
}

/// What `parts.library` returns: every family, or the one asked for.
#[derive(Debug, Serialize, JsonSchema)]
pub struct PartsLibrary {
    /// Without `family`: every family.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub families: Option<Vec<PartFamily>>,
    /// With `family`: that family.
    #[serde(flatten)]
    pub family: Option<PartFamily>,
    /// In the warn parameter-check mode: what the call's parameters got wrong.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub param_warnings: Vec<String>,
}

/// A standard part family.
#[derive(Debug, Serialize, JsonSchema)]
pub struct PartFamily {
    /// Its id, as `parts.insert` takes it.
    pub family: String,
    pub name: String,
    /// The standard its sizes follow.
    pub standard: String,
    /// `metric` or `inch`.
    pub system: String,
    /// Each size with its dimensions in mm (which depend on the family), and `default_length`
    /// where the part has a length.
    #[schemars(schema_with = "sizes_schema")]
    pub sizes: Vec<Value>,
    /// The standard lengths (mm), for the families that have one; else `null`.
    pub lengths: Option<Vec<f64>>,
    /// The drives `parts.insert` takes as `drive`, the default first, where the family comes
    /// with more than one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub drives: Option<Vec<String>>,
    /// The ways `parts.insert` makes the thread (`thread`), for a family with a metric thread.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub threads: Option<Vec<String>>,
    /// The fine pitches (mm) `parts.insert` takes as `pitch`, by size, for the sizes that have one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fine_pitches: Option<std::collections::BTreeMap<String, Vec<f64>>>,
    /// The points `parts.insert` takes as `point`, for set screws.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub points: Option<Vec<String>>,
}

fn sizes_schema(_: &mut SchemaGenerator) -> Schema {
    json_schema!({"type": "array", "items": {"type": "object", "properties": {"size": {"type": "string"}}, "required": ["size"]}})
}

/// What `parts.insert` and `parts.fastener` return.
#[derive(Debug, Serialize, JsonSchema)]
pub struct InsertedPart {
    /// The part's component: a new one, or the one the design already had (`shared`).
    pub component: u64,
    /// The occurrence placed (with several holes, the first).
    pub occurrence: u64,
    /// The design already had this part, and it was placed again rather than built again.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub shared: bool,
    /// The component's name.
    pub name: String,
    /// The part number it was given: its standard and size, such as `ISO 4762 M6x20`.
    pub part_number: String,
    /// How its thread was made (`cosmetic`, `modeled` or `none`), for a part with a metric thread.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thread: Option<String>,
    /// Its thread's pitch (mm), for a part with a metric thread.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pitch: Option<f64>,
    /// How far the part reaches along its own Z, `[lowest, top]` in mm, with its seating plane
    /// at 0: a screw's tip and the top of its head.
    pub extent: [f64; 2],
    /// With `at`: the first hole's joint.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub joint: Option<u64>,
    /// With `at`: an occurrence per hole, in pick order.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub occurrences: Option<Vec<u64>>,
    /// With `at`: a joint per hole, in pick order.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub joints: Option<Vec<u64>>,
    /// The way each placement's head points (its +Z, from the shank's tip toward the head), in
    /// world coordinates and in pick order: the direction that was chosen.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub directions: Vec<[f64; 3]>,
    /// In the warn parameter-check mode: what the call's parameters got wrong.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub param_warnings: Vec<String>,
}

/// What `parts.resize` returns.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ResizedPart {
    pub component: u64,
    /// The component's new name.
    pub name: String,
    pub size: String,
    /// Its length (mm), for a part that has one; else `null`.
    pub length: Option<f64>,
    /// How its thread is made now (`cosmetic`, `modeled` or `none`), for a part with a metric thread.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thread: Option<String>,
    /// In the warn parameter-check mode: what the call's parameters got wrong.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub param_warnings: Vec<String>,
}

#[cfg(test)]
#[path = "parts_tests.rs"]
mod tests;
