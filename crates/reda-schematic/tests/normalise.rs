//! Wire normalisation: every edit Command that creates or changes Wires or Units tidies the
//! Wires involved in the same undo step, as its `wiring` parameter says (the fixed
//! defaults when left out), plus `schematic.wire.break` and
//! `schematic.junctions.convert_cross`.

use std::path::{Path, PathBuf};

use reda_testkit::{TempDir, TestEngine, test_engine_with_library};
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

/// A point given in mil, as stored.
fn point(x: i64, y: i64) -> Value {
    json!({"x": x * 25_400, "y": y * 25_400})
}

/// Adds a Wire run through `points` (mil) with `wiring`, and returns the Wires it left.
fn wire_with(engine: &mut TestEngine, points: &[(i64, i64)], wiring: Value) -> Vec<Value> {
    let points: Vec<Value> = points.iter().map(|&(x, y)| at(x, y)).collect();
    let out = engine.run_ok(
        "schematic.wire.add",
        json!({"document": SHEET, "points": points, "wiring": wiring}),
    );
    out["result"]["wires"].as_array().unwrap().clone()
}

fn wire(engine: &mut TestEngine, points: &[(i64, i64)]) -> Vec<Value> {
    wire_with(engine, points, json!({}))
}

/// Every Wire on the sheet as its vertices in mil, sorted.
fn wires(engine: &mut TestEngine) -> Vec<Vec<(i64, i64)>> {
    let out = engine.run_ok("objects.query", json!({"document": SHEET, "type": "wire"}));
    let mut all: Vec<Vec<(i64, i64)>> = (out["objects"].as_array().unwrap().iter())
        .map(|w| {
            (w["object"]["vertices"].as_array().unwrap().iter())
                .map(|p| {
                    (
                        p["x"].as_i64().unwrap() / 25_400,
                        p["y"].as_i64().unwrap() / 25_400,
                    )
                })
                .collect()
        })
        .collect();
    all.sort();
    all
}

fn vertices(engine: &mut TestEngine, uuid: &Value) -> Value {
    engine.run_ok("object.get", json!({"uuid": uuid}))["object"]["vertices"].clone()
}

fn junctions(engine: &mut TestEngine) -> Value {
    engine.run_ok("schematic.nets", json!({"document": SHEET}))["junctions"].clone()
}

fn net_count(engine: &mut TestEngine) -> usize {
    let out = engine.run_ok("schematic.nets", json!({"document": SHEET}));
    out["nets"].as_array().unwrap().len()
}

fn lowest(a: &Value, b: &Value) -> Value {
    json!(a.as_str().unwrap().min(b.as_str().unwrap()))
}

#[test]
fn a_wire_ending_on_another_splits_it_at_the_junction() {
    let dir = TempDir::new("norm-t");
    let mut engine = demo(&dir);
    let a = wire(&mut engine, &[(0, 0), (1000, 0)]);
    let b = wire(&mut engine, &[(500, 0), (500, 500)]);
    assert_eq!(b.len(), 1);
    assert_eq!(
        wires(&mut engine),
        [
            vec![(0, 0), (500, 0)],
            vec![(500, 0), (500, 500)],
            vec![(500, 0), (1000, 0)]
        ]
    );
    // The piece from the first vertex keeps the UUID.
    assert_eq!(
        vertices(&mut engine, &a[0]),
        json!([point(0, 0), point(500, 0)])
    );
    assert_eq!(junctions(&mut engine), json!([point(500, 0)]));
    // One undo step for the run and its normalisation.
    engine.run_ok("edit.undo", json!({"document": SHEET}));
    assert_eq!(wires(&mut engine), [vec![(0, 0), (1000, 0)]]);
}

#[test]
fn a_new_wire_passing_over_a_wire_end_is_split_into_its_pieces() {
    let dir = TempDir::new("norm-t-pass");
    let mut engine = demo(&dir);
    wire(&mut engine, &[(500, 0), (500, 500)]);
    let run = wire(&mut engine, &[(0, 0), (1000, 0)]);
    assert_eq!(run.len(), 2);
    assert_eq!(
        vertices(&mut engine, &run[0]),
        json!([point(0, 0), point(500, 0)])
    );
    assert_eq!(
        vertices(&mut engine, &run[1]),
        json!([point(500, 0), point(1000, 0)])
    );
}

