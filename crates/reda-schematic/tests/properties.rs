//! Editing Component data with `schematic.component.set`: every Unit of a Component keeps
//! the same copy, and one edit can target many Components.

use std::path::{Path, PathBuf};

use reda_command::ErrorCode;
use reda_testkit::{TempDir, TestEngine, test_engine_with_library};
use serde_json::{Value, json};

const SHEET: &str = "00000000-0000-4000-8000-000000000002";
const R_10K: &str = "a0000000-0000-4000-8000-000000000100";
const C_100N: &str = "a0000000-0000-4000-8000-000000000110";
const LM358: &str = "a0000000-0000-4000-8000-000000000120";
const R_SHUNT: &str = "a0000000-0000-4000-8000-000000000150";
const R_2512_KELVIN: &str = "a0000000-0000-4000-8000-000000000070";
const R_0603: &str = "a0000000-0000-4000-8000-000000000040";
const R_0805: &str = "a0000000-0000-4000-8000-000000000050";
const SOIC_8: &str = "a0000000-0000-4000-8000-000000000060";

fn library() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/library/Test")
}

fn demo(dir: &TempDir) -> TestEngine {
    let mut engine = test_engine_with_library(&library());
    engine.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
    engine
}

/// Places a Component on the first sheet and returns the place result.
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

/// An LM358 with Unit A on the first sheet and Unit B on a second sheet: (component, unit
/// A, unit B).
fn op_amp_on_two_sheets(engine: &mut TestEngine) -> (Value, Value, Value) {
    let a = place(engine, LM358, "0mm");
    let component = a["component"]["uuid"].clone();
    let added = engine.run_ok("document.add", json!({"kind": "schematic"}));
    let second = added["result"]["uuid"].clone();
    let b = engine.run_ok(
        "schematic.unit.place",
        json!({"document": second, "component": component, "at": {"x": 0, "y": 0}}),
    );
    (component, a["unit"].clone(), b["result"]["unit"].clone())
}

#[test]
fn a_value_edit_reaches_every_unit_of_the_component_on_every_sheet() {
    let dir = TempDir::new("set-value");
    let mut engine = demo(&dir);
    let (component, a, b) = op_amp_on_two_sheets(&mut engine);

    engine.run_ok(
        "schematic.component.set",
        json!({"components": [component], "value": "TL072"}),
    );

    assert_eq!(object(&mut engine, &a)["fields"]["value"], "TL072");
    assert_eq!(object(&mut engine, &b)["fields"]["value"], "TL072");
}

#[test]
fn a_designator_edit_renames_every_unit_and_refuses_one_already_taken() {
    let dir = TempDir::new("set-designator");
    let mut engine = demo(&dir);
    let (component, a, b) = op_amp_on_two_sheets(&mut engine);
    let r1 = place(&mut engine, R_10K, "40mm");

    engine.run_ok(
        "schematic.component.set",
        json!({"components": [component], "designator": "U7"}),
    );
    assert_eq!(object(&mut engine, &a)["designator"], "U7");
    assert_eq!(object(&mut engine, &b)["designator"], "U7");

    let taken = engine.run_err(
        "schematic.component.set",
        json!({"components": [r1["component"]["uuid"]], "designator": "U7"}),
    );
    assert_eq!(taken.code, ErrorCode::Conflict);

    let many = engine.run_err(
        "schematic.component.set",
        json!({"components": [component, r1["component"]["uuid"]], "designator": "X1"}),
    );
    assert_eq!(many.code, ErrorCode::InvalidParams);

    let blank = engine.run_err(
        "schematic.component.set",
        json!({"components": [component], "designator": " "}),
    );
    assert_eq!(blank.code, ErrorCode::InvalidParams);
    assert_eq!(object(&mut engine, &r1["unit"])["designator"], "R1");
}

#[test]
fn field_edits_set_change_and_remove_fields_by_name() {
    let dir = TempDir::new("set-fields");
    let mut engine = demo(&dir);
    let r1 = place(&mut engine, R_10K, "0mm");
    let component = r1["component"]["uuid"].clone();

    engine.run_ok(
        "schematic.component.set",
        json!({"components": [component], "fields": {
            "mpn": "ERJ-3EKF1002V", "manufacturer": "Panasonic",
            "LCSC": null, "Notes": "hand-solder"}}),
    );

    let fields = object(&mut engine, &r1["unit"])["fields"].clone();
    assert_eq!(
        fields,
        json!({"value": "10k", "manufacturer": "Panasonic", "mpn": "ERJ-3EKF1002V",
               "custom": [{"name": "Notes", "value": "hand-solder"}]})
    );

    // An existing free-form field keeps its place when changed; an empty value removes it.
    engine.run_ok(
        "schematic.component.set",
        json!({"components": [component], "fields": {"DigiKey": "P10.0KHCT", "Notes": "x"}}),
    );
    engine.run_ok(
        "schematic.component.set",
        json!({"components": [component], "fields": {"Notes": "", "manufacturer": ""}}),
    );
    let fields = object(&mut engine, &r1["unit"])["fields"].clone();
    assert_eq!(
        fields,
        json!({"value": "10k", "mpn": "ERJ-3EKF1002V",
               "custom": [{"name": "DigiKey", "value": "P10.0KHCT"}]})
    );

    let value = engine.run_err(
        "schematic.component.set",
        json!({"components": [component], "fields": {"value": "1k"}}),
    );
    assert_eq!(value.code, ErrorCode::InvalidParams);
}

