//! `output.job.run`: what blocks a run, what only warns, where the files go, refilling
//! stale Zones first, and progress and cancellation.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use reda_command::{CancelToken, ErrorCode, Progress, Signals};
use reda_testkit::{TempDir, TestEngine, probe_engine, test_engine_with_library};
use serde_json::{Value, json};

/// The Output Job `project.new` makes, on seed 0.
const JOB: &str = "00000000-0000-4000-8000-000000000028";
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

/// A plated hole, so the run writes a PTH file.
fn via(engine: &mut TestEngine) {
    engine.run_ok(
        "pcb.via.add",
        json!({"at": pt(5.0, 5.0), "diameter": "0.6mm", "hole": "0.3mm"}),
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
    // One component at a time: the path is compared as a JSON string.
    let folder = dir.path().join("Demo").join("outputs").join("Generic");
    assert_eq!(out["destination"], json!(folder));
    assert_eq!(out["zip"], json!(folder.join("Demo-Generic.zip")));
    let names = names(&out, "files");
    let names: Vec<&str> = names.iter().map(String::as_str).collect();
    // An empty board: copper, mask and Profile are always written, while the empty
    // Silkscreen and Paste and the drill files with no holes are left out and reported.
    assert_eq!(
        names[..5],
        ["Demo.gtl", "Demo.gbl", "Demo.gts", "Demo.gbs", "Demo.gm1"]
    );
    assert!(!names.iter().any(|n| n.ends_with(".drl")), "{names:?}");
    assert_eq!(out["files"][0]["function"], "copper_top");
    assert_eq!(out["files"][0]["path"], "fab/Demo.gtl");
    assert_eq!(
        out["skipped"],
        json!([
            {"name": "Demo.gtp", "function": "paste_top"},
            {"name": "Demo.gbp", "function": "paste_bottom"},
            {"name": "Demo.gto", "function": "silkscreen_top"},
            {"name": "Demo.gbo", "function": "silkscreen_bottom"},
            {"name": "Demo-PTH.drl", "function": "drill_plated"},
            {"name": "Demo-NPTH.drl", "function": "drill_non_plated"},
        ])
    );
    for name in ["Demo.gtp", "Demo.gto", "Demo-PTH.drl", "Demo-NPTH.drl"] {
        assert!(
            !folder.join("fab").join(name).exists(),
            "{name} is not written"
        );
    }

    let zip = std::fs::read(folder.join("Demo-Generic.zip")).unwrap();
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(zip)).unwrap();
    assert_eq!(archive.len(), names.len());
    for name in names {
        let loose = std::fs::read(folder.join("fab").join(name)).unwrap();
        let mut entry = archive.by_name(name).unwrap();
        let mut zipped = Vec::new();
        std::io::Read::read_to_end(&mut entry, &mut zipped).unwrap();
        assert_eq!(zipped, loose, "{name}");
        let text = String::from_utf8(loose).unwrap();
        assert!(
            !text.contains("CreationDate"),
            "{name}: no date in the files"
        );
        assert!(
            [".csv", ".txt", ".gbrjob"]
                .iter()
                .any(|e| name.ends_with(e))
                || text.contains("TF.GenerationSoftware,openreda,REDA,0.0.0-test"),
            "{name}: the version comes from the engine"
        );
    }
    assert!(!engine.is_dirty(), "running a job is not an edit");
}

/// The run's zip entries (`files`, from their `fab/` copies), or its `skipped` names.
fn names(out: &Value, key: &str) -> Vec<String> {
    out[key]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|f| match key {
            "files" => f["path"].as_str().unwrap().strip_prefix("fab/"),
            _ => f["name"].as_str(),
        })
        .map(str::to_owned)
        .collect()
}

