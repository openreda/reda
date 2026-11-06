//! Scenario tests for creating, opening and saving a Project, driven through Commands.

use std::path::Path;

use reda_engine::command::ErrorCode;
use reda_testkit::{TempDir, assert_golden, read_tree, test_engine};
use serde_json::json;

const GOLDEN: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden/new_project");

fn new_project(dir: &Path, name: &str) -> reda_testkit::TestEngine {
    let mut engine = test_engine();
    engine.run_ok("project.new", json!({"path": dir, "name": name}));
    engine
}

#[test]
fn new_project_writes_the_whole_project_at_once() {
    let dir = TempDir::new("new-golden");
    let engine = new_project(dir.path(), "Demo");
    assert!(
        !engine.is_dirty(),
        "a new Project is saved as it is created"
    );

    let tree = read_tree(&dir.path().join("Demo"));
    let names: Vec<&str> = tree.keys().map(String::as_str).collect();
    assert_eq!(
        names,
        [
            ".reda/.gitignore",
            "Demo.reda",
            "Demo.redapcb",
            "Demo.redasch"
        ]
    );
    assert_eq!(tree[".reda/.gitignore"], b"*\n");
    for file in ["Demo.reda", "Demo.redapcb", "Demo.redasch"] {
        assert_golden(format!("{GOLDEN}/{file}"), &tree[file]);
    }
}

#[test]
fn new_project_reports_the_project_and_its_first_sheet() {
    let dir = TempDir::new("new-result");
    let mut engine = test_engine();
    let out = engine.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
    assert_eq!(out["name"], "Demo");
    assert_eq!(out["uuid"], "00000000-0000-4000-8000-000000000001");
    assert_eq!(
        out["documents"],
        json!([{
            "uuid": "00000000-0000-4000-8000-000000000002",
            "kind": "schematic",
            "name": "Demo",
            "title": "Sheet 1"
        }, {
            "uuid": "00000000-0000-4000-8000-000000000003",
            "kind": "pcb",
            "name": "Demo",
            "title": "Demo"
        }])
    );
    let path = Path::new(out["path"].as_str().unwrap());
    assert_eq!(path, dir.path().join("Demo").join("Demo.reda"));
}

#[test]
fn open_then_save_reproduces_the_same_bytes() {
    let dir = TempDir::new("roundtrip");
    new_project(dir.path(), "Demo").run_ok("project.save", json!({}));
    let folder = dir.path().join("Demo");
    let before = read_tree(&folder);

    let mut engine = test_engine();
    engine.run_ok("project.open", json!({"path": folder.join("Demo.reda")}));
    assert!(!engine.is_dirty());
    engine.run_ok("project.save", json!({}));
    assert_eq!(read_tree(&folder), before);
}

#[test]
fn open_accepts_the_project_folder() {
    let dir = TempDir::new("open-folder");
    new_project(dir.path(), "Demo").run_ok("project.save", json!({}));
    let mut engine = test_engine();
    let out = engine.run_ok("project.open", json!({"path": dir.path().join("Demo")}));
    assert_eq!(out["documents"][0]["title"], "Sheet 1");
}

#[test]
fn hand_edits_are_normalised_on_save() {
    let dir = TempDir::new("hand-edit");
    new_project(dir.path(), "Demo").run_ok("project.save", json!({}));
    let folder = dir.path().join("Demo");
    let canonical = read_tree(&folder);
    // Keys sorted and whitespace dropped. (Upper-case UUIDs are covered in reda-model.)
    for file in ["Demo.reda", "Demo.redapcb"] {
        let value: serde_json::Value = serde_json::from_slice(&canonical[file]).unwrap();
        let messy = serde_json::to_string(&value).unwrap();
        assert_ne!(messy.as_bytes(), &canonical[file][..]);
        std::fs::write(folder.join(file), messy).unwrap();
    }

    let mut engine = test_engine();
    engine.run_ok("project.open", json!({"path": folder.join("Demo.reda")}));
    engine.run_ok("project.save", json!({}));
    assert_eq!(read_tree(&folder), canonical);
}

#[test]
fn new_project_refuses_a_non_empty_folder() {
    let dir = TempDir::new("new-occupied");
    std::fs::create_dir_all(dir.path().join("Demo")).unwrap();
    std::fs::write(dir.path().join("Demo").join("notes.txt"), b"x").unwrap();
    let err = test_engine().run_err("project.new", json!({"path": dir.path(), "name": "Demo"}));
    assert_eq!(err.code, ErrorCode::Rejected);
    assert!(err.message.contains("not empty"), "{}", err.message);
}

