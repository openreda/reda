//! Project paths past Windows' legacy 260-character limit (`MAX_PATH`). CI runs this on
//! Windows too, where every file REDA reads, writes, renames and zips there must get
//! through; elsewhere it checks the same flow with nothing special about the length.

use std::path::{Path, PathBuf};

use reda_testkit::{TempDir, test_engine};
use serde_json::json;

/// A folder under `root` whose path is longer than 300 characters.
fn deep(root: &Path) -> PathBuf {
    let mut dir = root.to_path_buf();
    let segment = "a-folder-name-of-forty-characters-long-x";
    while dir.as_os_str().len() <= 300 {
        dir.push(segment);
    }
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn a_project_past_the_legacy_path_limit_saves_opens_renames_and_packages() {
    let tmp = TempDir::new("long-paths");
    let dir = deep(tmp.path());
    let mut engine = test_engine();
    engine.run_ok("project.new", json!({"path": dir, "name": "Demo"}));
    let folder = dir.join("Demo");
    assert!(folder.join("Demo.reda").as_os_str().len() > 260);

    let out = engine.run_ok(
        "document.add",
        json!({"kind": "schematic", "title": "Power"}),
    );
    let power = out["result"]["uuid"].as_str().unwrap().to_owned();
    engine.run_ok("project.save", json!({}));
    engine.run_ok(
        "document.rename",
        json!({"document": power, "title": "Power Supply"}),
    );
    engine.run_ok("project.save", json!({}));
    assert!(folder.join("Power Supply.redasch").is_file());
    assert!(!folder.join("Power.redasch").exists());

    let zip = dir.join("Demo.zip");
    let out = engine.run_ok("project.package", json!({"path": zip}));
    assert_eq!(out["files"].as_array().unwrap().len(), 4, "{out}");

    std::fs::rename(
        folder.join("Power Supply.redasch"),
        folder.join("moved.redasch"),
    )
    .unwrap();
    let mut again = test_engine();
    let out = again.run_ok("project.open", json!({"path": folder}));
    assert_eq!(out["missing"][0]["uuid"], power);
    again.run_ok(
        "document.relink",
        json!({"document": power, "path": folder.join("moved.redasch")}),
    );
    again.run_ok("project.save", json!({}));
    assert!(folder.join("moved.redasch").is_file());
}
