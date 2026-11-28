//! A timeline feature that fails fails `run`, and nothing is written: the bodies lack what the
//! feature should have made, and handing them out with exit 0 hides the failure. `eval` still
//! prints its report, and fails on the same rule (#11225).

use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

use serde_json::{Value, json};

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!("solidcraft-run-strict-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
        std::fs::create_dir(&dir).unwrap();
        Self(dir)
    }

    /// A filleted box whose fillet a later parameter change makes impossible: every command
    /// succeeds, the timeline ends with a failing feature.
    fn broken_script(&self) -> PathBuf {
        let path = self.0.join("broken.json");
        let script = json!({"commands": [
            {"command": "parameters.add", "params": {"name": "r", "expression": "2 mm"}},
            {"command": "solid.box", "params": {"length": 20, "width": 20, "height": 10}},
            {"command": "solid.fillet", "params": {"edges": [[10, 0, 10]], "radius": "r"}},
            {"command": "parameters.change", "params": {"name": "r", "expression": "50 mm"}},
        ]});
        std::fs::write(&path, script.to_string()).unwrap();
        path
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn cli() -> Command {
    Command::new(env!("CARGO_BIN_EXE_solidcraft-cli"))
}

#[test]
fn a_failing_feature_fails_the_run_and_writes_nothing() {
    let dir = Scratch::new();
    let script = dir.broken_script();
    let (step, saved) = (dir.0.join("part.step"), dir.0.join("part.solidcraft"));
    for extra in [&[][..], &["--results"][..], &["--quiet"][..]] {
        let output = cli().arg("run").arg(&script).args(extra).arg("--out").arg(&step).arg("--save").arg(&saved).output().unwrap();
        assert_eq!(output.status.code(), Some(1), "{extra:?}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("Fillet1") && stderr.contains("nothing was written"), "{stderr}");
        assert!(output.stdout.is_empty(), "{extra:?}");
        assert!(!step.exists() && !saved.exists(), "{extra:?}");
    }
}

#[test]
fn allow_errors_writes_anyway_and_a_saved_design_keeps_failing() {
    let dir = Scratch::new();
    let (step, saved) = (dir.0.join("part.step"), dir.0.join("part.solidcraft"));
    let output = cli().arg("run").arg(dir.broken_script()).arg("--allow-errors").arg("--out").arg(&step).arg("--save").arg(&saved).output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains(&format!("{}: warning: Model cache saved partially", saved.display())), "{stderr}");
    let measured: Value = serde_json::from_slice(&output.stdout).unwrap();
    // SMLib integrates the volume (a relative accuracy of 1e-4 asked for; a box comes out within
    // a few units in the last place), so the box's 4000 mm³ is checked to 1e-9, not to the bit.
    let volume = measured["total"]["volume_mm3"].as_f64().unwrap();
    assert!((volume - 4000.0).abs() < 4000.0 * 1e-9, "the box without its fillet: {volume}");
    assert!(std::fs::metadata(&step).unwrap().len() > 0);
    // The saved design still holds the failing feature: running it fails too, `eval` reports it.
    let again = dir.0.join("again.step");
    let output = cli().arg("run").arg(&saved).arg("--out").arg(&again).output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(!again.exists());
    let output = cli().arg("eval").arg(&saved).output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(report["timeline"].as_array().unwrap().iter().any(|f| f["error"].is_string()), "{}", report["timeline"]);
}

/// #11225: a parameter change that breaks a feature made `eval` exit 0 where `run` exits 1, so
/// a script checking the status took the broken rebuild for a good one. `eval` (and its alias
/// `inspect`) now fails too, after printing the whole report; `--allow-errors` exits 0.
#[test]
fn eval_reports_a_failing_feature_and_fails_like_run() {
    let dir = Scratch::new();
    let script = dir.0.join("repro.json");
    let commands = json!({"commands": [
        {"command": "parameters.add", "params": {"name": "Size", "expression": "10 mm"}},
        {"command": "solid.box", "params": {"length": "Size", "width": 10, "height": 10, "body_name": "Block"}},
        {"command": "parameters.change", "params": {"name": "Size", "expression": "0 mm"}},
    ]});
    std::fs::write(&script, commands.to_string()).unwrap();
    let failed = |report: &Value| {
        let timeline = report["timeline"].as_array().unwrap();
        timeline.iter().any(|f| f["name"] == "Box1" && f["error"].as_str().is_some_and(|e| e.contains("positive size")))
    };
    // Both inputs as one, and `--in` alone, are the same design.
    for command in ["eval", "inspect"] {
        for input in [&[][..], &["--in"][..]] {
            let output = cli().arg(command).args(input).arg(&script).output().unwrap();
            assert_eq!(output.status.code(), Some(1), "{command} {input:?}");
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(stderr.contains("1 timeline feature(s) failed") && stderr.contains("Box1: invalid input"), "{stderr}");
            assert!(!stderr.contains("nothing was written"), "eval writes nothing anyway: {stderr}");
            let report: Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(report["body_count"], 0, "{command} {input:?}");
            assert!(failed(&report), "{}", report["timeline"]);
        }
    }
    // The report still goes to `--json-out`, then the status says the timeline failed.
    let file = dir.0.join("eval.json");
    let output = cli().arg("eval").arg(&script).arg("--json-out").arg(&file).output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(failed(&serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap()));
    // Asked to inspect a broken design on purpose: the report and status 0, as before.
    let output = cli().arg("eval").arg(&script).arg("--allow-errors").output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert!(output.stderr.is_empty(), "{}", String::from_utf8_lossy(&output.stderr));
    assert!(failed(&serde_json::from_slice(&output.stdout).unwrap()));
    // `run` fails the same input on the same rule.
    let output = cli().arg("run").arg(&script).arg("--quiet").output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("Box1: invalid input"));
}

