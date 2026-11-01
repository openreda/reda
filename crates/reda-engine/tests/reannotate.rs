//! Re-annotate: renumbering Designators by a processing order, honouring Designator locks
//! and a scope, with a preview of old → new before it applies.

use std::path::{Path, PathBuf};

use reda_command::ErrorCode;
use reda_testkit::{TempDir, TestEngine, test_engine_with_library};
use serde_json::{Value, json};

const SHEET: &str = "00000000-0000-4000-8000-000000000002";
const R_10K: &str = "a0000000-0000-4000-8000-000000000100";
const C_100N: &str = "a0000000-0000-4000-8000-000000000110";
const LM358: &str = "a0000000-0000-4000-8000-000000000120";

fn library() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../reda-schematic/tests/library/Test")
}

fn demo(dir: &TempDir) -> TestEngine {
    let mut engine = test_engine_with_library(&library());
    engine.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
    engine
}

/// Places a Component on `sheet` at (`x`, `y`) mm and returns its Component UUID.
fn place_on(engine: &mut TestEngine, sheet: &str, part: &str, x: i64, y: i64) -> Value {
    engine.run_ok(
        "schematic.component.place",
        json!({"document": sheet, "part": part,
               "at": {"x": format!("{x}mm"), "y": format!("{y}mm")}}),
    )["result"]["component"]["uuid"]
        .clone()
}

fn place(engine: &mut TestEngine, part: &str, x: i64, y: i64) -> Value {
    place_on(engine, SHEET, part, x, y)
}

/// The Designator of every Component, in the order given.
fn designators(engine: &mut TestEngine, components: &[&Value]) -> Vec<String> {
    let units = engine.run_ok("objects.query", json!({"type": "unit"}))["objects"].clone();
    components
        .iter()
        .map(|c| {
            units
                .as_array()
                .unwrap()
                .iter()
                .map(|v| &v["object"])
                .find(|u| u["component"]["uuid"] == **c)
                .map(|u| u["designator"].as_str().unwrap().to_owned())
                .expect("a placed Component")
        })
        .collect()
}

/// Four resistors in a 2 × 2 grid, placed so their Designators run against every order:
/// (top left, top right, bottom left, bottom right). y grows upwards.
fn grid(engine: &mut TestEngine) -> [Value; 4] {
    let bottom_right = place(engine, R_10K, 40, 0); // R1
    let top_left = place(engine, R_10K, 0, 20); // R2
    let bottom_left = place(engine, R_10K, 0, 0); // R3
    let top_right = place(engine, R_10K, 40, 20); // R4
    [top_left, top_right, bottom_left, bottom_right]
}

#[test]
fn across_then_down_numbers_rows_from_the_top_left_to_right() {
    let dir = TempDir::new("reannotate-across-down");
    let mut engine = demo(&dir);
    let [tl, tr, bl, br] = grid(&mut engine);

    engine.run_ok(
        "schematic.reannotate",
        json!({"order": "across_then_down", "scope": "project"}),
    );

    assert_eq!(
        designators(&mut engine, &[&tl, &tr, &bl, &br]),
        ["R1", "R2", "R3", "R4"]
    );
}

#[test]
fn every_order_walks_the_sheet_its_own_way() {
    for (order, expected) in [
        ("across_then_up", ["R3", "R4", "R1", "R2"]),
        ("down_then_across", ["R1", "R3", "R2", "R4"]),
        ("up_then_across", ["R2", "R4", "R1", "R3"]),
    ] {
        let dir = TempDir::new("reannotate-orders");
        let mut engine = demo(&dir);
        let [tl, tr, bl, br] = grid(&mut engine);

        engine.run_ok(
            "schematic.reannotate",
            json!({"order": order, "scope": "project"}),
        );

        assert_eq!(
            designators(&mut engine, &[&tl, &tr, &bl, &br]),
            expected,
            "{order}"
        );
    }
}

#[test]
fn units_slightly_off_level_still_count_as_one_row() {
    let dir = TempDir::new("reannotate-band");
    let mut engine = demo(&dir);
    let right = place(&mut engine, R_10K, 40, 3); // R1, a little higher
    let left = place(&mut engine, R_10K, 0, 0); // R2

    engine.run_ok(
        "schematic.reannotate",
        json!({"order": "across_then_down", "scope": "project"}),
    );

    assert_eq!(designators(&mut engine, &[&left, &right]), ["R1", "R2"]);
}

