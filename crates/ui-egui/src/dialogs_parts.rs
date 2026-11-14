//! Insert Part: the standard parts library (screws, bolts, nuts, washers, set screws, pins,
//! bearings) by family, size and length, with the size's table values and a picture of the part.
//! Picking a hole (its wall or rim) seats the part in it with a rigid joint, and picking several
//! seats the same part in each; without one the part goes at the origin. The live preview shows it
//! placed. A threaded part also takes its drive, a set screw's point, how its thread is made
//! (cosmetic, cut into the body or none), a fine pitch and a left-hand thread; seated in holes, it
//! takes the joint that holds it there. Flip turns the part over where it is placed.

use crate::ui_kit::{choice_row, text_row};
use egui::vec2;
use serde_json::{Value, json};
use solidcraft_engine::{Sel, Session};

use crate::SolveApp;
use crate::dialogs::{Kind, combo, form_row};
use crate::selection::{EDGES, FACES, SelInput};
use solidcraft_theme::Tokens;

/// One family of the library.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Family {
    pub id: String,
    pub name: String,
    pub standard: String,
    /// `metric` or `inch`: the Unit system list shows the families of one at a time.
    pub system: String,
    /// Size names and their table rows.
    pub sizes: Vec<(String, Value)>,
    pub lengths: Option<Vec<f64>>,
    /// The drives it comes with, the default first (empty: one drive only).
    pub drives: Vec<String>,
    /// It has a metric thread, which the thread options act on.
    pub threaded: bool,
    /// The fine pitches (mm) of the sizes that have one.
    pub fine: std::collections::BTreeMap<String, Vec<f64>>,
    /// A set screw's points, the default first (empty: none to choose).
    pub points: Vec<String>,
}

/// What a part is asked for beyond its size and length, as the lists and boxes of the dialog
/// hold it: each index is into the list its row shows.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PartOptions {
    pub drive: usize,
    pub tip: usize,
    /// Into [`THREADS`].
    pub thread: usize,
    /// 0: coarse; else the size's fine pitch before it.
    pub pitch: usize,
    pub left_hand: bool,
}

/// How a thread is made: the command's word and the list's label.
const THREADS: [(&str, &str); 3] = [("cosmetic", "Cosmetic thread"), ("modeled", "Modeled thread"), ("none", "No thread")];

/// The joint a part seated in a hole gets: the command's word and the list's label.
const JOINTS: [(&str, &str); 5] = [
    ("rigid", "Fixed in place"),
    ("revolute", "Turns in place"),
    ("cylindrical", "Turns and slides"),
    ("screw", "Screws in and out"),
    ("planar", "Slides over the face"),
];

fn drive_label(id: &str) -> &'static str {
    match id {
        "hex" => "Hex socket",
        "hexalobular" => "Hexalobular (six-lobe)",
        "cross" => "Cross recess",
        _ => "Slotted",
    }
}

fn tip_label(id: &str) -> &'static str {
    match id {
        "cone" => "Cone point",
        "cup" => "Cup point",
        _ => "Flat point",
    }
}

/// The joints a family's parts can be seated with: only a threaded part screws in.
fn joints(f: &Family) -> Vec<(&'static str, &'static str)> {
    JOINTS.iter().copied().filter(|j| f.threaded || j.0 != "screw").collect()
}

