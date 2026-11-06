//! `project.close` closes the open Project without saving, and Library elements open for
//! editing stay open on the workbench (docs/spec/v0.1/app.md#closing-and-quitting).

use std::path::Path;

use reda_engine::command::ErrorCode;
use reda_engine::model::Scope;
use reda_testkit::{TempDir, TestEngine, test_engine_in};
use serde_json::json;

const DEMO_FOOTPRINT: &str = "00000030-0000-4000-8000-000000000000";

/// An engine with a writable copy of the Demo Library registered as a user Library.
fn engine(home: &TempDir, work: &TempDir) -> TestEngine {
    let demo = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/library/Demo");
    let lib = work.path().join("user-lib");
    for (rel, bytes) in reda_testkit::read_tree(&demo) {
        let path = lib.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }
    let mut e = test_engine_in(home.path());
    e.run_ok(
        "app.preferences.set",
        json!({"key": "libraries.paths", "value": [lib]}),
    );
    e
}

#[test]
fn closing_the_project_drops_its_unsaved_changes_and_leaves_the_files_alone() {
    let (home, work) = (TempDir::new("close-home"), TempDir::new("close-work"));
    let mut e = engine(&home, &work);
    e.run_ok("project.new", json!({"path": work.path(), "name": "Demo"}));
    let project_file = work.path().join("Demo/Demo.reda");
    let before = std::fs::read_to_string(&project_file).unwrap();
    e.run_ok("project.properties.set", json!({"title": "Amp"}));
    assert!(e.dirty_scopes().contains(&Scope::Project));

    e.run_ok("project.close", json!({}));
    assert!(e.project().is_none());
    assert!(e.location().is_none());
    assert!(e.dirty_scopes().is_empty());
    assert_eq!(std::fs::read_to_string(&project_file).unwrap(), before);

    // It opens again as it was saved.
    e.run_ok("project.open", json!({"path": project_file}));
    assert!(e.dirty_scopes().is_empty());
}

#[test]
fn open_library_elements_and_their_undo_stacks_stay_open_after_the_project_closes() {
    let (home, work) = (TempDir::new("close2-home"), TempDir::new("close2-work"));
    let mut e = engine(&home, &work);
    e.run_ok("project.new", json!({"path": work.path(), "name": "Demo"}));
    e.run_ok("library.element.open", json!({"element": DEMO_FOOTPRINT}));
    e.run_ok(
        "library.footprint.edit",
        json!({"footprint": DEMO_FOOTPRINT, "description": "Changed"}),
    );
    e.run_ok("project.close", json!({}));
    let uuid = DEMO_FOOTPRINT.parse().unwrap();
    assert!(e.project().is_none());
    assert!(e.element(uuid).is_some());
    assert_eq!(e.dirty_scopes(), [Scope::Element(uuid)]);
    e.run_ok("edit.undo", json!({"document": DEMO_FOOTPRINT}));
    assert!(e.dirty_scopes().is_empty());
}

#[test]
fn closing_with_no_project_open_is_unavailable() {
    let (home, work) = (TempDir::new("close3-home"), TempDir::new("close3-work"));
    let mut e = engine(&home, &work);
    let err = e.run_err("project.close", json!({}));
    assert_eq!(err.code, ErrorCode::Unavailable);
}
