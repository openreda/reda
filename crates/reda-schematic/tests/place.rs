//! Placing Components: `schematic.component.place` copies a Library Part into the sheet's
//! part table and places a Unit; `schematic.unit.place` places a further Unit of an existing
//! Component.

use std::path::{Path, PathBuf};

use reda_command::ErrorCode;
use reda_testkit::{
    TempDir, TestEngine, assert_golden, read_tree, test_engine, test_engine_with_library,
};
use serde_json::{Value, json};

const SHEET: &str = "00000000-0000-4000-8000-000000000002";
const R_10K: &str = "a0000000-0000-4000-8000-000000000100";
const C_100N: &str = "a0000000-0000-4000-8000-000000000110";
const LM358: &str = "a0000000-0000-4000-8000-000000000120";
const HOLE: &str = "a0000000-0000-4000-8000-000000000130";
const BROKEN: &str = "a0000000-0000-4000-8000-000000000140";
const R_0603: &str = "a0000000-0000-4000-8000-000000000040";
const R_0805: &str = "a0000000-0000-4000-8000-000000000050";
const OPAMP_A: &str = "a0000000-0000-4000-8000-000000000031";
const OPAMP_B: &str = "a0000000-0000-4000-8000-000000000032";
const OPAMP_POWER: &str = "a0000000-0000-4000-8000-000000000033";

fn library() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/library/Test")
}

fn demo(dir: &TempDir) -> TestEngine {
    let mut engine = test_engine_with_library(&library());
    engine.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
    engine
}

fn place(engine: &mut TestEngine, part: &str, x: &str) -> Value {
    engine.run_ok(
        "schematic.component.place",
        json!({"document": SHEET, "part": part, "at": {"x": x, "y": "0mm"}}),
    )
}

fn object(engine: &mut TestEngine, uuid: &Value) -> Value {
    engine.run_ok("object.get", json!({"uuid": uuid}))["object"].clone()
}

fn count(engine: &mut TestEngine, kind: &str) -> usize {
    let out = engine.run_ok("objects.query", json!({"document": SHEET, "type": kind}));
    out["objects"].as_array().unwrap().len()
}

#[test]
fn placing_copies_the_part_into_the_part_table_once_per_part() {
    let dir = TempDir::new("place-table");
    let mut engine = demo(&dir);
    let first = place(&mut engine, R_10K, "10mm");
    let entry = first["result"]["part_entry"].clone();
    assert_eq!(object(&mut engine, &entry)["part"]["uuid"], R_10K);
    assert_eq!(
        object(&mut engine, &entry)["symbol"]["designator_prefix"],
        "R"
    );
    assert_eq!(count(&mut engine, "part_entry"), 1);
    // Both alternates are copied.
    assert_eq!(count(&mut engine, "footprint_copy"), 2);

    let second = place(&mut engine, R_10K, "20mm");
    assert_eq!(second["result"]["part_entry"], entry);
    assert_eq!(count(&mut engine, "part_entry"), 1);
    // Only the new Unit is created.
    assert_eq!(
        second["changes"][SHEET]["created"],
        json!([second["result"]["unit"]])
    );

    // Another Part sharing the 0603 Footprint reuses its copy.
    place(&mut engine, C_100N, "30mm");
    assert_eq!(count(&mut engine, "part_entry"), 2);
    assert_eq!(count(&mut engine, "footprint_copy"), 2);
    assert_eq!(count(&mut engine, "unit"), 3);
}

#[test]
fn each_unit_carries_the_component_data() {
    let dir = TempDir::new("place-data");
    let mut engine = demo(&dir);
    let out = engine.run_ok(
        "schematic.component.place",
        json!({"document": SHEET, "part": R_10K, "at": {"x": "1in", "y": 2540000},
               "rotation": "90deg", "mirror": true, "footprint": R_0805}),
    );
    let result = &out["result"];
    assert_eq!(result["designator"], "R1");
    assert_eq!(result["component"]["path"], Value::Null);
    let unit = object(&mut engine, &result["unit"]);
    assert_eq!(
        unit,
        json!({
            "type": "unit",
            "uuid": result["unit"],
            "component": {"uuid": result["component"]["uuid"]},
            "part": result["part_entry"],
            "unit": "a0000000-0000-4000-8000-000000000011",
            "at": {"x": 25_400_000, "y": 2_540_000},
            "rotation": 90_000,
            "mirror": true,
            "designator": "R1",
            "fields": {"value": "10k", "mpn": "RC0603FR-0710KL",
                       "custom": [{"name": "LCSC", "value": "C25804"}]},
            "footprint": R_0805,
            "component_type": "standard"
        })
    );
    // Without a choice the Part's first alternate is the Footprint.
    let out = place(&mut engine, R_10K, "0mm");
    assert_eq!(
        object(&mut engine, &out["result"]["unit"])["footprint"],
        R_0603
    );
}

#[test]
fn a_component_gets_the_next_free_designator_for_its_prefix_across_sheets() {
    let dir = TempDir::new("place-designators");
    let mut engine = demo(&dir);
    assert_eq!(
        place(&mut engine, R_10K, "0mm")["result"]["designator"],
        "R1"
    );
    assert_eq!(
        place(&mut engine, C_100N, "5mm")["result"]["designator"],
        "C1"
    );
    assert_eq!(
        place(&mut engine, R_10K, "10mm")["result"]["designator"],
        "R2"
    );
    let added = engine.run_ok("document.add", json!({"kind": "schematic"}));
    let second = added["result"]["uuid"].clone();
    let out = engine.run_ok(
        "schematic.component.place",
        json!({"document": second, "part": R_10K, "at": {"x": 0, "y": 0}}),
    );
    assert_eq!(out["result"]["designator"], "R3");
    // The second sheet gets its own part table.
    let entry = object(&mut engine, &out["result"]["part_entry"]);
    assert_eq!(entry["part"]["uuid"], R_10K);
}