/// The rows for a part's options: only those its family and size have.
pub(crate) fn option_rows(ui: &mut egui::Ui, f: &Family, size: &str, o: &mut PartOptions) {
    if f.drives.len() > 1 {
        let labels: Vec<&str> = f.drives.iter().map(|d| drive_label(d)).collect();
        o.drive = o.drive.min(labels.len() - 1);
        choice_row(ui, "Drive type", "pt_drive", &mut o.drive, &labels);
    } else {
        o.drive = 0;
    }
    if f.points.len() > 1 {
        let labels: Vec<&str> = f.points.iter().map(|t| tip_label(t)).collect();
        o.tip = o.tip.min(labels.len() - 1);
        choice_row(ui, "Point type", "pt_tip", &mut o.tip, &labels);
    } else {
        o.tip = 0;
    }
    if !f.threaded {
        *o = PartOptions::default();
        return;
    }
    o.thread = o.thread.min(THREADS.len() - 1);
    choice_row(ui, "Thread type", "pt_thread", &mut o.thread, &THREADS.map(|t| t.1));
    if o.thread == 1 {
        ui.vertical(|ui| {
            crate::ui_kit::text(ui, solidcraft_ui_text::tr("parts.thread.modeled_hint"), crate::ui_kit::Text::Note("parts_modeled"));
        });
    }
    match f.fine.get(size).filter(|fine| !fine.is_empty()) {
        Some(fine) => {
            let numbers: Vec<String> = fine.iter().map(|p| format!("{p} mm")).collect();
            let labels: Vec<&str> = std::iter::once("Coarse pitch").chain(numbers.iter().map(String::as_str)).collect();
            o.pitch = o.pitch.min(labels.len() - 1);
            choice_row(ui, "Thread pitch", "pt_pitch", &mut o.pitch, &labels);
        }
        None => o.pitch = 0,
    }
    if o.thread != 2 && crate::ui_kit::checkbox_row(ui, "pt_left_hand", "Left-hand thread", o.left_hand) {
        o.left_hand = !o.left_hand;
    }
}

/// The options a part was made with, as the dialog's lists hold them.
pub(crate) fn options_of(f: &Family, part: &solidcraft_engine::doc::StandardPart) -> PartOptions {
    let index = |list: &[String], wanted: Option<&str>| wanted.and_then(|w| list.iter().position(|x| x == w)).unwrap_or(0);
    PartOptions {
        drive: index(&f.drives, part.drive.as_deref()),
        tip: index(&f.points, part.point.as_deref()),
        thread: THREADS.iter().position(|t| Some(t.0) == part.thread.as_deref()).unwrap_or(0),
        pitch: part.pitch.and_then(|p| f.fine.get(&part.size).and_then(|fine| fine.iter().position(|x| (*x - p).abs() < 1e-9))).map_or(0, |i| i + 1),
        left_hand: part.left_hand,
    }
}

/// The options as Change Part Size takes them: every one the family has, spelled out, so a
/// choice put back to its default replaces what the part was made with.
pub(crate) fn resize_option_params(f: &Family, size: &str, o: &PartOptions, p: &mut Value) {
    if f.drives.len() > 1 {
        p["drive"] = json!(f.drives.get(o.drive));
    }
    if f.points.len() > 1 {
        p["tip"] = json!(f.points.get(o.tip));
    }
    if !f.threaded {
        return;
    }
    p["thread"] = json!(THREADS.get(o.thread).map_or("cosmetic", |t| t.0));
    p["pitch"] = json!(o.pitch.checked_sub(1).and_then(|i| f.fine.get(size).and_then(|fine| fine.get(i))).copied().unwrap_or(0.0));
    p["left_hand"] = json!(o.left_hand && o.thread != 2);
}

/// The options as the commands take them: only those that are not the default.
pub(crate) fn option_params(f: &Family, size: &str, o: &PartOptions, p: &mut Value) {
    if let Some(d) = f.drives.get(o.drive).filter(|_| o.drive > 0) {
        p["drive"] = json!(d);
    }
    if let Some(t) = f.points.get(o.tip).filter(|_| o.tip > 0) {
        p["tip"] = json!(t);
    }
    if !f.threaded {
        return;
    }
    if let Some(t) = THREADS.get(o.thread).filter(|_| o.thread > 0) {
        p["thread"] = json!(t.0);
    }
    if let Some(fine) = f.fine.get(size)
        && let Some(pitch) = o.pitch.checked_sub(1).and_then(|i| fine.get(i))
    {
        p["pitch"] = json!(pitch);
    }
    if o.left_hand && o.thread != 2 {
        p["left_hand"] = json!(true);
    }
}

/// The library as `parts.library` gives it.
pub fn families(v: &Value) -> Vec<Family> {
    v.get("families")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|f| {
            let id = f.get("family")?.as_str()?.to_string();
            let sizes = f
                .get("sizes")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|s| Some((s.get("size")?.as_str().map(str::to_string).or_else(|| s.get("size").map(Value::to_string))?, s.clone())))
                .collect();
            Some(Family {
                name: f.get("name").and_then(Value::as_str).unwrap_or(&id).to_string(),
                standard: f.get("standard").and_then(Value::as_str).unwrap_or("").to_string(),
                system: f.get("system").and_then(Value::as_str).unwrap_or("metric").to_string(),
                sizes,
                lengths: f.get("lengths").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_f64).collect()),
                drives: texts(f.get("drives")),
                threaded: f.get("threads").is_some_and(Value::is_array),
                fine: f
                    .get("fine_pitches")
                    .and_then(Value::as_object)
                    .into_iter()
                    .flatten()
                    .map(|(size, pitches)| (size.clone(), pitches.as_array().into_iter().flatten().filter_map(Value::as_f64).collect()))
                    .collect(),
                points: texts(f.get("points")),
                id,
            })
        })
        .collect()
}

