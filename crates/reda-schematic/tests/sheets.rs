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

fn port(engine: &mut TestEngine, sheet: &str, name: &str, x: i64, y: i64) -> Value {
    let out = engine.run_ok(
        "schematic.port.add",
        json!({"document": sheet, "name": name, "at": at(x, y), "direction": "out"}),
    );
    out["result"]["port"].clone()
}

fn power(engine: &mut TestEngine, sheet: &str, name: &str, x: i64, y: i64) -> Value {
    let out = engine.run_ok(
        "schematic.power_port.add",
        json!({"document": sheet, "name": name, "at": at(x, y)}),
    );
    out["result"]["power_port"].clone()
}

fn label(engine: &mut TestEngine, sheet: &str, name: &str, x: i64, y: i64) {
    engine.run_ok(
        "schematic.net_label.add",
        json!({"document": sheet, "name": name, "at": at(x, y)}),
    );
}

/// The `designator-number` of every Pin in a Net.
fn pins(net: &Value) -> Vec<String> {
    net["pins"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            let designator = p["designator"].as_str().unwrap();
            format!("{designator}-{}", p["number"].as_str().unwrap())
        })
        .collect()
}

/// The Project-wide Net holding Pin `number` of `designator`.
fn project_net(engine: &mut TestEngine, pin: &str) -> Value {
    let all = engine.run_ok("schematic.project_nets", json!({}));
    all["nets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| pins(n).iter().any(|p| p == pin))
        .unwrap_or_else(|| panic!("{pin} is in no Net: {all}"))
        .clone()
}

/// The name of the Net holding `pin` on one sheet, as `schematic.nets` gives it.
fn sheet_net_name(engine: &mut TestEngine, sheet: &str, pin: &str) -> Value {
    let all = engine.run_ok("schematic.nets", json!({"document": sheet}));
    all["nets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| pins(n).iter().any(|p| p == pin))
        .unwrap_or_else(|| panic!("{pin} is in no Net: {all}"))["name"]
        .clone()
}

#[test]
fn ports_of_the_same_name_join_nets_across_sheets() {
    let dir = TempDir::new("ports-join");
    let (mut engine, second) = demo(&dir);
    place(&mut engine, SHEET, 0, 0); // R1
    place(&mut engine, &second, 0, 0); // R2
    engine.run_ok(
        "schematic.wire.add",
        json!({"document": SHEET, "points": [at(0, 150), at(0, 300)]}),
    );
    let sda = port(&mut engine, SHEET, "SDA", 0, 300);
    // A Port right on a Pin attaches to it.
    port(&mut engine, &second, "SDA", 0, 150);
    port(&mut engine, &second, "SCL", 0, -150);

    let net = project_net(&mut engine, "R1-1");
    assert_eq!(net["name"], "SDA");
    assert_eq!(pins(&net), ["R1-1", "R2-1"]);
    assert_eq!(net["sheets"], json!([SHEET, second]));
    assert_eq!(project_net(&mut engine, "R2-2")["name"], "SCL");
    assert_eq!(pins(&project_net(&mut engine, "R1-2")), ["R1-2"]);

    let stored = engine.run_ok("object.get", json!({"uuid": sda}))["object"].clone();
    assert_eq!(stored["type"], "port");
    assert_eq!(stored["direction"], "out");
    let sheet_nets = engine.run_ok("schematic.nets", json!({"document": SHEET}));
    let net = sheet_nets["nets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["name"] == "SDA")
        .unwrap();
    assert_eq!(net["ports"], json!([sda]));
    assert_eq!(net["port_names"], json!(["SDA"]));
}

#[test]
fn net_labels_do_not_join_across_sheets() {
    let dir = TempDir::new("labels-local");
    let (mut engine, second) = demo(&dir);
    place(&mut engine, SHEET, 0, 0);
    place(&mut engine, &second, 0, 0);
    label(&mut engine, SHEET, "DATA", 0, 150);
    label(&mut engine, &second, "DATA", 0, 150);
    assert_eq!(pins(&project_net(&mut engine, "R1-1")), ["R1-1"]);
    assert_eq!(project_net(&mut engine, "R2-1")["name"], "DATA");
}

#[test]
fn power_ports_join_across_the_project_and_win_the_name_over_ports() {
    let dir = TempDir::new("power-join");
    let (mut engine, second) = demo(&dir);
    let out = engine.run_ok("document.add", json!({"kind": "schematic"}));
    let third = out["result"]["uuid"].as_str().unwrap().to_owned();
    place(&mut engine, SHEET, 0, 0); // R1
    place(&mut engine, &second, 0, 0); // R2
    place(&mut engine, &third, 0, 0); // R3
    let gnd = power(&mut engine, SHEET, "GND", 0, -150);
    power(&mut engine, &third, "GND", 0, -150);
    // On the second sheet, R2-2 joins the first sheet through a Port only.
    port(&mut engine, &second, "RET", 0, -150);
    port(&mut engine, SHEET, "RET", 0, -150);

    let net = project_net(&mut engine, "R1-2");
    assert_eq!(net["name"], "GND");
    assert_eq!(pins(&net), ["R1-2", "R2-2", "R3-2"]);
    assert_eq!(net["names"], json!(["GND", "RET"]));
    // Each sheet names its part of the Net by the Project-wide winner.
    assert_eq!(sheet_net_name(&mut engine, &second, "R2-2"), "GND");

    let stored = engine.run_ok("object.get", json!({"uuid": gnd}))["object"].clone();
    assert_eq!(stored["type"], "power_port");
    assert_eq!(
        stored["style"], "ground",
        "GND defaults to the ground style"
    );
}

#[test]
fn name_priority_is_net_label_then_power_port_then_port_then_auto_name() {
    let dir = TempDir::new("name-priority");
    let (mut engine, _) = demo(&dir);
    place(&mut engine, SHEET, 0, 0); // R1
    place(&mut engine, SHEET, 1000, 0); // R2
    place(&mut engine, SHEET, 2000, 0); // R3
    place(&mut engine, SHEET, 3000, 0); // R4
    // R1-1: label, Port and Power Port.
    label(&mut engine, SHEET, "A_LABEL", 0, 150);
    port(&mut engine, SHEET, "B_PORT", 0, 150);
    power(&mut engine, SHEET, "VCC", 0, 150);
    // R2-1: label and Port.
    label(&mut engine, SHEET, "A_LABEL2", 1000, 150);
    port(&mut engine, SHEET, "B_PORT2", 1000, 150);
    // R3-1: a label.
    label(&mut engine, SHEET, "C_LABEL", 2000, 150);

    assert_eq!(sheet_net_name(&mut engine, SHEET, "R1-1"), "A_LABEL");
    assert_eq!(sheet_net_name(&mut engine, SHEET, "R2-1"), "A_LABEL2");
    assert_eq!(sheet_net_name(&mut engine, SHEET, "R3-1"), "C_LABEL");
    assert_eq!(sheet_net_name(&mut engine, SHEET, "R4-1"), "Net-(R4-1)");
}

#[test]
fn ports_and_power_ports_save_and_load() {
    let dir = TempDir::new("ports-save");
    let (mut engine, second) = demo(&dir);
    port(&mut engine, &second, "SDA", 0, 0);
    engine.run_ok(
        "schematic.power_port.add",
        json!({"document": second, "name": "3V3", "at": at(100, 0), "style": "arrow",
               "rotation": "90deg"}),
    );
    engine.run_ok("project.save", json!({}));
    let before = engine.project().unwrap().clone();
    let mut other = test_engine_with_library(&library());
    other.run_ok("project.open", json!({"path": dir.path().join("Demo")}));
    assert_eq!(other.project().unwrap(), &before);
    let text = std::fs::read_to_string(dir.path().join("Demo/sheet-2.redasch")).unwrap();
    assert!(text.contains("\"ports\": ["), "{text}");
    assert!(text.contains("\"style\":\"arrow\""), "{text}");
}

#[test]
fn a_port_or_power_port_needs_a_name() {
    let dir = TempDir::new("ports-name");
    let (mut engine, second) = demo(&dir);
    for id in ["schematic.port.add", "schematic.power_port.add"] {
        let err = engine.run_err(id, json!({"document": second, "name": " ", "at": at(0, 0)}));
        assert_eq!(err.code, reda_command::ErrorCode::InvalidParams);
    }
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
