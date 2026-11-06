//! Net names: case-insensitive joining, the name priority and its Project options, board
//! names with sheet numbers, the naming checks and the reserved characters.

use std::path::{Path, PathBuf};

use reda_command::ErrorCode;
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
/// Places a Resistor on `sheet` with its origin at (`x`, `y`) mil.
fn place(engine: &mut TestEngine, sheet: &str, x: i64, y: i64) {
    engine.run_ok(
        "schematic.component.place",
        json!({"document": sheet, "part": R_10K, "at": at(x, y)}),
    );
}

fn add(engine: &mut TestEngine, id: &str, sheet: &str, name: &str, x: i64, y: i64) -> Value {
    let out = engine.run_ok(id, json!({"document": sheet, "name": name, "at": at(x, y)}));
    out["result"]
        .as_object()
        .unwrap()
        .values()
        .next()
        .unwrap()
        .clone()
}

fn label(engine: &mut TestEngine, sheet: &str, name: &str, x: i64, y: i64) -> Value {
    add(engine, "schematic.net_label.add", sheet, name, x, y)
}

fn port(engine: &mut TestEngine, sheet: &str, name: &str, x: i64, y: i64) -> Value {
    add(engine, "schematic.port.add", sheet, name, x, y)
}

fn power(engine: &mut TestEngine, sheet: &str, name: &str, x: i64, y: i64) -> Value {
    add(engine, "schematic.power_port.add", sheet, name, x, y)
}

/// The `designator-number` of every Pin in a Net.
fn pins(net: &Value) -> Vec<String> {
    net["pins"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            format!(
                "{}-{}",
                p["designator"].as_str().unwrap(),
                p["number"].as_str().unwrap()
            )
        })
        .collect()
}

/// The Project-wide Net holding `pin`, such as `R1-1`.
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

fn options(engine: &mut TestEngine, options: Value) {
    engine.run_ok("schematic.options.set", options);
}

/// The Violations of check `check`.
fn violations(engine: &mut TestEngine, check: &str) -> Vec<Value> {
    let out = engine.run_ok("erc.run", json!({}));
    out["violations"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| v["check"] == check)
        .cloned()
        .collect()
}

fn sorted(mut v: Vec<Value>) -> Vec<Value> {
    v.sort_by_key(|v| v.to_string());
    v
}

#[test]
fn net_labels_ports_and_power_ports_join_ignoring_case() {
    let dir = TempDir::new("names-case");
    let (mut engine, second) = demo(&dir);
    place(&mut engine, SHEET, 0, 0); // R1
    place(&mut engine, SHEET, 1000, 0); // R2
    place(&mut engine, &second, 0, 0); // R3
    label(&mut engine, SHEET, "abc", 0, 150);
    label(&mut engine, SHEET, "ABC", 1000, 150);
    port(&mut engine, SHEET, "sda", 0, -150);
    port(&mut engine, &second, "SDA", 0, -150);
    power(&mut engine, SHEET, "vcc", 1000, -150);
    power(&mut engine, &second, "VCC", 0, 150);

    let net = project_net(&mut engine, "R1-1");
    assert_eq!(pins(&net), ["R1-1", "R2-1"]);
    // One name, spelled as its lowest by code point.
    assert_eq!(net["name"], "ABC");
    assert_eq!(net["names"], json!(["ABC"]));
    assert_eq!(pins(&project_net(&mut engine, "R1-2")), ["R1-2", "R3-2"]);
    assert_eq!(pins(&project_net(&mut engine, "R2-2")), ["R2-2", "R3-1"]);
    assert!(violations(&mut engine, "net_names_on_sheet").is_empty());
    assert!(violations(&mut engine, "duplicate_nets").is_empty());
}

#[test]
fn a_net_label_wins_over_a_power_port_and_a_power_port_over_a_port() {
    let dir = TempDir::new("names-priority");
    let (mut engine, _) = demo(&dir);
    place(&mut engine, SHEET, 0, 0); // R1
    place(&mut engine, SHEET, 1000, 0); // R2
    place(&mut engine, SHEET, 2000, 0); // R3
    label(&mut engine, SHEET, "Z_LABEL", 0, 150);
    power(&mut engine, SHEET, "A_POWER", 0, 150);
    port(&mut engine, SHEET, "A_PORT", 0, 150);
    power(&mut engine, SHEET, "Z_POWER", 1000, 150);
    port(&mut engine, SHEET, "A_PORT2", 1000, 150);
    // Two Net Labels: the lowest by code point wins, so `B` beats `a`.
    label(&mut engine, SHEET, "a", 2000, 150);
    label(&mut engine, SHEET, "B", 2000, 150);

    let net = project_net(&mut engine, "R1-1");
    assert_eq!(net["name"], "Z_LABEL");
    assert_eq!(net["name_source"], "net_label");
    assert_eq!(net["names"], json!(["Z_LABEL", "A_POWER", "A_PORT"]));
    assert_eq!(project_net(&mut engine, "R2-1")["name"], "Z_POWER");
    assert_eq!(project_net(&mut engine, "R3-1")["name"], "B");
    assert_eq!(project_net(&mut engine, "R3-2")["name"], "Net-(R3-2)");
    assert_eq!(project_net(&mut engine, "R3-2")["name_source"], "auto_name");
}

