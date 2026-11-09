//! `pcb.board.move`: the whole board moves, the Board Outline with its Board Cutouts, the
//! Output Origin and everything with any part inside the outline, locked or not.

use reda_command::ErrorCode;
use reda_model::geom::{Nm, Point, Vertex};
use reda_model::{GraphicShape, KeepoutShape, PcbDocument, TrackGeometry, Uuid};
use reda_testkit::{TempDir, TestEngine, probe_engine};
use serde_json::{Value, json};

const MM: i64 = 1_000_000;

fn engine_with_board(dir: &TempDir) -> TestEngine {
    let mut engine = probe_engine();
    engine.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
    // A 50 × 50 mm board at the origin.
    engine.run_ok(
        "pcb.board_outline.add",
        json!({"vertices": poly(&[(0, 0), (50, 0), (50, 50), (0, 50)])}),
    );
    engine
}

fn pcb(engine: &TestEngine) -> &PcbDocument {
    engine.project().unwrap().pcb().unwrap()
}

fn created(out: &Value) -> Uuid {
    out["result"]["uuid"].as_str().unwrap().parse().unwrap()
}

fn poly(points: &[(i64, i64)]) -> Value {
    points
        .iter()
        .map(|&(x, y)| json!({"x": x * MM, "y": y * MM}))
        .collect()
}

fn mm(x: i64, y: i64) -> Point {
    Point::new(Nm(x * MM), Nm(y * MM))
}

fn square(x: i64, y: i64, side: i64) -> Vec<Vertex> {
    [(x, y), (x + side, y), (x + side, y + side), (x, y + side)]
        .into_iter()
        .map(|(x, y)| Vertex::at(mm(x, y)))
        .collect()
}

