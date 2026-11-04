//! Library elements open, edit, undo, check and save with no Project open, and stay open
//! when a Project opens afterwards (docs/spec/v0.1/app.md: library editor tabs with no
//! Project open).

use std::path::Path;

use reda_engine::command::ErrorCode;
use reda_engine::model::Scope;
use reda_testkit::{TempDir, TestEngine, test_engine_in};
use serde_json::json;

const DEMO_FOOTPRINT: &str = "00000030-0000-4000-8000-000000000000";

fn demo_library() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/library/Demo")
}

/// An engine with no Project and a writable copy of the Demo Library registered in the
/// Preferences as a user Library.
fn engine(home: &TempDir, work: &TempDir) -> TestEngine {
    let lib = work.path().join("user-lib");
    for (rel, bytes) in reda_testkit::read_tree(&demo_library()) {
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
fn a_footprint_is_edited_undone_checked_and_saved_with_no_project_open() {
    let (home, work) = (TempDir::new("noproj-home"), TempDir::new("noproj-work"));
    let mut e = engine(&home, &work);
    assert!(e.project().is_none());

    let out = e.run_ok("library.element.open", json!({"element": DEMO_FOOTPRINT}));
    assert_eq!(out["read_only"], false);
    assert!(e.project().is_none(), "opening an element opens no Project");
    assert!(e.element(DEMO_FOOTPRINT.parse().unwrap()).is_some());

    e.run_ok(
        "library.footprint.pad.add",
        json!({"footprint": DEMO_FOOTPRINT, "number": "9", "kind": "smd", "shape": "rect",
               "at": {"x": "5mm", "y": "0mm"}, "width": "1mm", "height": "1mm"}),
    );
    let scope = Scope::Element(DEMO_FOOTPRINT.parse().unwrap());
    assert!(e.dirty_scopes().contains(&scope));
    e.run_ok("edit.undo", json!({"document": DEMO_FOOTPRINT}));
    assert!(!e.dirty_scopes().contains(&scope));
    e.run_ok("edit.redo", json!({"document": DEMO_FOOTPRINT}));

    let out = e.run_ok("library.check", json!({"element": DEMO_FOOTPRINT}));
    assert!(out["violations"].is_array());

    e.run_ok("library.element.save", json!({"element": DEMO_FOOTPRINT}));
    assert!(!e.dirty_scopes().contains(&scope));
    let file = work
        .path()
        .join("user-lib")
        .join("footprints")
        .join("SOIC-8.redafp");
    let text = std::fs::read_to_string(file).unwrap();
    assert!(text.contains("\"number\":\"9\""), "{text}");

    e.run_ok("library.element.close", json!({"element": DEMO_FOOTPRINT}));
    assert!(e.element(DEMO_FOOTPRINT.parse().unwrap()).is_none());
}

#[test]
fn design_commands_stay_unavailable_with_only_elements_open() {
    let (home, work) = (TempDir::new("noproj2-home"), TempDir::new("noproj2-work"));
    let mut e = engine(&home, &work);
    e.run_ok("library.element.open", json!({"element": DEMO_FOOTPRINT}));
    let err = e.run_err("project.overview", json!({}));
    assert_eq!(err.code, ErrorCode::Unavailable);
    let err = e.run_err(
        "batch",
        json!({"commands": [{"id": "project.overview", "params": {}}]}),
    );
    assert_eq!(err.code, ErrorCode::Unavailable);
    // A batch of element edits runs on the open element.
    e.run_ok(
        "batch",
        json!({"commands": [{"id": "library.footprint.edit",
                             "params": {"footprint": DEMO_FOOTPRINT, "description": "M3"}}]}),
    );
}

#[test]
fn open_elements_and_their_undo_stacks_stay_open_when_a_project_opens() {
    let (home, work) = (TempDir::new("noproj3-home"), TempDir::new("noproj3-work"));
    let mut e = engine(&home, &work);
    e.run_ok("library.element.open", json!({"element": DEMO_FOOTPRINT}));
    e.run_ok(
        "library.footprint.edit",
        json!({"footprint": DEMO_FOOTPRINT, "description": "Changed"}),
    );
    e.run_ok("project.new", json!({"path": work.path(), "name": "Demo"}));
    let uuid = DEMO_FOOTPRINT.parse().unwrap();
    assert!(e.project().unwrap().element(uuid).is_some());
    assert!(e.dirty_scopes().contains(&Scope::Element(uuid)));
    e.run_ok("edit.undo", json!({"document": DEMO_FOOTPRINT}));
    assert!(!e.dirty_scopes().contains(&Scope::Element(uuid)));
}

#[test]
fn elements_edited_with_no_project_snapshot_into_the_per_user_folder_and_restore() {
    let (home, work) = (TempDir::new("noproj4-home"), TempDir::new("noproj4-work"));
    let mut e = engine(&home, &work);
    e.run_ok("library.element.open", json!({"element": DEMO_FOOTPRINT}));
    e.run_ok(
        "library.footprint.pad.add",
        json!({"footprint": DEMO_FOOTPRINT, "number": "9", "kind": "smd", "shape": "rect",
               "at": {"x": "5mm", "y": "0mm"}, "width": "1mm", "height": "1mm"}),
    );
    let folder = reda_engine::recovery::user_dir(e.user_dirs().unwrap());
    reda_engine::recovery::View::of(&e)
        .write(&folder, &e.dirty_scopes(), 5)
        .unwrap();

    // The app went away uncleanly; the next one starts with no Project open.
    let rows = reda_engine::recovery::scan(&folder, None);
    assert_eq!(rows.len(), 1, "{rows:?}");
    let mut after = engine(&home, &work);
    after.run_ok(
        "recovery.restore",
        json!({"snapshots": [rows[0].snapshot.file.clone()]}),
    );
    assert!(after.project().is_none());
    let scope = Scope::Element(DEMO_FOOTPRINT.parse().unwrap());
    assert!(after.dirty_scopes().contains(&scope));
    after.run_ok("library.element.save", json!({"element": DEMO_FOOTPRINT}));
    let file = work.path().join("user-lib/footprints/SOIC-8.redafp");
    let text = std::fs::read_to_string(file).unwrap();
    assert!(text.contains("\"number\":\"9\""), "{text}");
}
