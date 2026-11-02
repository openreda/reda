//! Schematic editing Commands: delete, the clipboard, duplicate, move and drag, rotate and
//! mirror, align, distribute, snap to grid, and free text and graphics.

use std::path::{Path, PathBuf};

use reda_command::ErrorCode;
use reda_testkit::{TempDir, TestEngine, test_engine_with_library};
use serde_json::{Value, json};

const SHEET: &str = "00000000-0000-4000-8000-000000000002";
const R_10K: &str = "a0000000-0000-4000-8000-000000000100";
const C_100N: &str = "a0000000-0000-4000-8000-000000000110";

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

/// A point given in mil, as stored.
fn point(x: i64, y: i64) -> Value {
    json!({"x": x * 25_400, "y": y * 25_400})
}

/// Places Part `part` with its origin at (`x`, `y`) mil on `document`; returns the result.
fn place_on(engine: &mut TestEngine, document: &str, part: &str, x: i64, y: i64) -> Value {
    engine.run_ok(
        "schematic.component.place",
        json!({"document": document, "part": part, "at": at(x, y)}),
    )["result"]
        .clone()
}

/// Places Part `part` on the first sheet and returns its Unit's UUID.
fn place(engine: &mut TestEngine, part: &str, x: i64, y: i64) -> Value {
    place_on(engine, SHEET, part, x, y)["unit"].clone()
}

/// Adds a Wire run through `points`, given in mil, and returns its Wires' UUIDs.
fn wire(engine: &mut TestEngine, points: &[(i64, i64)]) -> Vec<Value> {
    let points: Vec<Value> = points.iter().map(|&(x, y)| at(x, y)).collect();
    let out = engine.run_ok(
        "schematic.wire.add",
        json!({"document": SHEET, "points": points}),
    );
    out["result"]["wires"].as_array().unwrap().clone()
}

fn label(engine: &mut TestEngine, name: &str, x: i64, y: i64) -> Value {
    engine.run_ok(
        "schematic.net_label.add",
        json!({"document": SHEET, "name": name, "at": at(x, y)}),
    )["result"]["net_label"]
        .clone()
}

fn object(engine: &mut TestEngine, uuid: &Value) -> Value {
    engine.run_ok("object.get", json!({"uuid": uuid}))["object"].clone()
}

fn objects(engine: &mut TestEngine, document: &str, kind: &str) -> Vec<Value> {
    let out = engine.run_ok("objects.query", json!({"document": document, "type": kind}));
    out["objects"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["object"].clone())
        .collect()
}

fn count(engine: &mut TestEngine, kind: &str) -> usize {
    objects(engine, SHEET, kind).len()
}

/// Every Wire on the sheet as `((x1, y1), (x2, y2))` in mil, ends sorted, sorted.
fn wire_segments(engine: &mut TestEngine) -> Vec<((i64, i64), (i64, i64))> {
    let mil = |p: &Value| {
        (
            p["x"].as_i64().unwrap() / 25_400,
            p["y"].as_i64().unwrap() / 25_400,
        )
    };
    let mut segments: Vec<_> = objects(engine, SHEET, "wire")
        .iter()
        .map(|w| {
            let (a, b) = (mil(&w["from"]), mil(&w["to"]));
            (a.min(b), a.max(b))
        })
        .collect();
    segments.sort();
    segments
}

