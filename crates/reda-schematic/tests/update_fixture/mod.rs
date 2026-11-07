//! The fixture of the Update from Library and Replace Part tests: a writable copy of the
//! hand-written test Library and a Project using it.
#![allow(dead_code)]

use std::path::{Path, PathBuf};

use reda_testkit::{TempDir, TestEngine, test_engine_with_library};
use serde_json::{Value, json};

pub const SHEET: &str = "00000000-0000-4000-8000-000000000002";

/// A writable copy of the fixture Library and a Project using it as its starter Library.
pub struct Fixture {
    pub library: PathBuf,
    pub project: PathBuf,
    pub engine: TestEngine,
    _dirs: (TempDir, TempDir),
}

pub fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

pub fn fixture(label: &str) -> Fixture {
    let lib_dir = TempDir::new(&format!("{label}-lib"));
    let library = lib_dir.path().join("Test");
    copy_dir(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/library/Test"),
        &library,
    );
    let project_dir = TempDir::new(label);
    let mut engine = test_engine_with_library(&library);
    engine.run_ok(
        "project.new",
        json!({"path": project_dir.path(), "name": "Demo"}),
    );
    Fixture {
        library,
        project: project_dir.path().to_owned(),
        engine,
        _dirs: (lib_dir, project_dir),
    }
}

impl Fixture {
    pub fn place(&mut self, part: &str, x: &str) -> Value {
        self.engine.run_ok(
            "schematic.component.place",
            json!({"document": SHEET, "part": part, "at": {"x": x, "y": "0mm"}}),
        )["result"]
            .clone()
    }

    /// Rewrites a Library file, replacing `from` with `to`.
    pub fn edit_library(&self, file: &str, from: &str, to: &str) {
        let path = self.library.join(file);
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains(from), "{file} has no {from:?}");
        std::fs::write(&path, text.replace(from, to)).unwrap();
    }

    /// The Components whose Library Status is not Up to Date.
    pub fn outdated(&mut self) -> Vec<Value> {
        self.status()
            .into_iter()
            .filter(|c| c["status"] != "up_to_date")
            .collect()
    }

    /// `schematic.component.library_status` for every placed Component.
    pub fn status(&mut self) -> Vec<Value> {
        self.engine
            .run_ok("schematic.component.library_status", json!({}))["components"]
            .as_array()
            .unwrap()
            .clone()
    }

    pub fn preview(&mut self, params: Value) -> Value {
        self.engine
            .run_ok("schematic.component.update_preview", params)
    }

    /// Previews with `scope`, then applies that preview with `scope` and `extra`.
    pub fn apply(&mut self, scope: Value, extra: Value) -> Value {
        let preview = self.preview(scope.clone());
        let mut params = scope;
        for (k, v) in extra.as_object().unwrap() {
            params[k] = v.clone();
        }
        params["preview_hash"] = preview["preview_hash"].clone();
        self.engine
            .run_ok("schematic.component.update_from_library", params)
    }

    pub fn object(&mut self, uuid: &Value) -> Value {
        self.engine.run_ok("object.get", json!({"uuid": uuid}))["object"].clone()
    }
}
