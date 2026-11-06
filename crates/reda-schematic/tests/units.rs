//! Units and field positions: Next Unit (`schematic.unit.cycle`) keeps every Unit
//! unique, and a field moved by hand (`schematic.field.move`) sits in its Unit's sparse
//! `field_text` until `schematic.field.reset_position` puts it back on Autoposition.

use std::path::Path;

use reda_command::ErrorCode;
use reda_testkit::{TempDir, TestEngine, test_engine_with_library};
use serde_json::{Value, json};

const SHEET: &str = "00000000-0000-4000-8000-000000000002";
const R_10K: &str = "a0000000-0000-4000-8000-000000000100";
const LM358: &str = "a0000000-0000-4000-8000-000000000120";
const OPAMP_A: &str = "a0000000-0000-4000-8000-000000000031";
const OPAMP_B: &str = "a0000000-0000-4000-8000-000000000032";
const OPAMP_POWER: &str = "a0000000-0000-4000-8000-000000000033";

fn demo(dir: &TempDir) -> TestEngine {
    let library = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/library/Test");
    let mut engine = test_engine_with_library(&library);
    engine.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
    engine
}

fn place(engine: &mut TestEngine, part: &str, x: &str) -> Value {
    engine.run_ok(
        "schematic.component.place",
        json!({"document": SHEET, "part": part, "at": {"x": x, "y": "0mm"}}),
    )["result"]
        .clone()
}

fn object(engine: &mut TestEngine, uuid: &Value) -> Value {
    engine.run_ok("object.get", json!({"uuid": uuid}))["object"].clone()
}

#[test]
fn next_unit_goes_to_the_next_unplaced_unit_and_wraps_round() {
    let dir = TempDir::new("unit-cycle");
    let mut engine = demo(&dir);
    let u1 = place(&mut engine, LM358, "0mm");
    let component = u1["component"]["uuid"].clone();
    engine.run_ok(
        "schematic.unit.place",
        json!({"document": SHEET, "component": component, "unit": "B",
               "at": {"x": "30mm", "y": 0}}),
    );
    assert_eq!(object(&mut engine, &u1["unit"])["unit"], OPAMP_A);

    let out = engine.run_ok("schematic.unit.cycle", json!({"unit": u1["unit"]}));

    assert_eq!(
        out["result"]["shows"], OPAMP_POWER,
        "B is placed, so it is skipped"
    );
    assert_eq!(out["result"]["name"], "Power");
    assert_eq!(object(&mut engine, &u1["unit"])["unit"], OPAMP_POWER);

    let out = engine.run_ok("schematic.unit.cycle", json!({"unit": u1["unit"]}));
    assert_eq!(out["result"]["shows"], OPAMP_A, "it wraps round to A");

    engine.run_ok("edit.undo", json!({"document": SHEET}));
    assert_eq!(object(&mut engine, &u1["unit"])["unit"], OPAMP_POWER);
}

#[test]
fn next_unit_is_refused_when_every_unit_is_placed() {
    let dir = TempDir::new("unit-cycle-full");
    let mut engine = demo(&dir);
    let u1 = place(&mut engine, LM358, "0mm");
    let component = u1["component"]["uuid"].clone();
    for (unit, x) in [("B", "30mm"), ("Power", "60mm")] {
        engine.run_ok(
            "schematic.unit.place",
            json!({"document": SHEET, "component": component, "unit": unit,
                   "at": {"x": x, "y": 0}}),
        );
    }
    let err = engine.run_err("schematic.unit.cycle", json!({"unit": u1["unit"]}));
    assert_eq!(err.code, ErrorCode::Rejected, "{err:?}");

    let r1 = place(&mut engine, R_10K, "90mm");
    let err = engine.run_err("schematic.unit.cycle", json!({"unit": r1["unit"]}));
    assert_eq!(
        err.code,
        ErrorCode::Rejected,
        "a one-Unit Symbol has no next Unit"
    );
}

#[test]
fn a_moved_field_is_stored_as_an_offset_in_the_symbols_frame() {
    let dir = TempDir::new("field-move");
    let mut engine = demo(&dir);
    let r1 = place(&mut engine, R_10K, "10mm");
    assert!(object(&mut engine, &r1["unit"]).get("field_text").is_none());

    engine.run_ok(
        "schematic.field.move",
        json!({"unit": r1["unit"], "field": "value", "at": {"x": "15mm", "y": "2mm"}}),
    );
    assert_eq!(
        object(&mut engine, &r1["unit"])["field_text"],
        json!([{"field": "value", "offset": {"x": 5_000_000, "y": 2_000_000}}])
    );

    // Rotated a quarter turn, then moved again with a size: the offset is undone from
    // the placement, so it stays in the Symbol's own frame.
    engine.run_ok(
        "schematic.unit.move",
        json!({"unit": r1["unit"], "at": {"x": "10mm", "y": 0}, "rotation": "90deg"}),
    );
    engine.run_ok(
        "schematic.field.move",
        json!({"unit": r1["unit"], "field": "designator", "at": {"x": "10mm", "y": "3mm"},
               "size": "1.5mm"}),
    );
    assert_eq!(
        object(&mut engine, &r1["unit"])["field_text"][0],
        json!({"field": "designator", "offset": {"x": 3_000_000, "y": 0}, "size": 1_500_000})
    );
}

#[test]
fn resetting_a_field_puts_it_back_on_autoposition() {
    let dir = TempDir::new("field-reset");
    let mut engine = demo(&dir);
    let r1 = place(&mut engine, R_10K, "0mm");
    for field in ["value", "LCSC"] {
        engine.run_ok(
            "schematic.field.move",
            json!({"unit": r1["unit"], "field": field, "at": {"x": "5mm", "y": "5mm"}}),
        );
    }

    engine.run_ok(
        "schematic.field.reset_position",
        json!({"unit": r1["unit"], "field": "value"}),
    );

    let field_text = object(&mut engine, &r1["unit"])["field_text"].clone();
    let fields: Vec<&Value> = field_text
        .as_array()
        .unwrap()
        .iter()
        .map(|t| &t["field"])
        .collect();
    assert_eq!(fields, [&json!("LCSC")]);
}

#[test]
fn a_field_the_component_does_not_have_is_refused() {
    let dir = TempDir::new("field-unknown");
    let mut engine = demo(&dir);
    let r1 = place(&mut engine, R_10K, "0mm");
    let err = engine.run_err(
        "schematic.field.move",
        json!({"unit": r1["unit"], "field": "DigiKey", "at": {"x": 0, "y": 0}}),
    );
    assert_eq!(err.code, ErrorCode::InvalidParams, "{err:?}");
}

#[test]
fn field_positions_belong_to_each_unit() {
    let dir = TempDir::new("field-per-unit");
    let mut engine = demo(&dir);
    let u1 = place(&mut engine, LM358, "0mm");
    engine.run_ok(
        "schematic.field.move",
        json!({"unit": u1["unit"], "field": "value", "at": {"x": "5mm", "y": "5mm"}}),
    );
    let b = engine.run_ok(
        "schematic.unit.place",
        json!({"document": SHEET, "component": u1["component"]["uuid"],
               "at": {"x": "30mm", "y": 0}}),
    );
    let b = object(&mut engine, &b["result"]["unit"]);
    assert_eq!(b["unit"], OPAMP_B);
    assert!(b.get("field_text").is_none(), "{b}");
}
