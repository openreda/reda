//! Adding and deleting Documents, the PCB Document in particular.

use std::path::Path;

use reda_engine::command::ErrorCode;
use reda_engine::model::{DrcCheck, LayerKind, Scope, Severity, Side, Uuid};
use reda_testkit::{TempDir, TestEngine, assert_golden, read_tree, test_engine};
use serde_json::{Value, json};

const GOLDEN: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden/pcb_document");

/// A new Project as `project.new` makes it, with its PCB Document.
fn new_project_with_pcb(dir: &Path) -> TestEngine {
    let mut engine = test_engine();
    engine.run_ok("project.new", json!({"path": dir, "name": "Demo"}));
    engine
}

/// A new Project whose PCB Document (and the Output Job plotting it) has been deleted, so
/// tests can add one.
fn new_project(dir: &Path) -> TestEngine {
    let mut engine = new_project_with_pcb(dir);
    let project = engine.project().unwrap();
    let job = project.output_jobs().next().unwrap().uuid;
    let pcb = project.pcb().unwrap().uuid;
    engine.run_ok("output.job.delete", json!({"job": job}));
    engine.run_ok("document.delete", json!({"document": pcb}));
    engine
}

fn add_pcb(engine: &mut TestEngine) -> Uuid {
    let out = engine.run_ok("document.add", json!({"kind": "pcb"}));
    out["result"]["uuid"].as_str().unwrap().parse().unwrap()
}

#[test]
fn adding_a_pcb_document_creates_the_fixed_layer_set() {
    let dir = TempDir::new("pcb-layers");
    let mut engine = new_project(dir.path());
    let out = engine.run_ok("document.add", json!({"kind": "pcb"}));
    assert_eq!(out["result"]["kind"], "pcb");
    assert_eq!(out["result"]["name"], "Demo");
    assert_eq!(out["result"]["title"], "Demo");

    let project = engine.project().unwrap();
    let pcb = project.pcb().expect("the Project has a PCB Document");
    let layers: Vec<(LayerKind, Option<Side>)> = pcb
        .layers()
        .layers
        .iter()
        .map(|l| (l.kind, l.side))
        .collect();
    use LayerKind::*;
    let (top, bottom) = (Some(Side::Top), Some(Side::Bottom));
    assert_eq!(
        layers,
        [
            (Courtyard, top),
            (Assembly, top),
            (Silkscreen, top),
            (Paste, top),
            (SolderMask, top),
            (Copper, top),
            (Copper, bottom),
            (SolderMask, bottom),
            (Paste, bottom),
            (Silkscreen, bottom),
            (Assembly, bottom),
            (Courtyard, bottom),
            (BoardOutline, None),
            (Mechanical, None),
            (Documentation, None),
        ]
    );
    let mut uuids: Vec<Uuid> = pcb.layers().layers.iter().map(|l| l.uuid).collect();
    uuids.sort();
    uuids.dedup();
    assert_eq!(uuids.len(), 15, "every Layer has its own UUID");
}

#[test]
fn a_new_pcb_document_has_generic_rules_default_severities_and_a_default_net_class() {
    let dir = TempDir::new("pcb-setup");
    let mut engine = new_project(dir.path());
    add_pcb(&mut engine);
    let pcb = engine.project().unwrap().pcb().unwrap();
    let setup = pcb.setup();
    // The Generic Fab Preset's 2-layer values from the fab research.
    assert_eq!(setup.rules.track_width.0, 200_000);
    assert_eq!(setup.rules.clearance.0, 160_000);
    assert_eq!(setup.rules.via_hole.0, 300_000);
    assert_eq!(setup.severities.get(DrcCheck::Clearance), Severity::Error);
    assert_eq!(
        setup.severities.get(DrcCheck::CourtyardOverlap),
        Severity::Warning
    );
    let default = pcb.net_class(setup.default_net_class).unwrap();
    assert_eq!(default.name, "Default");
    assert!(pcb.outline().is_none());
}

#[test]
fn a_second_pcb_document_is_unavailable() {
    let dir = TempDir::new("pcb-second");
    let mut engine = new_project(dir.path());
    add_pcb(&mut engine);
    let err = engine.run_err("document.add", json!({"kind": "pcb"}));
    assert_eq!(err.code, ErrorCode::Unavailable);
}

