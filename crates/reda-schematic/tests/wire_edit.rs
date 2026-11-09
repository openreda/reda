//! Editing a placed Wire: `schematic.wire.set_vertices` (handle drags and the vertex list
//! in Properties) and deleting single segments with `schematic.delete`.

use std::path::{Path, PathBuf};

use reda_command::ErrorCode;
use reda_testkit::{TempDir, TestEngine, test_engine_with_library};
use serde_json::{Value, json};

const SHEET: &str = "00000000-0000-4000-8000-000000000002";

fn library() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/library/Test")
}

fn demo(dir: &TempDir) -> TestEngine {
    let mut engine = test_engine_with_library(&library());
    engine.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
    engine
}

fn at(x: i64, y: i64) -> Value {
    json!({"x": format!("{x}mil"), "y": format!("{y}mil")})
}

fn wire(engine: &mut TestEngine, points: &[(i64, i64)]) -> Vec<Value> {
    let points: Vec<Value> = points.iter().map(|&(x, y)| at(x, y)).collect();
    let out = engine.run_ok(
        "schematic.wire.add",
        json!({"document": SHEET, "points": points}),
    );
    out["result"]["wires"].as_array().unwrap().clone()
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

fn junctions(engine: &mut TestEngine) -> usize {
    let out = engine.run_ok("schematic.nets", json!({"document": SHEET}));
    out["junctions"].as_array().unwrap().len()
}

#[test]
fn set_vertices_replaces_a_wires_vertices_in_one_undo_step() {
    let dir = TempDir::new("wire-set-vertices");
    let mut engine = demo(&dir);
    let run = wire(&mut engine, &[(0, 0), (1000, 0), (1000, 1000)]);
    let out = engine.run_ok(
        "schematic.wire.set_vertices",
        json!({"wire": run[0], "vertices": [at(0, 0), at(500, 0), at(500, 500), at(1000, 500)]}),
    );
    assert_eq!(out["result"]["wires"], json!([run[0]]));
    assert_eq!(
        wires(&mut engine),
        [vec![(0, 0), (500, 0), (500, 500), (1000, 500)]]
    );
    engine.run_ok("edit.undo", json!({"document": SHEET}));
    assert_eq!(wires(&mut engine), [vec![(0, 0), (1000, 0), (1000, 1000)]]);
}

#[test]
fn set_vertices_normalises_and_refuses_fewer_than_two_points() {
    let dir = TempDir::new("wire-set-vertices-normalise");
    let mut engine = demo(&dir);
    let run = wire(&mut engine, &[(0, 0), (1000, 0), (1000, 1000)]);
    // A collinear vertex goes, with Optimize Wires on.
    engine.run_ok(
        "schematic.wire.set_vertices",
        json!({"wire": run[0], "vertices": [at(0, 0), at(500, 0), at(1000, 0)]}),
    );
    assert_eq!(wires(&mut engine), [vec![(0, 0), (1000, 0)]]);
    let refused = engine.run_err(
        "schematic.wire.set_vertices",
        json!({"wire": run[0], "vertices": [at(0, 0), at(0, 0)]}),
    );
    assert_eq!(refused.code, ErrorCode::InvalidParams);
}

#[test]
fn deleting_a_segment_splits_the_wire_and_the_first_piece_keeps_the_uuid() {
    let dir = TempDir::new("wire-delete-segment");
    let mut engine = demo(&dir);
    let run = wire(
        &mut engine,
        &[(0, 0), (1000, 0), (1000, 1000), (2000, 1000)],
    );
    let out = engine.run_ok(
        "schematic.delete",
        json!({"objects": [], "segments": [{"wire": run[0], "index": 1}]}),
    );
    assert_eq!(out["result"]["deleted"], json!([]));
    assert_eq!(
        wires(&mut engine),
        [vec![(0, 0), (1000, 0)], vec![(1000, 1000), (2000, 1000)]]
    );
    let first = engine.run_ok("object.get", json!({"uuid": run[0]}));
    assert_eq!(first["object"]["vertices"][1]["x"], 1000 * 25_400);
}

#[test]
fn a_segment_deletion_stops_at_the_junction_and_takes_it_when_two_branches_are_left() {
    let dir = TempDir::new("wire-delete-at-junction");
    let mut engine = demo(&dir);
    // A T at (500, 0): three Wires meet there with a Junction.
    wire(&mut engine, &[(0, 0), (1000, 0)]);
    let branch = wire(&mut engine, &[(500, 0), (500, 500), (800, 500)]);
    assert_eq!(junctions(&mut engine), 1);
    // The branch's first segment goes, up to the Junction; its second stays.
    engine.run_ok(
        "schematic.delete",
        json!({"objects": [], "segments": [{"wire": branch[0], "index": 0}]}),
    );
    // Two branches are left at (500, 0): the Junction goes and they merge again.
    assert_eq!(junctions(&mut engine), 0);
    assert_eq!(
        wires(&mut engine),
        [vec![(0, 0), (1000, 0)], vec![(500, 500), (800, 500)]]
    );
}

#[test]
fn deleting_the_only_segment_removes_the_wire() {
    let dir = TempDir::new("wire-delete-only-segment");
    let mut engine = demo(&dir);
    let run = wire(&mut engine, &[(0, 0), (1000, 0)]);
    let out = engine.run_ok(
        "schematic.delete",
        json!({"objects": [], "segments": [{"wire": run[0], "index": 0}]}),
    );
    assert_eq!(out["result"]["deleted"], json!([run[0]]));
    assert!(wires(&mut engine).is_empty());
}

#[test]
fn a_segment_through_a_junction_is_deleted_only_up_to_the_junction() {
    let dir = TempDir::new("wire-delete-through-junction");
    let mut engine = demo(&dir);
    let keep_whole = json!({"break_wires_at_autojunctions": false});
    let add = |engine: &mut TestEngine, points: &[(i64, i64)]| {
        let points: Vec<Value> = points.iter().map(|&(x, y)| at(x, y)).collect();
        let out = engine.run_ok(
            "schematic.wire.add",
            json!({"document": SHEET, "points": points, "wiring": keep_whole}),
        );
        out["result"]["wires"][0].clone()
    };
    let main = add(&mut engine, &[(0, 0), (1000, 0)]);
    add(&mut engine, &[(300, 0), (300, 500)]);
    assert_eq!(junctions(&mut engine), 1);
    // The midpoint (500, 0) is past the Junction at (300, 0): the stretch from it to the
    // far end goes, and the stretch before it stays.
    engine.run_ok(
        "schematic.delete",
        json!({"objects": [], "segments": [{"wire": main, "index": 0}], "wiring": keep_whole}),
    );
    assert_eq!(
        wires(&mut engine),
        [vec![(0, 0), (300, 0)], vec![(300, 0), (300, 500)]]
    );
}
