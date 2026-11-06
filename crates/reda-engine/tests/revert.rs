//! `document.revert`: dropping a Document's unsaved changes by reading its file again, as
//! "Don't Save" does when its tab closes.

use std::path::Path;

use reda_engine::command::ErrorCode;
use reda_engine::model::{Scope, Uuid};
use reda_testkit::{TempDir, TestEngine, test_engine};
use serde_json::json;

fn new_project(dir: &Path) -> TestEngine {
    let mut engine = test_engine();
    engine.run_ok("project.new", json!({"path": dir, "name": "Demo"}));
    engine
}

fn sheet(engine: &TestEngine) -> Uuid {
    let project = engine.project().unwrap();
    project.documents().next().unwrap().uuid
}

fn add_wire(engine: &mut TestEngine, sheet: Uuid) {
    engine.run_ok(
        "schematic.wire.add",
        json!({"document": sheet, "points": [{"x": "0mm", "y": "0mm"}, {"x": "5mm", "y": "0mm"}]}),
    );
}

fn object_count(engine: &TestEngine, document: Uuid) -> usize {
    let project = engine.project().unwrap();
    let objects = project.objects();
    let mine = objects
        .iter()
        .filter(|o| o.scope() == Scope::Document(document));
    mine.count()
}

#[test]
fn reverting_a_document_brings_back_its_file_and_leaves_it_clean() {
    let dir = TempDir::new("revert-sheet");
    let mut engine = new_project(dir.path());
    let sheet = sheet(&engine);
    let before = object_count(&engine, sheet);
    add_wire(&mut engine, sheet);
    assert!(object_count(&engine, sheet) > before);
    assert!(engine.is_dirty());

    engine.run_ok("document.revert", json!({"document": sheet}));
    assert_eq!(object_count(&engine, sheet), before);
    assert!(!engine.is_dirty(), "{:?}", engine.dirty_scopes());
    // The dropped changes are not on the undo stack any more.
    let undo = engine.run("edit.undo", json!({"document": sheet}));
    assert!(undo.is_err() || object_count(&engine, sheet) == before);
}

#[test]
fn reverting_leaves_other_documents_and_the_project_file_dirty() {
    let dir = TempDir::new("revert-others");
    let mut engine = new_project(dir.path());
    let sheet = sheet(&engine);
    add_wire(&mut engine, sheet);
    engine.run_ok("project.properties.set", json!({"title": "Amp"}));

    engine.run_ok("document.revert", json!({"document": sheet}));
    assert_eq!(engine.dirty_scopes(), [Scope::Project]);
    assert_eq!(engine.project().unwrap().props().properties.title, "Amp");
}

#[test]
fn a_document_never_saved_cannot_be_reverted() {
    let dir = TempDir::new("revert-new");
    let mut engine = new_project(dir.path());
    let out = engine.run_ok("document.add", json!({"kind": "schematic"}));
    let added = out["result"]["uuid"].as_str().unwrap().to_owned();
    let err = engine
        .run("document.revert", json!({"document": added}))
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::Rejected);
}

#[test]
fn reverting_an_unknown_document_is_not_found() {
    let dir = TempDir::new("revert-unknown");
    let mut engine = new_project(dir.path());
    let err = engine
        .run(
            "document.revert",
            json!({"document": "00000000-0000-4000-8000-0000000000ff"}),
        )
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::NotFound);
}
