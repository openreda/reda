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
    let right = place(&mut engine, R_10K, 40, 2); // R1, a little higher
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
fn sheets_scope_renumbers_only_the_components_on_those_sheets() {
    let dir = TempDir::new("reannotate-sheet-scope");
    let mut engine = demo(&dir);
    let second = add_sheet(&mut engine);
    let right = place_on(&mut engine, &second, R_10K, 40, 0); // R1
    let left = place_on(&mut engine, &second, R_10K, 0, 0); // R2
    let other = place(&mut engine, R_10K, 0, 0); // R3, on the first sheet

    engine.run_ok(
        "schematic.reannotate",
        json!({"order": "across_then_down", "scope": "sheets", "sheets": [second]}),
    );

    assert_eq!(
        designators(&mut engine, &[&left, &right, &other]),
        ["R1", "R2", "R3"]
    );

    let missing = engine.run_err(
        "schematic.reannotate",
        json!({"order": "across_then_down", "scope": "sheets"}),
    );
    assert_eq!(missing.code, ErrorCode::InvalidParams);
    let board_order = engine.run_err(
        "schematic.reannotate",
        json!({"order": "descending_y_then_ascending_x", "scope": "project"}),
    );
    assert_eq!(board_order.code, ErrorCode::InvalidParams);
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
    assert!(
        touched.iter().any(|t| *t == SHEET) && touched.iter().any(|t| **t == second),
        "{applied}"
    );
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

/// Places `part` with its Symbol turned by `rotation` degrees.
fn place_turned(engine: &mut TestEngine, part: &str, x: i64, y: i64, rotation: i64) -> Value {
    engine.run_ok(
        "schematic.component.place",
        json!({"document": SHEET, "part": part, "rotation": format!("{rotation}deg"),
               "at": {"x": format!("{x}mm"), "y": format!("{y}mm")}}),
    )["result"]["component"]["uuid"]
        .clone()
}

#[test]
fn a_row_is_measured_from_its_first_part_not_chained_or_banded() {
    let dir = TempDir::new("reannotate-threshold");
    let mut engine = demo(&dir);
    // Each 2 mm below the last: the second joins the first's row (within 100 mil), the
    // third is 4 mm from the first and starts a row of its own.
    let top = place(&mut engine, R_10K, 40, 0); // R1
    let middle = place(&mut engine, R_10K, 0, -2); // R2
    let low = place(&mut engine, R_10K, -40, -4); // R3

    engine.run_ok(
        "schematic.reannotate",
        json!({"order": "across_then_down", "scope": "project"}),
    );
    assert_eq!(
        designators(&mut engine, &[&middle, &top, &low]),
        ["R1", "R2", "R3"]
    );

    // A wider threshold takes all three into one row.
    engine.run_ok(
        "schematic.reannotate",
        json!({"order": "across_then_down", "scope": "project", "threshold": "5mm"}),
    );
    assert_eq!(
        designators(&mut engine, &[&low, &middle, &top]),
        ["R1", "R2", "R3"]
    );
}

#[test]
fn parts_are_sorted_by_their_body_centre_not_their_origin() {
    let dir = TempDir::new("reannotate-body");
    let mut engine = demo(&dir);
    // Unit A of the op amp sits right of its origin (its body centre is 6.35 mm to the
    // right); turned half round, it sits left of it.
    let upright = place(&mut engine, LM358, 0, 20); // U1, centre x 6.35
    let turned = place_turned(&mut engine, LM358, 10, 0, 180); // U2, centre x 3.65

    engine.run_ok(
        "schematic.reannotate",
        json!({"order": "down_then_across", "scope": "project"}),
    );
    assert_eq!(designators(&mut engine, &[&turned, &upright]), ["U1", "U2"]);
}

#[test]
fn location_designator_sorts_by_where_the_designator_text_sits() {
    let dir = TempDir::new("reannotate-location");
    let mut engine = demo(&dir);
    let left = place(&mut engine, R_10K, 0, 0); // R1
    let right = place(&mut engine, R_10K, 40, 0); // R2
    // The right one's Designator is moved far to the left, above the row.
    let unit = first_unit_of(&mut engine, &right);
    engine.run_ok(
        "schematic.field.move",
        json!({"unit": unit, "field": "designator", "at": {"x": "-30mm", "y": "1mm"}}),
    );

    let params = json!({"order": "across_then_down", "scope": "project",
                        "location": "designator"});
    engine.run_ok("schematic.reannotate", params);
    assert_eq!(designators(&mut engine, &[&right, &left]), ["R1", "R2"]);

    // By the part, the left one is first again.
    engine.run_ok(
        "schematic.reannotate",
        json!({"order": "across_then_down", "scope": "project", "location": "part"}),
    );
    assert_eq!(designators(&mut engine, &[&left, &right]), ["R1", "R2"]);
}

#[test]
fn per_sheet_start_indices_number_each_sheet_from_its_own_start() {
    let dir = TempDir::new("reannotate-start");
    let mut engine = demo(&dir);
    let second = add_sheet(&mut engine);
    let a = place(&mut engine, R_10K, 0, 0);
    let b = place(&mut engine, R_10K, 20, 0);
    let c = place_on(&mut engine, &second, R_10K, 0, 0);
    let d = place_on(&mut engine, &second, C_100N, 20, 0);

    engine.run_ok(
        "schematic.reannotate",
        json!({"order": "across_then_down", "scope": "project",
               "per_sheet": [{"sheet": SHEET, "start_index": 100},
                             {"sheet": second, "start_index": 200}]}),
    );
    assert_eq!(
        designators(&mut engine, &[&a, &b, &c, &d]),
        ["R100", "R101", "R200", "C200"]
    );

    // A sheet that runs over into the next one's numbers takes the next free number.
    engine.run_ok(
        "schematic.reannotate",
        json!({"order": "across_then_down", "scope": "project",
               "per_sheet": [{"sheet": second, "start_index": 2}]}),
    );
    assert_eq!(
        designators(&mut engine, &[&a, &b, &c, &d]),
        ["R1", "R2", "R3", "C2"]
    );
}

/// Saves the Project, rewrites Designators in its sheet file and opens it again.
fn reopen_with(dir: &TempDir, engine: &mut TestEngine, rewrites: &[(&str, &str)]) -> TestEngine {
    engine.run_ok("project.save", json!({}));
    let folder = dir.path().join("Demo");
    let sheet = std::fs::read_dir(&folder)
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| p.extension().is_some_and(|e| e == "redasch"))
        .unwrap();
    let mut text = std::fs::read_to_string(&sheet).unwrap();
    for (from, to) in rewrites {
        let from = format!(r#""designator":"{from}""#);
        assert!(text.contains(&from), "{from}");
        text = text.replace(&from, &format!(r#""designator":"{to}""#));
    }
    std::fs::write(&sheet, text).unwrap();
    let mut reopened = test_engine_with_library(&library());
    reopened.run_ok("project.open", json!({"path": folder}));
    reopened
}

#[test]
fn fix_duplicates_only_keeps_the_first_and_renumbers_the_rest() {
    let dir = TempDir::new("reannotate-duplicates");
    let mut engine = demo(&dir);
    // One row, left to right.
    let a = place(&mut engine, R_10K, 0, 0); // R1 -> R2
    let b = place(&mut engine, R_10K, 10, 0); // R2
    let c = place(&mut engine, R_10K, 20, 0); // R3 -> R1
    let d = place(&mut engine, R_10K, 30, 0); // R4 -> R7, locked
    let e = place(&mut engine, R_10K, 40, 0); // R5 -> R7, locked
    let f = place(&mut engine, R_10K, 50, 0); // R6 -> R5
    let g = place(&mut engine, R_10K, 60, 0); // R7 -> R5, locked
    engine.run_ok(
        "schematic.component.set",
        json!({"components": [d, e, g], "designator_locked": true}),
    );
    let mut engine = reopen_with(
        &dir,
        &mut engine,
        &[
            ("R1", "Rx"),
            ("R2", "R2"),
            ("R3", "R1"),
            ("R4", "R7a"),
            ("R5", "R7b"),
            ("R7", "R5"),
            ("R6", "R5"),
            ("Rx", "R2"),
            ("R7a", "R7"),
            ("R7b", "R7"),
        ],
    );
    assert_eq!(
        designators(&mut engine, &[&a, &b, &c, &d, &e, &f, &g]),
        ["R2", "R2", "R1", "R7", "R7", "R5", "R5"]
    );

    let params = json!({"order": "across_then_down", "scope": "project",
                        "mode": "fix_duplicates_only"});
    let preview = engine.run_ok("schematic.reannotate.preview", params.clone());
    // The locked pair stays, and the preview says so.
    let mut pair = [d.as_str().unwrap(), e.as_str().unwrap()];
    pair.sort();
    assert_eq!(
        preview["kept_duplicates"],
        json!([{"designator": "R7", "components": pair}]),
        "{preview}"
    );
    engine.run_ok("schematic.reannotate", params);
    // a is first with R2 and keeps it; b takes the smallest free number; c is unique;
    // f's R5 is held by locked g, so f moves.
    assert_eq!(
        designators(&mut engine, &[&a, &b, &c, &d, &e, &f, &g]),
        ["R2", "R3", "R1", "R7", "R7", "R4", "R5"]
    );
}

#[test]
fn the_last_settings_are_saved_in_the_project_file_and_run_again() {
    let dir = TempDir::new("reannotate-saved");
    let mut engine = demo(&dir);
    let err = engine.run_err("schematic.reannotate", json!({"saved": true}));
    assert_eq!(err.code, ErrorCode::Unavailable);

    let [tl, tr, bl, br] = grid(&mut engine);
    let settings = json!({"order": "down_then_across", "scope": "project",
                          "threshold": "1mm",
                          "per_sheet": [{"sheet": SHEET, "start_index": 10}]});
    engine.run_ok("schematic.reannotate", settings);
    assert_eq!(
        designators(&mut engine, &[&tl, &tr, &bl, &br]),
        ["R10", "R12", "R11", "R13"]
    );
    engine.run_ok("project.save", json!({}));

    let tree = reda_testkit::read_tree(dir.path());
    let file = tree
        .iter()
        .find(|(path, _)| path.ends_with(".reda"))
        .map(|(_, bytes)| String::from_utf8(bytes.clone()).unwrap())
        .unwrap();
    let file: Value = serde_json::from_str(&file).unwrap();
    assert_eq!(
        file["reannotate"],
        json!({"source": "schematic", "mode": "renumber_all", "scope": "project",
               "order": "down_then_across", "location": "part", "threshold": 1_000_000,
               "per_sheet": [{"sheet": SHEET, "start_index": 10}],
               "sides": "top_then_bottom", "start_index": 1}),
        "{file}"
    );

    // Reopened and shuffled, the saved settings number the same way again.
    let mut reopened = test_engine_with_library(&library());
    reopened.run_ok("project.open", json!({"path": dir.path().join("Demo")}));
    reopened.run_ok(
        "schematic.component.set",
        json!({"components": [tl], "designator": "R50"}),
    );
    let preview = reopened.run_ok("schematic.reannotate.preview", json!({"saved": true}));
    assert_eq!(preview["designators"][0]["new"], "R10", "{preview}");
    reopened.run_ok("schematic.reannotate", json!({"saved": true}));
    assert_eq!(
        designators(&mut reopened, &[&tl, &tr, &bl, &br]),
        ["R10", "R12", "R11", "R13"]
    );

    let mixed = reopened.run_err(
        "schematic.reannotate",
        json!({"saved": true, "order": "across_then_up"}),
    );
    assert_eq!(mixed.code, ErrorCode::InvalidParams);
}