fn texts(v: Option<&Value>) -> Vec<String> {
    v.and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).map(str::to_string).collect()
}

/// A family's name in the user's language, by its id in the library: the library's own (English)
/// name for a family the catalogs don't know.
pub(crate) fn family_name(f: &Family) -> String {
    let id = format!("parts.family.{}", f.id);
    match solidcraft_ui_text::tr(&id) {
        shown if shown == id => f.name.clone(),
        shown => shown.to_string(),
    }
}

/// The Unit system list: each system's id in the library and its label.
const SYSTEMS: [(&str, &str); 2] = [("metric", "Metric"), ("inch", "Inch")];

#[derive(Clone, Debug, Default)]
pub struct Pt {
    pub family: usize,
    pub size: usize,
    /// Index into the family's lengths (none: the default length).
    pub length: Option<usize>,
    pub name: String,
    /// The part the picture shows, as its parameters (the texture is kept in egui's memory).
    pub picture: Option<String>,
    /// The library, read once.
    pub lib: Option<Vec<Family>>,
    pub options: PartOptions,
    /// Into the family's [`joints`]: how a part seated in a hole is held there.
    pub joint: usize,
    /// Turned over where it is placed: in a hole, the head goes in from the other way.
    pub flip: bool,
    /// A component of its own, even when the design already has this part.
    pub separate: bool,
}

/// The dialog for Insert Part.
pub fn start(_app: &SolveApp, id: &str) -> Option<(Kind, Vec<SelInput>)> {
    // Standard Parts itself only lists the library: started from the app it opens the dialog
    // that shows it (#8735), which did nothing before.
    matches!(id, "parts.fastener" | "parts.insert" | "parts.library")
        .then(|| (Kind::Part(Pt::default()), vec![SelInput::new("parts.holes", FACES | EDGES, true)]))
}

fn library(app: &mut SolveApp) -> Vec<Family> {
    app.session.execute("parts.library", &json!({})).map(|v| families(&v)).unwrap_or_default()
}

/// The insert command's parameters for the hole picks: one part seated in each.
pub fn insert_params(lib: &[Family], k: &Pt, holes: &[Sel]) -> Result<Value, String> {
    let f = lib.get(k.family).ok_or("the parts library is empty")?;
    let (size, _) = f.sizes.get(k.size).ok_or("pick a size")?;
    let mut p = json!({"family": f.id, "size": size});
    if let Some(l) = k.length.and_then(|i| f.lengths.as_ref().and_then(|ls| ls.get(i))) {
        p["length"] = json!(l);
    }
    option_params(f, size, &k.options, &mut p);
    if k.flip {
        p["flip"] = json!(true);
    }
    // The part the design already has is placed again, unless this one is to be its own.
    if !k.separate {
        p["share"] = json!(true);
    }
    let at: Vec<Value> = holes
        .iter()
        .filter_map(|h| match h {
            Sel::Face { point, .. } | Sel::Edge { point, .. } => Some(json!([point.x, point.y, point.z])),
            _ => None,
        })
        .collect();
    match at.len() {
        0 => {
            p["point"] = json!([0.0, 0.0, 0.0]);
            p["direction"] = json!([0.0, 0.0, 1.0]);
        }
        // One hole keeps the single pick's form.
        1 => p["at"] = at.into_iter().next().unwrap_or(Value::Null),
        _ => p["at"] = Value::Array(at),
    }
    if p.get("at").is_some()
        && let Some(j) = joints(f).get(k.joint).filter(|_| k.joint > 0)
    {
        p["joint"] = json!(j.0);
    }
    if !k.name.trim().is_empty() {
        p["name"] = json!(k.name.trim());
    }
    Ok(p)
}

