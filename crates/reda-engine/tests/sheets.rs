//! Several Schematic Sheets: adding, renaming, reordering and deleting Documents on the
//! Project's undo stack, and how those edits reach disk on save.

use std::path::{Path, PathBuf};

use reda_engine::command::ErrorCode;
use reda_engine::model::{Scope, Uuid};
use reda_testkit::{TempDir, TestEngine, read_tree, test_engine_with_trash};
use serde_json::{Value, json};

/// A saved new Project "Demo" in `dir`, on an engine whose trash is the folder
/// `dir/trash`; returns the engine and the Project folder.
fn new_project(dir: &Path) -> (TestEngine, PathBuf) {
    let mut engine = test_engine_with_trash(&dir.join("trash"));
    engine.run_ok("project.new", json!({"path": dir, "name": "Demo"}));
    (engine, dir.join("Demo"))
}

fn trashed(dir: &Path) -> Vec<String> {
    read_tree(&dir.join("trash")).into_keys().collect()
}

fn read(folder: &Path, name: &str) -> Vec<u8> {
    std::fs::read(folder.join(name)).unwrap()
}

#[test]
fn renaming_a_saved_sheet_renames_its_file_on_save() {
    let dir = TempDir::new("sheets-rename-save");
    let (mut engine, folder) = new_project(dir.path());
    let sheet = add_sheet(&mut engine);
    engine.run_ok("project.save", json!({}));
    let bytes = read(&folder, "sheet-2.redasch");

    engine.run_ok(
        "document.rename",
        json!({"document": sheet, "title": "Power"}),
    );
    assert!(
        folder.join("sheet-2.redasch").exists(),
        "nothing moves before save"
    );
    let out = engine.run_ok("project.save", json!({"document": "project"}));
    assert_eq!(
        out["renamed"],
        json!([{"from": "sheet-2.redasch", "to": "Power.redasch"}])
    );
    assert_eq!(
        files(&folder),
        [
            ".reda/.gitignore",
            "Demo.reda",
            "Demo.redapcb",
            "Demo.redasch",
            "Power.redasch"
        ]
    );
    assert_eq!(read(&folder, "Power.redasch"), bytes);
    assert!(!engine.is_dirty());
    assert!(trashed(dir.path()).is_empty());

    // The saved Project opens with the renamed sheet.
    let mut other = test_engine_with_trash(&dir.path().join("trash"));
    let out = other.run_ok("project.open", json!({"path": folder}));
    assert_eq!(out["documents"][2]["name"], "Power");
}

#[test]
fn swapping_two_sheet_names_and_changing_case_survive_save() {
    let dir = TempDir::new("sheets-swap");
    let (mut engine, folder) = new_project(dir.path());
    let a = add_sheet(&mut engine);
    let b = add_sheet(&mut engine);
    engine.run_ok("document.rename", json!({"document": a, "title": "A"}));
    engine.run_ok("document.rename", json!({"document": b, "title": "B"}));
    engine.run_ok("project.save", json!({}));
    engine.run_ok(
        "schematic.sheet.set_size",
        json!({"document": a, "size": "a3"}),
    );
    engine.run_ok("project.save", json!({}));
    let a_bytes = read(&folder, "A.redasch");

    engine.run_ok("document.rename", json!({"document": a, "title": "tmp"}));
    engine.run_ok("document.rename", json!({"document": b, "title": "A"}));
    engine.run_ok("document.rename", json!({"document": a, "title": "b"}));
    engine.run_ok("project.save", json!({"document": "project"}));
    assert_eq!(read(&folder, "b.redasch"), a_bytes);
    let names: Vec<String> = files(&folder);
    assert!(names.contains(&"A.redasch".to_string()), "{names:?}");
    assert!(!names.contains(&"B.redasch".to_string()), "{names:?}");
}

