//! Scenario tests for undo, redo, per-Document dirty state and per-Document save.
//!
//! Real Document object kinds arrive in later tickets, so these drive the test-only Probe
//! object through `reda_testkit::probe_engine()`.

use reda_engine::command::ErrorCode;
use reda_engine::model::{Scope, Uuid};
use reda_testkit::{TempDir, TestEngine, probe_engine, read_tree};
use serde_json::{Value, json};

const SHEET_1: &str = "00000000-0000-4000-8000-000000000002";

fn uuid(s: &str) -> Uuid {
    s.parse().unwrap()
}

fn new_project(dir: &TempDir) -> TestEngine {
    let mut engine = probe_engine();
    engine.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
    engine
}

/// Adds a second sheet (an edit on the Project stack) and returns its UUID.
fn add_sheet(engine: &mut TestEngine, name: &str) -> String {
    let out = engine.run_ok("test.sheet.add", json!({"name": name}));
    out["result"]["uuid"].as_str().unwrap().to_owned()
}

fn set_probes(engine: &mut TestEngine, probes: Value) -> Value {
    engine.run_ok("test.probe.set", json!({ "probes": probes }))
}

fn probe(engine: &TestEngine, id: &str) -> Option<i64> {
    engine.project().unwrap().probe(uuid(id)).map(|p| p.value)
}

const P1: &str = "00000000-0000-4000-8000-0000000000a1";
const P2: &str = "00000000-0000-4000-8000-0000000000a2";
const P0: &str = "00000000-0000-4000-8000-0000000000a0";

#[test]
fn undo_and_redo_a_single_document_edit() {
    let dir = TempDir::new("undo-single");
    let mut engine = new_project(&dir);
    set_probes(
        &mut engine,
        json!([{"uuid": P1, "document": SHEET_1, "value": 1}]),
    );
    set_probes(
        &mut engine,
        json!([{"uuid": P1, "document": SHEET_1, "value": 2}]),
    );

    let out = engine.run_ok("edit.undo", json!({"document": SHEET_1}));
    assert_eq!(probe(&engine, P1), Some(1));
    assert_eq!(out["result"]["label"], "Set Probes");
    assert_eq!(out["changes"][SHEET_1]["modified"], json!([P1]));

    engine.run_ok("edit.undo", json!({"document": SHEET_1}));
    assert_eq!(probe(&engine, P1), None);

    engine.run_ok("edit.redo", json!({"document": SHEET_1}));
    assert_eq!(probe(&engine, P1), Some(1));
    engine.run_ok("edit.redo", json!({"document": SHEET_1}));
    assert_eq!(probe(&engine, P1), Some(2));
}

#[test]
fn a_cross_document_edit_leaves_one_independent_entry_per_stack() {
    let dir = TempDir::new("undo-cross");
    let mut engine = new_project(&dir);
    let sheet_2 = add_sheet(&mut engine, "second");
    set_probes(
        &mut engine,
        json!([
            {"uuid": P0, "document": null, "value": 10},
            {"uuid": P1, "document": SHEET_1, "value": 11},
            {"uuid": P2, "document": sheet_2, "value": 12},
        ]),
    );

    // Undo only sheet 2's part: the other two Documents keep theirs.
    engine.run_ok("edit.undo", json!({"document": sheet_2}));
    assert_eq!(probe(&engine, P2), None);
    assert_eq!(probe(&engine, P1), Some(11));
    assert_eq!(probe(&engine, P0), Some(10));

    // Sheet 1's stack held exactly one entry.
    engine.run_ok("edit.undo", json!({"document": SHEET_1}));
    assert_eq!(probe(&engine, P1), None);
    let err = engine.run_err("edit.undo", json!({"document": SHEET_1}));
    assert_eq!(err.code, ErrorCode::Unavailable);

    // The Project stack holds the Probe part, then the sheet add underneath it.
    engine.run_ok("edit.undo", json!({"document": "project"}));
    assert_eq!(probe(&engine, P0), None);
    assert!(engine.project().unwrap().document(uuid(&sheet_2)).is_some());
    engine.run_ok("edit.undo", json!({"document": "project"}));
    assert!(engine.project().unwrap().document(uuid(&sheet_2)).is_none());

    // Redo in a different order than the edits were made.
    engine.run_ok("edit.redo", json!({"document": "project"}));
    engine.run_ok("edit.redo", json!({"document": SHEET_1}));
    assert_eq!(probe(&engine, P1), Some(11));
    assert_eq!(probe(&engine, P0), None);
    assert_eq!(probe(&engine, P2), None);
}