/// A length (mm) of a part of the family: in fractional inches for an inch family, in the
/// design's units otherwise.
pub(crate) fn length_label(app: &SolveApp, family: &str, mm: f64) -> String {
    solidcraft_engine::cmd::part_length_label(family, mm).unwrap_or_else(|| crate::prefs::show_mm(app, mm, 1))
}

/// Where the Length list opens for a size: the index of the length an insert of it gets when
/// none is asked for, which `parts.library` reports beside the size's table values. The list
/// carries no item of its own for it, so a size whose default is missing opens on the first.
fn default_length(f: &Family, size: usize, lengths: &[f64]) -> usize {
    f.sizes
        .get(size)
        .and_then(|(_, row)| row.get("default_length"))
        .and_then(Value::as_f64)
        .and_then(|d| lengths.iter().position(|l| (*l - d).abs() < 1e-9))
        .unwrap_or(0)
}

/// A picture of the part alone.
fn picture(p: &Value) -> Option<egui::ColorImage> {
    let mut s = Session::default();
    let mut q = p.clone();
    if let Value::Object(m) = &mut q {
        // The picture shows the part's shape: a thread cut into it takes seconds and shows little
        // at this size.
        m.remove("thread");
        m.remove("joint");
        m.remove("flip");
        m.remove("share");
        m.remove("at");
        m.insert("point".into(), json!([0.0, 0.0, 0.0]));
        m.insert("direction".into(), json!([0.0, 0.0, 1.0]));
    }
    s.execute("parts.insert", &q).ok()?;
    crate::home::thumbnail(&s)
}