#[test]
fn the_project_options_reorder_the_priority_and_are_saved_in_the_project_file() {
    let dir = TempDir::new("names-options");
    let (mut engine, _) = demo(&dir);
    place(&mut engine, SHEET, 0, 0); // R1
    place(&mut engine, SHEET, 1000, 0); // R2
    label(&mut engine, SHEET, "LABEL", 0, 150);
    power(&mut engine, SHEET, "POWER", 0, 150);
    port(&mut engine, SHEET, "PORT", 1000, 150);

    options(&mut engine, json!({"power_port_names_priority": true}));
    assert_eq!(project_net(&mut engine, "R1-1")["name"], "POWER");
    assert_eq!(project_net(&mut engine, "R2-1")["name"], "PORT");
    let out = engine.run_ok(
        "schematic.options.set",
        json!({"allow_ports_to_name_nets": false}),
    );
    assert_eq!(
        out["result"],
        json!({"power_port_names_priority": true, "allow_ports_to_name_nets": false,
               "append_sheet_numbers": false})
    );
    assert_eq!(project_net(&mut engine, "R2-1")["name"], "Net-(R2-1)");
    assert_eq!(project_net(&mut engine, "R2-1")["names"], json!([]));

    engine.run_ok("project.save", json!({}));
    let text = std::fs::read_to_string(dir.path().join("Demo/Demo.reda")).unwrap();
    assert!(
        text.contains(r#""schematic": {"power_port_names_priority":true,"allow_ports_to_name_nets":false,"append_sheet_numbers":false}"#),
        "{text}"
    );
    let before = engine.project().unwrap().clone();
    let mut other = test_engine_with_library(&library());
    other.run_ok("project.open", json!({"path": dir.path().join("Demo")}));
    assert_eq!(other.project().unwrap(), &before);

    engine.run_ok("edit.undo", json!({"document": "project"}));
    assert_eq!(project_net(&mut engine, "R2-1")["name"], "PORT");
}

#[test]
fn append_sheet_numbers_suffixes_the_board_name_of_local_nets_named_by_a_net_label() {
    let dir = TempDir::new("names-suffix");
    let (mut engine, second) = demo(&dir);
    place(&mut engine, SHEET, 0, 0); // R1
    place(&mut engine, &second, 0, 0); // R2
    label(&mut engine, SHEET, "DATA", 0, 150);
    label(&mut engine, &second, "DATA", 0, 150);
    // Not Local: a Port joins it to nothing else, but it is no Local Net.
    label(&mut engine, &second, "CLK", 0, -150);
    port(&mut engine, &second, "CLK", 0, -150);
    assert_eq!(project_net(&mut engine, "R1-1")["board_name"], "DATA");

    // Two Nets named DATA: a duplicate the suffix fixes.
    let found = violations(&mut engine, "duplicate_nets");
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0]["severity"], "error");
    assert_eq!(found[0]["args"], json!({"name": "DATA", "count": 2}));
    assert_eq!(found[0]["objects"].as_array().unwrap().len(), 2);

    options(&mut engine, json!({"append_sheet_numbers": true}));
    let first = project_net(&mut engine, "R1-1");
    assert_eq!(first["name"], "DATA");
    assert_eq!(first["board_name"], "DATA_1");
    assert_eq!(project_net(&mut engine, "R2-1")["board_name"], "DATA_2");
    assert_eq!(project_net(&mut engine, "R2-2")["board_name"], "CLK");
    assert_eq!(project_net(&mut engine, "R1-2")["board_name"], "Net-(R1-2)");
    assert!(violations(&mut engine, "duplicate_nets").is_empty());
    // Sheets keep showing the plain name.
    let nets = engine.run_ok("schematic.nets", json!({"document": second}));
    assert!(
        nets["nets"]
            .as_array()
            .unwrap()
            .iter()
            .any(|n| n["name"] == "DATA"),
        "{nets}"
    );
}