#[test]
fn a_new_edit_clears_redo_only_on_the_stacks_it_touches() {
    let dir = TempDir::new("undo-redo-clear");
    let mut engine = new_project(&dir);
    let sheet_2 = add_sheet(&mut engine, "second");
    set_probes(
        &mut engine,
        json!([{"uuid": P1, "document": SHEET_1, "value": 1}]),
    );
    set_probes(
        &mut engine,
        json!([{"uuid": P2, "document": sheet_2, "value": 2}]),
    );
    engine.run_ok("edit.undo", json!({"document": SHEET_1}));
    engine.run_ok("edit.undo", json!({"document": sheet_2}));

    set_probes(
        &mut engine,
        json!([{"uuid": P1, "document": SHEET_1, "value": 3}]),
    );

    let err = engine.run_err("edit.redo", json!({"document": SHEET_1}));
    assert_eq!(err.code, ErrorCode::Unavailable);
    engine.run_ok("edit.redo", json!({"document": sheet_2}));
    assert_eq!(probe(&engine, P2), Some(2));
}

#[test]
fn undo_names_an_unknown_stack_as_invalid_params() {
    let dir = TempDir::new("undo-bad-stack");
    let mut engine = new_project(&dir);
    let err = engine.run_err("edit.undo", json!({"document": "sheet one"}));
    assert_eq!(err.code, ErrorCode::InvalidParams);
}

#[test]
fn dirty_state_is_per_document_and_follows_undo_back_to_the_saved_state() {
    let dir = TempDir::new("undo-dirty");
    let mut engine = new_project(&dir);
    let sheet_2 = add_sheet(&mut engine, "second");
    engine.run_ok("project.save", json!({}));
    assert!(!engine.is_dirty());

    set_probes(
        &mut engine,
        json!([{"uuid": P1, "document": SHEET_1, "value": 1}]),
    );
    assert_eq!(engine.dirty_scopes(), [Scope::Document(uuid(SHEET_1))]);

    engine.run_ok("edit.undo", json!({"document": SHEET_1}));
    assert!(!engine.is_dirty(), "back at the saved state");
    engine.run_ok("edit.redo", json!({"document": SHEET_1}));
    assert_eq!(engine.dirty_scopes(), [Scope::Document(uuid(SHEET_1))]);

    set_probes(
        &mut engine,
        json!([{"uuid": P0, "document": null, "value": 1}]),
    );
    set_probes(
        &mut engine,
        json!([{"uuid": P2, "document": sheet_2, "value": 1}]),
    );
    assert_eq!(
        engine.dirty_scopes(),
        [
            Scope::Project,
            Scope::Document(uuid(SHEET_1)),
            Scope::Document(uuid(&sheet_2)),
        ]
    );
}

#[test]
fn undoing_past_the_saved_state_dirties_again() {
    let dir = TempDir::new("undo-past-save");
    let mut engine = new_project(&dir);
    set_probes(
        &mut engine,
        json!([{"uuid": P1, "document": SHEET_1, "value": 1}]),
    );
    engine.run_ok("project.save", json!({}));
    engine.run_ok("edit.undo", json!({"document": SHEET_1}));
    assert_eq!(engine.dirty_scopes(), [Scope::Document(uuid(SHEET_1))]);
}

#[test]
fn saving_one_document_writes_only_its_file_and_clears_only_its_dirty_state() {
    let dir = TempDir::new("save-one");
    let mut engine = new_project(&dir);
    let folder = dir.path().join("Demo");
    // project.new wrote everything; clear the folder to see what each save writes.
    std::fs::remove_dir_all(&folder).unwrap();
    std::fs::create_dir(&folder).unwrap();
    let sheet_2 = add_sheet(&mut engine, "second");
    set_probes(
        &mut engine,
        json!([{"uuid": P1, "document": SHEET_1, "value": 1}]),
    );

    let out = engine.run_ok("project.save", json!({"document": sheet_2}));
    assert_eq!(out["written"], json!(["second.redasch"]));
    let tree = read_tree(&folder);
    assert_eq!(tree.keys().collect::<Vec<_>>(), ["second.redasch"]);
    assert_eq!(
        engine.dirty_scopes(),
        [Scope::Project, Scope::Document(uuid(SHEET_1))]
    );

    let out = engine.run_ok("project.save", json!({"document": "project"}));
    assert_eq!(out["written"], json!([".reda/.gitignore", "Demo.reda"]));
    assert_eq!(engine.dirty_scopes(), [Scope::Document(uuid(SHEET_1))]);

    engine.run_ok("project.save", json!({}));
    assert!(!engine.is_dirty());
}