#[test]
fn the_footprint_is_chosen_from_the_parts_alternates_only() {
    let dir = TempDir::new("set-footprint");
    let mut engine = demo(&dir);
    let r1 = place(&mut engine, R_10K, "0mm");
    let r2 = place(&mut engine, R_10K, "10mm");
    let c1 = place(&mut engine, C_100N, "20mm");
    let resistors = json!([r1["component"]["uuid"], r2["component"]["uuid"]]);

    engine.run_ok(
        "schematic.component.set",
        json!({"components": resistors, "footprint": R_0805}),
    );
    assert_eq!(object(&mut engine, &r1["unit"])["footprint"], R_0805);
    assert_eq!(object(&mut engine, &r2["unit"])["footprint"], R_0805);

    // SOIC-8 is no alternate of R_10k.
    let err = engine.run_err(
        "schematic.component.set",
        json!({"components": resistors, "footprint": SOIC_8}),
    );
    assert_eq!(err.code, ErrorCode::InvalidParams);
    assert_eq!(err.details.as_ref().unwrap()["pointer"], "/footprint");

    // 0603 is an alternate of both Parts; 0805 only of the resistor, so nothing changes.
    let both = json!([r1["component"]["uuid"], c1["component"]["uuid"]]);
    engine.run_ok(
        "schematic.component.set",
        json!({"components": both, "footprint": R_0603}),
    );
    let err = engine.run_err(
        "schematic.component.set",
        json!({"components": both, "footprint": R_0805}),
    );
    assert_eq!(err.code, ErrorCode::InvalidParams);
    assert_eq!(object(&mut engine, &r1["unit"])["footprint"], R_0603);
    assert_eq!(object(&mut engine, &c1["unit"])["footprint"], R_0603);
}

#[test]
fn not_fitted_marks_every_unit() {
    let dir = TempDir::new("set-not-fitted");
    let mut engine = demo(&dir);
    let (component, a, b) = op_amp_on_two_sheets(&mut engine);

    engine.run_ok(
        "schematic.component.set",
        json!({"components": [component], "not_fitted": true}),
    );
    assert_eq!(object(&mut engine, &a)["not_fitted"], true);
    assert_eq!(object(&mut engine, &b)["not_fitted"], true);

    engine.run_ok(
        "schematic.component.set",
        json!({"components": [component], "not_fitted": false}),
    );
    assert_eq!(object(&mut engine, &a).get("not_fitted"), None);
}

