//! `library.symbol.generate` and `library.symbol.generate.read`: the Symbol Wizard onto a
//! new Symbol, a new Unit, and an existing Unit when it reopens.

use reda_engine::command::ErrorCode;
use reda_testkit::{TempDir, TestEngine, test_engine};
use serde_json::{Value, json};

const MIL: i64 = 25_400;

/// A new Project in `dir/Demo` with the empty Project Library `lib`, and its UUID.
fn engine_with_library(dir: &TempDir) -> (TestEngine, String) {
    let mut e = test_engine();
    e.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
    let out = e.run_ok("library.create", json!({"path": "lib", "name": "Mine"}));
    e.run_ok("project.library.add", json!({"path": "lib"}));
    (e, out["library"].as_str().unwrap().to_owned())
}

fn rows(n: usize) -> Value {
    Value::Array(
        (1..=n)
            .map(|i| json!({"number": i.to_string(), "name": format!("P{i}")}))
            .collect(),
    )
}

fn object(e: &mut TestEngine, uuid: &str) -> Value {
    e.run_ok("object.get", json!({ "uuid": uuid }))["object"].clone()
}

fn pin<'a>(symbol: &'a Value, number: &str) -> &'a Value {
    symbol["pins"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["number"] == number)
        .unwrap()
}

fn at(pin: &Value) -> (i64, i64) {
    (
        pin["at"]["x"].as_i64().unwrap() / MIL,
        pin["at"]["y"].as_i64().unwrap() / MIL,
    )
}

fn new_symbol(e: &mut TestEngine, library: &str, layout: &str, pins: Value) -> Value {
    e.run_ok(
        "library.symbol.generate",
        json!({"layout": layout, "pins": pins,
               "target": {"kind": "new_symbol", "library": library, "name": layout}}),
    )
}

#[test]
fn a_new_symbol_is_opened_unsaved_with_one_unit_its_pins_and_a_body() {
    let dir = TempDir::new("wizard-new-symbol");
    let (mut e, lib) = engine_with_library(&dir);
    let out = new_symbol(&mut e, &lib, "dil", rows(8));
    let symbol = out["symbol"].as_str().unwrap().to_owned();
    assert_eq!(out["unit_name"], "A");
    assert_eq!(out["pins"].as_array().unwrap().len(), 8);

    let s = object(&mut e, &symbol);
    assert_eq!(s["name"], "dil");
    assert_eq!(s["designator_prefix"], "U");
    assert_eq!(s["units"].as_array().unwrap().len(), 1);
    assert_eq!(at(pin(&s, "1")), (0, 0));
    assert_eq!(
        at(pin(&s, "8")),
        (500, 0),
        "two-letter names widen the body to 300 mil"
    );
    assert_eq!(pin(&s, "1")["name"], "P1");
    assert_eq!(s["graphics"].as_array().unwrap().len(), 1);
    assert_eq!(s["graphics"][0]["shape"]["kind"], "rect");
    assert_eq!(out["pins"][0], pin(&s, "1")["uuid"]);

    let file = dir.path().join("Demo/lib/symbols/dil.redasym");
    assert!(!file.exists(), "nothing is written until saved");
    e.run_ok("library.element.save", json!({ "element": symbol }));
    assert!(file.exists());
}

#[test]
fn connector_layouts_default_to_a_j_prefix() {
    let dir = TempDir::new("wizard-prefix");
    let (mut e, lib) = engine_with_library(&dir);
    for (layout, prefix) in [("connector", "J"), ("zig_zag", "J"), ("quad", "U")] {
        let out = new_symbol(&mut e, &lib, layout, rows(4));
        let s = object(&mut e, out["symbol"].as_str().unwrap());
        assert_eq!(s["designator_prefix"], prefix, "{layout}");
    }
    let out = e.run_ok(
        "library.symbol.generate",
        json!({"layout": "single_row", "pins": rows(2),
               "target": {"kind": "new_symbol", "library": lib, "name": "Sw",
                          "designator_prefix": "SW"}}),
    );
    assert_eq!(
        object(&mut e, out["symbol"].as_str().unwrap())["designator_prefix"],
        "SW"
    );
}

#[test]
fn a_new_unit_is_added_after_the_last_and_is_one_undo_step() {
    let dir = TempDir::new("wizard-new-unit");
    let (mut e, lib) = engine_with_library(&dir);
    let symbol = new_symbol(&mut e, &lib, "dil", rows(4))["symbol"]
        .as_str()
        .unwrap()
        .to_owned();
    let power = json!([{"number": "5", "name": "VCC", "electrical_type": "power_in"},
                       {"number": "6", "name": "GND", "electrical_type": "power_in"}]);
    let out = e.run_ok(
        "library.symbol.generate",
        json!({"layout": "dil", "pins": power,
               "target": {"kind": "new_unit", "symbol": symbol}}),
    );
    assert_eq!(out["unit_name"], "B");
    let s = object(&mut e, &symbol);
    assert_eq!(s["units"].as_array().unwrap().len(), 2);
    assert_eq!(pin(&s, "5")["unit"], out["unit"]);
    assert_eq!(
        at(pin(&s, "5")),
        (0, 0),
        "the Unit's first row is at the origin"
    );
    assert_eq!(s["graphics"].as_array().unwrap().len(), 2);

    let undone = e.run_ok("edit.undo", json!({ "document": symbol }));
    assert_eq!(undone["result"]["label"], "Symbol Wizard");
    let s = object(&mut e, &symbol);
    assert_eq!(s["units"].as_array().unwrap().len(), 1);
    assert_eq!(s["pins"].as_array().unwrap().len(), 4);
}