#[test]
fn with_break_wires_at_autojunctions_off_a_wire_stays_whole_through_a_junction() {
    let dir = TempDir::new("norm-t-off");
    let mut engine = demo(&dir);
    let off = json!({"break_wires_at_autojunctions": false});
    wire_with(&mut engine, &[(0, 0), (1000, 0)], off.clone());
    wire_with(&mut engine, &[(500, 0), (500, 500)], off);
    assert_eq!(
        wires(&mut engine),
        [vec![(0, 0), (1000, 0)], vec![(500, 0), (500, 500)]]
    );
    assert_eq!(junctions(&mut engine), json!([point(500, 0)]));
}

#[test]
fn collinear_wires_meeting_end_to_end_merge_into_the_lowest_uuid() {
    let dir = TempDir::new("norm-merge");
    let mut engine = demo(&dir);
    let a = wire(&mut engine, &[(0, 0), (500, 0)]);
    let b = wire(&mut engine, &[(1000, 0), (500, 0)]);
    let survivor = lowest(&a[0], &b[0]);
    assert_eq!(b, std::slice::from_ref(&survivor));
    assert_eq!(wires(&mut engine), [vec![(0, 0), (1000, 0)]]);
    let merged = vertices(&mut engine, &survivor);
    assert_eq!(merged.as_array().unwrap().len(), 2);
}

#[test]
fn with_optimize_wires_off_wires_stay_as_drawn() {
    let dir = TempDir::new("norm-merge-off");
    let mut engine = demo(&dir);
    let off = json!({"optimize_wires": false});
    wire_with(&mut engine, &[(0, 0), (500, 0)], off.clone());
    wire_with(&mut engine, &[(500, 0), (700, 0), (1000, 0)], off);
    assert_eq!(
        wires(&mut engine),
        [vec![(0, 0), (500, 0)], vec![(500, 0), (700, 0), (1000, 0)]]
    );
}

#[test]
fn overlapping_collinear_wires_merge() {
    let dir = TempDir::new("norm-overlap");
    let mut engine = demo(&dir);
    wire(&mut engine, &[(0, 0), (600, 0)]);
    wire(&mut engine, &[(400, 0), (1000, 0), (1000, 500)]);
    assert_eq!(wires(&mut engine), [vec![(0, 0), (1000, 0), (1000, 500)]]);
    // A Wire drawn along another disappears into it.
    wire(&mut engine, &[(200, 0), (300, 0)]);
    assert_eq!(wires(&mut engine), [vec![(0, 0), (1000, 0), (1000, 500)]]);
}

#[test]
fn collinear_vertices_inside_a_wire_are_removed() {
    let dir = TempDir::new("norm-collinear");
    let mut engine = demo(&dir);
    let run = wire(&mut engine, &[(0, 0), (300, 0), (600, 0), (600, 300)]);
    assert_eq!(
        vertices(&mut engine, &run[0]),
        json!([point(0, 0), point(600, 0), point(600, 300)])
    );
}

#[test]
fn wires_meeting_end_to_end_where_a_third_wire_ends_do_not_merge() {
    let dir = TempDir::new("norm-no-merge");
    let mut engine = demo(&dir);
    wire(&mut engine, &[(500, 0), (500, 500)]);
    wire(&mut engine, &[(0, 0), (500, 0)]);
    wire(&mut engine, &[(500, 0), (1000, 0)]);
    assert_eq!(wires(&mut engine).len(), 3);
    assert_eq!(junctions(&mut engine), json!([point(500, 0)]));
}

#[test]
fn deleting_one_branch_of_a_t_removes_the_junction_and_merges_the_other_two() {
    let dir = TempDir::new("norm-delete");
    let mut engine = demo(&dir);
    let a = wire(&mut engine, &[(0, 0), (1000, 0)]);
    let stem = wire(&mut engine, &[(500, 0), (500, 500)]);
    assert_eq!(wires(&mut engine).len(), 3);
    let out = engine.run_ok("schematic.delete", json!({"objects": stem}));
    assert_eq!(wires(&mut engine), [vec![(0, 0), (1000, 0)]]);
    assert_eq!(junctions(&mut engine), json!([]));
    assert_eq!(
        vertices(&mut engine, &a[0]),
        json!([point(0, 0), point(1000, 0)])
    );
    // The piece merged away is reported as deleted too.
    assert_eq!(out["result"]["deleted"].as_array().unwrap().len(), 2);
}

