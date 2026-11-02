//! `output.job.run`: what blocks a run, what only warns, where the files go, refilling
//! stale Zones first, and progress and cancellation.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use reda_command::{CancelToken, ErrorCode, Progress, Signals};
use reda_testkit::{TempDir, TestEngine, probe_engine, test_engine_with_library};
use serde_json::{Value, json};

/// The Output Job `project.new` makes, on seed 0.
const JOB: &str = "00000000-0000-4000-8000-000000000017";
const SHEET: &str = "00000000-0000-4000-8000-000000000002";

fn pt(x: f64, y: f64) -> Value {
    json!({"x": (x * 1e6).round() as i64, "y": (y * 1e6).round() as i64})
}

fn new_project(engine: &mut TestEngine, dir: &TempDir) {
    engine.run_ok(
        "project.new",
        json!({"path": dir.path(), "name": "Demo", "board": {"width": "30mm", "height": "20mm"}}),
    );
}

fn board(dir: &TempDir) -> TestEngine {
    let mut engine = probe_engine();
    new_project(&mut engine, dir);
    engine
}

fn run(engine: &mut TestEngine) -> Value {
    engine.run_ok("output.job.run", json!({"job": JOB}))
}

fn checks(list: &Value) -> Vec<String> {
    list.as_array()
        .unwrap()
        .iter()
        .map(|v| v["check"].as_str().unwrap().to_string())
        .collect()
}

fn zone(engine: &mut TestEngine) {
    let gnd = engine.run_ok("test.net.add", json!({"name": "GND"}))["result"]["uuid"].clone();
    engine.run_ok(
        "test.footprint.add",
        json!({"at": pt(5.0, 5.0), "pads": [{"at": pt(0.0, 0.0), "width": "1mm",
               "height": "1mm", "net": gnd}]}),
    );
    engine.run_ok(
        "pcb.zone.add",
        json!({"layer": "copper_top", "net": gnd,
               "vertices": [pt(1.0, 1.0), pt(29.0, 1.0), pt(29.0, 19.0), pt(1.0, 19.0)]}),
    );
}

#[test]
fn a_run_writes_the_files_and_their_zip_to_the_default_destination() {
    let dir = TempDir::new("output-run");
    let mut engine = board(&dir);
    let out = run(&mut engine);
    let folder = dir.path().join("Demo/outputs/Generic");
    assert_eq!(out["destination"], json!(folder));
    assert_eq!(out["zip"], json!(folder.join("Demo-Generic.zip")));
    let names: Vec<&str> = out["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["Demo.GTL", "Demo.GBL", "Demo.GKO"]);
    assert_eq!(out["files"][0]["function"], "copper_top");

    let zip = std::fs::read(folder.join("Demo-Generic.zip")).unwrap();
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(zip)).unwrap();
    assert_eq!(archive.len(), 3);
    for name in names {
        let loose = std::fs::read(folder.join(name)).unwrap();
        let mut entry = archive.by_name(name).unwrap();
        let mut zipped = Vec::new();
        std::io::Read::read_to_end(&mut entry, &mut zipped).unwrap();
        assert_eq!(zipped, loose, "{name}");
        let text = String::from_utf8(loose).unwrap();
        assert!(
            text.contains("%TF.CreationDate,2026-01-01T00:00:00+00:00*%"),
            "the time comes from the injected Clock"
        );
    }
    assert!(!engine.is_dirty(), "running a job is not an edit");
}

#[test]
fn each_run_overwrites_the_last_and_drops_files_it_no_longer_writes() {
    let dir = TempDir::new("output-overwrite");
    let mut engine = board(&dir);
    run(&mut engine);
    let folder = dir.path().join("Demo/outputs/Generic");
    std::fs::write(folder.join("Demo.GTL"), b"old").unwrap();
    std::fs::write(folder.join("notes.txt"), b"mine").unwrap();
    run(&mut engine);
    assert_ne!(std::fs::read(folder.join("Demo.GTL")).unwrap(), b"old");

    engine.run_ok(
        "output.job.set",
        json!({"job": JOB, "outputs": {"gerber": false}}),
    );
    run(&mut engine);
    assert!(!folder.join("Demo.GTL").exists(), "Gerber turned off");
    assert!(folder.join("Demo-Generic.zip").exists());
    assert_eq!(
        std::fs::read(folder.join("notes.txt")).unwrap(),
        b"mine",
        "files REDA does not write are left alone"
    );
}

