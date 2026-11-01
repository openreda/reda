//! `batch {commands}`: one atomic edit across steps.

mod support;

use reda_command::ErrorCode;
use reda_model::{Scope, Uuid};
use serde_json::json;
use support::{Harness, pointer};

const SHEET: &str = "aaaaaaaa-0000-4000-8000-000000000001";

#[test]
fn later_steps_reference_objects_created_by_earlier_steps_through_caller_uuids() {
    let mut h = Harness::with_project();
    let project = h.project().uuid().to_string();
    let out = h
        .run(
            "batch",
            json!({"commands": [
                {"id": "test.sheet.add", "params": {"uuid": SHEET, "title": "Power"}},
                {"id": "test.sheet.retitle", "params": {"document": SHEET, "title": "Power Supply"}},
                {"id": "test.project.name"},
            ]}),
        )
        .unwrap();
    assert_eq!(
        out,
        json!({
            "changes": {"project": {"created": [SHEET], "modified": [project], "deleted": []}},
            "result": [SHEET, null, "at 1767225600000"]
        })
    );
    let sheet: Uuid = SHEET.parse().unwrap();
    assert_eq!(h.project().document(sheet).unwrap().title, "Power Supply");
    assert_eq!(h.session.history.dirty_scopes(), [Scope::Project]);
}

#[test]
fn a_query_step_sees_the_steps_before_it() {
    let mut h = Harness::with_project();
    let out = h
        .run(
            "batch",
            json!({"commands": [
                {"id": "test.project.rename", "params": {"name": "Blinky"}},
                {"id": "test.project.name", "params": {}},
            ]}),
        )
        .unwrap();
    assert_eq!(out["result"][1], "Blinky");
}

#[test]
fn a_failing_step_rolls_back_every_step() {
    let mut h = Harness::with_project();
    let before = h.project().clone();
    let err = h
        .run(
            "batch",
            json!({"commands": [
                {"id": "test.project.rename", "params": {"name": "Blinky"}},
                {"id": "test.sheet.add", "params": {"uuid": SHEET, "title": "Power"}},
                {"id": "test.sheet.retitle", "params": {"document": "bbbbbbbb-0000-4000-8000-000000000001", "title": "X"}},
            ]}),
        )
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::NotFound);
    assert_eq!(err.details.as_ref().unwrap()["step"], 2);
    assert!(
        err.message.starts_with("step 2 (test.sheet.retitle)"),
        "{}",
        err.message
    );
    assert_eq!(h.project(), &before);
    assert!(h.session.history.dirty_scopes().is_empty());
}

#[test]
fn bad_step_params_point_into_the_batch() {
    let mut h = Harness::with_project();
    let err = h
        .run(
            "batch",
            json!({"commands": [
                {"id": "test.project.rename", "params": {"name": "Blinky"}},
                {"id": "test.sheet.add", "params": {"title": 3}},
            ]}),
        )
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidParams);
    assert_eq!(pointer(&err), "/commands/1/params/title");
    assert_eq!(h.name(), "at 1767225600000");

    let err = h
        .run("batch", json!({"commands": [{"params": {}}]}))
        .unwrap_err();
    assert_eq!(pointer(&err), "/commands/0/id");
}

#[test]
fn file_view_and_batch_commands_cannot_be_steps() {
    let mut h = Harness::with_project();
    for id in ["test.project.create", "test.view.zoom:actual", "batch"] {
        let err = h
            .run("batch", json!({"commands": [{"id": id, "params": {}}]}))
            .unwrap_err();
        assert_eq!(err.code, ErrorCode::Rejected, "{id}: {}", err.message);
    }
}

#[test]
fn an_unknown_step_is_not_found() {
    let mut h = Harness::with_project();
    let err = h
        .run("batch", json!({"commands": [{"id": "test.nope"}]}))
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::NotFound);
    assert_eq!(err.details.unwrap()["step"], 0);
}

#[test]
fn a_batch_needs_an_open_project() {
    let mut h = Harness::new();
    let err = h.run("batch", json!({"commands": []})).unwrap_err();
    assert_eq!(err.code, ErrorCode::Unavailable);
}

#[test]
fn an_empty_batch_changes_nothing() {
    let mut h = Harness::with_project();
    let out = h.run("batch", json!({"commands": []})).unwrap();
    assert_eq!(out, json!({"changes": {}, "result": []}));
    assert!(h.session.history.dirty_scopes().is_empty());
}
