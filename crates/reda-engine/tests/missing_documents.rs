//! A Document the Project file lists whose file is gone: it stays listed (and the Project
//! file keeps it) but is left out of the open Project, until `document.relink` brings a
//! file back or `document.delete` removes it from the Project.

use std::path::{Path, PathBuf};

use reda_engine::command::ErrorCode;
use reda_engine::model::{DocumentKind, Scope, Uuid};
use reda_testkit::{TempDir, TestEngine, read_tree, test_engine, test_engine_with_trash};
use serde_json::{Value, json};

/// A saved Project `Demo` with sheets "Sheet 1" and "Power" and its PCB Document, in that
/// order. Returns the folder and the Power sheet's UUID.
fn saved(dir: &Path) -> (PathBuf, Uuid) {
    let mut engine = test_engine();
    engine.run_ok("project.new", json!({"path": dir, "name": "Demo"}));
    let out = engine.run_ok(
        "document.add",
        json!({"kind": "schematic", "title": "Power"}),
    );
    let power: Uuid = out["result"]["uuid"].as_str().unwrap().parse().unwrap();
    let pcb = engine.project().unwrap().pcb().unwrap().uuid;
    engine.run_ok("document.reorder", json!({"document": pcb, "index": 2}));
    engine.run_ok("project.save", json!({}));
    (dir.join("Demo"), power)
}

/// A test engine whose new UUIDs differ from the ones `saved` used.
fn other_engine() -> TestEngine {
    TestEngine(
        reda_engine::Engine::builder()
            .ids(reda_engine::command::SeededIds::new(1))
            .version(reda_testkit::TEST_VERSION)
            .user_dirs(None)
            .starter_library(None)
            .build(),
    )
}

/// The `path` of each Document row of the Project file, in order.
fn listed(folder: &Path) -> Vec<String> {
    let text = std::fs::read_to_string(folder.join("Demo.reda")).unwrap();
    let file: Value = serde_json::from_str(&text).unwrap();
    file["documents"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["path"].as_str().unwrap().to_owned())
        .collect()
}

fn titles(engine: &TestEngine) -> Vec<String> {
    let project = engine.project().unwrap();
    project.documents().map(|d| d.title.clone()).collect()
}

#[test]
fn a_missing_sheet_stays_listed_but_is_left_out_and_never_written() {
    let dir = TempDir::new("missing-sheet");
    let (folder, power) = saved(dir.path());
    std::fs::remove_file(folder.join("Power.redasch")).unwrap();
    let before = read_tree(&folder);

    let mut engine = test_engine();
    let out = engine.run_ok("project.open", json!({"path": folder}));
    assert_eq!(out["missing"][0]["uuid"], power.to_string());
    assert_eq!(out["missing"][0]["title"], "Power");
    assert_eq!(titles(&engine), ["Sheet 1", "Demo"]);
    assert!(engine.project().unwrap().document(power).is_none());
    // The Project file on disk still matches what is open.
    assert!(!engine.is_dirty(), "{:?}", engine.dirty_scopes());
    // The full list, in Project file order, still has it.
    let list = engine.project().unwrap().document_list();
    let rows: Vec<(&str, bool)> = list
        .iter()
        .map(|d| (d.entry.title.as_str(), d.missing))
        .collect();
    assert_eq!(rows, [("Sheet 1", false), ("Power", true), ("Demo", false)]);

    engine.run_ok("project.save", json!({}));
    assert_eq!(
        read_tree(&folder),
        before,
        "nothing changes, nothing is recreated"
    );
    assert_eq!(
        listed(&folder),
        ["Demo.redasch", "Power.redasch", "Demo.redapcb"]
    );
}

#[test]
fn a_missing_pcb_stays_listed_until_a_new_pcb_document_replaces_it() {
    let dir = TempDir::new("missing-pcb");
    let (folder, _) = saved(dir.path());
    std::fs::remove_file(folder.join("Demo.redapcb")).unwrap();

    let mut engine = other_engine();
    let out = engine.run_ok("project.open", json!({"path": folder}));
    assert_eq!(out["missing"][0]["kind"], "pcb");
    assert!(engine.project().unwrap().pcb().is_none());
    assert!(!engine.is_dirty());
    engine.run_ok("project.save", json!({}));
    assert_eq!(
        listed(&folder),
        ["Demo.redasch", "Power.redasch", "Demo.redapcb"]
    );

    // Add PCB Document stays offered, and the new one takes the missing one's place.
    let out = engine.run_ok("document.add", json!({"kind": "pcb"}));
    assert_eq!(out["result"]["name"], "Demo");
    let project = engine.project().unwrap();
    assert!(project.document_list().iter().all(|d| !d.missing));
    engine.run_ok("project.save", json!({}));
    assert_eq!(
        listed(&folder),
        ["Demo.redasch", "Power.redasch", "Demo.redapcb"]
    );
    let mut again = test_engine();
    let out = again.run_ok("project.open", json!({"path": folder}));
    assert!(out.get("missing").is_none(), "{out}");
}