#[test]
fn the_further_units_of_a_multi_unit_component_share_its_data() {
    let dir = TempDir::new("place-units");
    let mut engine = demo(&dir);
    let out = place(&mut engine, LM358, "0mm");
    let component = out["result"]["component"]["uuid"].clone();
    assert_eq!(object(&mut engine, &out["result"]["unit"])["unit"], OPAMP_A);

    let added = engine.run_ok("document.add", json!({"kind": "schematic"}));
    let second = added["result"]["uuid"].clone();
    let b = engine.run_ok(
        "schematic.unit.place",
        json!({"document": second, "component": component, "at": {"x": 0, "y": 0}}),
    );
    let b_unit = object(&mut engine, &b["result"]["unit"]);
    assert_eq!(b_unit["unit"], OPAMP_B);
    assert_eq!(b_unit["designator"], "U1");
    assert_eq!(b_unit["component"]["uuid"], component);
    assert_eq!(b_unit["fields"]["value"], "LM358");
    assert_eq!(b["result"]["designator"], "U1");

    let power = engine.run_ok(
        "schematic.unit.place",
        json!({"document": SHEET, "component": component, "unit": "Power",
               "at": {"x": 0, "y": "20mm"}}),
    );
    assert_eq!(
        object(&mut engine, &power["result"]["unit"])["unit"],
        OPAMP_POWER
    );

    let err = engine.run_err(
        "schematic.unit.place",
        json!({"document": SHEET, "component": component, "at": {"x": 0, "y": 0}}),
    );
    assert_eq!(err.code, ErrorCode::Rejected, "{err}");
    let err = engine.run_err(
        "schematic.unit.place",
        json!({"document": SHEET, "component": component, "unit": "A",
               "at": {"x": 0, "y": 0}}),
    );
    assert_eq!(err.code, ErrorCode::Rejected, "{err}");
}

#[test]
fn placing_is_one_undo_step_on_the_sheet() {
    let dir = TempDir::new("place-undo");
    let mut engine = demo(&dir);
    place(&mut engine, R_10K, "0mm");
    engine.run_ok("edit.undo", json!({"document": SHEET}));
    assert_eq!(count(&mut engine, "unit"), 0);
    assert_eq!(count(&mut engine, "part_entry"), 0);
    assert_eq!(count(&mut engine, "footprint_copy"), 0);
    engine.run_ok("edit.redo", json!({"document": SHEET}));
    assert_eq!(count(&mut engine, "unit"), 1);
}

#[test]
fn parts_that_cannot_be_placed_are_refused() {
    let dir = TempDir::new("place-errors");
    let mut engine = demo(&dir);
    let mut err = |part: &str, extra: Value| {
        let mut params = json!({"document": SHEET, "part": part, "at": {"x": 0, "y": 0}});
        params
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        engine.run_err("schematic.component.place", params)
    };
    assert_eq!(
        err("a0000000-0000-4000-8000-0000000000ee", json!({})).code,
        ErrorCode::NotFound
    );
    assert_eq!(err(HOLE, json!({})).code, ErrorCode::Rejected);
    assert_eq!(err(BROKEN, json!({})).code, ErrorCode::NotFound);
    let wrong = err(
        R_10K,
        json!({"footprint": "a0000000-0000-4000-8000-000000000060"}),
    );
    assert_eq!(wrong.code, ErrorCode::InvalidParams);
    let no_sheet = err(
        R_10K,
        json!({"document": "a0000000-0000-4000-8000-0000000000ee"}),
    );
    assert_eq!(no_sheet.code, ErrorCode::NotFound);
}

#[test]
fn a_placed_project_opens_and_saves_with_no_library() {
    let dir = TempDir::new("place-no-library");
    let mut engine = demo(&dir);
    place(&mut engine, R_10K, "10mm");
    place(&mut engine, C_100N, "20mm");
    let op = place(&mut engine, LM358, "40mm");
    engine.run_ok(
        "schematic.unit.place",
        json!({"document": SHEET, "component": op["result"]["component"]["uuid"],
               "at": {"x": "60mm", "y": 0}}),
    );
    engine.run_ok("project.save", json!({}));
    let folder = dir.path().join("Demo");
    let tree = read_tree(&folder);
    assert_golden(
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/golden/placed/Demo.redasch"
        ),
        &tree["Demo.redasch"],
    );

    let mut bare = test_engine();
    bare.run_ok("project.open", json!({"path": folder}));
    assert_eq!(count(&mut bare, "unit"), 4);
    bare.run_ok("project.save", json!({}));
    assert_eq!(read_tree(&folder), tree);
}

#[test]
fn component_references_accept_an_empty_sheet_instance_path() {
    let dir = TempDir::new("place-path");
    let mut engine = demo(&dir);
    place(&mut engine, R_10K, "10mm");
    engine.run_ok("project.save", json!({}));
    let folder = dir.path().join("Demo");
    let file = folder.join("Demo.redasch");
    let saved = std::fs::read_to_string(&file).unwrap();
    let edited = saved.replace(r#""component":{"#, r#""component":{"path":[],"#);
    assert_ne!(saved, edited);
    std::fs::write(&file, edited).unwrap();

    let mut reopened = test_engine();
    reopened.run_ok("project.open", json!({"path": folder}));
    reopened.run_ok("project.save", json!({}));
    assert_eq!(std::fs::read_to_string(&file).unwrap(), saved);
}