/// The dialog's rows.
pub fn rows(app: &mut SolveApp, ui: &mut egui::Ui, k: &mut Pt, inputs: &[SelInput]) -> bool {
    let t = Tokens::get();
    let lib = match &k.lib {
        Some(l) => l.clone(),
        None => {
            let l = library(app);
            k.lib = Some(l.clone());
            l
        }
    };
    if lib.is_empty() {
        ui.vertical(|ui| {
            crate::ui_kit::text(
                ui,
                solidcraft_ui_text::tr_label("the parts library is not available"),
                crate::ui_kit::Text::Note("parts_unavailable"),
            );
        });
        return false;
    }
    // The Unit system list picks metric or inch; Type lists that system's families. The system
    // shown is the chosen family's, so a new system moves the choice to its first family.
    let current = lib.get(k.family).map_or("metric", |f| f.system.as_str());
    let mut system = SYSTEMS.iter().position(|s| s.0 == current).unwrap_or(0);
    let was_system = system;
    choice_row(ui, "Unit system", "pt_system", &mut system, &SYSTEMS.map(|s| s.1));
    let system_id = SYSTEMS.get(system).map_or("metric", |s| s.0);
    if system != was_system
        && let Some(first) = lib.iter().position(|f| f.system == system_id)
    {
        k.family = first;
        k.size = 0;
        k.length = None;
        k.options = PartOptions::default();
        k.joint = 0;
    }
    let shown: Vec<usize> = (0..lib.len()).filter(|i| lib.get(*i).is_some_and(|f| f.system == system_id)).collect();
    let names: Vec<String> = shown
        .iter()
        .filter_map(|i| lib.get(*i))
        .map(|f| if f.standard.is_empty() { family_name(f) } else { format!("{} ({})", family_name(f), f.standard) })
        .collect();
    let refs: Vec<&str> = names.iter().map(String::as_str).collect();
    let mut pick = shown.iter().position(|i| *i == k.family).unwrap_or(0);
    let before = pick;
    choice_row(ui, "Type", "pt_family", &mut pick, &refs);
    if pick != before
        && let Some(i) = shown.get(pick)
    {
        k.family = *i;
        k.size = 0;
        k.length = None;
        k.options = PartOptions::default();
        k.joint = 0;
    }
    let Some(f) = lib.get(k.family) else { return false };
    let sizes: Vec<&str> = f.sizes.iter().map(|s| s.0.as_str()).collect();
    let was_size = k.size;
    choice_row(ui, "Size", "pt_size", &mut k.size, &sizes);
    // Each size has a default length of its own, so a new size resolves the Length list again.
    if k.size != was_size {
        k.length = None;
    }
    if let Some(ls) = &f.lengths {
        let labels: Vec<String> = ls.iter().map(|l| length_label(app, &f.id, *l)).collect();
        let refs: Vec<&str> = labels.iter().map(String::as_str).collect();
        let mut i = k.length.unwrap_or_else(|| default_length(f, k.size, ls));
        form_row(ui, "Length", |ui| {
            combo(ui, "pt_len", &refs, &mut i);
            k.length = Some(i);
        });
    }
    let size_name = f.sizes.get(k.size).map_or("", |s| s.0.as_str()).to_string();
    option_rows(ui, f, &size_name, &mut k.options);
    if crate::ui_kit::checkbox_row(ui, "pt_flip", "Flip", k.flip) {
        k.flip = !k.flip;
    }
    if crate::ui_kit::checkbox_row(ui, "pt_separate", "Separate part", k.separate) {
        k.separate = !k.separate;
    }
    if k.separate {
        ui.vertical(|ui| {
            crate::ui_kit::text(ui, solidcraft_ui_text::tr("parts.separate_hint"), crate::ui_kit::Text::Note("parts_separate"));
        });
    }
    // How a part seated in a hole is held there: asked once a hole is picked.
    if inputs.first().is_some_and(|i| !i.items.is_empty()) {
        let kinds = joints(f);
        let labels: Vec<&str> = kinds.iter().map(|j| j.1).collect();
        k.joint = k.joint.min(labels.len().saturating_sub(1));
        choice_row(ui, "Seating joint", "pt_joint", &mut k.joint, &labels);
    }
    text_row(ui, "Name", &mut k.name, solidcraft_ui_text::tr("parts.automatic_name"));
    // The part, and its table values.
    let shown_part = insert_params(&lib, k, &[]).ok();
    let key = shown_part.as_ref().map(|p| {
        let mut p = p.clone();
        if let Value::Object(m) = &mut p {
            m.remove("thread");
            m.remove("name");
            m.remove("flip");
            m.remove("share");
        }
        p.to_string()
    });
    let tex_id = egui::Id::unique("sc_part_picture");
    if k.picture != key || key.is_none() {
        let tex = shown_part.as_ref().and_then(picture).map(|img| ui.ctx().load_texture("sc_part_picture", img, egui::TextureOptions::LINEAR));
        ui.data_mut(|d| match tex {
            Some(t) => {
                d.insert_temp(tex_id, t);
            }
            None => d.remove::<egui::TextureHandle>(tex_id),
        });
        k.picture = key;
    }
    ui.vertical(|ui| match ui.data(|d| d.get_temp::<egui::TextureHandle>(tex_id)) {
        Some(tx) => {
            ui.add(egui::Image::new(&tx).fit_to_exact_size(vec2(180.0, 112.0)).corner_radius(t.radius_sm));
        }
        None => {
            crate::ui_kit::text(ui, solidcraft_ui_text::tr_label("no picture"), crate::ui_kit::Text::Note("parts_picture"));
        }
    });
    if let Some((_, row)) = f.sizes.get(k.size)
        && let Some(o) = row.as_object()
    {
        // `default_length` picks the Length list's opening value; it is not a dimension.
        for (key, v) in o.iter().filter(|(key, _)| !matches!(key.as_str(), "size" | "default_length")) {
            let label = dimension_label(key).map_or_else(|| key.replace('_', " "), str::to_string);
            form_row(ui, &label, |ui| {
                crate::ui_kit::text(
                    ui,
                    &v.as_f64().map(|x| crate::prefs::show_mm(app, x, 1)).unwrap_or_else(|| v.to_string()),
                    crate::ui_kit::Text::Cell,
                );
            });
        }
    }
    if inputs.first().is_some_and(|i| i.items.is_empty()) {
        ui.vertical(|ui| {
            crate::ui_kit::text(ui, solidcraft_ui_text::tr("parts.placement_hint"), crate::ui_kit::Text::Note("parts_placement"));
        });
    }
    false
}

/// The commands OK runs.
pub fn commands(app: &SolveApp, k: &Pt, inputs: &[SelInput]) -> Result<Vec<(String, Value)>, String> {
    let lib = match &k.lib {
        Some(l) => l.clone(),
        None => app.session.scratch().execute("parts.library", &json!({})).map(|v| families(&v)).unwrap_or_default(),
    };
    let p = insert_params(&lib, k, inputs.first().map_or(&[][..], |i| &i.items))?;
    // The toolbar's command when the engine has it (same parameters).
    let id = if solidcraft_engine::find_command("parts.fastener").is_some() { "parts.fastener" } else { "parts.insert" };
    Ok(vec![(id.into(), p)])
}