#[test]
fn saving_a_renamed_sheet_alone_keeps_the_file_the_project_file_lists() {
    let dir = TempDir::new("sheets-rename-alone");
    let (mut engine, folder) = new_project(dir.path());
    let sheet = add_sheet(&mut engine);
    engine.run_ok("project.save", json!({}));
    engine.run_ok(
        "document.rename",
        json!({"document": sheet, "title": "Power"}),
    );
    engine.run_ok(
        "schematic.sheet.set_size",
        json!({"document": sheet, "size": "a3"}),
    );
    engine.run_ok("project.save", json!({"document": sheet}));
    assert!(!folder.join("Power.redasch").exists());
    assert!(
        String::from_utf8(read(&folder, "sheet-2.redasch"))
            .unwrap()
            .contains("a3")
    );
    engine.run_ok("project.save", json!({"document": "project"}));
    assert!(
        String::from_utf8(read(&folder, "Power.redasch"))
            .unwrap()
            .contains("a3")
    );
    assert!(!folder.join("sheet-2.redasch").exists());
}

#[test]
fn deleting_a_saved_sheet_sends_its_file_to_the_trash_on_save() {
    let dir = TempDir::new("sheets-delete-save");
    let (mut engine, folder) = new_project(dir.path());
    let sheet = add_sheet(&mut engine);
    engine.run_ok("project.save", json!({}));
    engine.run_ok("document.delete", json!({"document": sheet}));
    assert!(
        folder.join("sheet-2.redasch").exists(),
        "nothing moves before save"
    );
    let out = engine.run_ok("project.save", json!({}));
    assert_eq!(out["trashed"], json!(["sheet-2.redasch"]));
    assert!(!folder.join("sheet-2.redasch").exists());
    assert_eq!(trashed(dir.path()), ["sheet-2.redasch"]);
}

#[test]
fn a_new_sheet_reusing_a_deleted_sheets_file_name_trashes_the_old_file_first() {
    let dir = TempDir::new("sheets-delete-reuse");
    let (mut engine, folder) = new_project(dir.path());
    let old = add_sheet(&mut engine);
    engine.run_ok(
        "schematic.sheet.set_size",
        json!({"document": old, "size": "a3"}),
    );
    engine.run_ok("project.save", json!({}));
    engine.run_ok("document.delete", json!({"document": old}));
    let new = add_sheet(&mut engine);
    assert_eq!(names(&engine)[2].1, "sheet-2.redasch");
    engine.run_ok("project.save", json!({"document": new}));
    engine.run_ok("project.save", json!({}));
    assert!(
        !String::from_utf8(read(&folder, "sheet-2.redasch"))
            .unwrap()
            .contains("a3")
    );
    let trash = trashed(dir.path());
    assert_eq!(trash, ["sheet-2.redasch"]);
    let old_bytes = std::fs::read(dir.path().join("trash/sheet-2.redasch")).unwrap();
    assert!(String::from_utf8(old_bytes).unwrap().contains("a3"));
}

#[test]
fn an_unsaved_sheet_deleted_before_save_leaves_nothing_behind() {
    let dir = TempDir::new("sheets-delete-unsaved");
    let (mut engine, folder) = new_project(dir.path());
    let sheet = add_sheet(&mut engine);
    engine.run_ok("document.delete", json!({"document": sheet}));
    let out = engine.run_ok("project.save", json!({}));
    assert!(out.get("trashed").is_none(), "{out}");
    assert_eq!(files(&folder).len(), 4);
    assert!(trashed(dir.path()).is_empty());
}

fn uuid(value: &Value) -> Uuid {
    value.as_str().unwrap().parse().unwrap()
}

fn add_sheet(engine: &mut TestEngine) -> Uuid {
    uuid(&engine.run_ok("document.add", json!({"kind": "schematic"}))["result"]["uuid"])
}

fn names(engine: &TestEngine) -> Vec<(String, String)> {
    engine
        .project()
        .unwrap()
        .documents()
        .map(|d| (d.title.clone(), d.file_name()))
        .collect()
}

fn files(folder: &Path) -> Vec<String> {
    read_tree(folder).into_keys().collect()
}