#[test]
fn a_custom_destination_is_taken_from_the_project_folder() {
    let dir = TempDir::new("output-custom");
    let mut engine = board(&dir);
    engine.run_ok(
        "output.job.set",
        json!({"job": JOB, "destination": "fab/out"}),
    );
    let out = run(&mut engine);
    assert_eq!(out["destination"], json!(dir.path().join("Demo/fab/out")));
    assert!(dir.path().join("Demo/fab/out/Demo-Generic.zip").exists());
}

#[test]
fn stale_zones_are_refilled_before_plotting() {
    let dir = TempDir::new("output-refill");
    let mut engine = board(&dir);
    zone(&mut engine);
    assert_eq!(
        engine.run_ok("pcb.zone.fill_status", json!({}))["needs_refill"],
        true
    );
    run(&mut engine);
    let gtl = std::fs::read_to_string(dir.path().join("Demo/outputs/Generic/Demo.GTL")).unwrap();
    assert!(gtl.contains("G36*"), "the fill is plotted as a region");
    assert_eq!(
        engine.run_ok("pcb.zone.fill_status", json!({}))["needs_refill"],
        false,
        "the fresh fills are kept"
    );
}

#[derive(Default)]
struct Recorder(Mutex<Vec<(f64, Option<f64>)>>);

impl Progress for Recorder {
    fn report(&self, progress: f64, total: Option<f64>, _: &str) {
        self.0.lock().unwrap().push((progress, total));
    }
}

#[test]
fn a_run_reports_progress_and_a_cancelled_run_writes_nothing() {
    let dir = TempDir::new("output-cancel");
    let mut engine = board(&dir);
    let recorder = Arc::new(Recorder::default());
    let signals = Signals::new(CancelToken::new()).with_progress(recorder.clone());
    engine
        .run_with("output.job.run", json!({"job": JOB}), &signals)
        .unwrap();
    let seen = recorder.0.lock().unwrap().clone();
    assert!(seen.len() > 2, "{seen:?}");
    assert!(seen.windows(2).all(|w| w[0].0 <= w[1].0), "{seen:?}");
    let (last, total) = *seen.last().unwrap();
    assert_eq!(Some(last), total);

    let dir = TempDir::new("output-cancelled");
    let mut engine = board(&dir);
    let token = CancelToken::new();
    token.cancel();
    let err = engine
        .run_with("output.job.run", json!({"job": JOB}), &Signals::new(token))
        .unwrap_err();
    assert!(err.is_cancelled());
    assert!(!dir.path().join("Demo/outputs").exists());
}

#[test]
fn an_unknown_job_is_not_found() {
    let dir = TempDir::new("output-unknown");
    let mut engine = board(&dir);
    let err = engine.run_err(
        "output.job.run",
        json!({"job": "00000000-0000-4000-8000-0000000000ff"}),
    );
    assert_eq!(err.code, ErrorCode::NotFound);
}

fn blockers(err: &reda_command::CommandError) -> Vec<String> {
    checks(&err.details.as_ref().unwrap()["blockers"])
}

#[test]
fn duplicate_designators_block_the_run() {
    let dir = TempDir::new("output-duplicates");
    let mut engine = board(&dir);
    for x in [5.0, 10.0] {
        engine.run_ok(
            "test.footprint.add",
            json!({"at": pt(x, 5.0), "pads": [{"at": pt(0.0, 0.0), "width": "1mm",
                   "height": "1mm"}]}),
        );
    }
    let err = engine.run_err("output.job.run", json!({"job": JOB}));
    assert_eq!(err.code, ErrorCode::Rejected);
    assert_eq!(blockers(&err), ["duplicate_designator"]);
    let blocker = &err.details.as_ref().unwrap()["blockers"][0];
    assert_eq!(blocker["args"]["designator"], "TP");
    assert_eq!(blocker["objects"].as_array().unwrap().len(), 2);
    assert!(
        !dir.path().join("Demo/outputs").exists(),
        "nothing is written"
    );
}