#[test]
fn a_clean_timeline_still_writes() {
    let dir = Scratch::new();
    let script = dir.0.join("box.json");
    std::fs::write(&script, json!({"commands": [{"command": "solid.box", "params": {"length": 2, "width": 3, "height": 4}}]}).to_string()).unwrap();
    let step = dir.0.join("box.step");
    let output = cli().arg("run").arg(&script).arg("--out").arg(&step).arg("--quiet").output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert!(std::fs::metadata(&step).unwrap().len() > 0);
}

/// `run --in B.sc --save B.sc --results` (without `--out` or `--allow-errors`) saves the
/// updated design when `component.update_links` leaves a downstream chamfer with a lost reference,
/// while `--out B.step` without `--allow-errors` still fails and writes nothing.
#[test]
fn run_update_links_saves_design_when_chamfer_loses_reference_but_refuses_step_export() {
    let dir = Scratch::new();
    let part_a = dir.0.join("A.sc");
    let design_b = dir.0.join("B.sc");

    let a_init = dir.0.join("a_init.json");
    std::fs::write(
        &a_init,
        json!({"commands": [
            {"command": "parameters.add", "params": {"name": "step_d", "expression": "12 mm"}},
            {"command": "solid.cylinder", "params": {"diameter": 20, "height": 20, "body_name": "SteppedPin"}},
            {"command": "solid.cylinder", "params": {"diameter": "step_d", "height": 10, "base": [0, 0, 20], "operation": "join", "targets": ["SteppedPin"]}}
        ]})
        .to_string(),
    )
    .unwrap();
    let output = cli().arg("run").arg(&a_init).arg("--save").arg(&part_a).arg("--quiet").output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));

    let b_init = dir.0.join("b_init.json");
    std::fs::write(
        &b_init,
        json!({"commands": [
            {"command": "component.insert_design", "params": {"path": &part_a, "name": "Pin"}},
            {"command": "solid.chamfer", "params": {"edges": [[6, 0, 20]], "distance": 1, "body": "SteppedPin", "name": "Chamfer3"}},
            {"command": "solid.box", "params": {"length": 10, "width": 10, "height": 10, "corner": [40, 0, 0], "body_name": "Bracket"}},
            {"command": "solid.cylinder", "params": {"diameter": 4, "height": 12, "base": [45, 5, -1], "operation": "cut", "targets": ["Bracket"], "name": "MountHole"}}
        ]})
        .to_string(),
    )
    .unwrap();
    let output = cli().arg("run").arg(&b_init).arg("--save").arg(&design_b).arg("--quiet").output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));

    let a_edit = dir.0.join("a_edit.json");
    std::fs::write(
        &a_edit,
        json!({"commands": [
            {"command": "parameters.change", "params": {"name": "step_d", "expression": "20 mm"}}
        ]})
        .to_string(),
    )
    .unwrap();
    let output = cli().arg("run").arg(&a_edit).arg("--in").arg(&part_a).arg("--save").arg(&part_a).arg("--quiet").output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));

    let update_script = dir.0.join("update.json");
    std::fs::write(&update_script, json!({"commands": [{"command": "component.update_links", "params": {}}]}).to_string()).unwrap();

    // Exporting `--out B.step` without `--allow-errors` must still fail and write nothing.
    let step_out = dir.0.join("B.step");
    let failed_export =
        cli().arg("run").arg(&update_script).arg("--in").arg(&design_b).arg("--out").arg(&step_out).arg("--results").output().unwrap();
    assert_eq!(failed_export.status.code(), Some(1), "{}", String::from_utf8_lossy(&failed_export.stdout));
    let stderr = String::from_utf8_lossy(&failed_export.stderr);
    assert!(stderr.contains("Chamfer3") && stderr.contains("nothing was written"), "{stderr}");
    assert!(!step_out.exists());

    // Saving `--save B.sc` without `--out` and without `--allow-errors` must succeed.
    let saved = cli().arg("run").arg(&update_script).arg("--in").arg(&design_b).arg("--save").arg(&design_b).arg("--results").output().unwrap();
    assert!(saved.status.success(), "stderr: {}", String::from_utf8_lossy(&saved.stderr));
    let results: Value = serde_json::from_slice(&saved.stdout).unwrap();
    assert_eq!(results[0]["updated"], json!(["Pin"]), "{results}");
    let warnings = results[0]["warnings"].as_array().unwrap();
    assert!(warnings.iter().filter_map(Value::as_str).any(|w| w.contains("Chamfer3")), "{results}");

    // The saved B.sc holds the updated source geometry, Chamfer3 marked failed, and the rest rebuilt.
    let inspect = cli().arg("eval").arg(&design_b).arg("--allow-errors").output().unwrap();
    assert!(inspect.status.success(), "{}", String::from_utf8_lossy(&inspect.stderr));
    let report: Value = serde_json::from_slice(&inspect.stdout).unwrap();
    let timeline = report["timeline"].as_array().unwrap();
    let chamfer = timeline.iter().find(|f| f["name"] == "Chamfer3").unwrap();
    assert!(chamfer["error"].as_str().is_some_and(|e| e.contains("no longer exists") && e.contains("reselect")), "{timeline:?}");
    let mount_hole = timeline.iter().find(|f| f["name"] == "MountHole").unwrap();
    assert!(mount_hole["error"].is_null(), "{timeline:?}");
    let expected_volume = std::f64::consts::PI * 100.0 * 30.0 + (1000.0 - std::f64::consts::PI * 4.0 * 10.0);
    let actual_volume = report["total"]["volume_mm3"].as_f64().unwrap();
    assert!((actual_volume - expected_volume).abs() < 1.0, "expected {expected_volume}, got {actual_volume}");
}