#[test]
fn deleting_a_mixed_selection_removes_it_and_prunes_unused_part_entries() {
    let dir = TempDir::new("delete-mixed");
    let mut engine = demo(&dir);
    let r1 = place(&mut engine, R_10K, 0, 0);
    let r2 = place(&mut engine, R_10K, 1000, 0);
    let c1 = place(&mut engine, C_100N, 2000, 0);
    let wires = wire(&mut engine, &[(0, 150), (1000, 150)]);
    let vcc = label(&mut engine, "VCC", 500, 150);
    assert_eq!(count(&mut engine, "part_entry"), 2);
    assert_eq!(count(&mut engine, "footprint_copy"), 2);

    let out = engine.run_ok(
        "schematic.delete",
        json!({"objects": [r1, c1, wires[0], vcc]}),
    );
    // The capacitor's part entry goes with its only Unit; the 0603 copy it shares stays.
    let deleted = &out["changes"][SHEET]["deleted"];
    assert_eq!(deleted.as_array().unwrap().len(), 5, "{out}");
    assert_eq!(count(&mut engine, "unit"), 1);
    assert_eq!(object(&mut engine, &r2)["designator"], "R2");
    assert_eq!(count(&mut engine, "wire"), 0);
    assert_eq!(count(&mut engine, "net_label"), 0);
    assert_eq!(count(&mut engine, "part_entry"), 1);
    assert_eq!(count(&mut engine, "footprint_copy"), 2);

    // Deleting the last resistor prunes its entry and the Footprint copies no entry uses.
    engine.run_ok("schematic.delete", json!({"objects": [r2]}));
    assert_eq!(count(&mut engine, "part_entry"), 0);
    assert_eq!(count(&mut engine, "footprint_copy"), 0);

    // Each delete is one undo step.
    engine.run_ok("edit.undo", json!({"document": SHEET}));
    engine.run_ok("edit.undo", json!({"document": SHEET}));
    assert_eq!(count(&mut engine, "unit"), 3);
    assert_eq!(count(&mut engine, "wire"), 1);
}

#[test]
fn deleting_refuses_unknown_and_unselectable_objects() {
    let dir = TempDir::new("delete-refused");
    let mut engine = demo(&dir);
    let out = place_on(&mut engine, SHEET, R_10K, 0, 0);
    let err = engine.run_err(
        "schematic.delete",
        json!({"objects": ["00000000-0000-4000-8000-0000000000ff"]}),
    );
    assert_eq!(err.code, ErrorCode::NotFound);
    let err = engine.run_err("schematic.delete", json!({"objects": [out["part_entry"]]}));
    assert_eq!(err.code, ErrorCode::InvalidParams);
    assert_eq!(err.details.unwrap()["pointer"], "/objects/0");
}

/// Adds a Schematic Sheet and returns its UUID.
fn add_sheet(engine: &mut TestEngine) -> String {
    let added = engine.run_ok("document.add", json!({"kind": "schematic"}));
    added["result"]["uuid"].as_str().unwrap().to_owned()
}

/// Moves `objects` by (`dx`, `dy`) mil with `id` (`schematic.move` or `schematic.drag`).
fn shift(engine: &mut TestEngine, id: &str, objects: &[&Value], dx: i64, dy: i64) -> Value {
    engine.run_ok(id, json!({"objects": objects, "by": at(dx, dy)}))
}

#[test]
fn move_translates_every_selected_object_and_detaches_from_wires() {
    let dir = TempDir::new("move-detach");
    let mut engine = demo(&dir);
    let r1 = place(&mut engine, R_10K, 0, 0);
    let attached = wire(&mut engine, &[(0, 150), (0, 500)]);
    let loose = wire(&mut engine, &[(2000, 0), (2500, 0)]);
    let vcc = label(&mut engine, "VCC", 2000, 0);
    shift(
        &mut engine,
        "schematic.move",
        &[&r1, &loose[0], &vcc],
        1000,
        -100,
    );
    assert_eq!(object(&mut engine, &r1)["at"], point(1000, -100));
    assert_eq!(object(&mut engine, &vcc)["at"], point(3000, -100));
    // The selected Wire moves whole; the attached one is left where it was.
    assert_eq!(
        wire_segments(&mut engine),
        [((0, 150), (0, 500)), ((3000, -100), (3500, -100))]
    );
    assert_eq!(object(&mut engine, &attached[0])["from"], point(0, 150));
    // One undo step.
    engine.run_ok("edit.undo", json!({"document": SHEET}));
    assert_eq!(object(&mut engine, &r1)["at"], point(0, 0));
    assert_eq!(object(&mut engine, &vcc)["at"], point(2000, 0));
}

#[test]
fn move_onto_a_wire_splits_it_like_placing() {
    let dir = TempDir::new("move-split");
    let mut engine = demo(&dir);
    let r1 = place(&mut engine, R_10K, 2000, 0);
    wire(&mut engine, &[(-500, 150), (500, 150)]);
    shift(&mut engine, "schematic.move", &[&r1], -2000, 0);
    assert_eq!(
        wire_segments(&mut engine),
        [((-500, 150), (0, 150)), ((0, 150), (500, 150))]
    );
}

