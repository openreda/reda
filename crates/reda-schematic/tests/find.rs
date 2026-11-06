//! Finding objects with `schematic.find` and selecting similar ones with
//! `schematic.select_similar`.

use std::path::{Path, PathBuf};

use reda_command::ErrorCode;
use reda_testkit::{TempDir, TestEngine, test_engine_with_library};
use serde_json::{Value, json};

const SHEET: &str = "00000000-0000-4000-8000-000000000002";
const R_10K: &str = "a0000000-0000-4000-8000-000000000100";
const C_100N: &str = "a0000000-0000-4000-8000-000000000110";
const LM358: &str = "a0000000-0000-4000-8000-000000000120";
const R_0805: &str = "a0000000-0000-4000-8000-000000000050";

fn library() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/library/Test")
}

fn demo(dir: &TempDir) -> TestEngine {
    let mut engine = test_engine_with_library(&library());
    engine.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
    engine
}

fn place_on(engine: &mut TestEngine, sheet: &Value, part: &str, x: i64) -> Value {
    engine.run_ok(
        "schematic.component.place",
        json!({"document": sheet, "part": part, "at": {"x": x, "y": 0}}),
    )["result"]
        .clone()
}

fn second_sheet(engine: &mut TestEngine) -> Value {
    engine.run_ok("document.add", json!({"kind": "schematic"}))["result"]["uuid"].clone()
}

/// `(field, text, unit)` of each match.
fn find(engine: &mut TestEngine, params: Value) -> Vec<(String, String, Value)> {
    let out = engine.run_ok("schematic.find", params);
    out["matches"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| {
            (
                m["field"].as_str().unwrap().to_owned(),
                m["text"].as_str().unwrap().to_owned(),
                m["unit"].clone(),
            )
        })
        .collect()
}

fn m(field: &str, text: &str, unit: &Value) -> (String, String, Value) {
    (field.to_owned(), text.to_owned(), unit.clone())
}

#[test]
fn find_searches_designators_and_values_across_sheets() {
    let dir = TempDir::new("find");
    let mut engine = demo(&dir);
    let sheet = json!(SHEET);
    let second = second_sheet(&mut engine);
    // On the second sheet first, so Document order, not placing order, decides.
    let r1 = place_on(&mut engine, &second, R_10K, 0);
    let u1a = place_on(&mut engine, &sheet, LM358, 0);
    let u1b = engine.run_ok(
        "schematic.unit.place",
        json!({"document": second, "component": u1a["component"]["uuid"],
               "at": {"x": 1000, "y": 0}}),
    )["result"]
        .clone();
    let c1 = place_on(&mut engine, &sheet, C_100N, 2000);

    // Case does not matter, and every Unit of a Component matches.
    assert_eq!(
        find(&mut engine, json!({"text": "u1"})),
        vec![
            m("designator", "U1", &u1a["unit"]),
            m("designator", "U1", &u1b["unit"])
        ]
    );
    assert_eq!(
        find(&mut engine, json!({"text": "100N"})),
        vec![m("value", "100n", &c1["unit"])]
    );
    // A substring matches unless `exact`; designators come before values.
    assert_eq!(
        find(&mut engine, json!({"text": "1"})),
        vec![
            m("designator", "C1", &c1["unit"]),
            m("designator", "U1", &u1a["unit"]),
            m("value", "100n", &c1["unit"]),
            m("designator", "R1", &r1["unit"]),
            m("designator", "U1", &u1b["unit"]),
            m("value", "10k", &r1["unit"]),
        ]
    );
    assert_eq!(
        find(
            &mut engine,
            json!({"text": "1", "fields": ["value"], "exact": true})
        ),
        vec![]
    );
    assert_eq!(
        find(&mut engine, json!({"text": "lm358", "exact": true})),
        vec![
            m("value", "LM358", &u1a["unit"]),
            m("value", "LM358", &u1b["unit"])
        ]
    );
    let out = engine.run_ok("schematic.find", json!({"text": "R1"}));
    assert_eq!(out["matches"][0]["document"], second);
    assert_eq!(out["matches"][0]["component"], r1["component"]["uuid"]);

    let blank = engine.run_err("schematic.find", json!({"text": ""}));
    assert_eq!(blank.code, ErrorCode::InvalidParams);
}

#[test]
fn find_orders_designators_naturally() {
    let dir = TempDir::new("find-natural");
    let mut engine = demo(&dir);
    let sheet = json!(SHEET);
    let units: Vec<Value> = (0..10)
        .map(|i| place_on(&mut engine, &sheet, R_10K, i * 1000)["unit"].clone())
        .collect();
    let found: Vec<String> = find(&mut engine, json!({"text": "R1", "fields": ["designator"]}))
        .into_iter()
        .map(|(_, text, _)| text)
        .collect();
    assert_eq!(found, ["R1", "R10"]);
    let all = find(&mut engine, json!({"text": "R", "fields": ["designator"]}));
    assert_eq!(all[1], m("designator", "R2", &units[1]));
    assert_eq!(all[9], m("designator", "R10", &units[9]));
}

