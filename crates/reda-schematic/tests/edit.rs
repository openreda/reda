//! Schematic editing Commands: delete, the clipboard, duplicate, move and drag, rotate and
//! mirror, align, distribute, snap to grid, and free text and graphics.

use std::path::{Path, PathBuf};

use reda_command::ErrorCode;
use reda_testkit::{TempDir, TestEngine, test_engine_with_library};
use serde_json::{Value, json};

const SHEET: &str = "00000000-0000-4000-8000-000000000002";
const R_10K: &str = "a0000000-0000-4000-8000-000000000100";
const C_100N: &str = "a0000000-0000-4000-8000-000000000110";
const LM358: &str = "a0000000-0000-4000-8000-000000000120";

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

/// Every segment of every Wire on the sheet as `((x1, y1), (x2, y2))` in mil, ends sorted,
/// sorted.
fn wire_segments(engine: &mut TestEngine) -> Vec<((i64, i64), (i64, i64))> {
    let mil = |p: &Value| {
        (
            p["x"].as_i64().unwrap() / 25_400,
            p["y"].as_i64().unwrap() / 25_400,
        )
    };
    let mut segments: Vec<_> = objects(engine, SHEET, "wire")
        .iter()
        .flat_map(|w| {
            let vertices: Vec<_> = w["vertices"].as_array().unwrap().iter().map(mil).collect();
            let pairs: Vec<_> = vertices
                .windows(2)
                .map(|p| (p[0].min(p[1]), p[0].max(p[1])))
                .collect();
            pairs
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
    assert_eq!(
        object(&mut engine, &attached[0])["vertices"][0],
        point(0, 150)
    );
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
    // Across it: the Wire keeps its direction from the end that stayed, then turns a
    // corner, a new vertex of the same Wire.
    shift(&mut engine, "schematic.drag", &[&r1], 300, 0);
    assert_eq!(
        object(&mut engine, &up[0])["vertices"],
        json!([point(300, 250), point(0, 250), point(0, 500)])
    );
    // Still one Net from the Wire's far end to the Pin.
    let nets = engine.run_ok("schematic.nets", json!({"document": SHEET}));
    let net = nets["nets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["wires"].as_array().unwrap().contains(&up[0]))
        .unwrap()
        .clone();
    assert_eq!(net["wires"], json!([up[0]]));
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
    assert_eq!(
        object(&mut engine, &tail[0])["vertices"][1],
        point(1000, -600)
    );
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
    // Wires turn by any angle.
    engine.run_ok(
        "schematic.rotate",
        json!({"objects": [wires[0]], "angle": "45deg", "center": at(0, 0)}),
    );
    let turned = object(&mut engine, &wires[0]);
    for v in turned["vertices"].as_array().unwrap() {
        let (x, y) = (v["x"].as_i64().unwrap(), v["y"].as_i64().unwrap());
        assert!(x < 0 && (x + y).abs() <= 1, "{v} is not on the diagonal");
    }
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

#[test]
fn free_text_can_be_added_edited_moved_and_deleted() {
    let dir = TempDir::new("text");
    let mut engine = demo(&dir);
    let out = engine.run_ok(
        "schematic.text.add",
        json!({"document": SHEET, "text": "Power\nsupply", "at": at(100, 200)}),
    );
    let text = out["result"]["text"].clone();
    let stored = object(&mut engine, &text);
    assert_eq!(stored["type"], "sheet_text");
    assert_eq!(stored["text"], "Power\nsupply");
    assert_eq!(stored["at"], point(100, 200));
    // 50 mil high and upright by default.
    assert_eq!(stored["height"], 50 * 25_400);
    assert_eq!(stored["rotation"], 0);
    engine.run_ok(
        "schematic.text.set",
        json!({"uuid": text, "text": "Supply", "height": "100mil", "rotation": "90deg"}),
    );
    let stored = object(&mut engine, &text);
    assert_eq!(stored["text"], "Supply");
    assert_eq!(stored["height"], 100 * 25_400);
    assert_eq!(stored["rotation"], 90_000);
    assert_eq!(stored["at"], point(100, 200));
    let err = engine.run_err("schematic.text.set", json!({"uuid": text, "text": "  "}));
    assert_eq!(err.details.unwrap()["pointer"], "/text");
    shift(&mut engine, "schematic.move", &[&text], 100, 0);
    assert_eq!(at_mil(&mut engine, &text), (200, 200));
    engine.run_ok("schematic.delete", json!({"objects": [text]}));
    assert_eq!(count(&mut engine, "sheet_text"), 0);
}

#[test]
fn free_text_takes_nine_point_justification_and_several_lines() {
    let dir = TempDir::new("text-justify");
    let mut engine = demo(&dir);
    let out = engine.run_ok(
        "schematic.text.add",
        json!({"document": SHEET, "text": "Notes:\n1. Fit C3 last", "at": at(0, 0),
               "justify": "top_right"}),
    );
    let text = out["result"]["text"].clone();
    let stored = object(&mut engine, &text);
    assert_eq!(stored["justify"], "top_right");
    assert_eq!(stored["text"], "Notes:\n1. Fit C3 last");
    engine.run_ok(
        "schematic.text.set",
        json!({"uuid": text, "justify": "center"}),
    );
    assert_eq!(object(&mut engine, &text)["justify"], "center");
    // Bottom left is the default, and is left out of the file.
    engine.run_ok(
        "schematic.text.set",
        json!({"uuid": text, "justify": "bottom_left"}),
    );
    assert_eq!(object(&mut engine, &text).get("justify"), None);
    let err = engine.run_err(
        "schematic.text.set",
        json!({"uuid": text, "justify": "middle"}),
    );
    assert_eq!(err.code, ErrorCode::InvalidParams);
}

#[test]
fn lines_and_rectangles_can_be_added_edited_and_moved() {
    let dir = TempDir::new("graphics");
    let mut engine = demo(&dir);
    let line = engine.run_ok(
        "schematic.graphic.add_line",
        json!({"document": SHEET, "from": at(0, 0), "to": at(300, 100)}),
    )["result"]["graphic"]
        .clone();
    let rect = engine.run_ok(
        "schematic.graphic.add_rect",
        json!({"document": SHEET, "from": at(0, 0), "to": at(500, 400), "filled": true}),
    )["result"]["graphic"]
        .clone();
    let stored = object(&mut engine, &line);
    assert_eq!(stored["type"], "sheet_graphic");
    assert_eq!(
        stored["shape"],
        json!({"kind": "line", "from": point(0, 0), "to": point(300, 100)})
    );
    assert_eq!(object(&mut engine, &rect)["shape"]["filled"], true);
    engine.run_ok(
        "schematic.graphic.set",
        json!({"uuid": rect, "to": at(600, 400), "filled": false}),
    );
    assert_eq!(
        object(&mut engine, &rect)["shape"],
        json!({"kind": "rect", "from": point(0, 0), "to": point(600, 400)})
    );
    // Empty shapes are refused.
    let err = engine.run_err(
        "schematic.graphic.add_rect",
        json!({"document": SHEET, "from": at(0, 0), "to": at(0, 400)}),
    );
    assert_eq!(err.code, ErrorCode::InvalidParams);
    let err = engine.run_err(
        "schematic.graphic.set",
        json!({"uuid": line, "to": at(0, 0)}),
    );
    assert_eq!(err.code, ErrorCode::InvalidParams);
    // A line turns by any angle; a rectangle only by quarter turns.
    engine.run_ok(
        "schematic.rotate",
        json!({"objects": [line], "angle": "30deg", "center": at(0, 0)}),
    );
    let err = engine.run_err(
        "schematic.rotate",
        json!({"objects": [rect], "angle": "30deg"}),
    );
    assert_eq!(err.code, ErrorCode::Rejected);
    engine.run_ok(
        "schematic.rotate",
        json!({"objects": [rect], "center": at(0, 0)}),
    );
    assert_eq!(
        object(&mut engine, &rect)["shape"],
        json!({"kind": "rect", "from": point(0, 0), "to": point(-400, 600)})
    );
}

/// The Designators of the Units on `document`, sorted.
fn designators(engine: &mut TestEngine, document: &str) -> Vec<String> {
    let mut out: Vec<String> = objects(engine, document, "unit")
        .iter()
        .map(|u| u["designator"].as_str().unwrap().to_owned())
        .collect();
    out.sort();
    out
}

#[test]
fn copy_and_paste_across_sheets_brings_the_part_table_and_fresh_designators() {
    let dir = TempDir::new("paste-sheets");
    let mut engine = demo(&dir);
    let other = add_sheet(&mut engine);
    let r1 = place(&mut engine, R_10K, 0, 0);
    let c1 = place(&mut engine, C_100N, 1000, 0);
    let w = wire(&mut engine, &[(0, 150), (1000, 150)]);
    let vcc = label(&mut engine, "VCC", 500, 150);
    let text = engine.run_ok(
        "schematic.text.add",
        json!({"document": SHEET, "text": "Filter", "at": at(0, 500)}),
    )["result"]["text"]
        .clone();
    let clipboard = engine.run_ok(
        "schematic.copy",
        json!({"objects": [r1, c1, w[0], vcc, text]}),
    );
    // Pasting puts the extent's lower-left corner (0, -150) at `at`.
    let out = engine.run_ok(
        "schematic.paste",
        json!({"document": other, "clipboard": clipboard, "at": at(2000, 1000)}),
    );
    let pasted = out["result"]["objects"].as_array().unwrap().clone();
    assert_eq!(pasted.len(), 5);
    assert_eq!(designators(&mut engine, &other), ["C2", "R2"]);
    assert_eq!(objects(&mut engine, &other, "part_entry").len(), 2);
    assert_eq!(objects(&mut engine, &other, "footprint_copy").len(), 2);
    let r2 = objects(&mut engine, &other, "unit")
        .into_iter()
        .find(|u| u["designator"] == "R2")
        .unwrap();
    assert_eq!(r2["at"], point(2000, 1150));
    assert_ne!(
        r2["component"]["uuid"],
        object(&mut engine, &r1)["component"]["uuid"]
    );
    let labels = objects(&mut engine, &other, "net_label");
    assert_eq!(labels[0]["at"], point(2500, 1300));
    // The pasted drawing is connected as the original was.
    let nets = engine.run_ok("schematic.nets", json!({"document": other}));
    let vcc_net = nets["nets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["name"] == "VCC")
        .unwrap()
        .clone();
    assert_eq!(vcc_net["pins"].as_array().unwrap().len(), 2);
    // Pasting again on the same sheet reuses its part-table entries.
    engine.run_ok(
        "schematic.paste",
        json!({"document": other, "clipboard": clipboard, "at": at(5000, 1000)}),
    );
    assert_eq!(designators(&mut engine, &other), ["C2", "C3", "R2", "R3"]);
    assert_eq!(objects(&mut engine, &other, "part_entry").len(), 2);
    // Without `at`, objects land where they were copied from.
    engine.run_ok(
        "schematic.paste",
        json!({"document": SHEET, "clipboard": clipboard}),
    );
    assert_eq!(designators(&mut engine, SHEET), ["C1", "C4", "R1", "R4"]);
}

#[test]
fn units_of_one_component_stay_one_component_when_pasted() {
    let dir = TempDir::new("paste-units");
    let mut engine = demo(&dir);
    let a = place_on(&mut engine, SHEET, LM358, 0, 0);
    let b = engine.run_ok(
        "schematic.unit.place",
        json!({"document": SHEET, "component": a["component"]["uuid"], "at": at(2000, 0)}),
    )["result"]["unit"]
        .clone();
    let clipboard = engine.run_ok("schematic.copy", json!({"objects": [a["unit"], b]}));
    engine.run_ok(
        "schematic.paste",
        json!({"document": SHEET, "clipboard": clipboard, "at": at(0, 3000)}),
    );
    assert_eq!(designators(&mut engine, SHEET), ["U1", "U1", "U2", "U2"]);
    let units = objects(&mut engine, SHEET, "unit");
    let copies: Vec<_> = units.iter().filter(|u| u["designator"] == "U2").collect();
    assert_eq!(copies[0]["component"], copies[1]["component"]);
    assert_ne!(copies[0]["unit"], copies[1]["unit"]);
}

#[test]
fn cut_removes_the_selection_and_returns_what_paste_needs() {
    let dir = TempDir::new("cut");
    let mut engine = demo(&dir);
    let other = add_sheet(&mut engine);
    let r1 = place(&mut engine, R_10K, 0, 0);
    let w = wire(&mut engine, &[(0, 150), (0, 500)]);
    let out = engine.run_ok("schematic.cut", json!({"objects": [r1, w[0]]}));
    assert_eq!(count(&mut engine, "unit"), 0);
    assert_eq!(count(&mut engine, "part_entry"), 0);
    assert_eq!(count(&mut engine, "wire"), 0);
    let clipboard = out["result"].clone();
    engine.run_ok(
        "schematic.paste",
        json!({"document": other, "clipboard": clipboard}),
    );
    // R1 is free again once it is cut, so the pasted resistor takes it.
    assert_eq!(designators(&mut engine, &other), ["R1"]);
    assert_eq!(objects(&mut engine, &other, "wire").len(), 1);
}

#[test]
fn a_pasted_component_keeps_its_designator_while_it_is_free() {
    let dir = TempDir::new("paste-keeps-designator");
    let mut engine = demo(&dir);
    let r1 = place(&mut engine, R_10K, 0, 0);
    place(&mut engine, R_10K, 1000, 0);
    let r3 = place(&mut engine, R_10K, 2000, 0);
    let out = engine.run_ok("schematic.cut", json!({"objects": [r3]}));
    engine.run_ok("schematic.delete", json!({"objects": [r1]}));
    assert_eq!(designators(&mut engine, SHEET), ["R2"]);
    // R3 is free, so the pasted resistor keeps it rather than taking R1, the lowest free.
    let clipboard = out["result"].clone();
    let pasted = engine.run_ok(
        "schematic.paste",
        json!({"document": SHEET, "clipboard": clipboard, "at": at(0, 2000)}),
    );
    assert_eq!(designators(&mut engine, SHEET), ["R2", "R3"]);
    // Every paste gives new UUIDs, even after a cut.
    assert_ne!(pasted["result"]["objects"][0], r3);
    // Pasted again, R3 is taken, so the copy takes the next free one; none is ever left
    // unannotated.
    engine.run_ok(
        "schematic.paste",
        json!({"document": SHEET, "clipboard": clipboard, "at": at(0, 4000)}),
    );
    assert_eq!(designators(&mut engine, SHEET), ["R1", "R2", "R3"]);
}

#[test]
fn duplicate_pastes_a_copy_offset_on_the_same_sheet() {
    let dir = TempDir::new("duplicate");
    let mut engine = demo(&dir);
    let r1 = place(&mut engine, R_10K, 0, 0);
    let vcc = label(&mut engine, "VCC", 0, 150);
    let out = engine.run_ok(
        "schematic.duplicate",
        json!({"objects": [r1, vcc], "offset": at(500, 0)}),
    );
    let pasted = out["result"]["objects"].as_array().unwrap().clone();
    assert_eq!(pasted.len(), 2);
    assert_eq!(designators(&mut engine, SHEET), ["R1", "R2"]);
    assert_eq!(at_mil(&mut engine, &pasted[0]), (500, 0));
    assert_eq!(at_mil(&mut engine, &pasted[1]), (500, 150));
    assert_eq!(count(&mut engine, "part_entry"), 1);
    // 100 mil right and 100 mil down by default; one undo step each.
    engine.run_ok("schematic.duplicate", json!({"objects": [r1]}));
    assert_eq!(designators(&mut engine, SHEET), ["R1", "R2", "R3"]);
    engine.run_ok("edit.undo", json!({"document": SHEET}));
    assert_eq!(designators(&mut engine, SHEET), ["R1", "R2"]);
}

fn port(engine: &mut TestEngine, name: &str, x: i64, y: i64) -> Value {
    engine.run_ok(
        "schematic.port.add",
        json!({"document": SHEET, "name": name, "at": at(x, y), "direction": "out"}),
    )["result"]["port"]
        .clone()
}

fn power_port(engine: &mut TestEngine, name: &str, x: i64, y: i64) -> Value {
    engine.run_ok(
        "schematic.power_port.add",
        json!({"document": SHEET, "name": name, "at": at(x, y)}),
    )["result"]["power_port"]
        .clone()
}

#[test]
fn ports_and_power_ports_move_turn_flip_and_delete_like_labels() {
    let dir = TempDir::new("edit-ports");
    let mut engine = demo(&dir);
    let sda = port(&mut engine, "SDA", 1000, 0);
    let gnd = power_port(&mut engine, "GND", 2000, 500);
    shift(&mut engine, "schematic.move", &[&sda, &gnd], 100, 100);
    assert_eq!(pose(&mut engine, &sda), (point(1100, 100), json!(0)));
    assert_eq!(pose(&mut engine, &gnd), (point(2100, 600), json!(0)));
    engine.run_ok(
        "schematic.rotate",
        json!({"objects": [sda, gnd], "center": at(0, 0)}),
    );
    assert_eq!(pose(&mut engine, &sda), (point(-100, 1100), json!(90_000)));
    assert_eq!(pose(&mut engine, &gnd), (point(-600, 2100), json!(90_000)));
    // A lone Port turns about its own point.
    engine.run_ok("schematic.rotate", json!({"objects": [sda]}));
    assert_eq!(pose(&mut engine, &sda), (point(-100, 1100), json!(180_000)));
    engine.run_ok(
        "schematic.mirror",
        json!({"objects": [gnd], "axis": "x", "center": at(0, 0)}),
    );
    assert_eq!(pose(&mut engine, &gnd), (point(600, 2100), json!(90_000)));
    engine.run_ok(
        "schematic.align",
        json!({"objects": [sda, gnd], "to": "left"}),
    );
    assert_eq!(at_mil(&mut engine, &gnd), (-100, 2100));
    engine.run_ok("schematic.delete", json!({"objects": [sda, gnd]}));
    assert_eq!(count(&mut engine, "port"), 0);
    assert_eq!(count(&mut engine, "power_port"), 0);
}

#[test]
fn drag_brings_ports_and_power_ports_on_a_moved_pin() {
    let dir = TempDir::new("drag-ports");
    let mut engine = demo(&dir);
    let r1 = place(&mut engine, R_10K, 0, 0);
    let sda = port(&mut engine, "SDA", 0, 150);
    let gnd = power_port(&mut engine, "GND", 0, -150);
    let away = power_port(&mut engine, "VCC", 500, 150);
    shift(&mut engine, "schematic.drag", &[&r1], 200, 0);
    assert_eq!(object(&mut engine, &sda)["at"], point(200, 150));
    assert_eq!(object(&mut engine, &gnd)["at"], point(200, -150));
    assert_eq!(object(&mut engine, &away)["at"], point(500, 150));
    // A detached move leaves them behind.
    shift(&mut engine, "schematic.move", &[&r1], 200, 0);
    assert_eq!(object(&mut engine, &sda)["at"], point(200, 150));
}

#[test]
fn ports_and_power_ports_copy_paste_and_duplicate_with_their_names() {
    let dir = TempDir::new("paste-ports");
    let mut engine = demo(&dir);
    let sda = port(&mut engine, "SDA", 1000, 0);
    let gnd = power_port(&mut engine, "GND", 2000, 0);
    let other = add_sheet(&mut engine);
    let copied = engine.run_ok("schematic.copy", json!({"objects": [sda, gnd]}));
    let out = engine.run_ok(
        "schematic.paste",
        json!({"document": other, "clipboard": copied, "at": at(0, 0)}),
    );
    let pasted = out["result"]["objects"].as_array().unwrap().clone();
    assert_eq!(pasted.len(), 2);
    let p = object(&mut engine, &pasted[0]);
    assert_eq!((&p["type"], &p["name"]), (&json!("port"), &json!("SDA")));
    assert_eq!(p["direction"], "out");
    assert_eq!(p["at"], point(0, 0));
    let g = object(&mut engine, &pasted[1]);
    assert_eq!(
        (&g["type"], &g["name"]),
        (&json!("power_port"), &json!("GND"))
    );
    assert_eq!(g["style"], "ground");
    assert_eq!(g["at"], point(1000, 0));

    let out = engine.run_ok("schematic.duplicate", json!({"objects": [sda, gnd]}));
    assert_eq!(out["result"]["objects"].as_array().unwrap().len(), 2);
    assert_eq!(count(&mut engine, "port"), 2);
    assert_eq!(count(&mut engine, "power_port"), 2);
}

/// A previewed shift's Wire segments as `((x1, y1), (x2, y2))` in mil, ends sorted, sorted.
fn preview_segments(preview: &reda_schematic::ShiftPreview) -> Vec<((i64, i64), (i64, i64))> {
    let mil = |p: reda_model::geom::Point| (p.x.0 / 25_400, p.y.0 / 25_400);
    let mut segments: Vec<_> = preview
        .wires
        .iter()
        .flat_map(|v| v.windows(2))
        .map(|p| (mil(p[0]).min(mil(p[1])), mil(p[0]).max(mil(p[1]))))
        .collect();
    segments.sort();
    segments
}

fn uuid(v: &Value) -> reda_model::Uuid {
    v.as_str().unwrap().parse().unwrap()
}

fn mil(x: i64, y: i64) -> reda_model::geom::Point {
    use reda_model::geom::{Nm, Point};
    Point::new(Nm(x * 25_400), Nm(y * 25_400))
}

#[test]
fn a_drag_preview_shows_the_rubber_banded_wires_the_drag_then_commits() {
    let dir = TempDir::new("drag-preview");
    let mut engine = demo(&dir);
    let r1 = place(&mut engine, R_10K, 0, 0);
    let up = wire(&mut engine, &[(0, 150), (0, 500)]);
    let vcc = label(&mut engine, "VCC", 0, -150);
    let loose = wire(&mut engine, &[(2000, 0), (2500, 0)]);

    let project = engine.project().unwrap().clone();
    let preview = reda_schematic::shift_preview(&project, &[uuid(&r1)], mil(300, 0), true).unwrap();
    // The Unit moves whole and the label on its Pin comes along; the attached Wire is
    // drawn anew, bent round a corner; the loose Wire is untouched.
    let mut moved = vec![uuid(&r1), uuid(&vcc)];
    moved.sort();
    assert_eq!(preview.moved, moved);
    assert_eq!(preview.hidden, [uuid(&up[0])]);
    assert_eq!(
        preview_segments(&preview),
        [((0, 150), (0, 500)), ((0, 150), (300, 150))]
    );
    assert!(!preview.moved.contains(&uuid(&loose[0])));

    // Previewing commits nothing; the drag then makes what the preview showed.
    assert!(engine.project().unwrap() == &project);
    shift(&mut engine, "schematic.drag", &[&r1], 300, 0);
    assert_eq!(
        wire_segments(&mut engine),
        [
            ((0, 150), (0, 500)),
            ((0, 150), (300, 150)),
            ((2000, 0), (2500, 0))
        ]
    );
}

#[test]
fn a_detached_move_preview_leaves_every_other_object_alone() {
    let dir = TempDir::new("move-preview");
    let mut engine = demo(&dir);
    let r1 = place(&mut engine, R_10K, 0, 0);
    wire(&mut engine, &[(0, 150), (0, 500)]);
    label(&mut engine, "VCC", 0, -150);
    let project = engine.project().unwrap().clone();
    let preview =
        reda_schematic::shift_preview(&project, &[uuid(&r1)], mil(300, 0), false).unwrap();
    assert_eq!(preview.moved, [uuid(&r1)]);
    assert!(preview.hidden.is_empty());
    assert!(preview.wires.is_empty());
}

#[test]
fn a_pasted_wire_keeps_every_vertex_at_the_offset() {
    let dir = TempDir::new("paste-polyline");
    let mut engine = demo(&dir);
    let w = wire(&mut engine, &[(0, 0), (100, 50), (100, 200)]);
    let clipboard = engine.run_ok("schematic.copy", json!({"objects": [w[0]]}));
    // The extent's lower-left corner (0, 0) goes to `at`.
    let out = engine.run_ok(
        "schematic.paste",
        json!({"document": SHEET, "clipboard": clipboard, "at": at(1000, 1000)}),
    );
    let pasted = out["result"]["objects"][0].clone();
    assert_ne!(pasted, w[0]);
    assert_eq!(
        object(&mut engine, &pasted)["vertices"],
        json!([point(1000, 1000), point(1100, 1050), point(1100, 1200)])
    );
}

#[test]
fn deleting_a_wire_removes_the_whole_run() {
    let dir = TempDir::new("delete-polyline");
    let mut engine = demo(&dir);
    let w = wire(&mut engine, &[(0, 0), (100, 0), (100, 100), (200, 150)]);
    engine.run_ok("schematic.delete", json!({"objects": [w[0]]}));
    assert!(wire_segments(&mut engine).is_empty());
    engine.run_ok("edit.undo", json!({"document": SHEET}));
    assert_eq!(wire_segments(&mut engine).len(), 3);
}

#[test]
fn moving_a_wire_moves_every_vertex_and_dragging_bends_only_its_moved_end() {
    let dir = TempDir::new("drag-polyline");
    let mut engine = demo(&dir);
    let r1 = place(&mut engine, R_10K, 0, 0);
    // From Pin 1 up, across and down again. Dragged sideways, the first segment keeps its
    // direction from the vertex that stayed and turns a corner to the Pin.
    let run = wire(&mut engine, &[(0, 150), (0, 300), (500, 300), (500, 100)]);
    shift(&mut engine, "schematic.drag", &[&r1], -200, 0);
    assert_eq!(
        object(&mut engine, &run[0])["vertices"],
        json!([
            point(-200, 150),
            point(0, 150),
            point(0, 300),
            point(500, 300),
            point(500, 100)
        ])
    );
    shift(&mut engine, "schematic.move", &[&run[0]], 0, 1000);
    assert_eq!(
        object(&mut engine, &run[0])["vertices"],
        json!([
            point(-200, 1150),
            point(0, 1150),
            point(0, 1300),
            point(500, 1300),
            point(500, 1100)
        ])
    );
}

#[test]
fn dragging_the_end_of_a_diagonal_wire_keeps_it_straight() {
    let dir = TempDir::new("drag-diagonal");
    let mut engine = demo(&dir);
    let r1 = place(&mut engine, R_10K, 0, 0);
    let slope = wire(&mut engine, &[(0, 150), (300, 450)]);
    shift(&mut engine, "schematic.drag", &[&r1], 100, 0);
    assert_eq!(
        object(&mut engine, &slope[0])["vertices"],
        json!([point(100, 150), point(300, 450)])
    );
}