#[test]
fn a_net_label_and_a_power_port_of_one_name_join_only_through_a_wire() {
    let dir = TempDir::new("names-kinds");
    let (mut engine, _) = demo(&dir);
    place(&mut engine, SHEET, 0, 0); // R1
    let l = label(&mut engine, SHEET, "GND", 0, 150);
    let p = power(&mut engine, SHEET, "gnd", 0, -150);
    assert_eq!(pins(&project_net(&mut engine, "R1-1")), ["R1-1"]);
    let found = violations(&mut engine, "duplicate_nets");
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0]["args"]["name"], "GND");
    assert_eq!(
        sorted(found[0]["objects"].as_array().unwrap().clone()),
        sorted(vec![l, p])
    );
}

#[test]
fn different_names_on_one_sheet_are_an_error_that_says_which_won_and_why() {
    let dir = TempDir::new("names-on-sheet");
    let (mut engine, _) = demo(&dir);
    place(&mut engine, SHEET, 0, 0); // R1
    place(&mut engine, SHEET, 1000, 0); // R2
    let b = label(&mut engine, SHEET, "B", 0, 150);
    let a = label(&mut engine, SHEET, "A", 0, 150);
    let a2 = label(&mut engine, SHEET, "a", 0, 150);
    let p = power(&mut engine, SHEET, "VCC", 1000, 150);
    let l = label(&mut engine, SHEET, "X", 1000, 150);

    let found = violations(&mut engine, "net_names_on_sheet");
    assert_eq!(found.len(), 2, "{found:?}");
    assert_eq!(found[0]["severity"], "error");
    assert_eq!(
        found[0]["args"],
        json!({"net": "A", "names": "A, B", "kind": "net_label", "reason": "code_point"})
    );
    assert_eq!(
        sorted(found[0]["objects"].as_array().unwrap().clone()),
        sorted(vec![a, a2, b])
    );
    assert_eq!(
        found[1]["args"],
        json!({"net": "X", "names": "X, VCC", "kind": "net_label", "reason": "priority"})
    );
    assert_eq!(
        sorted(found[1]["objects"].as_array().unwrap().clone()),
        sorted(vec![l, p])
    );
    assert!(violations(&mut engine, "net_names_across_sheets").is_empty());
}

#[test]
fn different_names_on_different_sheets_are_a_warning() {
    let dir = TempDir::new("names-across");
    let (mut engine, second) = demo(&dir);
    place(&mut engine, SHEET, 0, 0); // R1
    place(&mut engine, &second, 0, 0); // R2
    port(&mut engine, SHEET, "BUS", 0, 150);
    port(&mut engine, &second, "bus", 0, 150);
    label(&mut engine, &second, "LOCAL", 0, 150);

    // On the second sheet LOCAL and bus meet: an error there, and a warning across.
    let on_sheet = violations(&mut engine, "net_names_on_sheet");
    assert_eq!(on_sheet.len(), 1, "{on_sheet:?}");
    let across = violations(&mut engine, "net_names_across_sheets");
    assert_eq!(across.len(), 1, "{across:?}");
    assert_eq!(across[0]["severity"], "warning");
    assert_eq!(
        across[0]["args"],
        json!({"net": "LOCAL", "names": "LOCAL, BUS", "kind": "net_label", "reason": "priority"})
    );
    assert_eq!(across[0]["objects"].as_array().unwrap().len(), 3);
}

#[test]
fn reserved_characters_are_refused_in_net_names() {
    let dir = TempDir::new("names-reserved");
    let (mut engine, _) = demo(&dir);
    for id in [
        "schematic.net_label.add",
        "schematic.port.add",
        "schematic.power_port.add",
    ] {
        for bad in ["D[0]", "D{0}", "A,B", "D0..7", "A\u{7}B"] {
            let err = engine.run_err(id, json!({"document": SHEET, "name": bad, "at": at(0, 0)}));
            assert_eq!(err.code, ErrorCode::InvalidParams, "{id} {bad:?}");
            assert_eq!(err.details.unwrap()["pointer"], "/name", "{id} {bad:?}");
        }
    }
    // Overbar markup is part of a name.
    label(&mut engine, SHEET, "R\\E\\S\\E\\T\\", 0, 0);
    label(&mut engine, SHEET, "\\RESET", 100, 0);
}

#[test]
fn a_sheet_file_with_a_reserved_name_is_refused_on_load() {
    let dir = TempDir::new("names-load");
    let (mut engine, _) = demo(&dir);
    label(&mut engine, SHEET, "DATA0", 0, 0);
    engine.run_ok("project.save", json!({}));
    let path = dir.path().join("Demo/Demo.redasch");
    let text = std::fs::read_to_string(&path).unwrap();
    std::fs::write(&path, text.replace("\"DATA0\"", "\"DATA[0]\"")).unwrap();
    let mut other = test_engine_with_library(&library());
    let err = other.run_err("project.open", json!({"path": dir.path().join("Demo")}));
    assert!(err.message.contains("DATA[0]"), "{}", err.message);
}