#[test]
fn drag_along_a_wire_stretches_it_and_across_it_turns_a_corner() {
    let dir = TempDir::new("drag-wires");
    let mut engine = demo(&dir);
    let r1 = place(&mut engine, R_10K, 0, 0);
    let up = wire(&mut engine, &[(0, 150), (0, 500)]);
    // Along the Wire: its attached end follows Pin 1 and it stays one straight Wire.
    shift(&mut engine, "schematic.drag", &[&r1], 0, 100);
    assert_eq!(wire_segments(&mut engine), [((0, 250), (0, 500))]);
    // Across it: the Wire keeps its direction from the end that stayed, then turns.
    shift(&mut engine, "schematic.drag", &[&r1], 300, 0);
    assert_eq!(
        wire_segments(&mut engine),
        [((0, 250), (0, 500)), ((0, 250), (300, 250))]
    );
    assert_eq!(object(&mut engine, &up[0])["to"], point(0, 500));
    // Still one Net from the Wire's far end to the Pin.
    let nets = engine.run_ok("schematic.nets", json!({"document": SHEET}));
    let net = nets["nets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["wires"].as_array().unwrap().contains(&up[0]))
        .unwrap()
        .clone();
    assert_eq!(net["wires"].as_array().unwrap().len(), 2);
    assert_eq!(net["pins"].as_array().unwrap().len(), 1);
}

#[test]
fn drag_moves_wires_between_dragged_pins_whole_and_brings_labels_on_pins() {
    let dir = TempDir::new("drag-whole");
    let mut engine = demo(&dir);
    let r1 = place(&mut engine, R_10K, 0, 0);
    let r2 = place(&mut engine, R_10K, 1000, 0);
    wire(&mut engine, &[(0, 150), (1000, 150)]);
    let tail = wire(&mut engine, &[(1000, -150), (1000, -600)]);
    let vcc = label(&mut engine, "VCC", 0, -150);
    shift(&mut engine, "schematic.drag", &[&r1, &r2], 0, 200);
    assert_eq!(
        wire_segments(&mut engine),
        [((0, 350), (1000, 350)), ((1000, -600), (1000, 50))]
    );
    assert_eq!(object(&mut engine, &tail[0])["to"], point(1000, -600));
    assert_eq!(object(&mut engine, &vcc)["at"], point(0, 50));
    // A drag that folds a Wire onto itself removes it.
    shift(&mut engine, "schematic.drag", &[&r1, &r2], 0, -650);
    assert_eq!(wire_segments(&mut engine), [((0, -300), (1000, -300))]);
}

#[test]
fn move_needs_objects_on_one_sheet() {
    let dir = TempDir::new("move-sheets");
    let mut engine = demo(&dir);
    let other = add_sheet(&mut engine);
    let r1 = place(&mut engine, R_10K, 0, 0);
    let r2 = place_on(&mut engine, &other, R_10K, 0, 0)["unit"].clone();
    let err = engine.run_err(
        "schematic.move",
        json!({"objects": [r1, r2], "by": at(100, 0)}),
    );
    assert_eq!(err.code, ErrorCode::InvalidParams);
    assert_eq!(err.details.unwrap()["pointer"], "/objects");
}

#[test]
fn rotate_turns_a_unit_about_its_origin_by_any_angle() {
    let dir = TempDir::new("rotate-unit");
    let mut engine = demo(&dir);
    let r1 = place(&mut engine, R_10K, 1000, 0);
    // A quarter turn by default, about the Unit's own origin when it is alone.
    engine.run_ok("schematic.rotate", json!({"objects": [r1]}));
    let rotated = object(&mut engine, &r1);
    assert_eq!(rotated["at"], point(1000, 0));
    assert_eq!(rotated["rotation"], 90_000);
    engine.run_ok(
        "schematic.rotate",
        json!({"objects": [r1], "angle": "12.345deg"}),
    );
    assert_eq!(object(&mut engine, &r1)["rotation"], 102_345);
    // Full turns wrap.
    engine.run_ok(
        "schematic.rotate",
        json!({"objects": [r1], "angle": "-102.345deg"}),
    );
    assert_eq!(object(&mut engine, &r1)["rotation"], 0);
}

/// The `at` and `rotation` of an object.
fn pose(engine: &mut TestEngine, uuid: &Value) -> (Value, Value) {
    let o = object(engine, uuid);
    (o["at"].clone(), o["rotation"].clone())
}

#[test]
fn rotate_a_selection_about_a_centre_turns_positions_and_directions() {
    let dir = TempDir::new("rotate-selection");
    let mut engine = demo(&dir);
    let r1 = place(&mut engine, R_10K, 1000, 0);
    let vcc = label(&mut engine, "VCC", 2000, 0);
    let wires = wire(&mut engine, &[(3000, 0), (3500, 0)]);
    engine.run_ok(
        "schematic.rotate",
        json!({"objects": [r1, vcc, wires[0]], "center": at(0, 0)}),
    );
    assert_eq!(pose(&mut engine, &r1), (point(0, 1000), json!(90_000)));
    assert_eq!(pose(&mut engine, &vcc), (point(0, 2000), json!(90_000)));
    assert_eq!(wire_segments(&mut engine), [((0, 3000), (0, 3500))]);
    // Wires turn only in quarter turns.
    let err = engine.run_err(
        "schematic.rotate",
        json!({"objects": [r1, wires[0]], "angle": "45deg"}),
    );
    assert_eq!(err.code, ErrorCode::Rejected);
}

#[test]
fn mirror_x_and_y_flip_units_and_labels() {
    let dir = TempDir::new("mirror");
    let mut engine = demo(&dir);
    let r1 = place(&mut engine, R_10K, 1000, 0);
    let vcc = label(&mut engine, "VCC", 2000, 500);
    engine.run_ok("schematic.mirror", json!({"objects": [r1], "axis": "x"}));
    let r = object(&mut engine, &r1);
    assert_eq!(r["at"], point(1000, 0));
    assert_eq!(r["mirror"], true);
    assert_eq!(r["rotation"], 0);
    // Mirroring top to bottom is a left-right flip turned by a half turn.
    engine.run_ok("schematic.mirror", json!({"objects": [r1], "axis": "y"}));
    let r = object(&mut engine, &r1);
    assert_eq!(r["mirror"], json!(null), "{r}");
    assert_eq!(r["rotation"], 180_000);
    // About a centre, positions flip too; a label's direction flips with it.
    engine.run_ok(
        "schematic.mirror",
        json!({"objects": [r1, vcc], "axis": "x", "center": at(0, 0)}),
    );
    assert_eq!(object(&mut engine, &r1)["at"], point(-1000, 0));
    assert_eq!(pose(&mut engine, &vcc), (point(-2000, 500), json!(180_000)));
}

/// The `at` of an object in mil.
fn at_mil(engine: &mut TestEngine, uuid: &Value) -> (i64, i64) {
    let o = object(engine, uuid);
    (
        o["at"]["x"].as_i64().unwrap() / 25_400,
        o["at"]["y"].as_i64().unwrap() / 25_400,
    )
}

#[test]
fn align_lines_up_edges_and_centres_of_the_selection() {
    let dir = TempDir::new("align");
    let mut engine = demo(&dir);
    // A resistor covers its Pins: 150 mil above and below its origin.
    let r1 = place(&mut engine, R_10K, 0, 0);
    let r2 = place(&mut engine, R_10K, 1000, 300);
    let vcc = label(&mut engine, "VCC", 500, -200);
    let all = json!([r1, r2, vcc]);
    let align = |engine: &mut TestEngine, to: &str| {
        engine.run_ok("schematic.align", json!({"objects": all, "to": to}));
    };
    align(&mut engine, "left");
    assert_eq!(at_mil(&mut engine, &r2), (0, 300));
    assert_eq!(at_mil(&mut engine, &vcc), (0, -200));
    engine.run_ok("edit.undo", json!({"document": SHEET}));
    align(&mut engine, "right");
    assert_eq!(at_mil(&mut engine, &r1), (1000, 0));
    assert_eq!(at_mil(&mut engine, &vcc), (1000, -200));
    engine.run_ok("edit.undo", json!({"document": SHEET}));
    // Up is +y.
    align(&mut engine, "top");
    assert_eq!(at_mil(&mut engine, &r1), (0, 300));
    assert_eq!(at_mil(&mut engine, &vcc), (500, 450));
    engine.run_ok("edit.undo", json!({"document": SHEET}));
    align(&mut engine, "bottom");
    assert_eq!(at_mil(&mut engine, &r2), (1000, -200 + 150));
    assert_eq!(at_mil(&mut engine, &r1), (0, -200 + 150));
    engine.run_ok("edit.undo", json!({"document": SHEET}));
    // Centres line up on the middle of the selection's extent.
    align(&mut engine, "center_x");
    assert_eq!(at_mil(&mut engine, &r1), (500, 0));
    assert_eq!(at_mil(&mut engine, &r2), (500, 300));
    engine.run_ok("edit.undo", json!({"document": SHEET}));
    align(&mut engine, "center_y");
    // Extent y: -200 (label) to 450 (R2's top Pin): middle 125.
    assert_eq!(at_mil(&mut engine, &r1), (0, 125));
    assert_eq!(at_mil(&mut engine, &vcc), (500, 125));
}

#[test]
fn distribute_spaces_centres_evenly_between_the_outermost() {
    let dir = TempDir::new("distribute");
    let mut engine = demo(&dir);
    let r1 = place(&mut engine, R_10K, 0, 0);
    let r2 = place(&mut engine, R_10K, 100, 0);
    let r3 = place(&mut engine, R_10K, 1000, 0);
    let r4 = place(&mut engine, R_10K, 700, 0);
    engine.run_ok(
        "schematic.distribute",
        json!({"objects": [r1, r2, r3, r4], "direction": "horizontal"}),
    );
    assert_eq!(at_mil(&mut engine, &r1), (0, 0));
    assert_eq!(at_mil(&mut engine, &r2), (333, 0));
    assert_eq!(at_mil(&mut engine, &r4), (666, 0));
    assert_eq!(at_mil(&mut engine, &r3), (1000, 0));
    let l1 = label(&mut engine, "A", 0, 0);
    let l2 = label(&mut engine, "B", 0, 900);
    let l3 = label(&mut engine, "C", 0, 100);
    engine.run_ok(
        "schematic.distribute",
        json!({"objects": [l1, l2, l3], "direction": "vertical"}),
    );
    assert_eq!(at_mil(&mut engine, &l3), (0, 450));
}

#[test]
fn snap_to_grid_moves_anchor_points_to_the_nearest_grid_point() {
    let dir = TempDir::new("snap");
    let mut engine = demo(&dir);
    let r1 = place(&mut engine, R_10K, 13, -26);
    let w = wire(&mut engine, &[(1010, 70), (1240, 70)]);
    // 50 mil by default.
    engine.run_ok("schematic.snap_to_grid", json!({"objects": [r1, w[0]]}));
    assert_eq!(at_mil(&mut engine, &r1), (0, -50));
    assert_eq!(wire_segments(&mut engine), [((1000, 50), (1250, 50))]);
    engine.run_ok(
        "schematic.snap_to_grid",
        json!({"objects": [r1], "grid": "100mil"}),
    );
    assert_eq!(at_mil(&mut engine, &r1), (0, -100));
}

#[test]
fn rotate_can_keep_connections_like_a_drag() {
    let dir = TempDir::new("rotate-keep");
    let mut engine = demo(&dir);
    let r1 = place(&mut engine, R_10K, 0, 0);
    wire(&mut engine, &[(0, 150), (0, 500)]);
    engine.run_ok(
        "schematic.rotate",
        json!({"objects": [r1], "keep_connections": true}),
    );
    // Pin 1 went from (0, 150) to (-150, 0); the Wire follows with a corner.
    assert_eq!(
        wire_segments(&mut engine),
        [((-150, 0), (0, 0)), ((0, 0), (0, 500))]
    );
}