#[test]
fn a_wire_that_would_make_a_4_way_is_offset_into_two_adjacent_3_ways() {
    let dir = TempDir::new("norm-cross");
    let mut engine = demo(&dir);
    wire(&mut engine, &[(0, 0), (1000, 0)]);
    wire(&mut engine, &[(500, 0), (500, 500)]);
    let run = wire(&mut engine, &[(500, -500), (500, 0)]);
    // One snap grid (50 mil) along the crossing Wire, with a mitred corner.
    assert_eq!(
        vertices(&mut engine, &run[0]),
        json!([point(500, -500), point(500, -50), point(550, 0)])
    );
    assert_eq!(
        junctions(&mut engine),
        json!([point(500, 0), point(550, 0)])
    );
    assert_eq!(net_count(&mut engine), 1);
}

#[test]
fn the_cross_offset_follows_the_snap_grid_given() {
    let dir = TempDir::new("norm-cross-grid");
    let mut engine = demo(&dir);
    wire(&mut engine, &[(0, 0), (1000, 0)]);
    wire(&mut engine, &[(500, 0), (500, 500)]);
    let run = wire_with(
        &mut engine,
        &[(500, -500), (500, 0)],
        json!({"snap_grid": "100mil"}),
    );
    assert_eq!(
        vertices(&mut engine, &run[0]),
        json!([point(500, -500), point(500, -100), point(600, 0)])
    );
}

#[test]
fn with_convert_cross_junctions_off_the_4_way_stays_as_drawn() {
    let dir = TempDir::new("norm-cross-off");
    let mut engine = demo(&dir);
    wire(&mut engine, &[(0, 0), (1000, 0)]);
    wire(&mut engine, &[(500, 0), (500, 500)]);
    wire_with(
        &mut engine,
        &[(500, -500), (500, 0)],
        json!({"convert_cross_junctions": false}),
    );
    assert!(wires(&mut engine).contains(&vec![(500, -500), (500, 0)]));
    assert_eq!(junctions(&mut engine), json!([point(500, 0)]));
}

/// Places R_10K at the origin with `wiring`: its Pins are at (0, 150) and (0, -150) mil.
fn place_at_origin(engine: &mut TestEngine, wiring: Value) {
    engine.run_ok(
        "schematic.component.place",
        json!({"document": SHEET, "part": R_10K, "at": at(0, 0), "wiring": wiring}),
    );
}

fn net_of_pin_count(engine: &mut TestEngine) -> usize {
    let out = engine.run_ok("schematic.nets", json!({"document": SHEET}));
    (out["nets"].as_array().unwrap().iter())
        .filter(|n| !n["pins"].as_array().unwrap().is_empty())
        .count()
}

#[test]
fn a_component_dropped_inline_cuts_out_the_piece_between_its_pins() {
    let dir = TempDir::new("norm-cut");
    let mut engine = demo(&dir);
    wire(&mut engine, &[(0, -500), (0, 500)]);
    place_at_origin(&mut engine, json!({}));
    assert_eq!(
        wires(&mut engine),
        [vec![(0, -500), (0, -150)], vec![(0, 150), (0, 500)]]
    );
    assert_eq!(net_of_pin_count(&mut engine), 2);
}

#[test]
fn the_pieces_between_the_pins_stay_when_a_t_sits_between_them() {
    let dir = TempDir::new("norm-cut-t");
    let mut engine = demo(&dir);
    let off = json!({"break_wires_at_autojunctions": false});
    wire_with(&mut engine, &[(0, -500), (0, 500)], off.clone());
    wire_with(&mut engine, &[(0, 0), (300, 0)], off.clone());
    place_at_origin(&mut engine, off);
    // Nothing is cut: removing the pieces would orphan the branch.
    assert_eq!(
        wires(&mut engine),
        [vec![(0, -500), (0, 500)], vec![(0, 0), (300, 0)]]
    );
    // With Break Wires At Autojunctions on, the Wire splits at each Pin, keeping every piece.
    engine.run_ok("edit.undo", json!({"document": SHEET}));
    place_at_origin(&mut engine, json!({}));
    assert_eq!(
        wires(&mut engine),
        [
            vec![(0, -500), (0, -150)],
            vec![(0, -150), (0, 0)],
            vec![(0, 0), (0, 150)],
            vec![(0, 0), (300, 0)],
            vec![(0, 150), (0, 500)],
        ]
    );
    assert_eq!(net_of_pin_count(&mut engine), 1);
}

