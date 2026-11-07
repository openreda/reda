//! Re-annotate from the board (`source: board`): numbering by where the Footprints sit, in
//! four directions, by side, with a threshold and a start index. The new Designators write
//! through to the board, with no pending Sync change left behind.

use std::path::{Path, PathBuf};

use reda_engine::command::ErrorCode;
use reda_testkit::{TempDir, TestEngine, test_engine_with_library};
use serde_json::{Value, json};

const SHEET: &str = "00000000-0000-4000-8000-000000000002";
const PCB: &str = "00000000-0000-4000-8000-000000000003";
const R_10K: &str = "a0000000-0000-4000-8000-000000000100";
const R_SHUNT: &str = "a0000000-0000-4000-8000-000000000150";
const R_KELVIN: &str = "a0000000-0000-4000-8000-000000000070";

fn library() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../reda-schematic/tests/library/Test")
}

fn demo(dir: &TempDir) -> TestEngine {
    let mut engine = test_engine_with_library(&library());
    engine.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
    engine
}

/// Places a Component on the sheet at (`x`, `y`) mm and returns its Component UUID.
fn place(engine: &mut TestEngine, part: &str, x: i64, y: i64) -> String {
    place_with(engine, json!({"part": part}), x, y)
}

fn place_with(engine: &mut TestEngine, mut params: Value, x: i64, y: i64) -> String {
    params["document"] = json!(SHEET);
    params["at"] = json!({"x": format!("{x}mm"), "y": format!("{y}mm")});
    engine.run_ok("schematic.component.place", params)["result"]["component"]["uuid"]
        .as_str()
        .unwrap()
        .to_owned()
}

fn sync(engine: &mut TestEngine) {
    let list = engine.run_ok("sync.preview", json!({}));
    engine.run_ok("sync.apply", json!({"hash": list["hash"]}));
}

fn pending(engine: &mut TestEngine) -> Value {
    engine.run_ok("sync.preview", json!({}))["count"].clone()
}

fn objects(engine: &mut TestEngine, kind: &str) -> Vec<Value> {
    engine.run_ok("objects.query", json!({"type": kind}))["objects"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["object"].clone())
        .collect()
}

/// The Footprint instance of a Component.
fn instance_of(engine: &mut TestEngine, component: &str) -> Value {
    objects(engine, "footprint_instance")
        .into_iter()
        .find(|i| i["component"]["uuid"] == component)
        .expect("a synced Component")
}

/// Moves a Component's Footprint so its origin is at (`x`, `y`) mm.
fn move_to(engine: &mut TestEngine, component: &str, x: i64, y: i64) {
    let instance = instance_of(engine, component);
    let at = &instance["at"];
    let by = json!({"x": x * 1_000_000 - at["x"].as_i64().unwrap(),
                    "y": y * 1_000_000 - at["y"].as_i64().unwrap()});
    engine.run_ok("pcb.move", json!({"objects": [instance["uuid"]], "by": by}));
}

fn flip(engine: &mut TestEngine, component: &str) {
    let instance = instance_of(engine, component)["uuid"].clone();
    engine.run_ok("pcb.flip", json!({"objects": [instance]}));
}

fn designator_of(engine: &mut TestEngine, component: &str) -> String {
    objects(engine, "unit")
        .into_iter()
        .find(|u| u["component"]["uuid"] == component)
        .unwrap()["designator"]
        .as_str()
        .unwrap()
        .to_owned()
}

fn designators(engine: &mut TestEngine, components: &[&String]) -> Vec<String> {
    components
        .iter()
        .map(|c| designator_of(engine, c))
        .collect()
}

/// The undo scopes a Command touched.
fn touched(done: &Value) -> Vec<String> {
    done["changes"]
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect()
}