fn footprint(engine: &mut TestEngine, x: i64, y: i64) -> Uuid {
    let out = engine.run_ok(
        "test.footprint.add",
        json!({"at": {"x": x * MM, "y": y * MM}, "pads": [
            {"at": {"x": 0, "y": 0}, "width": "1mm", "height": "1mm"}]}),
    );
    out["result"]["footprint"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap()
}

fn track(engine: &mut TestEngine, from: (i64, i64), to: (i64, i64)) -> Uuid {
    let out = engine.run_ok(
        "pcb.track.add",
        json!({"layer": "copper_top", "width": "0.25mm", "points": poly(&[from, to])}),
    );
    out["result"]["tracks"][0]
        .as_str()
        .unwrap()
        .parse()
        .unwrap()
}

fn line(from: Point, to: Point) -> TrackGeometry {
    TrackGeometry::Line { from, to }
}

#[test]
fn the_board_moves_with_everything_partly_inside_it() {
    let dir = TempDir::new("board-move");
    let mut engine = engine_with_board(&dir);
    let cutout = created(&engine.run_ok(
        "pcb.board_cutout.add",
        json!({"vertices": poly(&[(20, 20), (25, 20), (25, 25), (20, 25)])}),
    ));
    engine.run_ok(
        "pcb.output_origin.set",
        json!({"at": {"x": "-5mm", "y": "-5mm"}}),
    );
    let inside = footprint(&mut engine, 10, 10);
    // Its Pad pokes 0.5 mm over the edge at x = 50: partly inside, so it moves.
    let on_edge = footprint(&mut engine, 50, 30);
    // Footprints of an unplaced Sync cluster, wholly outside, stay.
    let outside = footprint(&mut engine, 80, 10);
    let crossing = track(&mut engine, (40, 40), (70, 40));
    let stray = track(&mut engine, (60, 0), (70, 0));
    let via = created(&engine.run_ok("pcb.via.add", json!({"at": {"x": "5mm", "y": "45mm"}})));
    let zone = created(&engine.run_ok(
        "pcb.zone.add",
        json!({"layer": "copper_top", "vertices": poly(&[(30, 5), (40, 5), (40, 15), (30, 15)])}),
    ));
    let zone_cutout = created(&engine.run_ok(
        "pcb.zone_cutout.add",
        json!({"layer": "copper_top", "vertices": poly(&[(31, 6), (33, 6), (33, 8), (31, 8)])}),
    ));
    let keepout = created(&engine.run_ok(
        "pcb.keepout.add",
        json!({"shape": "polygon", "vertices": poly(&[(45, 45), (55, 45), (55, 55), (45, 55)])}),
    ));
    let far_keepout = created(&engine.run_ok(
        "pcb.keepout.add",
        json!({"shape": "polygon", "vertices": poly(&[(60, 60), (70, 60), (70, 70), (60, 70)])}),
    ));
    let text = created(&engine.run_ok(
        "pcb.text.add",
        json!({"layer": "silkscreen_top", "text": "REV A", "x": "2mm", "y": "2mm"}),
    ));
    let graphic = created(&engine.run_ok(
        "pcb.graphic.add_line",
        json!({"layer": "mechanical", "width": "0.2mm",
               "from": {"x": "-10mm", "y": "25mm"}, "to": {"x": "10mm", "y": "25mm"}}),
    ));
    let free_pad = created(&engine.run_ok(
        "pcb.free_pad.add",
        json!({"kind": "smd", "at": {"x": "15mm", "y": "35mm"}, "width": "1mm"}),
    ));
    let dimension = created(&engine.run_ok(
        "pcb.dimension.add",
        json!({"layer": "mechanical", "a": {"point": {"x": "5mm", "y": "48mm"}},
               "b": {"point": {"x": "45mm", "y": "48mm"}}}),
    ));

    let out = engine.run_ok("pcb.board.move", json!({"dx": "100mm", "dy": "-20mm"}));
    let by = |x: i64, y: i64| mm(x + 100, y - 20);

    let pcb = pcb(&engine);
    assert_eq!(pcb.outline().unwrap().vertices, square(100, -20, 50));
    assert_eq!(pcb.cutout(cutout).unwrap().vertices, square(120, 0, 5));
    assert_eq!(pcb.output_origin().unwrap().at, by(-5, -5));
    assert_eq!(pcb.footprint(inside).unwrap().at, by(10, 10));
    assert_eq!(pcb.footprint(on_edge).unwrap().at, by(50, 30));
    assert_eq!(
        pcb.footprint(outside).unwrap().at,
        mm(80, 10),
        "wholly outside stays"
    );
    assert_eq!(
        pcb.track(crossing).unwrap().geometry,
        line(by(40, 40), by(70, 40)),
        "a Track crossing the edge moves whole"
    );
    assert_eq!(
        pcb.track(stray).unwrap().geometry,
        line(mm(60, 0), mm(70, 0))
    );
    assert_eq!(pcb.via(via).unwrap().at, by(5, 45));
    assert_eq!(pcb.zone(zone).unwrap().vertices, square(130, -15, 10));
    assert_eq!(
        pcb.zone_cutout(zone_cutout).unwrap().vertices,
        square(131, -14, 2)
    );
    assert_eq!(
        pcb.keepout(keepout).unwrap().shape,
        KeepoutShape::Polygon {
            vertices: square(145, 25, 10)
        }
    );
    assert_eq!(
        pcb.keepout(far_keepout).unwrap().shape,
        KeepoutShape::Polygon {
            vertices: square(60, 60, 10)
        }
    );
    assert_eq!(pcb.text(text).unwrap().position, by(2, 2));
    assert_eq!(
        pcb.graphic(graphic).unwrap().shape,
        GraphicShape::Line {
            from: by(-10, 25),
            to: by(10, 25)
        }
    );
    assert_eq!(pcb.free_pad(free_pad).unwrap().at, by(15, 35));
    let d = pcb.dimension(dimension).unwrap();
    assert_eq!((d.a.point, d.b.point), (by(5, 48), by(45, 48)));

    let moved: Vec<Uuid> = out["result"]["objects"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().parse().unwrap())
        .collect();
    for stays in [outside, stray, far_keepout] {
        assert!(!moved.contains(&stays), "{stays} is reported as moved");
    }
    assert!(moved.contains(&inside) && moved.contains(&crossing));
}

#[test]
fn locked_footprints_move_with_the_board_and_stay_locked() {
    let dir = TempDir::new("board-move-locked");
    let mut engine = engine_with_board(&dir);
    let fp = footprint(&mut engine, 10, 10);
    engine.run_ok("pcb.lock", json!({"objects": [fp], "locked": true}));
    engine.run_ok("pcb.board.move", json!({"dx": "5mm", "dy": "5mm"}));
    let f = pcb(&engine).footprint(fp).unwrap();
    assert_eq!(f.at, mm(15, 15));
    assert!(f.locked, "the lock is kept");
    // An ordinary move still refuses it.
    let err = engine.run_err(
        "pcb.move",
        json!({"objects": [fp], "by": {"x": "1mm", "y": 0}}),
    );
    assert_eq!(err.code, ErrorCode::Rejected);
}

#[test]
fn a_zone_following_the_outline_keeps_following_it() {
    let dir = TempDir::new("board-move-follow");
    let mut engine = engine_with_board(&dir);
    engine.run_ok(
        "pcb.zone.add_from_board_outline",
        json!({"layers": ["copper_top"]}),
    );
    engine.run_ok("pcb.board.move", json!({"dx": "10mm", "dy": 0}));
    let pcb = pcb(&engine);
    let zone = pcb.zones().next().unwrap();
    assert!(zone.follows_outline);
    assert_eq!(pcb.zone_vertices(zone), square(10, 0, 50).as_slice());
}

#[test]
fn a_board_with_no_outline_cannot_move() {
    let dir = TempDir::new("board-move-none");
    let mut engine = engine_with_board(&dir);
    engine.run_ok("pcb.board_outline.delete", json!({}));
    let err = engine.run_err("pcb.board.move", json!({"dx": "1mm", "dy": 0}));
    assert_eq!(err.code, ErrorCode::NotFound);
}

#[test]
fn moving_the_board_is_one_undo_step() {
    let dir = TempDir::new("board-move-undo");
    let mut engine = engine_with_board(&dir);
    let fp = footprint(&mut engine, 10, 10);
    engine.run_ok("pcb.board.move", json!({"dx": "1mm", "dy": "2mm"}));
    let doc = pcb(&engine).uuid.to_string();
    engine.run_ok("edit.undo", json!({"document": doc}));
    assert_eq!(pcb(&engine).footprint(fp).unwrap().at, mm(10, 10));
    assert_eq!(pcb(&engine).outline().unwrap().vertices, square(0, 0, 50));
}

#[test]
fn move_board_shape_moves_the_outline_alone() {
    let dir = TempDir::new("board-shape-move");
    let mut engine = engine_with_board(&dir);
    let cutout = created(&engine.run_ok(
        "pcb.board_cutout.add",
        json!({"vertices": poly(&[(20, 20), (25, 20), (25, 25), (20, 25)])}),
    ));
    let fp = footprint(&mut engine, 10, 10);
    let board = pcb(&engine).outline().unwrap().uuid;
    engine.run_ok(
        "pcb.move",
        json!({"objects": [board], "by": {"x": "3mm", "y": "4mm"}}),
    );
    let pcb_now = pcb(&engine);
    assert_eq!(pcb_now.outline().unwrap().vertices, square(3, 4, 50));
    assert_eq!(
        pcb_now.cutout(cutout).unwrap().vertices,
        square(20, 20, 5),
        "Cutouts stay"
    );
    assert_eq!(pcb_now.footprint(fp).unwrap().at, mm(10, 10));
    // Selected too, a Cutout goes with it.
    engine.run_ok(
        "pcb.move",
        json!({"objects": [board, cutout], "by": {"x": "-3mm", "y": "-4mm"}}),
    );
    assert_eq!(pcb(&engine).outline().unwrap().vertices, square(0, 0, 50));
    assert_eq!(
        pcb(&engine).cutout(cutout).unwrap().vertices,
        square(17, 16, 5)
    );
}

#[test]
fn the_board_shape_turns_and_mirrors_like_any_move() {
    let dir = TempDir::new("board-shape-turn");
    let mut engine = engine_with_board(&dir);
    // A 20 × 10 board with one rounded corner: an arc from (20, 5) to (15, 10).
    engine.run_ok(
        "pcb.board_outline.add",
        json!({"vertices": [
            {"x": 0, "y": 0}, {"x": "20mm", "y": 0},
            {"x": "20mm", "y": "5mm", "sweep": 90000}, {"x": "15mm", "y": "10mm"},
            {"x": 0, "y": "10mm"}
        ]}),
    );
    let board = pcb(&engine).outline().unwrap().uuid;
    // Alone, it turns about the middle of its box, (10, 5).
    engine.run_ok("pcb.rotate", json!({"objects": [board]}));
    let at = |x: i64, y: i64| Vertex::at(mm(x, y));
    let arc = |x: i64, y: i64| Vertex::arc(mm(x, y), reda_model::geom::Millideg::from_deg(90));
    assert_eq!(
        pcb(&engine).outline().unwrap().vertices,
        vec![at(15, -5), at(15, 15), arc(10, 15), at(5, 10), at(5, -5)]
    );
    // Mirrored left-right about x = 10, the arc turns the other way.
    engine.run_ok("pcb.flip", json!({"objects": [board]}));
    let cw = |x: i64, y: i64| Vertex::arc(mm(x, y), reda_model::geom::Millideg::from_deg(-90));
    assert_eq!(
        pcb(&engine).outline().unwrap().vertices,
        vec![at(5, -5), at(5, 15), cw(10, 15), at(15, 10), at(15, -5)]
    );
}
