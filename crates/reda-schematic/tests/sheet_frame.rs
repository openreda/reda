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
            "shown": true,
            "title": "Motor driver",
            "revision": "B",
            "revision_from_project": true,
            "drawing_number": "",
            "date": "2026-11-01",
            "author": "Ada",
            "company": "Acme",
            "sheet_title": "Sheet 1",
            "sheet_number": 1,
            "sheet_count": 1,
            "size": "a4",
            "orientation": "landscape",
            "width": 297_000_000,
            "height": 210_000_000,
            "zones": {"columns": 6, "rows": 4}
        })
    );
}

#[test]
fn the_frame_is_stored_in_the_sheet_file_and_undoable_on_the_sheet_stack() {
    let dir = TempDir::new("frame-size");
    let mut engine = demo(&dir);
    let out = engine.run_ok(
        "schematic.sheet.set_frame",
        json!({"document": SHEET, "size": "letter"}),
    );
    assert_eq!(
        out["result"],
        json!({"size": "letter", "orientation": "landscape", "width": 279_400_000,
               "height": 215_900_000, "zones": {"columns": 6, "rows": 4},
               "title_block": true, "drawing_number": "", "revision": ""})
    );
    assert_eq!(
        out["changes"][SHEET]["created"].as_array().unwrap().len(),
        1
    );
    let out = engine.run_ok(
        "schematic.sheet.set_frame",
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
        "schematic.sheet.set_frame",
        json!({"document": other, "size": "a4"}),
    );
    assert_eq!(err.code, ErrorCode::NotFound);
}

#[test]
fn a_new_sheets_frame_has_border_zones_and_a_title_block() {
    let dir = TempDir::new("frame-defaults");
    let mut engine = demo(&dir);
    let block = engine.run_ok("schematic.sheet.title_block", json!({"document": SHEET}));
    assert_eq!(block["orientation"], "landscape");
    assert_eq!(block["shown"], true);
    // About 50 mm each: A4 is 297 x 210 mm.
    assert_eq!(block["zones"], json!({"columns": 6, "rows": 4}));
    assert_eq!(block["drawing_number"], "");
}

#[test]
fn a_custom_portrait_sheet_turns_its_longer_side_upright_and_derives_its_zones() {
    let dir = TempDir::new("frame-custom");
    let mut engine = demo(&dir);
    let out = engine.run_ok(
        "schematic.sheet.set_frame",
        json!({"document": SHEET, "size": {"width": "400mm", "height": "250mm"},
               "orientation": "portrait"}),
    );
    let frame = &out["result"];
    assert_eq!(frame["width"], 250_000_000);
    assert_eq!(frame["height"], 400_000_000);
    assert_eq!(frame["zones"], json!({"columns": 5, "rows": 8}));

    // The other settings stay when only one changes.
    let out = engine.run_ok(
        "schematic.sheet.set_frame",
        json!({"document": SHEET, "border_zones": false}),
    );
    assert_eq!(out["result"]["zones"], json!(null));
    assert_eq!(out["result"]["orientation"], "portrait");
    assert_eq!(
        out["result"]["size"],
        json!({"width": 400_000_000, "height": 250_000_000})
    );
}

#[test]
fn a_sheet_has_its_own_drawing_number_and_revision_or_shows_the_projects() {
    let dir = TempDir::new("frame-revision");
    let mut engine = demo(&dir);
    engine.run_ok("project.properties.set", json!({"revision": "B"}));
    let block = engine.run_ok("schematic.sheet.title_block", json!({"document": SHEET}));
    assert_eq!(block["revision"], "B");
    assert_eq!(block["revision_from_project"], true);

    engine.run_ok(
        "schematic.sheet.set_frame",
        json!({"document": SHEET, "drawing_number": "DWG-0042", "revision": "C",
               "title_block": false}),
    );
    let block = engine.run_ok("schematic.sheet.title_block", json!({"document": SHEET}));
    assert_eq!(block["revision"], "C");
    assert_eq!(block["revision_from_project"], false);
    assert_eq!(block["drawing_number"], "DWG-0042");
    assert_eq!(block["shown"], false);

    engine.run_ok("project.save", json!({}));
    let tree = read_tree(&dir.path().join("Demo"));
    let sheet = String::from_utf8(tree["Demo.redasch"].clone()).unwrap();
    assert!(
        sheet.contains(r#""drawing_number":"DWG-0042""#)
            && sheet.contains(r#""title_block":false"#),
        "{sheet}"
    );
    // Settings left at their defaults are not written.
    assert!(!sheet.contains("border_zones"), "{sheet}");
}

#[test]
fn a_custom_size_needs_two_positive_lengths() {
    let dir = TempDir::new("frame-bad-size");
    let mut engine = demo(&dir);
    let err = engine.run_err(
        "schematic.sheet.set_frame",
        json!({"document": SHEET, "size": {"width": "0mm", "height": "200mm"}}),
    );
    assert_eq!(err.code, ErrorCode::InvalidParams);
}
