//! `project.library.add` and `project.library.remove`: Project Library paths, undoable on
//! the Project stack.

use reda_command::ErrorCode;
use reda_model::Scope;
use reda_testkit::{TempDir, TestEngine, test_engine};
use serde_json::json;

fn new_project(dir: &TempDir) -> TestEngine {
    let mut engine = test_engine();
    engine.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
    engine.run_ok("project.save", json!({}));
    engine
}

fn libraries(engine: &TestEngine) -> Vec<String> {
    engine.project().unwrap().props().libraries.clone()
}

#[test]
fn adding_a_library_path_is_an_undoable_project_edit() {
    let dir = TempDir::new("lib-add");
    let mut engine = new_project(&dir);
    let project = engine.project().unwrap().uuid().to_string();

    let out = engine.run_ok("project.library.add", json!({"path": "parts"}));
    assert_eq!(out["result"]["libraries"], json!(["parts"]));
    assert_eq!(out["changes"]["project"]["modified"], json!([project]));
    engine.run_ok("project.library.add", json!({"path": "../shared/libs"}));
    assert_eq!(libraries(&engine), ["parts", "../shared/libs"]);
    assert_eq!(engine.dirty_scopes(), [Scope::Project]);

    let out = engine.run_ok("edit.undo", json!({"document": "project"}));
    assert_eq!(out["result"]["label"], "Add Project Library");
    assert_eq!(libraries(&engine), ["parts"]);
    engine.run_ok("edit.undo", json!({"document": "project"}));
    assert_eq!(libraries(&engine), Vec::<String>::new());
    assert!(!engine.is_dirty());
    engine.run_ok("edit.redo", json!({"document": "project"}));
    assert_eq!(libraries(&engine), ["parts"]);
}

#[test]
fn removing_a_library_path_is_an_undoable_project_edit() {
    let dir = TempDir::new("lib-remove");
    let mut engine = new_project(&dir);
    engine.run_ok("project.library.add", json!({"path": "a"}));
    engine.run_ok("project.library.add", json!({"path": "b"}));

    let out = engine.run_ok("project.library.remove", json!({"path": "a"}));
    assert_eq!(out["result"]["libraries"], json!(["b"]));
    assert_eq!(libraries(&engine), ["b"]);

    let out = engine.run_ok("edit.undo", json!({"document": "project"}));
    assert_eq!(out["result"]["label"], "Remove Project Library");
    assert_eq!(libraries(&engine), ["a", "b"]);
}

#[test]
fn paths_are_stored_normalised() {
    let dir = TempDir::new("lib-normalise");
    let mut engine = new_project(&dir);
    engine.run_ok("project.library.add", json!({"path": "./libs\\shared/"}));
    assert_eq!(libraries(&engine), ["libs/shared"]);
    // The same folder spelled differently is found for removal and refused as a duplicate.
    let err = engine.run_err("project.library.add", json!({"path": "libs/./shared"}));
    assert_eq!(err.code, ErrorCode::Rejected);
    engine.run_ok("project.library.remove", json!({"path": "libs\\shared"}));
    assert_eq!(libraries(&engine), Vec::<String>::new());
}

#[test]
fn bad_paths_are_refused() {
    let dir = TempDir::new("lib-bad");
    let mut engine = new_project(&dir);
    for path in [
        "",
        ".",
        "/abs/libs",
        "\\\\server\\share",
        "C:\\libs",
        "c:/libs",
    ] {
        let err = engine.run_err("project.library.add", json!({ "path": path }));
        assert_eq!(err.code, ErrorCode::InvalidParams, "{path:?}: {err}");
        assert_eq!(err.details, Some(json!({"pointer": "/path"})), "{path:?}");
    }
    let err = engine.run_err("project.library.remove", json!({"path": "missing"}));
    assert_eq!(err.code, ErrorCode::NotFound);
    assert!(!engine.is_dirty());
}

#[test]
fn library_paths_are_saved_in_the_project_file_and_read_back() {
    let dir = TempDir::new("lib-save");
    let mut engine = new_project(&dir);
    engine.run_ok("project.library.add", json!({"path": "parts"}));
    engine.run_ok("project.library.add", json!({"path": "../shared"}));
    engine.run_ok("project.save", json!({}));

    let file = dir.path().join("Demo").join("Demo.reda");
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(
        text.contains("  \"libraries\": [\n    \"parts\",\n    \"../shared\"\n  ],\n"),
        "{text}"
    );

    let mut reopened = test_engine();
    reopened.run_ok("project.open", json!({ "path": file }));
    assert_eq!(libraries(&reopened), ["parts", "../shared"]);
}

#[test]
fn a_project_without_library_paths_writes_no_libraries_key() {
    let dir = TempDir::new("lib-none");
    let engine = new_project(&dir);
    drop(engine);
    let text = std::fs::read_to_string(dir.path().join("Demo").join("Demo.reda")).unwrap();
    assert!(!text.contains("libraries"), "{text}");
}