#[test]
fn a_new_unit_cannot_reuse_another_units_pin_number() {
    let dir = TempDir::new("wizard-clash");
    let (mut e, lib) = engine_with_library(&dir);
    let symbol = new_symbol(&mut e, &lib, "dil", rows(4))["symbol"]
        .as_str()
        .unwrap()
        .to_owned();
    let err = e.run_err(
        "library.symbol.generate",
        json!({"layout": "dil", "pins": rows(2),
               "target": {"kind": "new_unit", "symbol": symbol}}),
    );
    assert_eq!(err.code, ErrorCode::Rejected);
    assert_eq!(
        object(&mut e, &symbol)["units"].as_array().unwrap().len(),
        1
    );
}

#[test]
fn reopening_reads_the_pins_and_regenerating_keeps_their_uuids_by_number() {
    let dir = TempDir::new("wizard-reopen");
    let (mut e, lib) = engine_with_library(&dir);
    let out = new_symbol(&mut e, &lib, "quad", rows(8));
    let (symbol, unit) = (
        out["symbol"].as_str().unwrap().to_owned(),
        out["unit"].as_str().unwrap().to_owned(),
    );
    let before = object(&mut e, &symbol);

    let read = e.run_ok("library.symbol.generate.read", json!({ "symbol": symbol }));
    assert_eq!(read["layout"], "manual");
    assert_eq!(read["pin_length"], 100 * MIL);
    assert_eq!(
        read["pins"][0],
        json!({"number": "1", "name": "P1",
                                       "electrical_type": "passive", "side": "left"})
    );

    // Same rows, back through the manual layout: nothing moves.
    e.run_ok(
        "library.symbol.generate",
        json!({"layout": read["layout"], "pins": read["pins"], "pin_length": read["pin_length"],
               "target": {"kind": "unit", "symbol": symbol, "unit": unit}}),
    );
    let same = object(&mut e, &symbol);
    assert_eq!(same["pins"], before["pins"]);

    // Fewer rows in another layout: kept numbers keep their UUIDs, others go.
    let out = e.run_ok(
        "library.symbol.generate",
        json!({"layout": "dil", "pins": rows(6),
               "target": {"kind": "unit", "symbol": symbol, "unit": unit}}),
    );
    let after = object(&mut e, &symbol);
    assert_eq!(after["pins"].as_array().unwrap().len(), 6);
    assert_eq!(pin(&after, "3")["uuid"], pin(&before, "3")["uuid"]);
    assert_eq!(out["pins"][2], pin(&before, "3")["uuid"]);
    assert_eq!(after["graphics"].as_array().unwrap().len(), 1);
}

#[test]
fn bad_params_are_refused_before_anything_changes() {
    let dir = TempDir::new("wizard-bad");
    let (mut e, lib) = engine_with_library(&dir);
    let err = e.run_err(
        "library.symbol.generate",
        json!({"layout": "manual", "pins": rows(2),
               "target": {"kind": "new_symbol", "library": lib, "name": "M"}}),
    );
    assert_eq!(err.code, ErrorCode::InvalidParams);
    let err = e.run_err(
        "library.symbol.generate",
        json!({"layout": "dil", "pins": rows(2),
               "target": {"kind": "new_symbol", "library": lib, "name": "a/b"}}),
    );
    assert_eq!(err.code, ErrorCode::InvalidParams);
    let err = e.run_err(
        "library.symbol.generate",
        json!({"layout": "dil", "pins": rows(2),
               "target": {"kind": "unit", "symbol": lib, "unit": lib}}),
    );
    assert_eq!(err.code, ErrorCode::NotFound);
}

#[test]
fn starter_symbols_are_read_only_but_can_be_read() {
    let mut e = reda_testkit::test_engine_with_library(&reda_testkit::starter_library());
    let out = e.run_ok("library.element.open", json!({"element": STARTER_555}));
    let symbol = out["element"].as_str().unwrap().to_owned();
    let read = e.run_ok("library.symbol.generate.read", json!({ "symbol": symbol }));
    assert_eq!(read["pins"].as_array().unwrap().len(), 8);
    let err = e.run_err(
        "library.symbol.generate",
        json!({"layout": read["layout"], "pins": read["pins"],
               "target": {"kind": "unit", "symbol": symbol,
                          "unit": "ef661553-ab0f-4180-af7e-49a6f3efbf77"}}),
    );
    assert_eq!(err.code, ErrorCode::Rejected);
}

const STARTER_555: &str = "eebdad0d-b78a-4f9b-be97-ae74568d41ed";