#[test]
fn added_sheets_default_to_numbered_files() {
    let dir = TempDir::new("sheets-default");
    let (mut engine, _) = new_project(dir.path());
    add_sheet(&mut engine);
    add_sheet(&mut engine);
    assert_eq!(
        names(&engine),
        [
            ("Sheet 1".into(), "Demo.redasch".into()),
            ("Demo".into(), "Demo.redapcb".into()),
            ("Sheet 2".into(), "sheet-2.redasch".into()),
            ("Sheet 3".into(), "sheet-3.redasch".into()),
        ]
    );
}

#[test]
fn renaming_a_sheet_sets_its_title_and_file_stem_and_undoes_on_the_project_stack() {
    let dir = TempDir::new("sheets-rename");
    let (mut engine, _) = new_project(dir.path());
    let sheet = add_sheet(&mut engine);
    let out = engine.run_ok(
        "document.rename",
        json!({"document": sheet, "title": "Power: 3V3/5V"}),
    );
    assert_eq!(out["result"]["title"], "Power: 3V3/5V");
    assert_eq!(out["result"]["name"], "Power_ 3V3_5V");
    assert!(engine.dirty_scopes().contains(&Scope::Project));

    engine.run_ok("edit.undo", json!({"document": "project"}));
    assert_eq!(
        names(&engine)[2],
        ("Sheet 2".into(), "sheet-2.redasch".into())
    );
    engine.run_ok("edit.redo", json!({"document": "project"}));
    assert_eq!(
        names(&engine)[2],
        ("Power: 3V3/5V".into(), "Power_ 3V3_5V.redasch".into())
    );
}

#[test]
fn a_renamed_sheet_keeps_a_unique_file_stem() {
    let dir = TempDir::new("sheets-rename-unique");
    let (mut engine, _) = new_project(dir.path());
    let sheet = add_sheet(&mut engine);
    // The first sheet's file is Demo.redasch; another sheet titled Demo needs another stem.
    let out = engine.run_ok(
        "document.rename",
        json!({"document": sheet, "title": "Demo"}),
    );
    assert_eq!(out["result"]["name"], "Demo 2");
    // Renaming a sheet to its own name keeps its stem.
    let out = engine.run_ok(
        "document.rename",
        json!({"document": sheet, "title": "demo 2"}),
    );
    assert_eq!(out["result"]["name"], "demo 2");
}

#[test]
fn renaming_refuses_an_empty_title_and_an_unknown_document() {
    let dir = TempDir::new("sheets-rename-bad");
    let (mut engine, _) = new_project(dir.path());
    let sheet = add_sheet(&mut engine);
    let err = engine.run_err("document.rename", json!({"document": sheet, "title": " "}));
    assert_eq!(err.code, ErrorCode::InvalidParams);
    let err = engine.run_err(
        "document.rename",
        json!({"document": "00000000-0000-4000-8000-0000000000ff", "title": "X"}),
    );
    assert_eq!(err.code, ErrorCode::NotFound);
}

#[test]
fn reordering_moves_a_document_and_gives_the_sheet_numbers() {
    let dir = TempDir::new("sheets-reorder");
    let (mut engine, _) = new_project(dir.path());
    let second = add_sheet(&mut engine);
    let out = engine.run_ok("document.reorder", json!({"document": second, "index": 0}));
    let order = out["result"]["documents"].as_array().unwrap();
    assert_eq!(order.len(), 3);
    assert_eq!(order[0], json!(second));
    let block = engine.run_ok("schematic.sheet.title_block", json!({"document": second}));
    assert_eq!(
        (&block["sheet_number"], &block["sheet_count"]),
        (&json!(1), &json!(2))
    );

    engine.run_ok("edit.undo", json!({"document": "project"}));
    let block = engine.run_ok("schematic.sheet.title_block", json!({"document": second}));
    assert_eq!(block["sheet_number"], 2);
    // An index past the end moves it last.
    engine.run_ok("document.reorder", json!({"document": second, "index": 0}));
    engine.run_ok("document.reorder", json!({"document": second, "index": 99}));
    assert_eq!(names(&engine)[2].0, "Sheet 2");
}