#[test]
fn a_run_writes_the_fab_notes_drill_report_and_job_file_into_the_zip_after_the_drill_files() {
    let dir = TempDir::new("output-fab-files");
    let mut engine = board(&dir);
    via(&mut engine);
    engine.run_ok("pcb.fab_notes.set", json!({"text": "Hello fab."}));
    let out = run(&mut engine);
    let names = names(&out, "files");
    let at = names.iter().position(|n| n == "Demo-PTH.drl").unwrap();
    assert_eq!(
        names[at + 1..at + 4],
        [
            "Demo-fab-notes.txt",
            "Demo-drill-report.txt",
            "Demo-job.gbrjob"
        ]
    );
    let functions: Vec<&str> = out["files"].as_array().unwrap()[at + 1..at + 4]
        .iter()
        .map(|f| f["function"].as_str().unwrap())
        .collect();
    assert_eq!(functions, ["fab_notes", "drill_report", "gerber_job"]);

    let folder = dir.path().join("Demo/outputs/Generic");
    let notes = std::fs::read_to_string(folder.join("fab/Demo-fab-notes.txt")).unwrap();
    // The Fab Notes list exactly the files in the zip, themselves included.
    let zip = std::fs::read(folder.join("Demo-Generic.zip")).unwrap();
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(zip)).unwrap();
    let zipped: Vec<String> = (0..archive.len())
        .map(|i| archive.by_index(i).unwrap().name().unwrap().to_string())
        .collect();
    let listed: Vec<String> = notes
        .lines()
        .skip_while(|l| *l != "Files in the zip")
        .skip(1)
        .take_while(|l| !l.is_empty())
        .map(|l| l.trim().to_owned())
        .collect();
    assert_eq!(listed, names);
    assert_eq!(zipped, names, "the zip holds the files in run report order");
    assert!(
        notes.ends_with(
            "
Notes
Hello fab.
"
        ),
        "{notes}"
    );

    let job: Value =
        serde_json::from_slice(&std::fs::read(folder.join("fab/Demo-job.gbrjob")).unwrap())
            .unwrap();
    let paths: Vec<&str> = job["FilesAttributes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["Path"].as_str().unwrap())
        .collect();
    assert_eq!(
        paths,
        names[..at + 1],
        "the job file lists the Gerber and drill files"
    );
}

#[test]
fn the_fab_notes_drill_report_and_job_file_each_have_an_outputs_toggle() {
    let dir = TempDir::new("output-fab-toggles");
    let mut engine = board(&dir);
    via(&mut engine);
    run(&mut engine);
    let folder = dir.path().join("Demo/outputs/Generic/fab");
    let toggles = [
        ("fab_notes", "Demo-fab-notes.txt"),
        ("drill_report", "Demo-drill-report.txt"),
        ("gerber_job", "Demo-job.gbrjob"),
    ];
    for (key, name) in toggles {
        assert!(folder.join(name).exists(), "{name} is on by default");
        engine.run_ok(
            "output.job.set",
            json!({"job": JOB, "outputs": {key: false}}),
        );
        let out = run(&mut engine);
        let written = names(&out, "files");
        assert!(!written.contains(&name.to_owned()), "{key} off");
        assert!(
            !folder.join(name).exists(),
            "{key} off drops the last run's {name}"
        );
        for (other, other_name) in toggles {
            if other != key {
                assert!(
                    written.contains(&other_name.to_owned()),
                    "{key} off keeps {other}"
                );
            }
        }
        engine.run_ok(
            "output.job.set",
            json!({"job": JOB, "outputs": {key: true}}),
        );
    }
    // With the Gerber files off, the job file lists only the drill file.
    engine.run_ok(
        "output.job.set",
        json!({"job": JOB, "outputs": {"gerber": false}}),
    );
    run(&mut engine);
    let job: Value =
        serde_json::from_slice(&std::fs::read(folder.join("Demo-job.gbrjob")).unwrap()).unwrap();
    assert_eq!(job["FilesAttributes"].as_array().unwrap().len(), 1);
    assert_eq!(job["FilesAttributes"][0]["Path"], "Demo-PTH.drl");
}