#[test]
fn reannotate_from_the_board_numbers_by_footprint_position_and_writes_through() {
    let dir = TempDir::new("reannotate-board");
    let mut engine = demo(&dir);
    // Placed on the schematic against the board order. y grows upwards on the board.
    let bottom_right = place(&mut engine, R_10K, 0, 0); // R1
    let bottom_left = place(&mut engine, R_10K, 10, 0); // R2
    let top_right = place(&mut engine, R_10K, 20, 0); // R3
    let top_left = place(&mut engine, R_10K, 30, 0); // R4
    let underneath = place(&mut engine, R_10K, 40, 0); // R5
    sync(&mut engine);
    let not_on_board = place(&mut engine, R_10K, 50, 0); // R6, not synced yet
    flip(&mut engine, &underneath);
    move_to(&mut engine, &top_left, 0, 10);
    move_to(&mut engine, &top_right, 20, 11);
    move_to(&mut engine, &bottom_left, 0, 0);
    move_to(&mut engine, &bottom_right, 20, 0);
    move_to(&mut engine, &underneath, 40, 20);
    assert_eq!(pending(&mut engine), 1);

    // The default order is rows from the top, each left to right.
    let params = json!({"source": "board", "scope": "project"});
    let preview = engine.run_ok("schematic.reannotate.preview", params.clone());
    let new: Vec<&str> = preview["designators"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["new"].as_str().unwrap())
        .collect();
    assert_eq!(new, ["R1", "R2", "R3", "R4", "R5", "R6"]);
    let done = engine.run_ok("schematic.reannotate", params);

    // The top side first, then the bottom side, then the Components not on the board.
    for (component, designator) in [
        (&top_left, "R1"),
        (&top_right, "R2"),
        (&bottom_left, "R3"),
        (&bottom_right, "R4"),
        (&underneath, "R5"),
    ] {
        assert_eq!(designator_of(&mut engine, component), designator);
        assert_eq!(
            instance_of(&mut engine, component)["designator"],
            designator
        );
    }
    assert_eq!(designator_of(&mut engine, &not_on_board), "R6");
    let scopes = touched(&done);
    assert!(scopes.iter().any(|s| s == SHEET), "{scopes:?}");
    assert!(scopes.iter().any(|s| s == PCB), "{scopes:?}");
    // Only the add that was already pending is left.
    assert_eq!(pending(&mut engine), 1);

    engine.run_ok("edit.undo", json!({"document": PCB}));
    assert_eq!(instance_of(&mut engine, &top_left)["designator"], "R4");
    assert_eq!(designator_of(&mut engine, &top_left), "R1");
}

#[test]
fn reannotate_from_the_board_needs_a_pcb() {
    let dir = TempDir::new("reannotate-board-no-pcb");
    let mut engine = demo(&dir);
    place(&mut engine, R_10K, 0, 0);
    engine.run_ok("document.delete", json!({"document": PCB}));
    let err = engine.run_err(
        "schematic.reannotate",
        json!({"source": "board", "scope": "project"}),
    );
    assert_eq!(err.code, ErrorCode::Unavailable);
}

/// Four Footprints in a 2 × 2 grid on the board, 20 mm by 10 mm apart, synced from a
/// schematic that numbers them against every order: (top left, top right, bottom left,
/// bottom right).
fn board_grid(engine: &mut TestEngine) -> [String; 4] {
    let br = place(engine, R_10K, 0, 0); // R1
    let tl = place(engine, R_10K, 10, 0); // R2
    let bl = place(engine, R_10K, 20, 0); // R3
    let tr = place(engine, R_10K, 30, 0); // R4
    sync(engine);
    move_to(engine, &tl, 0, 10);
    move_to(engine, &tr, 20, 10);
    move_to(engine, &bl, 0, 0);
    move_to(engine, &br, 20, 0);
    [tl, tr, bl, br]
}

