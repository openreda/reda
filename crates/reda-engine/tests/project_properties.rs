//! Project properties: the title block's Project fields, stored in `.reda` and edited on
//! the Project's undo stack.

use reda_engine::command::ErrorCode;
use reda_testkit::{TempDir, read_tree, test_engine};
use serde_json::json;

const PROJECT: &str = "00000000-0000-4000-8000-000000000001";

#[test]
fn set_changes_only_the_given_properties_and_returns_them_all() {
    let dir = TempDir::new("props-set");
    let mut engine = test_engine();
    engine.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
    let out = engine.run_ok(
        "project.properties.set",
        json!({"title": "Motor driver", "revision": "B"}),
    );
    assert_eq!(out["changes"]["project"]["modified"], json!([PROJECT]));
    assert_eq!(
        out["result"],
        json!({"title": "Motor driver", "revision": "B", "date": "", "author": "", "company": ""})
    );
    let out = engine.run_ok("project.properties.set", json!({"author": "Ada"}));
    assert_eq!(out["result"]["title"], "Motor driver");
    assert_eq!(out["result"]["author"], "Ada");

    let got = engine.run_ok("object.get", json!({"uuid": PROJECT}));
    assert_eq!(got["object"]["properties"]["revision"], "B");
}

#[test]
fn properties_are_saved_in_the_project_file_and_read_back() {
    let dir = TempDir::new("props-save");
    let mut engine = test_engine();
    engine.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
    engine.run_ok(
        "project.properties.set",
        json!({"title": "Motor driver", "revision": "B", "date": "2026-11-01",
               "author": "Ada", "company": "Acme"}),
    );
    engine.run_ok("project.save", json!({}));
    let folder = dir.path().join("Demo");
    let tree = read_tree(&folder);
    let text = String::from_utf8(tree["Demo.reda"].clone()).unwrap();
    assert!(
        text.contains(
            r#"  "properties": {"title":"Motor driver","revision":"B","date":"2026-11-01","author":"Ada","company":"Acme"}"#
        ),
        "{text}"
    );

    let mut engine = test_engine();
    engine.run_ok("project.open", json!({"path": folder}));
    let got = engine.run_ok("object.get", json!({"uuid": PROJECT}));
    assert_eq!(got["object"]["properties"]["company"], "Acme");
    engine.run_ok("project.save", json!({}));
    assert_eq!(read_tree(&folder), tree);
}

#[test]
fn setting_properties_is_undoable_on_the_project_stack() {
    let dir = TempDir::new("props-undo");
    let mut engine = test_engine();
    engine.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
    engine.run_ok("project.properties.set", json!({"title": "X"}));
    engine.run_ok("edit.undo", json!({"document": "project"}));
    let got = engine.run_ok("object.get", json!({"uuid": PROJECT}));
    assert_eq!(got["object"]["properties"]["title"], "");
}

#[test]
fn set_needs_an_open_project() {
    let mut engine = test_engine();
    let err = engine.run_err("project.properties.set", json!({"title": "X"}));
    assert_eq!(err.code, ErrorCode::Unavailable);
}

#[test]
fn board_thickness_and_copper_weight_are_project_properties_for_fab_notes() {
    let dir = TempDir::new("props-board");
    let mut engine = test_engine();
    engine.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
    let out = engine.run_ok(
        "project.properties.set",
        json!({"board_thickness": "1.6mm", "copper_weight": "1 oz"}),
    );
    assert_eq!(out["result"]["board_thickness"], 1_600_000);
    assert_eq!(out["result"]["copper_weight"], "1 oz");
    engine.run_ok("project.save", json!({}));
    let folder = dir.path().join("Demo");
    let text = String::from_utf8(read_tree(&folder)["Demo.reda"].clone()).unwrap();
    assert!(
        text.contains(
            r#""properties": {"title":"","revision":"","date":"","author":"","company":"","board_thickness":1600000,"copper_weight":"1 oz"}"#
        ),
        "{text}"
    );

    // Zero clears the thickness, which is then left out.
    let out = engine.run_ok("project.properties.set", json!({"board_thickness": 0}));
    assert_eq!(out["result"].get("board_thickness"), None);
}

#[test]
fn a_negative_board_thickness_is_refused() {
    let dir = TempDir::new("props-board-negative");
    let mut engine = test_engine();
    engine.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
    let err = engine.run_err("project.properties.set", json!({"board_thickness": "-1mm"}));
    assert_eq!(err.code, ErrorCode::InvalidParams);
    assert_eq!(err.details.unwrap()["pointer"], "/board_thickness");
}