/// The message a size's dimension is labelled with, by its key in the library: "Head height",
/// not `head_height`. A key the library adds later shows as it is until it has one here.
fn dimension_label(key: &str) -> Option<&'static str> {
    Some(match key {
        "across_flats" => "parts.dimension.across_flats",
        "bore" => "parts.dimension.bore",
        "d" => "parts.dimension.d",
        "head_diameter" => "parts.dimension.head_diameter",
        "head_height" => "parts.dimension.head_height",
        "height" => "parts.dimension.height",
        "inner" => "parts.dimension.inner",
        "outer" => "parts.dimension.outer",
        "pitch" => "parts.dimension.pitch",
        "socket" => "parts.dimension.socket",
        "socket_depth" => "parts.dimension.socket_depth",
        "thickness" => "parts.dimension.thickness",
        "width" => "parts.dimension.width",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use solidcore_record::Vec3;

    fn lib() -> Vec<Family> {
        families(&json!({"families": [
            {"family": "socket_head_cap_screw", "name": "Socket Head Cap Screw", "standard": "ISO 4762",
             "sizes": [{"size": "M5", "head_diameter": 8.5, "default_length": 16},
                       {"size": "M6", "head_diameter": 10, "default_length": 20}], "lengths": [10, 16, 20]},
            {"family": "bearing", "name": "Deep Groove Ball Bearing", "standard": "", "sizes": [{"size": "608", "bore": 8}], "lengths": null},
        ]}))
    }

    #[test]
    fn library_rows_parse() {
        let l = lib();
        assert_eq!(l.len(), 2);
        assert_eq!(l[0].sizes[1].0, "M6");
        assert_eq!(l[0].lengths.as_deref(), Some(&[10.0, 16.0, 20.0][..]));
        assert!(l[1].lengths.is_none());
    }

    /// The library's families are named in the user's language, and a family the catalogs
    /// don't know keeps the library's name.
    #[test]
    fn family_names_follow_the_language() {
        crate::i18n::install();
        let l = families(&solidcraft_engine::Session::default().execute("parts.library", &json!({})).unwrap());
        assert!(l.len() >= 7, "{}", l.len());
        let _german = solidcraft_ui_text::scope("de");
        for f in &l {
            assert_ne!(family_name(f), f.name, "{}", f.id);
        }
        let unknown = Family { id: "spring".into(), name: "Spring".into(), system: "metric".into(), ..Default::default() };
        assert_eq!(family_name(&unknown), "Spring");
    }

    /// The Length list is lengths only: it opens on the size's default rather than on an item
    /// called “Default”, and a size that reports none opens on the first length.
    #[test]
    fn the_length_list_opens_on_the_size_s_default() {
        let l = lib();
        let ls = l[0].lengths.clone().unwrap_or_default();
        assert_eq!(default_length(&l[0], 0, &ls), 1, "M5 opens on 16");
        assert_eq!(default_length(&l[0], 1, &ls), 2, "M6 opens on 20");
        assert_eq!(default_length(&l[1], 0, &ls), 0, "a size without one opens on the first length");
        assert_eq!(default_length(&l[0], 99, &ls), 0, "so does a size that is not there");
    }

    /// A hole pick seats the part (`at`), several picks seat it in each hole (#9341); without one
    /// it goes to the origin along Z.
    #[test]
    fn insert_params_for_picks() {
        let l = lib();
        let k = Pt { family: 0, size: 1, length: Some(2), name: String::new(), picture: None, lib: None, ..Default::default() };
        let hole = Sel::Face { body: "Body1".into(), index: 3, point: Vec3::new(5.0, 0.0, 10.0) };
        let p = insert_params(&l, &k, std::slice::from_ref(&hole)).unwrap();
        assert_eq!(p, json!({"family": "socket_head_cap_screw", "size": "M6", "length": 20.0, "share": true, "at": [5.0, 0.0, 10.0]}));
        let rim = Sel::Edge { body: "Body1".into(), index: 7, point: Vec3::new(25.0, 0.0, 10.0) };
        let p = insert_params(&l, &k, &[hole, rim]).unwrap();
        assert_eq!(p["at"], json!([[5.0, 0.0, 10.0], [25.0, 0.0, 10.0]]));
        assert!(p.get("point").is_none());
        let k = Pt { family: 1, size: 0, length: None, name: "Front bearing".into(), picture: None, lib: None, ..Default::default() };
        let p = insert_params(&l, &k, &[]).unwrap();
        assert_eq!(p["point"], json!([0.0, 0.0, 0.0]));
        assert_eq!(p["name"], "Front bearing");
        assert!(p.get("length").is_none());
        assert!(insert_params(&[], &k, &[]).is_err());
    }

    /// #10316: the Unit system list above Type picks metric or inch, and Type then lists that
    /// system's families only.
    #[test]
    fn the_system_list_picks_the_families_type_shows() {
        let chosen = |h: &crate::scenario::Harness| {
            let Kind::Part(k) = &h.app.dialog.as_ref().unwrap().kind else { panic!("not Insert Part") };
            commands(&h.app, k, &[]).unwrap()[0].1.clone()
        };
        let mut h = crate::scenario::Harness::new();
        h.app.home.open = false;
        h.app.start("parts.insert");
        h.frames(3);
        assert!(h.widget_near("Metric", "Unit system").is_some());
        assert_eq!(chosen(&h)["family"], "socket_head_cap_screw");
        h.step(&json!({"click":{"handle":"dialog:Unit system"}})).unwrap();
        h.step(&json!({"click":{"handle":"dialog:Unit system:Inch"}})).unwrap();
        h.frames(2);
        assert_eq!((chosen(&h)["family"].as_str(), chosen(&h)["size"].as_str()), (Some("hex_cap_screw_inch"), Some("1/4-20 UNC")));
        // Type now lists the inch families and no metric one.
        h.step(&json!({"click":{"handle":"dialog:Type"}})).unwrap();
        assert!(crate::scenario::handle_at("dialog:Type:Plain washer, USS (wide) (ANSI B18.22.1 Type A)").is_some());
        assert!(crate::scenario::handle_at("dialog:Type:Hex nut (ISO 4032)").is_none());
        h.step(&json!({"click":{"handle":"dialog:Type:Hex nut (inch) (ASME B18.2.2)"}})).unwrap();
        h.frames(2);
        assert_eq!(chosen(&h)["family"], "hex_nut_inch");
        // Back to metric: the first metric family again.
        h.step(&json!({"click":{"handle":"dialog:Unit system"}})).unwrap();
        h.step(&json!({"click":{"handle":"dialog:Unit system:Metric"}})).unwrap();
        h.frames(2);
        assert_eq!(chosen(&h)["family"], "socket_head_cap_screw");
    }

    /// Every dimension the library gives for a size is labelled in words, in each family.
    #[test]
    fn every_library_dimension_has_a_label_in_words() {
        crate::i18n::install();
        let mut s = solidcraft_engine::Session::default();
        let l = families(&s.execute("parts.library", &json!({})).unwrap());
        assert!(l.len() >= 7, "{}", l.len());
        for f in &l {
            for (size, row) in &f.sizes {
                for key in row.as_object().into_iter().flat_map(|o| o.keys()).filter(|k| !matches!(k.as_str(), "size" | "default_length")) {
                    let label = dimension_label(key).map(solidcraft_ui_text::tr);
                    assert!(
                        label.is_some_and(|l| l.starts_with(char::is_uppercase) && !l.contains('_') && !l.starts_with("parts.")),
                        "{} {size}: {key} is labelled {label:?}",
                        f.name
                    );
                }
            }
        }
    }

    /// The options a family and size have reach the command, and only those that are not the
    /// default: a part asked for as before gets the parameters it always got.
    #[test]
    fn options_reach_the_command_only_where_they_differ_from_the_default() {
        let mut s = solidcraft_engine::Session::default();
        let l = families(&s.execute("parts.library", &json!({})).unwrap());
        let find = |id: &str| l.iter().position(|f| f.id == id).unwrap();
        let size = |f: usize, name: &str| l[f].sizes.iter().position(|s| s.0 == name).unwrap();
        // A cap screw as it opens: nothing but family, size and placement.
        let shcs = find("socket_head_cap_screw");
        let k = Pt { family: shcs, size: size(shcs, "M10"), ..Default::default() };
        let p = insert_params(&l, &k, &[]).unwrap();
        assert_eq!(
            p,
            json!({"family": "socket_head_cap_screw", "size": "M10", "share": true, "point": [0.0, 0.0, 0.0], "direction": [0.0, 0.0, 1.0]})
        );
        // The same with every option set, seated in a hole.
        let options = PartOptions { drive: 1, tip: 0, thread: 1, pitch: 1, left_hand: true };
        let k = Pt { family: shcs, size: size(shcs, "M10"), options, joint: 3, flip: true, separate: true, ..Default::default() };
        let hole = Sel::Face { body: "Body1".into(), index: 3, point: Vec3::new(5.0, 0.0, 10.0) };
        let p = insert_params(&l, &k, std::slice::from_ref(&hole)).unwrap();
        assert_eq!(
            p,
            json!({"family": "socket_head_cap_screw", "size": "M10", "drive": "hexalobular", "thread": "modeled", "pitch": 1.25,
                   "left_hand": true, "flip": true, "at": [5.0, 0.0, 10.0], "joint": "screw"})
        );
        // With no hole there is no joint to ask for.
        assert!(insert_params(&l, &k, &[]).unwrap().get("joint").is_none());
        // A set screw's point, and a part without a thread, which has no options at all.
        let set = find("set_screw");
        let k = Pt { family: set, size: size(set, "M6"), options: PartOptions { tip: 2, ..Default::default() }, ..Default::default() };
        assert_eq!(insert_params(&l, &k, &[]).unwrap()["tip"], "cup");
        let washer = find("washer");
        let k = Pt { family: washer, options: PartOptions { thread: 1, left_hand: true, ..Default::default() }, joint: 3, ..Default::default() };
        let p = insert_params(&l, &k, std::slice::from_ref(&hole)).unwrap();
        assert!(p.get("thread").is_none() && p.get("left_hand").is_none(), "{p}");
        assert_eq!(p["joint"], "planar", "a washer has no screw joint: the fourth of its joints is the last");
        // Every command the options build is one the engine takes.
        for (family, options) in [
            ("pan_head_screw", PartOptions { drive: 2, thread: 2, ..Default::default() }),
            ("set_screw", PartOptions { tip: 1, ..Default::default() }),
            ("hex_nut", PartOptions { left_hand: true, ..Default::default() }),
        ] {
            let f = find(family);
            let k = Pt { family: f, size: size(f, "M6"), options, ..Default::default() };
            let p = insert_params(&l, &k, &[]).unwrap();
            assert!(solidcraft_engine::Session::default().execute("parts.insert", &p).is_ok(), "{p}");
        }
    }

    /// The rows follow the family: a cap screw shows its drive and thread rows, a washer none of
    /// them, and the joint row comes with the first hole picked.
    #[test]
    fn the_option_rows_follow_the_family() {
        let mut h = crate::scenario::Harness::new();
        h.app.home.open = false;
        h.app.start("parts.insert");
        h.frames(3);
        for row in ["Drive type", "Thread type"] {
            assert!(crate::scenario::handle_at(&format!("dialog:{row}")).is_some(), "{row}");
        }
        assert!(crate::scenario::handle_at("dialog:Seating joint").is_none());
        assert!(crate::scenario::handle_at("dialog:Thread pitch").is_none(), "M2 has no fine pitch");
        h.step(&json!({"click":{"handle":"dialog:Thread type"}})).unwrap();
        h.step(&json!({"click":{"handle":"dialog:Thread type:Modeled thread"}})).unwrap();
        h.frames(2);
        let Kind::Part(k) = &h.app.dialog.as_ref().unwrap().kind else { panic!("not Insert Part") };
        assert_eq!(commands(&h.app, k, &[]).unwrap()[0].1["thread"], "modeled");
        h.step(&json!({"click":{"handle":"dialog:Type"}})).unwrap();
        h.step(&json!({"click":{"handle":"dialog:Type:Plain washer (ISO 7089)"}})).unwrap();
        h.frames(2);
        for row in ["Drive type", "Thread type"] {
            assert!(crate::scenario::handle_at(&format!("dialog:{row}")).is_none(), "{row}");
        }
        let Kind::Part(k) = &h.app.dialog.as_ref().unwrap().kind else { panic!("not Insert Part") };
        assert!(commands(&h.app, k, &[]).unwrap()[0].1.get("thread").is_none());
    }
}
