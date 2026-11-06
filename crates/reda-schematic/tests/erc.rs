//! ERC: `erc.run` over the schematic, its severities and Waivers.

use std::path::{Path, PathBuf};

use reda_command::ErrorCode;
use reda_testkit::{TempDir, TestEngine, test_engine_with_library};
use serde_json::{Value, json};

const SHEET: &str = "00000000-0000-4000-8000-000000000002";
const R_10K: &str = "a0000000-0000-4000-8000-000000000100";
const LM358: &str = "a0000000-0000-4000-8000-000000000120";
const LOGIC: &str = "a0000000-0000-4000-8000-000000000210";
/// Pins of the R Symbol.
const R_PIN_1: &str = "a0000000-0000-4000-8000-000000000012";
const R_PIN_2: &str = "a0000000-0000-4000-8000-000000000013";

fn library() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/erc_library/Erc")
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

/// Places Part `part` with its origin at (`x`, `y`) mil on `sheet`; returns the Unit's UUID.
fn place_on(engine: &mut TestEngine, sheet: &str, part: &str, x: i64, y: i64) -> String {
    let out = engine.run_ok(
        "schematic.component.place",
        json!({"document": sheet, "part": part, "at": at(x, y)}),
    );
    out["result"]["unit"].as_str().unwrap().to_owned()
}

fn place(engine: &mut TestEngine, part: &str, x: i64, y: i64) -> String {
    place_on(engine, SHEET, part, x, y)
}

fn wire_on(engine: &mut TestEngine, sheet: &str, points: &[(i64, i64)]) -> Vec<String> {
    let points: Vec<Value> = points.iter().map(|&(x, y)| at(x, y)).collect();
    let out = engine.run_ok(
        "schematic.wire.add",
        json!({"document": sheet, "points": points}),
    );
    strings(&out["result"]["wires"])
}

fn wire(engine: &mut TestEngine, points: &[(i64, i64)]) -> Vec<String> {
    wire_on(engine, SHEET, points)
}

fn strings(v: &Value) -> Vec<String> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap().to_owned())
        .collect()
}

fn erc(engine: &mut TestEngine) -> Value {
    engine.run_ok("erc.run", json!({}))
}

/// The Violations of check `check`.
fn of(result: &Value, check: &str) -> Vec<Value> {
    result["violations"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| v["check"] == check)
        .cloned()
        .collect()
}

#[test]
fn an_unwired_passive_pin_is_an_unconnected_pin_warning() {
    let dir = TempDir::new("erc-unconnected");
    let mut engine = demo(&dir);
    let r = place(&mut engine, R_10K, 0, 0);
    let out = erc(&mut engine);
    let found = of(&out, "unconnected_pin");
    assert_eq!(found.len(), 2, "{out}");
    assert_eq!(found[0]["severity"], "warning");
    assert_eq!(found[0]["objects"], json!([r, R_PIN_1]));
    assert_eq!(found[0]["args"]["pin"], "R1-1");
    assert_eq!(found[1]["objects"], json!([r, R_PIN_2]));
}

/// Logic Pins, by number: 1 IN (input), 2 OUT (output), 3 TRI (tri-state), 4 OC (open
/// collector), 5 PWR (power out), 6 VCC (power in), 7 NC (not connected), 8 IO
/// (bidirectional), 100 mil apart straight down from the origin.
fn logic_pin(number: u32) -> String {
    format!("a0000000-0000-4000-8000-{:012x}", 0x201 + number)
}

fn label(engine: &mut TestEngine, name: &str, x: i64, y: i64) -> String {
    let out = engine.run_ok(
        "schematic.net_label.add",
        json!({"document": SHEET, "name": name, "at": at(x, y)}),
    );
    out["result"]["net_label"].as_str().unwrap().to_owned()
}