#[test]
fn the_four_board_directions() {
    for (order, expected) in [
        ("ascending_x_then_ascending_y", ["R2", "R4", "R1", "R3"]),
        ("ascending_x_then_descending_y", ["R1", "R3", "R2", "R4"]),
        ("ascending_y_then_ascending_x", ["R3", "R4", "R1", "R2"]),
        ("descending_y_then_ascending_x", ["R1", "R2", "R3", "R4"]),
    ] {
        let dir = TempDir::new("reannotate-board-orders");
        let mut engine = demo(&dir);
        let [tl, tr, bl, br] = board_grid(&mut engine);

        engine.run_ok(
            "schematic.reannotate",
            json!({"source": "board", "order": order, "scope": "project"}),
        );
        assert_eq!(
            designators(&mut engine, &[&tl, &tr, &bl, &br]),
            expected,
            "{order}"
        );
    }

    let dir = TempDir::new("reannotate-board-sheet-order");
    let mut engine = demo(&dir);
    let err = engine.run_err(
        "schematic.reannotate",
        json!({"source": "board", "order": "across_then_down", "scope": "project"}),
    );
    assert_eq!(err.code, ErrorCode::InvalidParams);
}

#[test]
fn sides_pick_the_top_or_the_bottom_and_the_bottom_is_seen_from_below() {
    let dir = TempDir::new("reannotate-board-sides");
    let mut engine = demo(&dir);
    let top = place(&mut engine, R_10K, 0, 0); // R1
    let under_left = place(&mut engine, R_10K, 10, 0); // R2
    let under_right = place(&mut engine, R_10K, 20, 0); // R3
    sync(&mut engine);
    flip(&mut engine, &under_left);
    flip(&mut engine, &under_right);
    move_to(&mut engine, &top, 50, 0);
    move_to(&mut engine, &under_left, 0, 0);
    move_to(&mut engine, &under_right, 20, 0);

    // Only the bottom: seen from below, the right one is on the left. The top one keeps
    // R1, so the bottom ones share out the rest.
    engine.run_ok(
        "schematic.reannotate",
        json!({"source": "board", "sides": "bottom", "scope": "project",
               "order": "ascending_x_then_ascending_y"}),
    );
    assert_eq!(
        designators(&mut engine, &[&top, &under_right, &under_left]),
        ["R1", "R2", "R3"]
    );

    // Only the top, from 10.
    engine.run_ok(
        "schematic.reannotate",
        json!({"source": "board", "sides": "top", "scope": "project", "start_index": 10}),
    );
    assert_eq!(
        designators(&mut engine, &[&top, &under_right, &under_left]),
        ["R10", "R2", "R3"]
    );
}

#[test]
fn footprints_sort_by_their_courtyard_or_pad_edge_not_their_origin() {
    let dir = TempDir::new("reannotate-board-edge");
    let mut engine = demo(&dir);
    // A 0603 (Pads ±1.2 mm) and a 2512 Kelvin (Pads ±3.75 mm), with no courtyards.
    let small = place(&mut engine, R_10K, 0, 0); // R1
    let large = place_with(
        &mut engine,
        json!({"part": R_SHUNT, "footprint": R_KELVIN}),
        10,
        0,
    ); // R2
    sync(&mut engine);
    move_to(&mut engine, &small, 0, 10);
    // Its origin is 2 mm right of the small one's, its left edge 0.55 mm left of it.
    move_to(&mut engine, &large, 2, 0);

    engine.run_ok(
        "schematic.reannotate",
        json!({"source": "board", "scope": "project", "threshold": "0mm",
               "order": "ascending_x_then_ascending_y"}),
    );
    assert_eq!(designators(&mut engine, &[&large, &small]), ["R1", "R2"]);
}

#[test]
fn a_threshold_puts_nearly_level_footprints_in_one_row() {
    let dir = TempDir::new("reannotate-board-threshold");
    let mut engine = demo(&dir);
    let right = place(&mut engine, R_10K, 0, 0); // R1
    let left = place(&mut engine, R_10K, 10, 0); // R2
    sync(&mut engine);
    move_to(&mut engine, &right, 20, 2);
    move_to(&mut engine, &left, 0, 0);

    // 2 mm apart: one row with the default 100 mil, so left to right.
    engine.run_ok(
        "schematic.reannotate",
        json!({"source": "board", "scope": "project"}),
    );
    assert_eq!(designators(&mut engine, &[&left, &right]), ["R1", "R2"]);

    // Two rows with a 1 mm threshold, the higher one first.
    engine.run_ok(
        "schematic.reannotate",
        json!({"source": "board", "scope": "project", "threshold": "1mm"}),
    );
    assert_eq!(designators(&mut engine, &[&right, &left]), ["R1", "R2"]);
}