fn similar(engine: &mut TestEngine, unit: &Value, on: Value, scope: &str) -> Value {
    engine.run_ok(
        "schematic.select_similar",
        json!({"unit": unit, "match": on, "scope": scope}),
    )["units"]
        .clone()
}

fn set(engine: &mut TestEngine, placed: &Value, params: Value) {
    let mut params = params;
    params["components"] = json!([placed["component"]["uuid"]]);
    engine.run_ok("schematic.component.set", params);
}

#[test]
fn select_similar_matches_on_the_chosen_fields_on_a_sheet_or_the_project() {
    let dir = TempDir::new("similar");
    let mut engine = demo(&dir);
    let sheet = json!(SHEET);
    let second = second_sheet(&mut engine);
    let r1 = place_on(&mut engine, &sheet, R_10K, 0);
    let r2 = place_on(&mut engine, &second, R_10K, 0);
    let r3 = place_on(&mut engine, &sheet, R_10K, 1000);
    let c1 = place_on(&mut engine, &sheet, C_100N, 2000);
    let u1a = place_on(&mut engine, &sheet, LM358, 3000);
    let u1b = engine.run_ok(
        "schematic.unit.place",
        json!({"document": second, "component": u1a["component"]["uuid"],
               "at": {"x": 1000, "y": 0}}),
    )["result"]
        .clone();
    set(
        &mut engine,
        &r3,
        json!({"value": "4k7", "footprint": R_0805}),
    );
    let (r1, r2, r3, c1) = (&r1["unit"], &r2["unit"], &r3["unit"], &c1["unit"]);

    assert_eq!(
        similar(&mut engine, r1, json!(["value"]), "project"),
        json!([r1, r2])
    );
    assert_eq!(
        similar(&mut engine, r1, json!(["value"]), "sheet"),
        json!([r1])
    );
    assert_eq!(
        similar(&mut engine, r1, json!(["part"]), "project"),
        json!([r1, r3, r2])
    );
    assert_eq!(
        similar(&mut engine, r1, json!(["part", "footprint"]), "project"),
        json!([r1, r2])
    );
    assert_eq!(
        similar(&mut engine, r3, json!(["designator_prefix"]), "sheet"),
        json!([r1, r3])
    );
    assert_eq!(
        similar(
            &mut engine,
            c1,
            json!(["component_type", "not_fitted"]),
            "sheet"
        ),
        json!([c1, r1, r3, u1a["unit"]])
    );
    assert_eq!(
        similar(&mut engine, &u1a["unit"], json!(["part"]), "project"),
        json!([u1a["unit"], u1b["unit"]])
    );
    // A Unit matches on the Symbol Unit it shows.
    assert_eq!(
        similar(&mut engine, &u1b["unit"], json!(["unit"]), "project"),
        json!([u1b["unit"]])
    );

    // Any field by name; a missing field matches others that lack it.
    let r2_component = object_component(&mut engine, r2);
    engine.run_ok(
        "schematic.component.set",
        json!({"components": [r2_component], "fields": {"LCSC": null}}),
    );
    assert_eq!(
        similar(&mut engine, r1, json!([{"field": "LCSC"}]), "project"),
        json!([r1, r3])
    );
    assert_eq!(
        similar(
            &mut engine,
            c1,
            json!([{"field": "LCSC"}, "designator_prefix"]),
            "project"
        ),
        json!([c1])
    );

    let none = engine.run_err(
        "schematic.select_similar",
        json!({"unit": r1, "match": [], "scope": "project"}),
    );
    assert_eq!(none.code, ErrorCode::InvalidParams);
    let missing = engine.run_err(
        "schematic.select_similar",
        json!({"unit": "00000000-0000-4000-8000-0000000000ff", "match": ["value"],
               "scope": "project"}),
    );
    assert_eq!(missing.code, ErrorCode::NotFound);
}

fn object_component(engine: &mut TestEngine, unit: &Value) -> Value {
    engine.run_ok("object.get", json!({"uuid": unit}))["object"]["component"]["uuid"].clone()
}

#[test]
fn find_matches_a_designator_with_or_without_its_unit_suffix() {
    let dir = TempDir::new("find-unit-suffix");
    let mut engine = demo(&dir);
    let sheet = json!(SHEET);
    let a = place_on(&mut engine, &sheet, LM358, 0);
    let b = engine.run_ok(
        "schematic.unit.place",
        json!({"document": SHEET, "component": a["component"]["uuid"], "at": {"x": "30mm", "y": 0}}),
    )["result"]
        .clone();
    let (a, b) = (&a["unit"], &b["unit"]);

    let exact = |text: &str| json!({"text": text, "fields": ["designator"], "exact": true});
    assert_eq!(
        find(&mut engine, exact("U1")),
        [m("designator", "U1", a), m("designator", "U1", b)]
    );
    assert_eq!(find(&mut engine, exact("u1b")), [m("designator", "U1B", b)]);
    assert_eq!(
        find(&mut engine, exact("U1:1")),
        [m("designator", "U1:1", a)]
    );
}