#[test]
fn a_board_without_a_closed_outline_blocks_the_run() {
    let dir = TempDir::new("output-outline");
    let mut engine = board(&dir);
    engine.run_ok("pcb.board_outline.delete", json!({}));
    let err = engine.run_err("output.job.run", json!({"job": JOB}));
    assert_eq!(blockers(&err), ["board_outline_open"]);
}

fn erc_library() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../reda-schematic/tests/erc_library/Erc")
}

/// A Project whose one Component (a Logic part: an unconnected Input, so an ERC error)
/// has no Footprint.
fn footprintless(dir: &TempDir) -> TestEngine {
    let mut engine = test_engine_with_library(&erc_library());
    new_project(&mut engine, dir);
    engine.run_ok(
        "schematic.component.place",
        json!({"document": SHEET, "part": "a0000000-0000-4000-8000-000000000210",
               "at": {"x": 0, "y": 0}}),
    );
    engine
}

#[test]
fn components_without_a_footprint_block_a_run_that_writes_a_bom_or_cpl() {
    let dir = TempDir::new("output-no-footprint");
    let mut engine = footprintless(&dir);
    let err = engine.run_err("output.job.run", json!({"job": JOB}));
    assert_eq!(blockers(&err), ["missing_footprint"]);
    let blocker = &err.details.as_ref().unwrap()["blockers"][0];
    assert_eq!(blocker["args"]["designator"], "U1");

    engine.run_ok(
        "output.job.set",
        json!({"job": JOB, "outputs": {"bom": false, "cpl": false}}),
    );
    let out = run(&mut engine);
    assert!(
        checks(&out["warnings"]).contains(&"erc_errors".to_string()),
        "{out}"
    );
    let erc = out["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|w| w["check"] == "erc_errors")
        .unwrap();
    assert_eq!(erc["severity"], "warning");
    assert!(erc["args"]["count"].as_i64().unwrap() >= 1);
}

#[test]
fn rules_looser_than_the_preset_and_missing_fonts_only_warn() {
    let dir = TempDir::new("output-warnings");
    let mut engine = board(&dir);
    engine.run_ok("pcb.rules.set", json!({"clearance": "0.1mm"}));
    engine.run_ok(
        "pcb.text.add",
        json!({"layer": "silkscreen_top", "text": "REDA", "x": "5mm", "y": "5mm",
               "font": "No Such Sans"}),
    );
    let out = run(&mut engine);
    let warnings = &out["warnings"];
    let rule = warnings
        .as_array()
        .unwrap()
        .iter()
        .find(|w| w["check"] == "rule_below_preset")
        .unwrap_or_else(|| panic!("{warnings}"));
    assert_eq!(rule["args"]["rule"], "clearance");
    assert_eq!(rule["args"]["board"], 100_000);
    assert_eq!(rule["args"]["preset"], "Generic");
    assert!(rule["args"]["minimum"].as_i64().unwrap() > 100_000);
    let font = warnings
        .as_array()
        .unwrap()
        .iter()
        .find(|w| w["check"] == "missing_font")
        .unwrap_or_else(|| panic!("{warnings}"));
    assert_eq!(font["args"]["font"], "No Such Sans");
}

fn library() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../reda-schematic/tests/library/Test")
}

#[test]
fn an_out_of_sync_board_only_warns() {
    let dir = TempDir::new("output-out-of-sync");
    let mut engine = test_engine_with_library(&library());
    new_project(&mut engine, &dir);
    engine.run_ok(
        "schematic.component.place",
        json!({"document": SHEET, "part": "a0000000-0000-4000-8000-000000000100",
               "at": {"x": 0, "y": 0}}),
    );
    let out = run(&mut engine);
    let warnings = &out["warnings"];
    assert!(
        checks(warnings).contains(&"out_of_sync".to_string()),
        "{warnings}"
    );
}