#[test]
fn a_net_label_between_the_pins_keeps_the_pieces_too() {
    let dir = TempDir::new("norm-cut-label");
    let mut engine = demo(&dir);
    wire(&mut engine, &[(0, -500), (0, 500)]);
    engine.run_ok(
        "schematic.net_label.add",
        json!({"document": SHEET, "name": "MID", "at": at(0, 50)}),
    );
    place_at_origin(&mut engine, json!({}));
    assert_eq!(wires(&mut engine).len(), 3);
    assert_eq!(net_of_pin_count(&mut engine), 1);
}

#[test]
fn with_components_cut_wires_off_the_pins_connect_and_the_part_is_shorted() {
    let dir = TempDir::new("norm-cut-off");
    let mut engine = demo(&dir);
    wire(&mut engine, &[(0, -500), (0, 500)]);
    place_at_origin(
        &mut engine,
        json!({"components_cut_wires": false, "break_wires_at_autojunctions": false}),
    );
    assert_eq!(wires(&mut engine), [vec![(0, -500), (0, 500)]]);
    assert_eq!(net_of_pin_count(&mut engine), 1);
    // Breaking at the Pins' Junctions splits it, still shorted.
    engine.run_ok("edit.undo", json!({"document": SHEET}));
    place_at_origin(&mut engine, json!({"components_cut_wires": false}));
    assert_eq!(wires(&mut engine).len(), 3);
    assert_eq!(net_of_pin_count(&mut engine), 1);
}

#[test]
fn moving_a_component_away_does_not_heal_the_cut_wire() {
    let dir = TempDir::new("norm-cut-move");
    let mut engine = demo(&dir);
    wire(&mut engine, &[(0, -500), (0, 500)]);
    let out = engine.run_ok(
        "schematic.component.place",
        json!({"document": SHEET, "part": R_10K, "at": at(0, 0)}),
    );
    let unit = out["result"]["unit"].clone();
    engine.run_ok(
        "schematic.move",
        json!({"objects": [unit], "by": at(2000, 0)}),
    );
    assert_eq!(
        wires(&mut engine),
        [vec![(0, -500), (0, -150)], vec![(0, 150), (0, 500)]]
    );
}

#[test]
fn pasting_onto_a_wire_end_merges_and_the_result_names_the_survivor() {
    let dir = TempDir::new("norm-paste");
    let mut engine = demo(&dir);
    let a = wire(&mut engine, &[(0, 0), (500, 0)]);
    let clip = engine.run_ok("schematic.copy", json!({"objects": a}));
    let out = engine.run_ok(
        "schematic.paste",
        json!({"document": SHEET, "clipboard": clip, "at": at(500, 0)}),
    );
    assert_eq!(wires(&mut engine), [vec![(0, 0), (1000, 0)]]);
    assert_eq!(out["result"]["objects"], json!(a));
}

#[test]
fn dragging_a_wire_end_onto_another_wire_splits_it() {
    let dir = TempDir::new("norm-drag");
    let mut engine = demo(&dir);
    wire(&mut engine, &[(0, 0), (1000, 0)]);
    let stem = wire(&mut engine, &[(500, 200), (500, 500)]);
    assert_eq!(wires(&mut engine).len(), 2);
    engine.run_ok(
        "schematic.move",
        json!({"objects": stem, "by": at(0, -200)}),
    );
    assert_eq!(wires(&mut engine).len(), 3);
    assert_eq!(junctions(&mut engine), json!([point(500, 0)]));
}

#[test]
fn normalising_touches_only_the_wires_involved() {
    let dir = TempDir::new("norm-local");
    let mut engine = demo(&dir);
    let off = json!({"optimize_wires": false, "break_wires_at_autojunctions": false});
    // An untidy drawing far away: collinear Wires end to end and a T through one Wire.
    wire_with(&mut engine, &[(0, 2000), (500, 2000)], off.clone());
    wire_with(&mut engine, &[(500, 2000), (1000, 2000)], off.clone());
    wire_with(&mut engine, &[(0, 3000), (1000, 3000)], off.clone());
    wire_with(&mut engine, &[(500, 3000), (500, 3500)], off);
    let untidy = wires(&mut engine);
    wire(&mut engine, &[(0, 0), (500, 0)]);
    let mut expected = untidy;
    expected.push(vec![(0, 0), (500, 0)]);
    expected.sort();
    assert_eq!(wires(&mut engine), expected);
}

