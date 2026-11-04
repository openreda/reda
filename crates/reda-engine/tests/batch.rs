//! `batch` on the engine: one atomic edit, one undo entry per touched Document.

use reda_engine::command::ErrorCode;
use reda_engine::model::Uuid;
use reda_testkit::{TempDir, TestEngine, probe_engine};
use serde_json::json;

const SHEET_1: &str = "00000000-0000-4000-8000-000000000002";
const P1: &str = "00000000-0000-4000-8000-0000000000a1";
const P0: &str = "00000000-0000-4000-8000-0000000000a0";

fn new_project(dir: &TempDir) -> TestEngine {
    let mut engine = probe_engine();
    engine.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
    engine.run_ok("project.save", json!({}));
    engine
}

fn probe(engine: &TestEngine, id: &str) -> Option<i64> {
    let uuid: Uuid = id.parse().unwrap();
    engine.project().unwrap().probe(uuid).map(|p| p.value)
}

#[test]
fn a_batch_leaves_one_undo_entry_per_touched_document() {
    let dir = TempDir::new("batch-undo");
    let mut engine = new_project(&dir);
    let out = engine.run_ok(
        "batch",
        json!({"commands": [
            {"id": "test.probe.set", "params": {"probes": [{"uuid": P1, "document": SHEET_1, "value": 1}]}},
            {"id": "test.probe.set", "params": {"probes": [{"uuid": P0, "document": null, "value": 7}]}},
            {"id": "test.probe.set", "params": {"probes": [{"uuid": P1, "document": SHEET_1, "value": 2}]}},
        ]}),
    );
    assert_eq!(out["changes"][SHEET_1]["created"], json!([P1]));
    assert_eq!(out["changes"]["project"]["created"], json!([P0]));
    assert!(engine.is_dirty());

    let undo = engine.run_ok("edit.undo", json!({"document": SHEET_1}));
    assert_eq!(undo["result"]["label"], "Batch");
    assert_eq!(probe(&engine, P1), None);
    assert_eq!(probe(&engine, P0), Some(7));
    engine.run_ok("edit.undo", json!({"document": "project"}));
    assert_eq!(probe(&engine, P0), None);
    assert!(!engine.is_dirty());
}

#[test]
fn undo_and_redo_cannot_be_batch_steps() {
    let dir = TempDir::new("batch-no-undo");
    let mut engine = new_project(&dir);
    engine.run_ok(
        "test.probe.set",
        json!({"probes": [{"uuid": P1, "document": SHEET_1, "value": 1}]}),
    );
    let err = engine.run_err(
        "batch",
        json!({"commands": [{"id": "edit.undo", "params": {"document": SHEET_1}}]}),
    );
    assert_eq!(err.code, ErrorCode::Rejected);
    assert_eq!(probe(&engine, P1), Some(1));
}
