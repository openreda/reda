//! `project.overview`, `object.get` and `objects.query` on what the model holds so far.

use reda_engine::command::ErrorCode;
use reda_testkit::{TempDir, TestEngine, test_engine};
use serde_json::json;

const PROJECT: &str = "00000000-0000-4000-8000-000000000001";
const SHEET: &str = "00000000-0000-4000-8000-000000000002";
const PCB: &str = "00000000-0000-4000-8000-000000000003";

fn demo(dir: &TempDir) -> TestEngine {
    let mut engine = test_engine();
    engine.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
    engine
}

#[test]
fn overview_lists_the_project_and_its_documents_in_order() {
    let dir = TempDir::new("overview");
    let mut engine = demo(&dir);
    assert_eq!(
        engine.run_ok("project.overview", json!({})),
        json!({
            "uuid": PROJECT,
            "name": "Demo",
            "documents": [{
                "uuid": SHEET,
                "kind": "schematic",
                "name": "Demo",
                "title": "Sheet 1",
                "objects": {}
            }, {
                "uuid": PCB,
                "kind": "pcb",
                "name": "Demo",
                "title": "Demo",
                "objects": {"layer_set": 1, "board_setup": 1, "net_class": 1}
            }]
        })
    );
}

#[test]
fn object_get_returns_any_object_with_its_document() {
    let dir = TempDir::new("object-get");
    let mut engine = demo(&dir);
    assert_eq!(
        engine.run_ok("object.get", json!({"uuid": SHEET})),
        json!({
            "document": null,
            "object": {"type": "document", "uuid": SHEET, "kind": "schematic", "name": "Demo", "title": "Sheet 1"}
        })
    );
    let mut project = engine.run_ok("object.get", json!({"uuid": PROJECT}));
    // The ERC severities ride along; erc.rs in reda-schematic tests them.
    let erc = project["object"].as_object_mut().unwrap().remove("erc");
    assert_eq!(erc.unwrap()["unconnected_input"], "error");
    let schematic = project["object"]
        .as_object_mut()
        .unwrap()
        .remove("schematic");
    assert_eq!(schematic.unwrap()["allow_ports_to_name_nets"], true);
    assert_eq!(
        project["object"],
        json!({"type": "project", "uuid": PROJECT, "name": "Demo", "documents": [SHEET, PCB],
               "properties": {"title": "", "revision": "", "date": "", "author": "", "company": ""}})
    );
}

#[test]
fn object_get_of_an_unknown_uuid_is_not_found() {
    let dir = TempDir::new("object-get-missing");
    let mut engine = demo(&dir);
    let err = engine.run_err(
        "object.get",
        json!({"uuid": "ffffffff-0000-4000-8000-000000000000"}),
    );
    assert_eq!(err.code, ErrorCode::NotFound);
}

#[test]
fn objects_query_filters_by_type_and_document() {
    let dir = TempDir::new("objects-query");
    let mut engine = demo(&dir);
    let all = engine.run_ok("objects.query", json!({}));
    let uuids: Vec<&str> = all["objects"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["object"]["uuid"].as_str().unwrap())
        .collect();
    assert_eq!(uuids.len(), 8, "{uuids:?}");
    assert_eq!(&uuids[..3], [PROJECT, SHEET, PCB]);
    let mut sorted = uuids.clone();
    sorted.sort();
    assert_eq!(uuids, sorted);

    let documents = engine.run_ok("objects.query", json!({"type": "document"}));
    assert_eq!(documents["objects"].as_array().unwrap().len(), 2);
    assert_eq!(documents["objects"][0]["object"]["uuid"], SHEET);

    let jobs = engine.run_ok("objects.query", json!({"type": "output_job"}));
    assert_eq!(jobs["objects"][0]["document"], json!(null));
    assert_eq!(jobs["objects"][0]["object"]["name"], "Generic");

    // A new sheet holds no objects yet; the Document's own entry lives in the Project file.
    let in_sheet = engine.run_ok("objects.query", json!({"document": SHEET}));
    assert_eq!(in_sheet, json!({"objects": []}));
}

#[test]
fn objects_query_refuses_an_unknown_document_or_type() {
    let dir = TempDir::new("objects-query-bad");
    let mut engine = demo(&dir);
    let err = engine.run_err(
        "objects.query",
        json!({"document": "ffffffff-0000-4000-8000-000000000000"}),
    );
    assert_eq!(err.code, ErrorCode::NotFound);
    let err = engine.run_err("objects.query", json!({"type": "spaceship"}));
    assert_eq!(err.code, ErrorCode::InvalidParams);
    assert_eq!(err.details.unwrap()["pointer"], "/type");
}

#[test]
fn a_batch_of_queries_returns_each_result_and_changes_nothing() {
    let dir = TempDir::new("batch-queries");
    let mut engine = demo(&dir);
    let out = engine.run_ok(
        "batch",
        json!({"commands": [
            {"id": "object.get", "params": {"uuid": SHEET}},
            {"id": "objects.query", "params": {"type": "project"}},
        ]}),
    );
    assert_eq!(out["changes"], json!({}));
    assert_eq!(out["result"][0]["object"]["title"], "Sheet 1");
    assert_eq!(out["result"][1]["objects"][0]["object"]["uuid"], PROJECT);

    let err = engine.run_err(
        "batch",
        json!({"commands": [{"id": "project.save", "params": {}}]}),
    );
    assert_eq!(err.code, ErrorCode::Rejected);
}

#[test]
fn queries_need_an_open_project() {
    let mut engine = test_engine();
    for id in ["project.overview", "objects.query"] {
        let err = engine.run_err(id, json!({}));
        assert_eq!(err.code, ErrorCode::Unavailable, "{id}");
    }
}