#[test]
fn the_wiring_parameter_defaults_are_fixed_and_fields_left_out_take_them() {
    let dir = TempDir::new("norm-defaults");
    let mut engine = demo(&dir);
    // A partial `wiring` keeps the other defaults: here Optimize Wires stays on.
    wire_with(
        &mut engine,
        &[(0, 0), (500, 0)],
        json!({"convert_cross_junctions": false}),
    );
    wire_with(
        &mut engine,
        &[(500, 0), (1000, 0)],
        json!({"convert_cross_junctions": false}),
    );
    assert_eq!(wires(&mut engine), [vec![(0, 0), (1000, 0)]]);
}

#[test]
fn break_wire_cuts_a_gap_and_the_first_piece_keeps_the_uuid() {
    let dir = TempDir::new("break-gap");
    let mut engine = demo(&dir);
    let run = wire(&mut engine, &[(0, 0), (1000, 0), (1000, 1000)]);
    let out = engine.run_ok(
        "schematic.wire.break",
        json!({"wire": run[0], "at": at(300, 10), "length": "200mil"}),
    );
    let pieces = out["result"]["wires"].as_array().unwrap().clone();
    assert_eq!(pieces.len(), 2);
    assert_eq!(pieces[0], run[0]);
    assert_eq!(
        vertices(&mut engine, &pieces[0]),
        json!([point(0, 0), point(200, 0)])
    );
    assert_eq!(
        vertices(&mut engine, &pieces[1]),
        json!([point(400, 0), point(1000, 0), point(1000, 1000)])
    );
    // One undo step.
    engine.run_ok("edit.undo", json!({"document": SHEET}));
    assert_eq!(wires(&mut engine), [vec![(0, 0), (1000, 0), (1000, 1000)]]);
}

#[test]
fn break_wire_by_default_takes_the_whole_segment() {
    let dir = TempDir::new("break-segment");
    let mut engine = demo(&dir);
    let run = wire(&mut engine, &[(0, 0), (1000, 0), (1000, 1000)]);
    let out = engine.run_ok(
        "schematic.wire.break",
        json!({"wire": run[0], "at": at(500, 0)}),
    );
    // Only the piece after the gap is left, and it keeps the UUID.
    assert_eq!(out["result"]["wires"], json!([run[0]]));
    assert_eq!(wires(&mut engine), [vec![(1000, 0), (1000, 1000)]]);
}

#[test]
fn the_cutter_shrinks_to_a_shorter_segment_and_slides_to_fit_near_an_end() {
    let dir = TempDir::new("break-fit");
    let mut engine = demo(&dir);
    let run = wire(&mut engine, &[(0, 0), (1000, 0)]);
    engine.run_ok(
        "schematic.wire.break",
        json!({"wire": run[0], "at": at(50, 0), "length": "200mil"}),
    );
    assert_eq!(wires(&mut engine), [vec![(200, 0), (1000, 0)]]);
    let out = engine.run_ok(
        "schematic.wire.break",
        json!({"wire": run[0], "at": at(600, 0), "length": "5000mil"}),
    );
    assert_eq!(out["result"]["wires"], json!([]));
    assert_eq!(wires(&mut engine), Vec::<Vec<(i64, i64)>>::new());
}

#[test]
fn break_wire_refuses_what_is_not_a_wire_and_a_gap_of_no_length() {
    let dir = TempDir::new("break-bad");
    let mut engine = demo(&dir);
    let run = wire(&mut engine, &[(0, 0), (1000, 0)]);
    let label = engine.run_ok(
        "schematic.net_label.add",
        json!({"document": SHEET, "name": "A", "at": at(0, 0)}),
    )["result"]["net_label"]
        .clone();
    let e = engine.run_err(
        "schematic.wire.break",
        json!({"wire": label, "at": at(0, 0)}),
    );
    assert_eq!(e.details.unwrap()["pointer"], "/wire");
    let e = engine.run_err(
        "schematic.wire.break",
        json!({"wire": run[0], "at": at(0, 0), "length": 0}),
    );
    assert_eq!(e.details.unwrap()["pointer"], "/length");
}