#[test]
fn remove_from_project_drops_a_missing_document_and_undoes() {
    let dir = TempDir::new("missing-remove");
    let (folder, power) = saved(dir.path());
    std::fs::remove_file(folder.join("Power.redasch")).unwrap();
    let mut engine = test_engine();
    engine.run_ok("project.open", json!({"path": folder}));

    engine.run_ok("document.delete", json!({"document": power}));
    assert!(
        engine
            .project()
            .unwrap()
            .document_list()
            .iter()
            .all(|d| !d.missing)
    );
    assert_eq!(engine.dirty_scopes(), [Scope::Project]);
    engine.run_ok("edit.undo", json!({"document": "project"}));
    assert!(engine.project().unwrap().document_list()[1].missing);
    engine.run_ok("edit.redo", json!({"document": "project"}));
    engine.run_ok("project.save", json!({}));
    assert_eq!(listed(&folder), ["Demo.redasch", "Demo.redapcb"]);
}

#[test]
fn a_new_document_does_not_take_a_missing_documents_file_name() {
    let dir = TempDir::new("missing-name");
    let (folder, _) = saved(dir.path());
    std::fs::remove_file(folder.join("Power.redasch")).unwrap();
    let mut engine = other_engine();
    engine.run_ok("project.open", json!({"path": folder}));
    let out = engine.run_ok(
        "document.add",
        json!({"kind": "schematic", "title": "Power"}),
    );
    assert_eq!(out["result"]["name"], "Power 2");
    let sheet = out["result"]["uuid"].as_str().unwrap();
    let out = engine.run_ok(
        "document.rename",
        json!({"document": sheet, "title": "Power"}),
    );
    assert_eq!(out["result"]["name"], "Power 2");
}

#[test]
fn relink_takes_a_file_with_the_same_document_uuid_from_the_project_folder() {
    let dir = TempDir::new("relink-same");
    let (folder, power) = saved(dir.path());
    std::fs::rename(
        folder.join("Power.redasch"),
        folder.join("power-old.redasch"),
    )
    .unwrap();
    let mut engine = test_engine();
    engine.run_ok("project.open", json!({"path": folder}));

    let out = engine.run_ok(
        "document.relink",
        json!({"document": power, "path": folder.join("power-old.redasch")}),
    );
    assert_eq!(out["name"], "power-old");
    assert_eq!(out["title"], "Power");
    // Back in its place, read from the file it was relinked to.
    assert_eq!(titles(&engine), ["Sheet 1", "Power", "Demo"]);
    assert!(engine.project().unwrap().schematic_sheet(power).is_some());
    // Only the Project file changed: the sheet is as its file holds it.
    assert_eq!(engine.dirty_scopes(), [Scope::Project]);
    engine.run_ok("project.save", json!({}));
    assert_eq!(
        listed(&folder),
        ["Demo.redasch", "power-old.redasch", "Demo.redapcb"]
    );
    let mut again = test_engine();
    let out = again.run_ok("project.open", json!({"path": folder}));
    assert!(out.get("missing").is_none(), "{out}");
}