#[test]
fn each_run_overwrites_the_last_and_drops_files_it_no_longer_writes() {
    let dir = TempDir::new("output-overwrite");
    let mut engine = board(&dir);
    via(&mut engine);
    run(&mut engine);
    let folder = dir.path().join("Demo/outputs/Generic");
    let fab = folder.join("fab");
    std::fs::write(fab.join("Demo.gtl"), b"old").unwrap();
    std::fs::write(folder.join("Demo.gtl"), b"from a run before fab/").unwrap();
    std::fs::write(folder.join("notes.txt"), b"mine").unwrap();
    run(&mut engine);
    assert_ne!(std::fs::read(fab.join("Demo.gtl")).unwrap(), b"old");
    assert!(
        !folder.join("Demo.gtl").exists(),
        "the old layout's loose file is gone"
    );

    engine.run_ok(
        "output.job.set",
        json!({"job": JOB, "outputs": {"gerber": false}}),
    );
    run(&mut engine);
    for name in ["Demo.gtl", "Demo.gts", "Demo.gbp", "Demo.gto"] {
        assert!(!fab.join(name).exists(), "Gerber turned off: {name}");
    }
    assert!(fab.join("Demo-PTH.drl").exists());
    std::fs::write(fab.join("Demo-NPTH.drl"), b"old").unwrap();
    run(&mut engine);
    assert!(
        !fab.join("Demo-NPTH.drl").exists(),
        "a drill file with no holes drops the last run's"
    );

    engine.run_ok(
        "output.job.set",
        json!({"job": JOB, "outputs": {"drill": false}}),
    );
    run(&mut engine);
    assert!(!fab.join("Demo-PTH.drl").exists(), "drill turned off");
    assert!(!fab.join("Demo-NPTH.drl").exists(), "drill turned off");
    assert!(folder.join("Demo-Generic.zip").exists());
    assert_eq!(
        std::fs::read(folder.join("notes.txt")).unwrap(),
        b"mine",
        "files REDA does not write are left alone"
    );
}