/// Draws a 4-way at (500, 0) from four Wire ends, leaving it as drawn.
fn four_way(engine: &mut TestEngine) -> Vec<Value> {
    let off = json!({"convert_cross_junctions": false, "optimize_wires": false});
    [
        [(0, 0), (500, 0)],
        [(500, 0), (1000, 0)],
        [(500, 0), (500, 500)],
        [(500, 0), (500, -500)],
    ]
    .iter()
    .flat_map(|run| wire_with(engine, run, off.clone()))
    .collect()
}

#[test]
fn convert_cross_junctions_turns_each_4_way_on_the_sheet_into_two_3_ways() {
    let dir = TempDir::new("convert-sheet");
    let mut engine = demo(&dir);
    four_way(&mut engine);
    assert_eq!(junctions(&mut engine), json!([point(500, 0)]));
    let out = engine.run_ok(
        "schematic.junctions.convert_cross",
        json!({"document": SHEET}),
    );
    assert_eq!(
        out["result"]["converted"],
        json!([{"document": SHEET, "at": point(500, 0)}])
    );
    // The lowest UUID's Wire, arriving from the left, moves up the vertical one.
    assert_eq!(
        junctions(&mut engine),
        json!([point(500, 0), point(500, 50)])
    );
    assert_eq!(net_count(&mut engine), 1);
    // Nothing is left to convert.
    let out = engine.run_ok("schematic.junctions.convert_cross", json!({}));
    assert_eq!(out["result"]["converted"], json!([]));
}

#[test]
fn convert_cross_junctions_takes_a_miter_size() {
    let dir = TempDir::new("convert-miter");
    let mut engine = demo(&dir);
    four_way(&mut engine);
    engine.run_ok(
        "schematic.junctions.convert_cross",
        json!({"miter": "100mil"}),
    );
    assert_eq!(
        junctions(&mut engine),
        json!([point(500, 0), point(500, 100)])
    );
}

#[test]
fn with_selected_objects_every_wire_entering_the_4_way_must_be_selected() {
    let dir = TempDir::new("convert-selected");
    let mut engine = demo(&dir);
    let all = four_way(&mut engine);
    let out = engine.run_ok(
        "schematic.junctions.convert_cross",
        json!({"document": SHEET, "objects": all[..3]}),
    );
    assert_eq!(out["result"]["converted"], json!([]));
    let out = engine.run_ok(
        "schematic.junctions.convert_cross",
        json!({"document": SHEET, "objects": all}),
    );
    assert_eq!(out["result"]["converted"].as_array().unwrap().len(), 1);
}

#[test]
fn loading_leaves_wires_that_are_not_normalised_as_they_are() {
    let dir = TempDir::new("norm-load");
    let mut engine = demo(&dir);
    four_way(&mut engine);
    let drawn = wires(&mut engine);
    engine.run_ok("project.save", json!({}));
    let mut other = test_engine_with_library(&library());
    other.run_ok("project.open", json!({"path": dir.path().join("Demo")}));
    assert_eq!(wires(&mut other), drawn);
}

#[test]
fn commands_ignore_the_wiring_preferences_so_every_machine_gets_the_same_result() {
    let home = TempDir::new("norm-prefs-home");
    let dir = TempDir::new("norm-prefs");
    let mut engine = reda_testkit::test_engine_in(home.path());
    engine.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
    engine.run_ok(
        "app.preferences.set",
        json!({"key": "schematic.optimize_wires", "value": false}),
    );
    let prefs = engine.run_ok(
        "app.preferences.get",
        json!({"key": "schematic.optimize_wires"}),
    );
    assert_eq!(prefs["value"], false, "{prefs}");
    let points = |run: &[(i64, i64)]| run.iter().map(|&(x, y)| at(x, y)).collect::<Vec<_>>();
    for run in [[(0, 0), (500, 0)], [(500, 0), (1000, 0)]] {
        engine.run_ok(
            "schematic.wire.add",
            json!({"document": SHEET, "points": points(&run)}),
        );
    }
    assert_eq!(wires(&mut engine), [vec![(0, 0), (1000, 0)]]);
}