#[test]
fn relink_asks_before_taking_a_file_of_another_document() {
    let dir = TempDir::new("relink-other");
    let (folder, power) = saved(dir.path());
    std::fs::remove_file(folder.join("Power.redasch")).unwrap();
    // Another Project's sheet, outside this Project's folder.
    let elsewhere = TempDir::new("relink-other-src");
    let mut other = other_engine();
    other.run_ok(
        "project.new",
        json!({"path": elsewhere.path(), "name": "Other"}),
    );
    let source = elsewhere.path().join("Other/Other.redasch");

    let mut engine = test_engine();
    engine.run_ok("project.open", json!({"path": folder}));
    let err = engine.run_err(
        "document.relink",
        json!({"document": power, "path": source}),
    );
    assert_eq!(err.code, ErrorCode::Conflict, "{}", err.message);
    let details = err.details.unwrap();
    assert_eq!(details["confirm"], true);
    assert!(engine.project().unwrap().document(power).is_none());

    engine.run_ok(
        "document.relink",
        json!({"document": power, "path": source, "confirm": true}),
    );
    // It keeps this Project's UUID and name, and is written into the Project folder.
    let entry = engine.project().unwrap().document(power).unwrap().clone();
    assert_eq!(
        (entry.name.as_str(), entry.kind),
        ("Power", DocumentKind::Schematic)
    );
    assert!(engine.dirty_scopes().contains(&Scope::Document(power)));
    engine.run_ok("project.save", json!({}));
    assert_eq!(
        listed(&folder),
        ["Demo.redasch", "Power.redasch", "Demo.redapcb"]
    );
    let mut again = test_engine();
    let out = again.run_ok("project.open", json!({"path": folder}));
    assert!(out.get("missing").is_none(), "{out}");
    assert!(again.project().unwrap().document(power).is_some());
}

#[test]
fn relink_refuses_the_wrong_kind_a_present_document_and_another_documents_file() {
    let dir = TempDir::new("relink-refused");
    let (folder, power) = saved(dir.path());
    std::fs::remove_file(folder.join("Power.redasch")).unwrap();
    let mut engine = test_engine();
    engine.run_ok("project.open", json!({"path": folder}));
    let sheet1 = engine.project().unwrap().documents().next().unwrap().uuid;

    let err = engine.run_err(
        "document.relink",
        json!({"document": power, "path": folder.join("Demo.redapcb")}),
    );
    assert_eq!(err.code, ErrorCode::InvalidParams, "{}", err.message);
    let err = engine.run_err(
        "document.relink",
        json!({"document": sheet1, "path": folder.join("Demo.redasch")}),
    );
    assert_eq!(err.code, ErrorCode::Rejected, "{}", err.message);
    // Sheet 1's own file belongs to Sheet 1.
    let err = engine.run_err(
        "document.relink",
        json!({"document": power, "path": folder.join("Demo.redasch"), "confirm": true}),
    );
    assert_eq!(err.code, ErrorCode::Rejected, "{}", err.message);
    let err = engine.run_err(
        "document.relink",
        json!({"document": power, "path": folder.join("nowhere.redasch")}),
    );
    assert_eq!(err.code, ErrorCode::NotFound, "{}", err.message);
}

#[test]
fn undoing_a_relink_makes_it_missing_again_and_leaves_the_file_alone() {
    let dir = TempDir::new("relink-undo");
    let (folder, power) = saved(dir.path());
    std::fs::rename(folder.join("Power.redasch"), folder.join("moved.redasch")).unwrap();
    let trash = dir.path().join("trash");
    let mut engine = test_engine_with_trash(&trash);
    engine.run_ok("project.open", json!({"path": folder}));
    engine.run_ok(
        "document.relink",
        json!({"document": power, "path": folder.join("moved.redasch")}),
    );
    engine.run_ok("edit.undo", json!({"document": "project"}));
    assert!(engine.project().unwrap().document(power).is_none());
    assert!(engine.project().unwrap().document_list()[1].missing);
    engine.run_ok("project.save", json!({}));
    assert!(folder.join("moved.redasch").is_file(), "not trashed");
    assert!(!trash.exists());
    assert_eq!(
        listed(&folder),
        ["Demo.redasch", "Power.redasch", "Demo.redapcb"]
    );
}

#[test]
fn a_relinked_pcb_document_comes_back_whole() {
    let dir = TempDir::new("relink-pcb");
    let (folder, _) = saved(dir.path());
    std::fs::rename(folder.join("Demo.redapcb"), folder.join("board.redapcb")).unwrap();
    let mut engine = test_engine();
    engine.run_ok("project.open", json!({"path": folder}));
    let pcb = engine.project().unwrap().document_list()[2].entry.uuid;
    engine.run_ok(
        "document.relink",
        json!({"document": pcb, "path": folder.join("board.redapcb")}),
    );
    let project = engine.project().unwrap();
    assert_eq!(project.pcb().unwrap().uuid, pcb);
    assert_eq!(project.pcb().unwrap().layers().layers.len(), 15);
}