fn add_sheet(engine: &mut TestEngine) -> String {
    engine.run_ok("document.add", json!({"kind": "schematic"}))["result"]["uuid"]
        .as_str()
        .unwrap()
        .to_owned()
}

#[test]
fn sheets_are_numbered_in_document_order_and_prefixes_count_separately() {
    let dir = TempDir::new("reannotate-sheets");
    let mut engine = demo(&dir);
    let second = add_sheet(&mut engine);
    let on_second = place_on(&mut engine, &second, R_10K, 0, 100); // R1
    let r_first = place(&mut engine, R_10K, 50, 0); // R2
    let c_first = place(&mut engine, C_100N, 0, 0); // C1

    engine.run_ok(
        "schematic.reannotate",
        json!({"order": "across_then_down", "scope": "project"}),
    );

    assert_eq!(
        designators(&mut engine, &[&c_first, &r_first, &on_second]),
        ["C1", "R1", "R2"]
    );
}

#[test]
fn a_locked_designator_is_kept_and_its_number_is_not_handed_out() {
    let dir = TempDir::new("reannotate-lock");
    let mut engine = demo(&dir);
    let [tl, tr, bl, br] = grid(&mut engine);
    // R4 (top right) is locked; across then down would give it R2.
    engine.run_ok(
        "schematic.component.set",
        json!({"components": [tr], "designator_locked": true}),
    );

    let preview = engine.run_ok(
        "schematic.reannotate.preview",
        json!({"order": "across_then_down", "scope": "project"}),
    );
    assert!(
        preview["designators"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["component"] != tr),
        "a locked Component is not renumbered: {preview}"
    );

    engine.run_ok(
        "schematic.reannotate",
        json!({"order": "across_then_down", "scope": "project"}),
    );
    assert_eq!(
        designators(&mut engine, &[&tl, &tr, &bl, &br]),
        ["R1", "R4", "R2", "R3"]
    );

    // Unlocked again, it takes part.
    engine.run_ok(
        "schematic.component.set",
        json!({"components": [tr], "designator_locked": false}),
    );
    engine.run_ok(
        "schematic.reannotate",
        json!({"order": "across_then_down", "scope": "project"}),
    );
    assert_eq!(
        designators(&mut engine, &[&tl, &tr, &bl, &br]),
        ["R1", "R2", "R3", "R4"]
    );
}

