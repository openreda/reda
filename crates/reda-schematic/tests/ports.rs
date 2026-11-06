//! Port and Power Port depth: editing them, their hide flags, the Power Port Presets, and
//! a Net Label sitting on a Pin end with no stub Wire.

use std::path::{Path, PathBuf};

use reda_command::ErrorCode;
use reda_testkit::{TempDir, TestEngine, read_tree, test_engine_with_library};
use serde_json::{Value, json};

const SHEET: &str = "00000000-0000-4000-8000-000000000002";
const R_10K: &str = "a0000000-0000-4000-8000-000000000100";

fn library() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/library/Test")
}

fn demo(dir: &TempDir) -> TestEngine {
    let mut engine = test_engine_with_library(&library());
    engine.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
    engine
}

/// A point given in mil, as Command params.
fn at(x: i64, y: i64) -> Value {
    json!({"x": format!("{x}mil"), "y": format!("{y}mil")})
}

fn object(engine: &mut TestEngine, uuid: &Value) -> Value {
    engine.run_ok("object.get", json!({"uuid": uuid}))["object"].clone()
}

fn sheet_file(engine: &mut TestEngine, dir: &TempDir) -> String {
    engine.run_ok("project.save", json!({}));
    let tree = read_tree(&dir.path().join("Demo"));
    String::from_utf8(tree["Demo.redasch"].clone()).unwrap()
}

#[test]
fn a_port_can_be_renamed_turned_round_and_hide_its_cross_reference() {
    let dir = TempDir::new("port-set");
    let mut engine = demo(&dir);
    let out = engine.run_ok(
        "schematic.port.add",
        json!({"document": SHEET, "name": "SDA", "at": at(0, 0)}),
    );
    let port = out["result"]["port"].clone();
    engine.run_ok(
        "schematic.port.set",
        json!({"uuid": port, "name": " I2C_SDA ", "direction": "out",
               "hide_cross_reference": true}),
    );
    let stored = object(&mut engine, &port);
    assert_eq!(stored["name"], "I2C_SDA");
    assert_eq!(stored["direction"], "out");
    assert_eq!(stored["hide_cross_reference"], true);
    assert!(sheet_file(&mut engine, &dir).contains(r#""hide_cross_reference":true"#));

    let err = engine.run_err("schematic.port.set", json!({"uuid": port, "name": "A,B"}));
    assert_eq!(err.code, ErrorCode::InvalidParams);
    let err = engine.run_err("schematic.port.set", json!({"uuid": SHEET, "name": "X"}));
    assert_eq!(err.code, ErrorCode::NotFound);
}

#[test]
fn a_power_port_can_hide_its_name_and_still_joins_its_net() {
    let dir = TempDir::new("power-hide");
    let mut engine = demo(&dir);
    let out = engine.run_ok(
        "schematic.power_port.add",
        json!({"document": SHEET, "name": "GND", "at": at(0, 0), "hide_name": true}),
    );
    let gnd = out["result"]["power_port"].clone();
    assert_eq!(object(&mut engine, &gnd)["hide_name"], true);
    assert_eq!(object(&mut engine, &gnd)["style"], "ground");

    let nets = engine.run_ok("schematic.project_nets", json!({}));
    assert_eq!(nets["nets"][0]["name"], "GND", "{nets}");

    engine.run_ok(
        "schematic.power_port.set",
        json!({"uuid": gnd, "hide_name": false, "style": "earth", "name": "PE"}),
    );
    let stored = object(&mut engine, &gnd);
    assert_eq!(stored["style"], "earth");
    assert_eq!(stored["name"], "PE");
    // A shown name is the default, and is left out of the file.
    assert!(!sheet_file(&mut engine, &dir).contains("hide_name"));
}

#[test]
fn every_power_port_style_can_be_placed() {
    let dir = TempDir::new("power-styles");
    let mut engine = demo(&dir);
    let styles = [
        "bar",
        "arrow",
        "circle",
        "wave",
        "ground",
        "signal_ground",
        "earth",
    ];
    for (i, style) in styles.iter().enumerate() {
        let out = engine.run_ok(
            "schematic.power_port.add",
            json!({"document": SHEET, "name": "VCC", "at": at(i as i64 * 200, 0),
                   "style": style}),
        );
        assert_eq!(
            object(&mut engine, &out["result"]["power_port"])["style"],
            *style
        );
    }
    let err = engine.run_err(
        "schematic.power_port.add",
        json!({"document": SHEET, "name": "VCC", "at": at(0, 0), "style": "triangle"}),
    );
    assert_eq!(err.code, ErrorCode::InvalidParams);
}

#[test]
fn the_power_port_presets_are_gnd_3v3_5v_and_vcc() {
    let presets: Vec<_> = reda_schematic::POWER_PORT_PRESETS
        .iter()
        .map(|(name, style)| (*name, serde_json::to_value(style).unwrap()))
        .collect();
    assert_eq!(
        presets,
        [
            ("GND", json!("ground")),
            ("3V3", json!("bar")),
            ("5V", json!("bar")),
            ("VCC", json!("bar")),
        ]
    );
}

#[test]
fn a_net_label_on_a_pin_end_names_that_pins_net_with_no_wire() {
    let dir = TempDir::new("label-on-pin");
    let mut engine = demo(&dir);
    // The test Resistor's Pin 1 is 150 mil above its origin.
    engine.run_ok(
        "schematic.component.place",
        json!({"document": SHEET, "part": R_10K, "at": at(0, 0)}),
    );
    engine.run_ok(
        "schematic.net_label.add",
        json!({"document": SHEET, "name": "VIN", "at": at(0, 150)}),
    );
    let nets = engine.run_ok("schematic.project_nets", json!({}));
    let vin = nets["nets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["name"] == "VIN")
        .unwrap_or_else(|| panic!("no VIN Net: {nets}"));
    assert_eq!(vin["pins"][0]["designator"], "R1");
    assert_eq!(vin["pins"][0]["number"], "1");
}
