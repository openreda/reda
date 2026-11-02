//! Several Schematic Sheets: deleting a sheet that holds Components, and the connectivity
//! across sheets that Ports and Power Ports give.

use std::path::{Path, PathBuf};

use reda_testkit::{TempDir, TestEngine, test_engine_with_library};
use serde_json::{Value, json};

const SHEET: &str = "00000000-0000-4000-8000-000000000002";
const R_10K: &str = "a0000000-0000-4000-8000-000000000100";

fn library() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/library/Test")
}

/// A new Project with a second sheet; returns the engine and the second sheet's UUID.
fn demo(dir: &TempDir) -> (TestEngine, String) {
    let mut engine = test_engine_with_library(&library());
    engine.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
    let out = engine.run_ok("document.add", json!({"kind": "schematic"}));
    let second = out["result"]["uuid"].as_str().unwrap().to_owned();
    (engine, second)
}

/// A point given in mil, as Command params.
fn at(x: i64, y: i64) -> Value {
    json!({"x": format!("{x}mil"), "y": format!("{y}mil")})
}

// The test Resistor's Pin 1 is 150 mil above its origin and Pin 2 150 mil below.
/// Places a Resistor on `sheet` with its origin at (`x`, `y`) mil; returns its Designator.
fn place(engine: &mut TestEngine, sheet: &str, x: i64, y: i64) -> String {
    let out = engine.run_ok(
        "schematic.component.place",
        json!({"document": sheet, "part": R_10K, "at": at(x, y)}),
    );
    out["result"]["designator"].as_str().unwrap().to_owned()
}

#[test]
fn deleting_a_sheet_with_components_is_one_undoable_step() {
    let dir = TempDir::new("sheet-delete-components");
    let (mut engine, second) = demo(&dir);
    place(&mut engine, &second, 0, 0);
    engine.run_ok(
        "schematic.wire.add",
        json!({"document": second, "points": [at(0, 150), at(0, 300)]}),
    );
    let before = engine.project().unwrap().clone();

    let out = engine.run_ok("document.delete", json!({"document": second}));
    let changes = out["changes"].as_object().unwrap();
    assert!(changes.contains_key("project"), "{out}");
    let project = engine.project().unwrap();
    assert!(project.schematic_sheet(second.parse().unwrap()).is_none());
    assert_eq!(project.documents().count(), 2);
    // The Designator is free again.
    assert_eq!(place(&mut engine, SHEET, 0, 0), "R1");
    engine.run_ok("edit.undo", json!({"document": SHEET}));

    engine.run_ok("edit.undo", json!({"document": "project"}));
    assert_eq!(engine.project().unwrap(), &before);
    engine.run_ok("edit.redo", json!({"document": "project"}));
    assert_eq!(engine.project().unwrap().documents().count(), 2);
}