#[test]
fn a_run_writes_the_bom_and_cpl_with_the_job_preset_columns() {
    let dir = TempDir::new("output-assembly");
    let mut engine = board(&dir);
    engine.run_ok(
        "test.footprint.add",
        json!({"at": pt(5.0, 5.0), "pads": [{"at": pt(0.0, 0.0), "width": "1mm",
               "height": "1mm"}]}),
    );
    let out = run(&mut engine);
    let files: Vec<(&str, &str)> = out["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| (f["path"].as_str().unwrap(), f["function"].as_str().unwrap()))
        .collect();
    // Beside the zip, named from the name template.
    assert!(
        files.contains(&("Demo-Generic-BOM.csv", "bom")),
        "{files:?}"
    );
    assert!(
        files.contains(&("Demo-Generic-CPL.csv", "cpl")),
        "{files:?}"
    );
    assert!(
        !out["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|w| w["check"] == "output_not_built"),
        "{}",
        out["warnings"]
    );
    let folder = dir.path().join("Demo/outputs/Generic");
    assert_eq!(
        std::fs::read_to_string(folder.join("Demo-Generic-CPL.csv")).unwrap(),
        "Designator,Mid X,Mid Y,Layer,Rotation\n",
        "a Footprint with no Component is a Mechanical Part, so it is left out"
    );
    assert!(
        std::fs::read_to_string(folder.join("Demo-Generic-BOM.csv"))
            .unwrap()
            .starts_with("Designator,Quantity,Value,Footprint,Manufacturer,MPN\n")
    );

    engine.run_ok(
        "output.job.set",
        json!({"job": JOB, "outputs": {"bom": false, "cpl": false}}),
    );
    run(&mut engine);
    assert!(
        !folder.join("Demo-Generic-BOM.csv").exists(),
        "BOM turned off"
    );
    assert!(
        !folder.join("Demo-Generic-CPL.csv").exists(),
        "CPL turned off"
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
    let destination = dir.path().join("Demo").join("fab").join("out");
    assert_eq!(out["destination"], json!(destination));
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
    let gtl =
        std::fs::read_to_string(dir.path().join("Demo/outputs/Generic/fab/Demo.gtl")).unwrap();
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
fn a_coordinate_a_gerber_file_cannot_hold_blocks_the_run() {
    let dir = TempDir::new("output-range");
    let mut engine = board(&dir);
    engine.run_ok(
        "pcb.track.add",
        json!({"layer": "copper_top", "width": "0.2mm",
               "points": [pt(5.0, 5.0), {"x": "2147.5mm", "y": "5mm"}]}),
    );
    let err = engine.run_err("output.job.run", json!({"job": JOB}));
    assert_eq!(err.code, ErrorCode::Rejected);
    assert_eq!(blockers(&err), ["coordinate_out_of_range"]);
    let blocker = &err.details.as_ref().unwrap()["blockers"][0];
    assert_eq!(blocker["args"]["file"], "Demo.gtl");
    assert_eq!(blocker["args"]["x"], 2_147_500_000i64);
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
fn a_component_without_a_footprint_only_warns_even_when_the_job_writes_a_bom_and_cpl() {
    // The BOM and CPL read only the job's PCB, which the footprintless U1 never reaches.
    let dir = TempDir::new("output-no-footprint");
    let mut engine = footprintless(&dir);
    let out = run(&mut engine);
    let functions: Vec<&str> = out["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["function"].as_str().unwrap())
        .collect();
    assert!(
        functions.contains(&"bom") && functions.contains(&"cpl"),
        "{out}"
    );
    let unassigned = out["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|w| w["check"] == "unassigned_footprint")
        .unwrap_or_else(|| panic!("no unassigned_footprint in {out}"));
    assert_eq!(unassigned["severity"], "warning");
    assert_eq!(unassigned["args"]["designator"], "U1");
    assert_eq!(unassigned["objects"].as_array().unwrap().len(), 1);
    let erc = out["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|w| w["check"] == "erc_errors")
        .unwrap();
    assert_eq!(erc["severity"], "warning");
    assert!(erc["args"]["count"].as_i64().unwrap() >= 1);
}

/// [`footprintless`] with its Logic Part retyped `component_type`, from a copy of the
/// Library.
fn footprintless_of_type(dir: &TempDir, component_type: &str) -> TestEngine {
    let library = dir.path().join("Erc");
    for (rel, bytes) in reda_testkit::read_tree(&erc_library()) {
        let path = library.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }
    let part = library.join("parts/Logic.redapart");
    let text = std::fs::read_to_string(&part).unwrap().replace(
        "\"component_type\":\"standard\"",
        &format!("\"component_type\":\"{component_type}\""),
    );
    std::fs::write(&part, text).unwrap();
    let mut engine = test_engine_with_library(&library);
    new_project(&mut engine, dir);
    engine.run_ok(
        "schematic.component.place",
        json!({"document": SHEET, "part": "a0000000-0000-4000-8000-000000000210",
               "at": {"x": 0, "y": 0}}),
    );
    engine
}

#[test]
fn a_net_tie_without_a_footprint_warns_and_a_mechanical_part_does_not() {
    let unassigned = |engine: &mut TestEngine| {
        run(engine)["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|w| w["check"] == "unassigned_footprint")
            .count()
    };
    let dir = TempDir::new("output-net-tie-no-footprint");
    assert_eq!(unassigned(&mut footprintless_of_type(&dir, "net_tie")), 1);
    let dir = TempDir::new("output-mechanical-no-footprint");
    assert_eq!(
        unassigned(&mut footprintless_of_type(&dir, "mechanical")),
        0
    );
}

#[test]
fn drc_errors_only_warn() {
    let dir = TempDir::new("output-drc");
    let mut engine = board(&dir);
    let gnd = engine.run_ok("test.net.add", json!({"name": "GND"}))["result"]["uuid"].clone();
    // Two Pads on GND with nothing joining them: an unrouted connection.
    engine.run_ok(
        "test.footprint.add",
        json!({"at": pt(5.0, 10.0), "pads": [
            {"at": pt(0.0, 0.0), "width": "1mm", "height": "1mm", "net": gnd},
            {"at": pt(15.0, 0.0), "width": "1mm", "height": "1mm", "net": gnd}]}),
    );
    let out = run(&mut engine);
    let drc = out["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|w| w["check"] == "drc_errors")
        .unwrap_or_else(|| panic!("no drc_errors in {out}"));
    assert_eq!(drc["severity"], "warning");
    assert!(drc["args"]["count"].as_i64().unwrap() >= 1);
}

#[test]
fn rules_looser_than_the_preset_and_missing_fonts_only_warn() {
    let dir = TempDir::new("output-warnings");
    let mut engine = board(&dir);
    engine.run_ok(
        "pcb.rules.set",
        json!({"kind": "clearance", "values": {"value": "0.1mm"}}),
    );
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
        .find(|w| w["check"] == "rule_outside_preset")
        .unwrap_or_else(|| panic!("{warnings}"));
    assert_eq!(rule["args"]["rule"], "clearance");
    assert_eq!(rule["args"]["value"], "value");
    assert_eq!(rule["args"]["board"], 100_000);
    assert_eq!(rule["args"]["preset"], "Generic");
    assert_eq!(rule["args"]["limit"], "min");
    assert_eq!(rule["args"]["preset_limit"], 160_000);
    assert_eq!(rule["measured"], json!({"length": 100_000}));
    assert_eq!(rule["required"], json!({"length": 160_000}));
    assert!(rule["rule"].is_string());
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

#[test]
fn a_designator_shared_only_between_a_sheet_and_the_board_does_not_block() {
    // The sheet's U1 and the board's U1 are different Components (the board is out of
    // sync), but the BOM and CPL read only the board, where U1 is unique.
    let dir = TempDir::new("output-dup-sync");
    let mut engine = TestEngine(
        reda_engine::Engine::builder()
            .version(reda_testkit::TEST_VERSION)
            .ids(reda_command::SeededIds::new(0))
            .clock(reda_command::FixedClock::EPOCH)
            .user_dirs(None)
            .starter_library(Some(erc_library()))
            .commands(reda_testkit::probe_commands)
            .build(),
    );
    new_project(&mut engine, &dir);
    engine.run_ok(
        "schematic.component.place",
        json!({"document": SHEET, "part": "a0000000-0000-4000-8000-000000000210",
               "at": {"x": 0, "y": 0}}),
    );
    engine.run_ok(
        "test.footprint.add",
        json!({"at": pt(5.0, 5.0), "designator": "U1",
               "pads": [{"at": pt(0.0, 0.0), "width": "1mm", "height": "1mm"}]}),
    );
    engine.run_ok(
        "output.job.set",
        json!({"job": JOB, "outputs": {"bom": false, "cpl": false}}),
    );
    let out = run(&mut engine);
    assert!(!checks(&out["warnings"]).contains(&"duplicate_designator".to_string()));
}

#[test]
fn gerber_drill_and_cpl_share_the_presets_resolved_origin() {
    for (preset, relative) in [("Generic", false), ("JLCPCB", false), ("PCBWay", true)] {
        let dir = TempDir::new("output-common-origin");
        let library = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../library/starter");
        let mut engine = test_engine_with_library(&library);
        engine.run_ok(
            "project.new",
            json!({"path": dir.path(), "name": "Demo", "fab_preset": preset}),
        );
        engine.run_ok(
            "schematic.component.place",
            json!({"document": SHEET,
            "part": "84d5651a-bf4b-462e-9480-39e8f454a042",
            "footprint": "0691e5e7-9eb6-49f3-9f3e-d4159258693e", "at": pt(20.0, 40.0)}),
        );
        let hash = engine.run_ok("sync.preview", json!({}))["hash"].clone();
        engine.run_ok("sync.apply", json!({"hash": hash}));
        let footprint = engine
            .project()
            .unwrap()
            .pcb()
            .unwrap()
            .footprints()
            .next()
            .unwrap()
            .clone();
        engine.run_ok(
            "pcb.move",
            json!({"objects": [footprint.uuid],
            "by": {"x": 20_000_000 - footprint.at.x.0, "y": 15_000_000 - footprint.at.y.0}}),
        );
        engine.run_ok("pcb.free_pad.add", json!({"kind": "plated_through_hole", "shape": "circle",
            "at": pt(20.0, 15.0), "width": "1mm", "height": "1mm", "hole": {"width": "0.5mm", "height": "0.5mm"}}));
        engine.run_ok(
            "pcb.track.add",
            json!({"layer": "copper_top", "width": "0.25mm",
            "points": [pt(20.0, 15.0), pt(21.0, 15.0)]}),
        );
        // Curved left edge reaches x=0: the bounding box is not just its vertices.
        engine.run_ok(
            "pcb.board_outline.add",
            json!({"vertices": [
            {"x": "10mm", "y": "5mm"}, {"x": "40mm", "y": "5mm"},
            {"x": "40mm", "y": "25mm"}, {"x": "10mm", "y": "25mm", "sweep": "180deg"}]}),
        );
        let job = engine.project().unwrap().output_jobs().next().unwrap().uuid;
        let mut coordinates = None;
        for (origin, x, y) in [
            (false, 20, if relative { 10 } else { 15 }),
            (
                true,
                if relative { 0 } else { 20 },
                if relative { 5 } else { 15 },
            ),
        ] {
            if origin {
                engine.run_ok("pcb.output_origin.set", json!({"at": pt(20.0, 10.0)}));
            }
            let out = engine.run_ok("output.job.run", json!({"job": job}));
            let destination = PathBuf::from(out["destination"].as_str().unwrap());
            let read = |name: &str| {
                let fab = destination.join("fab").join(name);
                let path = if fab.exists() {
                    fab
                } else {
                    destination.join(name)
                };
                std::fs::read_to_string(path).unwrap()
            };
            let copper = read("Demo.gtl");
            assert!(
                copper.contains(&format!("X{}Y{}D02*", x * 1_000_000, y * 1_000_000)),
                "{preset}, origin={origin}: {copper}"
            );
            let id = |text: &str| {
                text.lines()
                    .find_map(|line| {
                        line.split_once("TF.SameCoordinates,")
                            .map(|(_, value)| value.trim_end_matches("*%").to_owned())
                    })
                    .unwrap()
            };
            let current = id(&copper);
            if let Some(previous) = &coordinates {
                if relative {
                    assert_ne!(
                        previous, &current,
                        "moved relative origins cannot claim alignment"
                    );
                } else {
                    assert_eq!(
                        previous, &current,
                        "absolute output ignores the placed origin"
                    );
                }
            }
            coordinates = Some(current.clone());
            let drill = read("Demo-PTH.drl");
            assert_eq!(
                id(&drill),
                current,
                "Gerber and drill declare the same coordinate system"
            );
            assert!(
                drill.contains(&format!("X{x}.000Y{y}.000\n")),
                "{preset}, origin={origin}: {drill}"
            );
            let cpl_path = out["files"]
                .as_array()
                .unwrap()
                .iter()
                .find(|f| f["function"] == "cpl")
                .unwrap()["path"]
                .as_str()
                .unwrap()
                .to_owned();
            let cpl = read(&cpl_path);
            let mut lines = cpl.lines();
            let headers: Vec<_> = lines.next().unwrap().split(',').collect();
            let cells: Vec<_> = lines.next().unwrap().split(',').collect();
            for (header, expected) in [("Mid X", x), ("Mid Y", y)] {
                let i = headers.iter().position(|h| *h == header).unwrap();
                assert_eq!(
                    cells[i].trim_end_matches("mm").trim(),
                    format!("{expected}.000000"),
                    "{preset}, origin={origin}"
                );
            }
        }
    }
}

#[test]
fn merge_drill_writes_one_drill_file_and_drops_the_split_ones() {
    let dir = TempDir::new("output-merge-drill");
    let mut engine = board(&dir);
    via(&mut engine);
    engine.run_ok(
        "pcb.free_pad.add",
        json!({"kind": "non_plated", "shape": "circle", "at": pt(20.0, 10.0),
               "width": "3.2mm", "hole": {"width": "3.2mm"}}),
    );
    run(&mut engine);
    let folder = dir.path().join("Demo/outputs/Generic/fab");
    assert!(folder.join("Demo-PTH.drl").exists());
    assert!(folder.join("Demo-NPTH.drl").exists());

    // Turn `merge_drill` on in the Project's copy of the preset, then reopen.
    engine.run_ok("project.save", json!({}));
    let file = dir.path().join("Demo/Demo.reda");
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(text.contains("\"merge_drill\":false"));
    std::fs::write(
        &file,
        text.replace("\"merge_drill\":false", "\"merge_drill\":true"),
    )
    .unwrap();
    engine.run_ok("project.open", json!({"path": dir.path().join("Demo")}));
    let out = run(&mut engine);
    let drills: Vec<&Value> = out["files"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| f["path"].as_str().unwrap().ends_with(".drl"))
        .collect();
    assert_eq!(
        drills,
        [&json!({"path": "fab/Demo.drl", "function": "drill_mixed"})]
    );
    let merged = std::fs::read_to_string(folder.join("Demo.drl")).unwrap();
    assert!(
        merged.contains("TF.FileFunction,MixedPlating,1,2,Drill"),
        "{merged}"
    );
    assert!(!folder.join("Demo-PTH.drl").exists());
    assert!(!folder.join("Demo-NPTH.drl").exists());
}

#[test]
fn a_bom_row_missing_a_required_field_warns_naming_its_designators_and_stays() {
    let dir = TempDir::new("output-required-field");
    let library = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../library/starter");
    let mut engine = test_engine_with_library(&library);
    engine.run_ok(
        "project.new",
        json!({"path": dir.path(), "name": "Demo", "fab_preset": "JLCPCB",
               "board": {"width": "60mm", "height": "60mm"}}),
    );
    for x in [20.0, 40.0] {
        engine.run_ok(
            "schematic.component.place",
            json!({"document": SHEET,
            "part": "84d5651a-bf4b-462e-9480-39e8f454a042",
            "footprint": "0691e5e7-9eb6-49f3-9f3e-d4159258693e", "at": pt(x, 40.0)}),
        );
    }
    let hash = engine.run_ok("sync.preview", json!({}))["hash"].clone();
    engine.run_ok("sync.apply", json!({"hash": hash}));
    let out = run(&mut engine);
    let missing: Vec<&Value> = out["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|w| w["check"] == "bom_missing_field")
        .collect();
    assert_eq!(missing.len(), 1, "{}", out["warnings"]);
    assert_eq!(missing[0]["severity"], "warning");
    assert_eq!(missing[0]["args"]["field"], "LCSC");
    assert_eq!(missing[0]["args"]["designators"], "R1,R2");
    assert_eq!(missing[0]["objects"].as_array().unwrap().len(), 2);
    let bom = std::fs::read_to_string(dir.path().join("Demo/outputs/JLCPCB/Demo-JLCPCB-BOM.csv"))
        .unwrap();
    assert!(bom.contains("10k,\"R1,R2\",RESC1608X55N,\n"), "{bom}");
    let manifest =
        std::fs::read_to_string(dir.path().join("Demo/outputs/JLCPCB/manifest.json")).unwrap();
    assert!(manifest.contains("BOM row R1,R2 has no LCSC"), "{manifest}");

    engine.run_ok(
        "output.job.set",
        json!({"job": JOB, "outputs": {"bom": false}}),
    );
    assert!(
        !checks(&run(&mut engine)["warnings"]).contains(&"bom_missing_field".to_string()),
        "no BOM, no warning"
    );
}