#[test]
fn new_project_accepts_an_existing_empty_folder() {
    let dir = TempDir::new("new-empty-folder");
    std::fs::create_dir_all(dir.path().join("Demo")).unwrap();
    new_project(dir.path(), "Demo").run_ok("project.save", json!({}));
    assert!(dir.path().join("Demo").join("Demo.reda").is_file());
}

#[test]
fn new_project_refuses_names_that_are_not_a_file_stem() {
    let dir = TempDir::new("new-bad-name");
    for name in ["", "a/b", "a\\b", "..", "."] {
        let err = test_engine().run_err("project.new", json!({"path": dir.path(), "name": name}));
        assert_eq!(err.code, ErrorCode::InvalidParams, "{name:?}");
    }
}

#[test]
fn save_without_a_project_is_unavailable() {
    let err = test_engine().run_err("project.save", json!({}));
    assert_eq!(err.code, ErrorCode::Unavailable);
}

#[test]
fn open_reports_a_missing_project_as_not_found() {
    let dir = TempDir::new("open-missing");
    let err = test_engine().run_err(
        "project.open",
        json!({"path": dir.path().join("Nope.reda")}),
    );
    assert_eq!(err.code, ErrorCode::NotFound);
}

fn saved_project(label: &str) -> (TempDir, std::path::PathBuf) {
    let dir = TempDir::new(label);
    new_project(dir.path(), "Demo").run_ok("project.save", json!({}));
    let folder = dir.path().join("Demo");
    (dir, folder)
}

fn edit(path: &Path, from: &str, to: &str) {
    let text = std::fs::read_to_string(path).unwrap();
    assert!(text.contains(from), "{text}");
    std::fs::write(path, text.replace(from, to)).unwrap();
}

#[test]
fn open_refuses_a_file_from_a_newer_reda() {
    let (_dir, folder) = saved_project("open-newer");
    edit(
        &folder.join("Demo.redasch"),
        "\"format\": 1",
        "\"format\": 99",
    );
    let err = test_engine().run_err("project.open", json!({"path": folder}));
    assert_eq!(err.code, ErrorCode::Rejected);
    assert!(err.message.contains("newer"), "{}", err.message);
    assert!(err.message.contains("Demo.redasch"), "{}", err.message);
}

#[test]
fn open_refuses_unknown_fields() {
    let (_dir, folder) = saved_project("open-unknown");
    edit(
        &folder.join("Demo.reda"),
        "\"format\": 1",
        "\"format\": 1, \"colour\": 3",
    );
    let err = test_engine().run_err("project.open", json!({"path": folder}));
    assert_eq!(err.code, ErrorCode::Rejected);
    assert!(err.message.contains("colour"), "{}", err.message);
}

#[test]
fn open_refuses_a_sheet_whose_uuid_disagrees_with_the_project_file() {
    let (_dir, folder) = saved_project("open-mismatch");
    edit(
        &folder.join("Demo.redasch"),
        "00000000-0000-4000-8000-000000000002",
        "00000000-0000-4000-8000-000000000009",
    );
    let err = test_engine().run_err("project.open", json!({"path": folder}));
    assert_eq!(err.code, ErrorCode::Rejected);
}

/// Document-level grids and guides are reserved names for v0.3: the PCB file has no
/// tables for them, not even empty ones, and refuses them on read.
#[test]
fn grids_and_guides_stay_reserved_in_the_pcb_file() {
    let (_dir, folder) = saved_project("open-reserved");
    let file = folder.join("Demo.redapcb");
    let text = std::fs::read_to_string(&file).unwrap();
    for name in ["\"grids\"", "\"guides\""] {
        assert!(!text.contains(name), "{text}");
    }
    let schema = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/schemas/redapcb.schema.json"
    );
    let schema: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(schema).unwrap()).unwrap();
    for name in ["grids", "guides"] {
        assert!(schema["properties"].get(name).is_none(), "{name}");
        let err = {
            edit(
                &file,
                "\"format\": 1",
                &format!("\"format\": 1, \"{name}\": []"),
            );
            test_engine().run_err("project.open", json!({"path": folder}))
        };
        assert_eq!(err.code, ErrorCode::Rejected);
        assert!(err.message.contains(name), "{}", err.message);
        std::fs::write(&file, &text).unwrap();
    }
}
