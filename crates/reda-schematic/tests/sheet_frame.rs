//! The sheet frame: each sheet's size, and the built-in title block filled from the
//! Project properties and the sheet.

use reda_command::ErrorCode;
use reda_testkit::{TempDir, TestEngine, read_tree, test_engine};
use serde_json::json;

const SHEET: &str = "00000000-0000-4000-8000-000000000002";

fn demo(dir: &TempDir) -> TestEngine {
    let mut engine = test_engine();
    engine.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
    engine
}

#[test]
fn a_new_sheet_is_a4_and_its_title_block_shows_the_project_properties() {
    let dir = TempDir::new("frame-default");
    let mut engine = demo(&dir);
    engine.run_ok(
        "project.properties.set",
        json!({"title": "Motor driver", "revision": "B", "date": "2026-11-01",
               "author": "Ada", "company": "Acme"}),
    );
    let block = engine.run_ok("schematic.sheet.title_block", json!({"document": SHEET}));
    assert_eq!(
        block,
        json!({
            "title": "Motor driver",
            "revision": "B",
            "date": "2026-11-01",
            "author": "Ada",
            "company": "Acme",
            "sheet_title": "Sheet 1",
            "sheet_number": 1,
            "sheet_count": 1,
            "size": "a4",
            "width": 297_000_000,
            "height": 210_000_000
        })
    );
}

#[test]
fn set_size_is_stored_in_the_sheet_file_and_undoable_on_the_sheet_stack() {
    let dir = TempDir::new("frame-size");
    let mut engine = demo(&dir);
    let out = engine.run_ok(
        "schematic.sheet.set_size",
        json!({"document": SHEET, "size": "letter"}),
    );
    assert_eq!(
        out["result"],
        json!({"size": "letter", "width": 279_400_000, "height": 215_900_000})
    );
    assert_eq!(
        out["changes"][SHEET]["created"].as_array().unwrap().len(),
        1
    );
    let out = engine.run_ok(
        "schematic.sheet.set_size",
        json!({"document": SHEET, "size": "a3"}),
    );
    assert_eq!(
        out["changes"][SHEET]["modified"].as_array().unwrap().len(),
        1
    );
    engine.run_ok("project.save", json!({}));

    let folder = dir.path().join("Demo");
    let tree = read_tree(&folder);
    let sheet = String::from_utf8(tree["Demo.redasch"].clone()).unwrap();
    assert!(sheet.contains(r#""size":"a3""#), "{sheet}");

    let mut reopened = test_engine();
    reopened.run_ok("project.open", json!({"path": folder}));
    let block = reopened.run_ok("schematic.sheet.title_block", json!({"document": SHEET}));
    assert_eq!(block["size"], "a3");
    reopened.run_ok("project.save", json!({}));
    assert_eq!(read_tree(&folder), tree);

    engine.run_ok("edit.undo", json!({"document": SHEET}));
    let block = engine.run_ok("schematic.sheet.title_block", json!({"document": SHEET}));
    assert_eq!(block["size"], "letter");
}

#[test]
fn sheet_n_of_m_follows_the_document_order() {
    let dir = TempDir::new("frame-order");
    let mut engine = demo(&dir);
    let added = engine.run_ok("document.add", json!({"kind": "schematic"}));
    let second = added["result"]["uuid"].as_str().unwrap().to_owned();
    let block = engine.run_ok("schematic.sheet.title_block", json!({"document": second}));
    assert_eq!(block["sheet_number"], 2);
    assert_eq!(block["sheet_count"], 2);
    assert_eq!(block["sheet_title"], "Sheet 2");
    let block = engine.run_ok("schematic.sheet.title_block", json!({"document": SHEET}));
    assert_eq!(block["sheet_number"], 1);
}

#[test]
fn an_unknown_sheet_is_not_found() {
    let dir = TempDir::new("frame-missing");
    let mut engine = demo(&dir);
    let other = "00000000-0000-4000-8000-0000000000ff";
    let err = engine.run_err("schematic.sheet.title_block", json!({"document": other}));
    assert_eq!(err.code, ErrorCode::NotFound);
    let err = engine.run_err(
        "schematic.sheet.set_size",
        json!({"document": other, "size": "a4"}),
    );
    assert_eq!(err.code, ErrorCode::NotFound);
}