#[test]
fn saving_an_unknown_document_is_not_found() {
    let dir = TempDir::new("save-unknown");
    let mut engine = new_project(&dir);
    let err = engine.run_err(
        "project.save",
        json!({"document": "00000000-0000-4000-8000-0000000000ff"}),
    );
    assert_eq!(err.code, ErrorCode::NotFound);
}

#[test]
fn undo_is_unlimited_by_default() {
    let dir = TempDir::new("undo-unlimited");
    let mut engine = new_project(&dir);
    for value in 0..200 {
        set_probes(
            &mut engine,
            json!([{"uuid": P1, "document": SHEET_1, "value": value}]),
        );
    }
    for _ in 0..200 {
        engine.run_ok("edit.undo", json!({"document": SHEET_1}));
    }
    assert_eq!(probe(&engine, P1), None);
}

#[test]
fn the_undo_depth_caps_each_document_stack() {
    let dir = TempDir::new("undo-depth");
    let mut engine = new_project(&dir);
    engine.set_undo_depth(Some(2));
    engine.run_ok("project.save", json!({}));
    for value in 1..=3 {
        set_probes(
            &mut engine,
            json!([{"uuid": P1, "document": SHEET_1, "value": value}]),
        );
    }
    engine.run_ok("edit.undo", json!({"document": SHEET_1}));
    engine.run_ok("edit.undo", json!({"document": SHEET_1}));
    assert_eq!(probe(&engine, P1), Some(1));
    let err = engine.run_err("edit.undo", json!({"document": SHEET_1}));
    assert_eq!(err.code, ErrorCode::Unavailable);

    // The trimmed state is still dirty: the oldest edit can no longer be undone.
    assert_eq!(engine.dirty_scopes(), [Scope::Document(uuid(SHEET_1))]);
}

#[test]
fn lowering_the_undo_depth_trims_existing_stacks() {
    let dir = TempDir::new("undo-depth-lower");
    let mut engine = new_project(&dir);
    for value in 1..=3 {
        set_probes(
            &mut engine,
            json!([{"uuid": P1, "document": SHEET_1, "value": value}]),
        );
    }
    engine.set_undo_depth(Some(1));
    engine.run_ok("edit.undo", json!({"document": SHEET_1}));
    assert_eq!(probe(&engine, P1), Some(2));
    let err = engine.run_err("edit.undo", json!({"document": SHEET_1}));
    assert_eq!(err.code, ErrorCode::Unavailable);
}

#[test]
fn opening_a_project_starts_with_empty_stacks() {
    let dir = TempDir::new("undo-open");
    let mut engine = new_project(&dir);
    set_probes(
        &mut engine,
        json!([{"uuid": P1, "document": SHEET_1, "value": 1}]),
    );
    engine.run_ok("project.save", json!({}));
    engine.run_ok("project.open", json!({"path": dir.path().join("Demo")}));
    let err = engine.run_err("edit.undo", json!({"document": SHEET_1}));
    assert_eq!(err.code, ErrorCode::Unavailable);
}

#[test]
fn a_bridged_edit_is_labelled_with_its_source_in_undo() {
    use reda_engine::command::Signals;
    use reda_engine::journal::Source;
    let dir = TempDir::new("undo-source-label");
    let mut engine = new_project(&dir);
    let probes =
        |value: i64| json!({"probes": [{"uuid": P1, "document": SHEET_1, "value": value}]});
    // The engine's own source (the test engine is a CLI one) gets no prefix.
    set_probes(
        &mut engine,
        json!([{"uuid": P1, "document": SHEET_1, "value": 1}]),
    );
    engine
        .run_from(
            Source::Mcp,
            "test.probe.set",
            probes(2),
            &Signals::default(),
        )
        .unwrap();
    let step = json!({"id": "test.probe.set", "params": probes(3)});
    engine
        .run_from(
            Source::Gui,
            "batch",
            json!({ "commands": [step] }),
            &Signals::default(),
        )
        .unwrap();

    let undo = |engine: &mut TestEngine| {
        let out = engine.run_ok("edit.undo", json!({"document": SHEET_1}));
        out["result"]["label"].as_str().unwrap().to_owned()
    };
    assert_eq!(undo(&mut engine), "GUI: Batch");
    assert_eq!(undo(&mut engine), "MCP: Set Probes");
    assert_eq!(undo(&mut engine), "Set Probes");
    // Redo keeps the label the entry was recorded with.
    let out = engine.run_ok("edit.redo", json!({"document": SHEET_1}));
    assert_eq!(out["result"]["label"], "Set Probes");
    let out = engine.run_ok("edit.redo", json!({"document": SHEET_1}));
    assert_eq!(out["result"]["label"], "MCP: Set Probes");
}