#[test]
fn not_fitted_display_is_a_project_option_that_starts_as_cross() {
    let dir = TempDir::new("not-fitted-display");
    let mut engine = demo(&dir);
    let out = engine.run_ok("schematic.options.set", json!({}));
    assert_eq!(out["result"]["not_fitted_display"], "cross");
    for display in ["mesh", "none", "cross", "mesh"] {
        let out = engine.run_ok(
            "schematic.options.set",
            json!({"not_fitted_display": display}),
        );
        assert_eq!(out["result"]["not_fitted_display"], display);
    }
    // `text` is reserved for a later text overlay.
    let err = engine.run_err(
        "schematic.options.set",
        json!({"not_fitted_display": "text"}),
    );
    assert_eq!(err.code, ErrorCode::InvalidParams, "{err:?}");
    assert!(err.message.contains("reserved"), "{}", err.message);

    engine.run_ok("project.save", json!({}));
    let text = std::fs::read_to_string(dir.path().join("Demo/Demo.reda")).unwrap();
    assert!(text.contains(r#""not_fitted_display":"mesh""#), "{text}");
    let mut other = test_engine_with_library(&library());
    other.run_ok("project.open", json!({"path": dir.path().join("Demo")}));
    let out = other.run_ok("schematic.options.set", json!({}));
    assert_eq!(out["result"]["not_fitted_display"], "mesh");
}

#[test]
fn field_visibility_is_shared_by_every_unit_and_survives_save_and_open() {
    let dir = TempDir::new("set-visible");
    let mut engine = demo(&dir);
    let (component, a, b) = op_amp_on_two_sheets(&mut engine);
    // Designator and value are shown by default, which the file leaves out.
    assert_eq!(object(&mut engine, &a).get("visible_fields"), None);

    engine.run_ok(
        "schematic.component.set",
        json!({"components": [component], "visible": {"value": false, "mpn": true}}),
    );
    assert_eq!(
        object(&mut engine, &a)["visible_fields"],
        json!(["designator", "mpn"])
    );
    assert_eq!(
        object(&mut engine, &b)["visible_fields"],
        json!(["designator", "mpn"])
    );

    engine.run_ok("project.save", json!({}));
    let mut reopened = test_engine_with_library(&library());
    reopened.run_ok("project.open", json!({"path": dir.path().join("Demo")}));
    assert_eq!(
        object(&mut reopened, &b)["visible_fields"],
        json!(["designator", "mpn"])
    );

    let blank = engine.run_err(
        "schematic.component.set",
        json!({"components": [component], "visible": {"": true}}),
    );
    assert_eq!(blank.code, ErrorCode::InvalidParams);
}

#[test]
fn one_edit_applies_to_every_target_component() {
    let dir = TempDir::new("set-many");
    let mut engine = demo(&dir);
    let r1 = place(&mut engine, R_10K, "0mm");
    let r2 = place(&mut engine, R_10K, "10mm");
    let u1 = place(&mut engine, LM358, "20mm");

    let out = engine.run_ok(
        "schematic.component.set",
        json!({"components": [r1["component"]["uuid"], r2["component"]["uuid"]],
               "value": "4k7"}),
    );

    assert_eq!(out["result"]["units"], json!([r1["unit"], r2["unit"]]));
    assert_eq!(object(&mut engine, &r1["unit"])["fields"]["value"], "4k7");
    assert_eq!(object(&mut engine, &r2["unit"])["fields"]["value"], "4k7");
    assert_eq!(object(&mut engine, &u1["unit"])["fields"]["value"], "LM358");
}

#[test]
fn an_unknown_target_fails_the_whole_edit() {
    let dir = TempDir::new("set-unknown");
    let mut engine = demo(&dir);
    let r1 = place(&mut engine, R_10K, "0mm");

    let err = engine.run_err(
        "schematic.component.set",
        json!({"components": [r1["component"]["uuid"], "00000000-0000-4000-8000-0000000000ff"],
               "value": "4k7"}),
    );

    assert_eq!(err.code, ErrorCode::NotFound);
    assert_eq!(object(&mut engine, &r1["unit"])["fields"]["value"], "10k");
}

/// `(number, pads)` for each Pin `schematic.component.pins` lists.
fn pin_pads(engine: &mut TestEngine, component: &Value) -> Vec<(String, Vec<String>)> {
    let out = engine.run_ok("schematic.component.pins", json!({"component": component}));
    out["pins"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            let pads = p["pads"].as_array().unwrap();
            let pads = pads.iter().map(|n| n.as_str().unwrap().to_owned());
            (p["number"].as_str().unwrap().to_owned(), pads.collect())
        })
        .collect()
}

fn pins(list: &[(&str, &[&str])]) -> Vec<(String, Vec<String>)> {
    list.iter()
        .map(|(n, pads)| (n.to_string(), pads.iter().map(|p| p.to_string()).collect()))
        .collect()
}

#[test]
fn pins_show_the_mapped_pads_of_the_current_footprint() {
    let dir = TempDir::new("pins");
    let mut engine = demo(&dir);
    // R_Shunt maps Pin 1 to Pads 1 and 3, and Pin 2 to Pads 2 and 4. Its default
    // Footprint, R_0603, only has Pads 1 and 2; R_2512_Kelvin has all four.
    let shunt = place(&mut engine, R_SHUNT, "0mm");
    let component = shunt["component"]["uuid"].clone();
    assert_eq!(
        pin_pads(&mut engine, &component),
        pins(&[("1", &["1"]), ("2", &["2"])])
    );

    engine.run_ok(
        "schematic.component.set",
        json!({"components": [component], "footprint": R_2512_KELVIN}),
    );
    let out = engine.run_ok("schematic.component.pins", json!({"component": component}));
    assert_eq!(out["footprint"], R_2512_KELVIN);
    assert_eq!(out["pins"][0]["unit"], "A");
    assert_eq!(
        pin_pads(&mut engine, &component),
        pins(&[("1", &["1", "3"]), ("2", &["2", "4"])])
    );
}

#[test]
fn every_unit_of_a_multi_unit_component_lists_its_pins() {
    let dir = TempDir::new("pins-units");
    let mut engine = demo(&dir);
    let (component, _, _) = op_amp_on_two_sheets(&mut engine);
    let out = engine.run_ok("schematic.component.pins", json!({"component": component}));
    let units: Vec<&str> = out["pins"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["unit"].as_str().unwrap())
        .collect();
    assert!(units.contains(&"A") && units.contains(&"B") && units.contains(&"Power"));

    let missing = engine.run_err(
        "schematic.component.pins",
        json!({"component": "00000000-0000-4000-8000-0000000000ff"}),
    );
    assert_eq!(missing.code, ErrorCode::NotFound);
}