fn port_on(engine: &mut TestEngine, sheet: &str, name: &str, x: i64, y: i64) -> String {
    let out = engine.run_ok(
        "schematic.port.add",
        json!({"document": sheet, "name": name, "at": at(x, y)}),
    );
    out["result"]["port"].as_str().unwrap().to_owned()
}

fn checks(result: &Value) -> Vec<String> {
    result["violations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["check"].as_str().unwrap().to_owned())
        .collect()
}

fn sorted(mut v: Vec<String>) -> Vec<String> {
    v.sort();
    v
}

/// The contents of the saved file whose name ends in `suffix`.
fn saved(dir: &TempDir, suffix: &str) -> (String, String) {
    let (rel, bytes) = reda_testkit::read_tree(dir.path())
        .into_iter()
        .find(|(path, _)| path.ends_with(suffix))
        .unwrap();
    (rel, String::from_utf8(bytes).unwrap())
}

fn reopen(dir: &TempDir) -> TestEngine {
    let mut reopened = test_engine_with_library(&library());
    reopened.run_ok("project.open", json!({"path": dir.path().join("Demo")}));
    reopened
}

/// Saves, rewrites the sheet file with `edit` and opens the Project in a new engine, as
/// a hand-edited or merged file would arrive.
fn hand_edit(
    engine: &mut TestEngine,
    dir: &TempDir,
    edit: impl Fn(String) -> String,
) -> TestEngine {
    engine.run_ok("project.save", json!({}));
    let (rel, text) = saved(dir, ".redasch");
    std::fs::write(dir.path().join(rel), edit(text)).unwrap();
    reopen(dir)
}

#[test]
fn unconnected_input_and_power_in_pins_are_errors_and_not_connected_pins_are_exempt() {
    let dir = TempDir::new("erc-unconnected-input");
    let mut engine = demo(&dir);
    let u = place(&mut engine, LOGIC, 0, 0);
    let out = erc(&mut engine);
    let inputs = of(&out, "unconnected_input");
    assert_eq!(inputs.len(), 2, "{out}");
    assert_eq!(inputs[0]["severity"], "error");
    assert_eq!(inputs[0]["objects"], json!([u, logic_pin(1)]));
    assert_eq!(inputs[1]["objects"], json!([u, logic_pin(6)]));
    let others: Vec<Value> = of(&out, "unconnected_pin")
        .iter()
        .map(|v| v["args"]["pin"].clone())
        .collect();
    assert_eq!(
        json!(others),
        json!(["U1-2", "U1-3", "U1-4", "U1-5", "U1-8"])
    );
}

#[test]
fn a_no_connect_marker_exempts_its_pin_until_the_pin_is_wired() {
    let dir = TempDir::new("erc-no-connect");
    let mut engine = demo(&dir);
    let r = place(&mut engine, R_10K, 0, 0);
    let marker = engine.run_ok(
        "schematic.no_connect.add",
        json!({"document": SHEET, "at": at(0, 150)}),
    )["result"]["no_connect"]
        .clone();
    let out = erc(&mut engine);
    assert_eq!(checks(&out), ["unconnected_pin"], "{out}");
    assert_eq!(of(&out, "unconnected_pin")[0]["args"]["pin"], "R1-2");

    wire(&mut engine, &[(0, 150), (0, 300), (200, 300)]);
    let out = erc(&mut engine);
    let found = of(&out, "no_connect_connected");
    assert_eq!(found.len(), 1, "{out}");
    assert_eq!(found[0]["severity"], "warning");
    assert_eq!(found[0]["objects"], json!([marker, r, R_PIN_1]));
}

#[test]
fn outputs_on_one_net_are_a_pin_conflict_error() {
    let dir = TempDir::new("erc-conflict");
    let mut engine = demo(&dir);
    let u1 = place(&mut engine, LOGIC, 0, 0);
    let u2 = place(&mut engine, LOGIC, 1000, 0);
    wire(&mut engine, &[(0, -100), (1000, -100)]);
    let out = erc(&mut engine);
    let found = of(&out, "pin_conflict");
    assert_eq!(found.len(), 1, "{out}");
    assert_eq!(found[0]["severity"], "error");
    assert_eq!(
        found[0]["objects"],
        json!([u1, logic_pin(2), u2, logic_pin(2)])
    );
    assert_eq!(found[0]["args"]["type_a"], "output");
    assert_eq!(found[0]["args"]["pin_b"], "U2-2");
    assert!(of(&out, "pin_conflict_mixed").is_empty());
}

#[test]
fn tri_state_with_output_is_a_mixed_pin_conflict_warning() {
    let dir = TempDir::new("erc-mixed");
    let mut engine = demo(&dir);
    let u1 = place(&mut engine, LOGIC, 0, 0);
    let u2 = place(&mut engine, LOGIC, 1000, -100);
    // U1 TRI at (0, -200) to U2 OUT at (1000, -200).
    wire(&mut engine, &[(0, -200), (1000, -200)]);
    let out = erc(&mut engine);
    let found = of(&out, "pin_conflict_mixed");
    assert_eq!(found.len(), 1, "{out}");
    assert_eq!(found[0]["severity"], "warning");
    assert_eq!(
        found[0]["objects"],
        json!([u1, logic_pin(3), u2, logic_pin(2)])
    );
    assert_eq!(found[0]["args"]["type_a"], "tri_state");
    assert_eq!(found[0]["args"]["type_b"], "output");
    assert!(of(&out, "pin_conflict").is_empty());
}

#[test]
fn power_in_pins_with_no_driver_are_undriven_until_a_power_port_feeds_them() {
    let dir = TempDir::new("erc-undriven");
    let mut engine = demo(&dir);
    let u = place(&mut engine, LOGIC, 0, 0);
    wire(&mut engine, &[(0, -500), (-200, -500)]);
    label(&mut engine, "VCC", -200, -500);
    let out = erc(&mut engine);
    let found = of(&out, "undriven_power");
    assert_eq!(found.len(), 1, "{out}");
    assert_eq!(found[0]["args"]["net"], "VCC");
    assert_eq!(found[0]["objects"], json!([u, logic_pin(6)]));

    engine.run_ok(
        "schematic.power_port.add",
        json!({"document": SHEET, "name": "VCC", "at": at(-200, -500)}),
    );
    assert!(of(&erc(&mut engine), "undriven_power").is_empty());
}

#[test]
fn a_power_out_pin_drives_power_in_pins() {
    let dir = TempDir::new("erc-driven");
    let mut engine = demo(&dir);
    place(&mut engine, LOGIC, 0, 0);
    // U1 PWR at (0, -400) round to U1 VCC at (0, -500).
    wire(
        &mut engine,
        &[(0, -400), (-100, -400), (-100, -500), (0, -500)],
    );
    let out = erc(&mut engine);
    assert!(of(&out, "undriven_power").is_empty(), "{out}");
}

#[test]
fn a_wired_net_with_one_pin_is_a_single_pin_net() {
    let dir = TempDir::new("erc-single-pin");
    let mut engine = demo(&dir);
    let r = place(&mut engine, R_10K, 0, 0);
    wire(&mut engine, &[(0, 150), (0, 300)]);
    label(&mut engine, "A", 0, 300);
    let out = erc(&mut engine);
    let found = of(&out, "single_pin_net");
    assert_eq!(found.len(), 1, "{out}");
    assert_eq!(found[0]["args"]["net"], "A");
    assert_eq!(found[0]["objects"], json!([r, R_PIN_1]));
    // The wired Pin is not also unconnected.
    let unconnected = of(&out, "unconnected_pin");
    assert_eq!(unconnected.len(), 1);
    assert_eq!(unconnected[0]["args"]["pin"], "R1-2");
    assert!(of(&out, "dangling_wire").is_empty());
}

#[test]
fn a_wire_end_touching_nothing_is_dangling() {
    let dir = TempDir::new("erc-dangling");
    let mut engine = demo(&dir);
    place(&mut engine, R_10K, 0, 0);
    let wires = wire(&mut engine, &[(0, 150), (0, 300), (200, 300)]);
    let out = erc(&mut engine);
    let found = of(&out, "dangling_wire");
    assert_eq!(found.len(), 1, "{out}");
    assert_eq!(found[0]["objects"], json!([wires[1]]));
    assert_eq!(
        found[0]["args"],
        json!({"x": 200 * 25_400, "y": 300 * 25_400})
    );
}

#[test]
fn a_net_label_or_port_on_nothing_is_unattached() {
    let dir = TempDir::new("erc-unattached");
    let mut engine = demo(&dir);
    let l = label(&mut engine, "A", 500, 500);
    let p = port_on(&mut engine, SHEET, "B", 800, 500);
    let out = erc(&mut engine);
    let found = of(&out, "unattached_label");
    assert_eq!(found.len(), 2, "{out}");
    assert_eq!(found[0]["objects"], json!([l]));
    assert_eq!(found[0]["args"]["name"], "A");
    assert_eq!(found[1]["objects"], json!([p]));
}

#[test]
fn a_net_with_two_label_names_has_several_names_on_its_sheet() {
    let dir = TempDir::new("erc-names");
    let mut engine = demo(&dir);
    wire(&mut engine, &[(0, 0), (100, 0)]);
    let b = label(&mut engine, "B", 0, 0);
    let a = label(&mut engine, "A", 100, 0);
    let out = erc(&mut engine);
    let found = of(&out, "net_names_on_sheet");
    assert_eq!(found.len(), 1, "{out}");
    assert_eq!(found[0]["args"]["names"], "A, B");
    assert_eq!(found[0]["args"]["net"], "A");
    assert_eq!(sorted(strings(&found[0]["objects"])), sorted(vec![a, b]));
    assert!(of(&out, "dangling_wire").is_empty());
}

#[test]
fn a_port_whose_name_no_other_sheet_has_is_on_only_one_sheet() {
    let dir = TempDir::new("erc-port");
    let mut engine = demo(&dir);
    wire(&mut engine, &[(0, 0), (100, 0)]);
    let p = port_on(&mut engine, SHEET, "X", 0, 0);
    let out = erc(&mut engine);
    let found = of(&out, "single_sheet_port");
    assert_eq!(found.len(), 1, "{out}");
    assert_eq!(found[0]["args"]["name"], "X");
    assert_eq!(found[0]["objects"], json!([p]));

    let added = engine.run_ok("document.add", json!({"kind": "schematic"}));
    let other = added["result"]["uuid"].as_str().unwrap().to_owned();
    wire_on(&mut engine, &other, &[(0, 0), (100, 0)]);
    port_on(&mut engine, &other, "X", 0, 0);
    let out = erc(&mut engine);
    assert!(of(&out, "single_sheet_port").is_empty(), "{out}");
}

#[test]
fn components_sharing_a_designator_are_duplicates() {
    let dir = TempDir::new("erc-duplicate");
    let mut engine = demo(&dir);
    let r1 = place(&mut engine, R_10K, 0, 0);
    let r2 = place(&mut engine, R_10K, 500, 0);
    assert!(of(&erc(&mut engine), "duplicate_designator").is_empty());
    let mut reopened = hand_edit(&mut engine, &dir, |text| {
        text.replace("\"designator\":\"R2\"", "\"designator\":\"R1\"")
    });
    let out = erc(&mut reopened);
    let found = of(&out, "duplicate_designator");
    assert_eq!(found.len(), 1, "{out}");
    assert_eq!(found[0]["severity"], "error");
    assert_eq!(found[0]["args"]["designator"], "R1");
    assert_eq!(sorted(strings(&found[0]["objects"])), sorted(vec![r1, r2]));
}

#[test]
fn a_placed_multi_unit_component_with_unplaced_units_is_reported() {
    let dir = TempDir::new("erc-unplaced");
    let mut engine = demo(&dir);
    let a = place(&mut engine, LM358, 0, 0);
    let out = erc(&mut engine);
    let found = of(&out, "unplaced_units");
    assert_eq!(found.len(), 1, "{out}");
    assert_eq!(
        found[0]["args"],
        json!({"designator": "U1", "units": "B, Power"})
    );
    assert_eq!(found[0]["objects"], json!([a]));
}

#[test]
fn units_of_one_component_with_differing_data_are_a_mismatch() {
    let dir = TempDir::new("erc-mismatch");
    let mut engine = demo(&dir);
    let a = place(&mut engine, LM358, 0, 0);
    let unit = engine.run_ok("object.get", json!({"uuid": a}));
    let component = unit["object"]["component"]["uuid"].clone();
    let placed = engine.run_ok(
        "schematic.unit.place",
        json!({"document": SHEET, "component": component, "at": at(1000, 0)}),
    );
    let b = placed["result"]["unit"].as_str().unwrap().to_owned();
    assert!(of(&erc(&mut engine), "unit_data_mismatch").is_empty());
    let mut reopened = hand_edit(&mut engine, &dir, |text| {
        // The last value is a Unit's: the part table comes first.
        let at = text.rfind("\"value\":\"LM358\"").unwrap();
        format!("{}\"value\":\"LM324\"{}", &text[..at], &text[at + 15..])
    });
    let out = erc(&mut reopened);
    let found = of(&out, "unit_data_mismatch");
    assert_eq!(found.len(), 1, "{out}");
    assert_eq!(found[0]["severity"], "error");
    assert_eq!(found[0]["args"]["differs"], "value");
    assert_eq!(sorted(strings(&found[0]["objects"])), sorted(vec![a, b]));
}

#[test]
fn severities_are_set_per_check_stored_in_the_project_and_undone_on_its_stack() {
    let dir = TempDir::new("erc-severity");
    let mut engine = demo(&dir);
    place(&mut engine, R_10K, 0, 0);
    engine.run_ok(
        "erc.set_severity",
        json!({"check": "unconnected_pin", "severity": "error"}),
    );
    let out = erc(&mut engine);
    assert_eq!(of(&out, "unconnected_pin")[0]["severity"], "error");
    engine.run_ok(
        "erc.set_severity",
        json!({"check": "unconnected_pin", "severity": "off"}),
    );
    assert_eq!(erc(&mut engine)["violations"], json!([]));

    engine.run_ok("project.save", json!({}));
    let (_, file) = saved(&dir, ".reda");
    assert!(file.contains("\"unconnected_pin\":\"off\""), "{file}");
    assert_eq!(erc(&mut reopen(&dir))["violations"], json!([]));

    engine.run_ok("edit.undo", json!({"document": "project"}));
    let out = erc(&mut engine);
    assert_eq!(of(&out, "unconnected_pin")[0]["severity"], "error");
    engine.run_ok("edit.undo", json!({"document": "project"}));
    let out = erc(&mut engine);
    assert_eq!(of(&out, "unconnected_pin")[0]["severity"], "warning");
}

#[test]
fn a_project_with_default_severities_writes_no_erc_line() {
    let dir = TempDir::new("erc-default");
    let mut engine = demo(&dir);
    engine.run_ok("project.save", json!({}));
    let (_, file) = saved(&dir, ".reda");
    assert!(!file.contains("\"erc\""), "{file}");
}

#[test]
fn a_waiver_hides_its_violation_until_the_objects_no_longer_break_the_check() {
    let dir = TempDir::new("erc-waiver");
    let mut engine = demo(&dir);
    let r = place(&mut engine, R_10K, 0, 0);
    let waived = engine.run_ok(
        "erc.waive",
        json!({"check": "unconnected_pin", "objects": [R_PIN_1, r], "comment": "test point"}),
    );
    let waiver = waived["result"]["waiver"].as_str().unwrap().to_owned();
    assert_eq!(waived["result"]["document"], SHEET);
    let stored = engine.run_ok("object.get", json!({"uuid": waiver}))["object"].clone();
    assert_eq!(stored["type"], "waiver");
    assert_eq!(stored["check"], "unconnected_pin");
    assert_eq!(
        stored["objects"],
        json!(sorted(vec![r.clone(), R_PIN_1.into()]))
    );
    assert_eq!(stored["comment"], "test point");

    let out = erc(&mut engine);
    assert_eq!(out["waived"], 1);
    let shown = of(&out, "unconnected_pin");
    assert_eq!(shown.len(), 1);
    assert_eq!(shown[0]["args"]["pin"], "R1-2");

    let all = engine.run_ok("erc.run", json!({"include_waived": true}));
    let both = of(&all, "unconnected_pin");
    assert_eq!(both.len(), 2);
    assert_eq!(both[0]["waiver"], waiver.as_str());
    assert!(both[1].get("waiver").is_none());

    // Waiving it twice, or something that is not a current Violation, is refused.
    let again = engine.run_err(
        "erc.waive",
        json!({"check": "unconnected_pin", "objects": [r, R_PIN_1]}),
    );
    assert_eq!(again.code, ErrorCode::Rejected);
    let none = engine.run_err(
        "erc.waive",
        json!({"check": "pin_conflict", "objects": [r, R_PIN_1]}),
    );
    assert_eq!(none.code, ErrorCode::Rejected);

    // Wiring the Pin ends that Violation, so the Waiver matches nothing.
    wire(&mut engine, &[(0, 150), (0, 300)]);
    label(&mut engine, "A", 0, 300);
    let out = erc(&mut engine);
    assert_eq!(out["waived"], 0);
    assert_eq!(out["unmatched_waivers"], json!([waiver]));
}

#[test]
fn deleting_a_waiver_reports_the_violation_again_and_waivers_are_saved() {
    let dir = TempDir::new("erc-unwaive");
    let mut engine = demo(&dir);
    let r = place(&mut engine, R_10K, 0, 0);
    let waived = engine.run_ok(
        "erc.waive",
        json!({"check": "unconnected_pin", "objects": [r, R_PIN_2]}),
    );
    let waiver = waived["result"]["waiver"].clone();
    engine.run_ok("project.save", json!({}));
    let (_, file) = saved(&dir, ".redasch");
    assert!(file.contains("\"waivers\""), "{file}");
    assert_eq!(erc(&mut reopen(&dir))["waived"], 1);

    let err = engine.run_err("erc.unwaive", json!({"waivers": [r]}));
    assert_eq!(err.code, ErrorCode::InvalidParams);
    engine.run_ok("erc.unwaive", json!({"waivers": [waiver]}));
    let out = erc(&mut engine);
    assert_eq!(out["waived"], 0);
    assert_eq!(of(&out, "unconnected_pin").len(), 2);
    engine.run_ok("edit.undo", json!({"document": SHEET}));
    assert_eq!(erc(&mut engine)["waived"], 1);
}

#[test]
fn results_are_stale_once_the_design_is_edited_after_the_run() {
    let dir = TempDir::new("erc-stale");
    let mut engine = demo(&dir);
    place(&mut engine, R_10K, 0, 0);
    let design = erc(&mut engine)["design"].clone();
    let stale = |engine: &mut TestEngine| {
        engine.run_ok("erc.stale", json!({"design": design}))["stale"].clone()
    };
    assert_eq!(stale(&mut engine), false);
    wire(&mut engine, &[(0, 150), (0, 300)]);
    assert_eq!(stale(&mut engine), true);
    engine.run_ok("edit.undo", json!({"document": SHEET}));
    assert_eq!(stale(&mut engine), false);
    engine.run_ok(
        "erc.set_severity",
        json!({"check": "dangling_wire", "severity": "off"}),
    );
    assert_eq!(stale(&mut engine), true);
}
