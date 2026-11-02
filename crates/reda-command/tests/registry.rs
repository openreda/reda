//! Drives a registry of toy Commands the way the engine, MCP and the CLI do.

mod support;

use reda_command::{Clock, CommandError, ErrorCode, FixedClock, Kind, Registry};
use reda_model::Scope;
use serde_json::json;
use support::{Harness, Name, ZoomLevel, pointer};

#[test]
fn edit_returns_the_change_envelope_and_dirties_its_scope() {
    let mut h = Harness::with_project();
    let project = h.session.project.as_ref().unwrap().uuid();
    let out = h
        .run("test.project.rename", json!({"name": "Blinky"}))
        .unwrap();
    assert_eq!(
        out,
        json!({
            "changes": {"project": {"created": [], "modified": [project.to_string()], "deleted": []}},
            "result": {"old": "at 1767225600000"}
        })
    );
    assert_eq!(h.name(), "Blinky");
    assert_eq!(h.session.history.dirty_scopes(), [Scope::Project]);
    assert_eq!(
        h.session.history.next_undo(Scope::Project).unwrap().label,
        "Rename Project"
    );
}

#[test]
fn a_failed_edit_rolls_back_fully() {
    let mut h = Harness::with_project();
    let err = h
        .run("test.project.rename", json!({"name": "fail-after-edit"}))
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::Rejected);
    assert_eq!(h.name(), "at 1767225600000");
    assert!(h.session.history.dirty_scopes().is_empty());
    assert!(h.session.history.next_undo(Scope::Project).is_none());
}

#[test]
fn query_returns_its_result_bare() {
    let mut h = Harness::with_project();
    assert_eq!(
        h.run("test.project.name", json!({})).unwrap(),
        json!("at 1767225600000")
    );
}

#[test]
fn only_edit_commands_may_open_a_transaction() {
    let mut h = Harness::with_project();
    let err = h.run("test.sneaky", json!({})).unwrap_err();
    assert_eq!(err.code, ErrorCode::Internal);
}

#[test]
fn bad_params_are_invalid_params() {
    let mut h = Harness::with_project();
    for (params, at) in [
        (json!({"name": 3}), "/name"),
        (json!({}), "/name"),
        (json!({"name": "x", "extra": 1}), "/extra"),
    ] {
        let err = h.run("test.project.rename", params.clone()).unwrap_err();
        assert_eq!(err.code, ErrorCode::InvalidParams, "{params}");
        assert_eq!(pointer(&err), at, "{params}: {}", err.message);
    }
}

#[test]
fn unknown_command_is_not_found() {
    let mut h = Harness::new();
    let err = h.run("test.nope", json!({})).unwrap_err();
    assert_eq!(err.code, ErrorCode::NotFound);
    assert!(err.message.contains("test.nope"), "{}", err.message);
}

#[test]
fn view_commands_are_unavailable_headless() {
    let mut h = Harness::with_project();
    let err = h.run("test.view.zoom", json!({"level": 100})).unwrap_err();
    assert_eq!(err.code, ErrorCode::Unavailable);
    assert_eq!(err.message, "requires the REDA app");
}

#[test]
fn a_view_command_changes_the_view_state_the_app_keeps_in_the_session() {
    let mut h = Harness::new();
    h.headless = false;
    h.session.view.set(ZoomLevel(100));
    assert_eq!(
        h.run("test.view.zoom", json!({"level": 150})).unwrap(),
        json!(150)
    );
    assert_eq!(h.session.view.get::<ZoomLevel>(), Some(&ZoomLevel(150)));

    let err = h.run("test.view.zoom", json!({"level": 0})).unwrap_err();
    assert_eq!(err.code, ErrorCode::Rejected);
    assert_eq!(h.session.view.get::<ZoomLevel>(), Some(&ZoomLevel(150)));
}

#[test]
fn a_view_command_without_its_view_state_is_unavailable() {
    let mut h = Harness::new();
    h.headless = false;
    let err = h.run("test.view.zoom", json!({"level": 150})).unwrap_err();
    assert_eq!(err.code, ErrorCode::Unavailable);
    assert_eq!(err.message, "requires the REDA app");
}

#[test]
fn edits_and_queries_need_an_open_project() {
    let mut h = Harness::new();
    for id in ["test.project.rename", "test.project.name"] {
        let err = h.run(id, json!({"name": "x"})).unwrap_err();
        assert_eq!(err.code, ErrorCode::Unavailable, "{id}");
    }
}

#[test]
fn file_commands_use_the_injected_ids_and_clock() {
    let mut h = Harness::new();
    let out = h.run("test.project.create", json!({})).unwrap();
    assert_eq!(out, json!("00000000-0000-4000-8000-000000000001"));
    assert_eq!(h.name(), format!("at {}", FixedClock::EPOCH.now().0));
}

#[test]
fn errors_serialise_as_code_and_message() {
    let err = CommandError::not_found("no such thing");
    assert_eq!(
        serde_json::to_value(&err).unwrap(),
        json!({"code": "not_found", "message": "no such thing"})
    );
    let err = CommandError::invalid_params("/name", "expected a string");
    assert_eq!(
        serde_json::to_value(&err).unwrap(),
        json!({"code": "invalid_params", "message": "expected a string", "details": {"pointer": "/name"}})
    );
}

#[test]
fn the_registry_lists_metadata_sorted_by_id() {
    let h = Harness::new();
    let ids: Vec<&str> = h.registry.commands().map(|m| m.id).collect();
    assert_eq!(
        ids,
        [
            "batch",
            "test.measure",
            "test.project.create",
            "test.project.name",
            "test.project.rename",
            "test.project.title",
            "test.sheet.add",
            "test.sheet.retitle",
            "test.sneaky",
            "test.view.zoom"
        ]
    );
    assert_eq!(h.registry.get("test.view.zoom").unwrap().kind, Kind::View);
}

#[test]
#[should_panic(expected = "test.project.name")]
fn registering_an_id_twice_panics() {
    let mut registry = Registry::new();
    registry.add(Name);
    registry.add(Name);
}