#[test]
fn the_pcb_document_can_be_deleted_and_added_again() {
    let dir = TempDir::new("pcb-delete");
    let mut engine = new_project(dir.path());
    let pcb = add_pcb(&mut engine);
    let out = engine.run_ok("document.delete", json!({"document": pcb}));
    assert!(
        out["changes"]["project"]["deleted"]
            .as_array()
            .unwrap()
            .contains(&json!(pcb))
    );
    let project = engine.project().unwrap();
    assert!(project.pcb().is_none());
    assert!(project.object(pcb).is_none());
    assert_eq!(
        project.objects().len(),
        3,
        "the Project, its sheet and its Fab Preset copy remain"
    );
    add_pcb(&mut engine);
}

#[test]
fn deleting_an_unknown_document_is_not_found() {
    let dir = TempDir::new("doc-delete-unknown");
    let mut engine = new_project(dir.path());
    let err = engine.run_err(
        "document.delete",
        json!({"document": "00000000-0000-4000-8000-0000000000ff"}),
    );
    assert_eq!(err.code, ErrorCode::NotFound);
}

#[test]
fn a_schematic_sheet_can_be_added_with_a_title() {
    let dir = TempDir::new("sheet-add");
    let mut engine = new_project(dir.path());
    let out = engine.run_ok(
        "document.add",
        json!({"kind": "schematic", "title": "Power"}),
    );
    assert_eq!(out["result"]["name"], "Power");
    assert_eq!(out["result"]["title"], "Power");
    let out = engine.run_ok("document.add", json!({"kind": "schematic"}));
    assert_eq!(out["result"]["title"], "Sheet 3");
    let titles: Vec<String> = engine
        .project()
        .unwrap()
        .documents()
        .map(|d| d.title.clone())
        .collect();
    assert_eq!(titles, ["Sheet 1", "Power", "Sheet 3"]);
}

#[test]
fn adding_the_pcb_document_is_one_undo_step_on_the_project_stack() {
    let dir = TempDir::new("pcb-undo");
    let mut engine = new_project(dir.path());
    let pcb = add_pcb(&mut engine);
    let before = engine.project().unwrap().clone();

    engine.run_ok("edit.undo", json!({"document": "project"}));
    assert!(engine.project().unwrap().pcb().is_none());
    assert!(!engine.dirty_scopes().contains(&Scope::Document(pcb)));
    let err = engine.run_err("edit.undo", json!({"document": pcb}));
    assert_eq!(err.code, ErrorCode::Unavailable);

    engine.run_ok("edit.redo", json!({"document": "project"}));
    assert_eq!(engine.project().unwrap(), &before);
    assert!(engine.dirty_scopes().contains(&Scope::Document(pcb)));
}

#[test]
fn deleting_the_pcb_document_undoes_with_its_contents() {
    let dir = TempDir::new("pcb-delete-undo");
    let mut engine = new_project(dir.path());
    let pcb = add_pcb(&mut engine);
    engine.run_ok(
        "pcb.board_outline.add_rect",
        json!({"width": 100_000_000, "height": 80_000_000}),
    );
    let before = engine.project().unwrap().clone();
    engine.run_ok("document.delete", json!({"document": pcb}));
    engine.run_ok("edit.undo", json!({"document": "project"}));
    assert_eq!(engine.project().unwrap(), &before);
    // The PCB Document's own stack still undoes the outline.
    engine.run_ok("edit.undo", json!({"document": pcb}));
    assert!(engine.project().unwrap().pcb().unwrap().outline().is_none());
}