#[test]
fn the_lock_is_saved_with_the_sheet() {
    let dir = TempDir::new("reannotate-lock-file");
    let mut engine = demo(&dir);
    let r1 = place(&mut engine, R_10K, 0, 0);
    let r2 = place(&mut engine, R_10K, 50, 0);
    engine.run_ok(
        "schematic.component.set",
        json!({"components": [r2], "designator": "R7"}),
    );
    engine.run_ok(
        "schematic.component.set",
        json!({"components": [r2], "designator_locked": true}),
    );
    engine.run_ok("project.save", json!({}));

    let tree = reda_testkit::read_tree(dir.path());
    let sheet = tree
        .iter()
        .find(|(path, _)| path.ends_with(".redasch"))
        .map(|(_, bytes)| String::from_utf8(bytes.clone()).unwrap())
        .unwrap();
    assert_eq!(sheet.matches(r#""designator_locked":true"#).count(), 1);

    let mut reopened = test_engine_with_library(&library());
    reopened.run_ok("project.open", json!({"path": dir.path().join("Demo")}));
    let preview = reopened.run_ok(
        "schematic.reannotate.preview",
        json!({"order": "across_then_down", "scope": "project"}),
    );
    assert_eq!(
        preview["designators"],
        json!([{"component": r1, "old": "R1", "new": "R1"}])
    );
}

#[test]
fn sheet_scope_renumbers_only_the_components_on_that_sheet() {
    let dir = TempDir::new("reannotate-sheet-scope");
    let mut engine = demo(&dir);
    let second = add_sheet(&mut engine);
    let right = place_on(&mut engine, &second, R_10K, 40, 0); // R1
    let left = place_on(&mut engine, &second, R_10K, 0, 0); // R2
    let other = place(&mut engine, R_10K, 0, 0); // R3, on the first sheet

    engine.run_ok(
        "schematic.reannotate",
        json!({"order": "across_then_down", "scope": "sheet", "document": second}),
    );

    assert_eq!(
        designators(&mut engine, &[&left, &right, &other]),
        ["R1", "R2", "R3"]
    );

    let missing = engine.run_err(
        "schematic.reannotate",
        json!({"order": "across_then_down", "scope": "sheet"}),
    );
    assert_eq!(missing.code, ErrorCode::InvalidParams);
}

fn first_unit_of(engine: &mut TestEngine, component: &Value) -> Value {
    engine.run_ok("objects.query", json!({"type": "unit"}))["objects"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["object"].clone())
        .find(|u| u["component"]["uuid"] == *component)
        .unwrap()["uuid"]
        .clone()
}

#[test]
fn selection_scope_renumbers_the_selected_components_around_the_rest() {
    let dir = TempDir::new("reannotate-selection");
    let mut engine = demo(&dir);
    let [tl, tr, bl, br] = grid(&mut engine);
    // Selected: top left (R2) and bottom right (R1), the latter by its Unit. R3 and R4
    // are kept, so the selection shares out R1 and R2 in order.
    let unit_of_br = first_unit_of(&mut engine, &br);

    engine.run_ok(
        "schematic.reannotate",
        json!({"order": "across_then_down", "scope": "selection",
               "selection": [tl, unit_of_br]}),
    );

    assert_eq!(
        designators(&mut engine, &[&tl, &tr, &bl, &br]),
        ["R1", "R4", "R3", "R2"]
    );

    let unknown = engine.run_err(
        "schematic.reannotate",
        json!({"order": "across_then_down", "scope": "selection",
               "selection": ["00000000-0000-4000-8000-0000000000ff"]}),
    );
    assert_eq!(unknown.code, ErrorCode::NotFound);
}

#[test]
fn the_units_of_a_multi_unit_component_stay_together_across_sheets() {
    let dir = TempDir::new("reannotate-units");
    let mut engine = demo(&dir);
    let first_r = place(&mut engine, R_10K, 0, 0); // R1
    let op_amp = place(&mut engine, LM358, 50, 0); // U1, Unit A on the first sheet
    let second = add_sheet(&mut engine);
    engine.run_ok(
        "schematic.unit.place",
        json!({"document": second, "component": op_amp, "at": {"x": 0, "y": 0}}),
    );
    let other_u = place_on(&mut engine, &second, LM358, -50, 0); // U2, left of Unit B
    engine.run_ok(
        "schematic.component.set",
        json!({"components": [op_amp], "designator": "U9"}),
    );

    let preview = engine.run_ok(
        "schematic.reannotate.preview",
        json!({"order": "across_then_down", "scope": "project"}),
    );
    assert_eq!(
        preview["designators"],
        json!([
            {"component": first_r, "old": "R1", "new": "R1"},
            {"component": op_amp, "old": "U9", "new": "U1"},
            {"component": other_u, "old": "U2", "new": "U2"},
        ])
    );
    // The preview changes nothing.
    assert_eq!(designators(&mut engine, &[&op_amp]), ["U9"]);

    engine.run_ok(
        "schematic.reannotate",
        json!({"order": "across_then_down", "scope": "project"}),
    );
    let units = engine.run_ok("objects.query", json!({"type": "unit"}))["objects"].clone();
    let op_amp_units: Vec<&Value> = units
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| v["object"]["component"]["uuid"] == op_amp)
        .collect();
    assert_eq!(op_amp_units.len(), 2);
    assert!(
        op_amp_units
            .iter()
            .all(|v| v["object"]["designator"] == "U1")
    );
}

#[test]
fn applying_records_an_undo_entry_in_each_touched_sheet() {
    let dir = TempDir::new("reannotate-undo");
    let mut engine = demo(&dir);
    let second = add_sheet(&mut engine);
    let on_second = place_on(&mut engine, &second, R_10K, 0, 0); // R1
    let on_first = place(&mut engine, R_10K, 0, 0); // R2

    let applied = engine.run_ok(
        "schematic.reannotate",
        json!({"order": "across_then_down", "scope": "project"}),
    );
    let touched: Vec<&String> = applied["changes"].as_object().unwrap().keys().collect();
    assert_eq!(touched.len(), 2, "{applied}");
    assert_eq!(
        designators(&mut engine, &[&on_first, &on_second]),
        ["R1", "R2"]
    );

    engine.run_ok("edit.undo", json!({"document": SHEET}));
    assert_eq!(
        designators(&mut engine, &[&on_first, &on_second]),
        ["R2", "R2"]
    );
    engine.run_ok("edit.undo", json!({"document": second}));
    assert_eq!(
        designators(&mut engine, &[&on_first, &on_second]),
        ["R2", "R1"]
    );
}