#[test]
fn selected_only_takes_the_next_free_number_past_an_unselected_one() {
    let dir = TempDir::new("reannotate-board-collision");
    let mut engine = demo(&dir);
    let a = place(&mut engine, R_10K, 0, 0); // R1
    let b = place(&mut engine, R_10K, 10, 0); // R2
    let c = place(&mut engine, R_10K, 20, 0); // R3
    sync(&mut engine);
    move_to(&mut engine, &a, 40, 0);
    move_to(&mut engine, &b, 0, 0);
    move_to(&mut engine, &c, 20, 0);

    // b and c are selected on the board, by their Footprints, from 1. R1 is held by the
    // unselected a, so b takes R2 and c takes R3, and a is not renamed.
    let selection = [
        instance_of(&mut engine, &b)["uuid"].clone(),
        instance_of(&mut engine, &c)["uuid"].clone(),
    ];
    let done = engine.run_ok(
        "schematic.reannotate",
        json!({"source": "board", "scope": "selection", "selection": selection,
               "start_index": 1}),
    );
    assert_eq!(designators(&mut engine, &[&a, &b, &c]), ["R1", "R2", "R3"]);
    assert_eq!(done["result"]["designators"].as_array().unwrap().len(), 2);

    // From 3: b takes R3 (c gives it up), c the next free, R4.
    engine.run_ok(
        "schematic.reannotate",
        json!({"source": "board", "scope": "selection", "selection": selection,
               "start_index": 3}),
    );
    assert_eq!(designators(&mut engine, &[&a, &b, &c]), ["R1", "R3", "R4"]);

    // From 1 again, with a moved to R2's place in the order: a still keeps R1.
    engine.run_ok(
        "schematic.component.set",
        json!({"components": [a], "designator": "R2"}),
    );
    engine.run_ok(
        "schematic.reannotate",
        json!({"source": "board", "scope": "selection", "selection": selection,
               "start_index": 1}),
    );
    assert_eq!(designators(&mut engine, &[&a, &b, &c]), ["R2", "R1", "R3"]);
}

const R_0805: &str = "a0000000-0000-4000-8000-000000000050";

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

#[test]
fn a_courtyard_beats_the_pads_box() {
    let dir = TempDir::new("reannotate-board-courtyard");
    // The test Library, with a Courtyard on the 0805 that reaches 8 mm left of its
    // origin, while its Pads reach only 1.5 mm.
    let copy = dir.path().join("Test");
    copy_dir(&library(), &copy);
    let file = copy.join("footprints/R_0805.redafp");
    let text = std::fs::read_to_string(&file).unwrap();
    let courtyard = r#"  "graphics": [
    {"uuid":"a0000000-0000-4000-8000-00000000005d","layer":"courtyard_top","shape":{"kind":"rect","start":{"x":-8000000,"y":-1000000},"end":{"x":1500000,"y":1000000}},"width":50000}
  ],
  "texts": ["#;
    assert!(text.contains(r#"  "texts": ["#));
    std::fs::write(&file, text.replacen(r#"  "texts": ["#, courtyard, 1)).unwrap();
    let mut engine = test_engine_with_library(&copy);
    engine.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));

    let small = place(&mut engine, R_10K, 0, 0); // R1, the 0603: Pads ±1.2 mm
    let wide = place_with(
        &mut engine,
        json!({"part": R_10K, "footprint": R_0805}),
        10,
        0,
    ); // R2
    sync(&mut engine);
    // The 0603's left edge is at -1.2 mm. The 0805's origin (5 mm) and Pads (3.5 mm) are
    // right of it, but its Courtyard starts at -3 mm.
    move_to(&mut engine, &small, 0, 10);
    move_to(&mut engine, &wide, 5, 0);

    engine.run_ok(
        "schematic.reannotate",
        json!({"source": "board", "scope": "project", "threshold": "0mm",
               "order": "ascending_x_then_ascending_y"}),
    );
    assert_eq!(designators(&mut engine, &[&wide, &small]), ["R1", "R2"]);
}