fn saved_with_pcb(label: &str) -> (TempDir, std::path::PathBuf) {
    let dir = TempDir::new(label);
    let mut engine = new_project_with_pcb(dir.path());
    engine.run_ok(
        "pcb.board_outline.add_rect",
        json!({"width": "100mm", "height": "80mm", "corner_radius": "2mm"}),
    );
    engine.run_ok(
        "pcb.board_cutout.add",
        json!({"vertices": [
            {"x": "10mm", "y": "10mm"}, {"x": "20mm", "y": "10mm"}, {"x": "20mm", "y": "20mm"}
        ]}),
    );
    engine.run_ok(
        "pcb.net_class.add",
        json!({"name": "Power", "track_width": "0.5mm", "clearance": "0.3mm"}),
    );
    engine.run_ok(
        "pcb.text.add",
        json!({"layer": "silkscreen_top", "text": "Demo\nrev A", "x": "5mm", "y": "70mm",
               "height": "2mm", "rotation": "90deg"}),
    );
    engine.run_ok(
        "pcb.text.add",
        json!({"layer": "silkscreen_bottom", "text": "Bottom", "x": "5mm", "y": "5mm",
               "font": "Noto Sans"}),
    );
    engine.run_ok(
        "pcb.graphic.add_line",
        json!({"layer": "silkscreen_top", "from": {"x": "1mm", "y": "1mm"},
               "to": {"x": "9mm", "y": "1mm"}}),
    );
    engine.run_ok(
        "pcb.graphic.add_arc",
        json!({"layer": "mechanical", "from": {"x": "60mm", "y": "40mm"},
               "to": {"x": "50mm", "y": "50mm"}, "sweep": "90deg", "width": "0.1mm"}),
    );
    engine.run_ok(
        "pcb.graphic.add_rect",
        json!({"layer": "copper_top", "from": {"x": "70mm", "y": "10mm"},
               "to": {"x": "80mm", "y": "20mm"}, "filled": true, "width": 0}),
    );
    engine.run_ok("project.save", json!({}));
    let folder = dir.path().join("Demo");
    (dir, folder)
}

#[test]
fn the_pcb_document_saves_as_the_golden_file() {
    let (_dir, folder) = saved_with_pcb("pcb-golden");
    let tree = read_tree(&folder);
    let names: Vec<&str> = tree.keys().map(String::as_str).collect();
    assert_eq!(
        names,
        [
            ".reda/.gitignore",
            "Demo.reda",
            "Demo.redapcb",
            "Demo.redasch"
        ]
    );
    assert_golden(format!("{GOLDEN}/Demo.reda"), &tree["Demo.reda"]);
    assert_golden(format!("{GOLDEN}/Demo.redapcb"), &tree["Demo.redapcb"]);
}

#[test]
fn a_saved_pcb_document_opens_and_saves_to_the_same_bytes() {
    let (_dir, folder) = saved_with_pcb("pcb-roundtrip");
    let before = read_tree(&folder);
    let mut engine = test_engine();
    let out = engine.run_ok("project.open", json!({"path": folder}));
    assert!(out.get("missing").is_none(), "{out}");
    assert!(!engine.is_dirty());
    let pcb = engine.project().unwrap().pcb().unwrap();
    assert_eq!((pcb.texts().count(), pcb.graphics().count()), (2, 3));
    engine.run_ok("project.save", json!({}));
    assert_eq!(read_tree(&folder), before);
}

#[test]
fn a_project_opened_without_its_pcb_file_reports_it_and_can_add_one() {
    let (_dir, folder) = saved_with_pcb("pcb-missing");
    std::fs::remove_file(folder.join("Demo.redapcb")).unwrap();
    // Another seed, so its new UUIDs differ from those the saved Project already uses.
    let mut engine = TestEngine(
        reda_engine::Engine::builder()
            .ids(reda_engine::command::SeededIds::new(1))
            .build(),
    );
    let out = engine.run_ok("project.open", json!({"path": folder}));
    let missing = &out["missing"];
    assert_eq!(missing.as_array().unwrap().len(), 1, "{out}");
    assert_eq!(missing[0]["kind"], "pcb");
    assert_eq!(missing[0]["name"], "Demo");
    let kinds: Vec<&Value> = out["documents"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| &d["kind"])
        .collect();
    assert_eq!(kinds, [&json!("schematic")]);
    // The Project file no longer matches what is open.
    assert_eq!(engine.dirty_scopes(), [Scope::Project]);
    add_pcb(&mut engine);
}

#[test]
fn a_broken_pcb_file_is_refused() {
    let (_dir, folder) = saved_with_pcb("pcb-broken");
    std::fs::write(folder.join("Demo.redapcb"), b"{\"format\": 1}").unwrap();
    let err = test_engine().run_err("project.open", json!({"path": folder}));
    assert_eq!(err.code, ErrorCode::Rejected);
    assert!(err.message.contains("Demo.redapcb"), "{}", err.message);
}
