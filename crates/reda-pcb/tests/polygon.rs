//! The polygon Commands (`pcb.polygon.set_vertices`, `fillet`, `chamfer`) on every polygon
//! object, and their self-intersection guard.

use reda_command::ErrorCode;
use reda_model::geom::{Millideg, Nm, Point, Vertex};
use reda_model::{KeepoutShape, PcbDocument, Uuid};
use reda_testkit::{TempDir, TestEngine, probe_engine};
use serde_json::{Value, json};

const MM: i64 = 1_000_000;

fn engine_with_pcb(dir: &TempDir) -> TestEngine {
    let mut engine = probe_engine();
    engine.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
    engine
}

fn pcb(engine: &TestEngine) -> &PcbDocument {
    engine.project().unwrap().pcb().unwrap()
}

fn created(out: &Value) -> Uuid {
    out["result"]["uuid"].as_str().unwrap().parse().unwrap()
}

/// Vertices params from (x, y) pairs in mm, all straight.
fn poly(points: &[(i64, i64)]) -> Value {
    points
        .iter()
        .map(|&(x, y)| json!({"x": x * MM, "y": y * MM}))
        .collect()
}

fn v(x: i64, y: i64) -> Vertex {
    Vertex::at(Point::new(Nm(x), Nm(y)))
}

fn arc(x: i64, y: i64, deg: i64) -> Vertex {
    Vertex::arc(Point::new(Nm(x), Nm(y)), Millideg::from_deg(deg))
}

fn outline(engine: &TestEngine) -> (Uuid, Vec<Vertex>) {
    let o = pcb(engine).outline().unwrap();
    (o.uuid, o.vertices.clone())
}

/// A 10 × 10 mm square Board Outline at the origin.
fn square_outline(engine: &mut TestEngine) -> Uuid {
    engine.run_ok(
        "pcb.board_outline.add",
        json!({"vertices": poly(&[(0, 0), (10, 0), (10, 10), (0, 10)])}),
    );
    outline(engine).0
}

#[test]
fn set_vertices_replaces_the_board_outline_shape() {
    let dir = TempDir::new("polygon");
    let mut engine = engine_with_pcb(&dir);
    let board = square_outline(&mut engine);
    engine.run_ok(
        "pcb.polygon.set_vertices",
        json!({"object": board, "vertices": poly(&[(0, 0), (20, 0), (20, 5), (0, 5)])}),
    );
    let (uuid, vertices) = outline(&engine);
    assert_eq!(uuid, board, "the same Board Outline, edited in place");
    assert_eq!(
        vertices,
        vec![v(0, 0), v(20 * MM, 0), v(20 * MM, 5 * MM), v(0, 5 * MM)]
    );
}

#[test]
fn set_vertices_rejects_a_self_intersecting_shape_with_the_crossing() {
    let dir = TempDir::new("polygon");
    let mut engine = engine_with_pcb(&dir);
    let board = square_outline(&mut engine);
    // A bow tie: the edges (0,0)-(10,10) and (10,0)-(0,10) cross at (5, 5).
    let err = engine.run_err(
        "pcb.polygon.set_vertices",
        json!({"object": board, "vertices": poly(&[(0, 0), (10, 10), (10, 0), (0, 10)])}),
    );
    assert_eq!(err.code, ErrorCode::Rejected);
    assert_eq!(
        err.details.as_ref().unwrap()["crossing"],
        json!({"x": 5 * MM, "y": 5 * MM})
    );
    assert!(err.message.contains("crosses itself"), "{}", err.message);
    assert_eq!(outline(&engine).1.len(), 4, "the outline is unchanged");
}

#[test]
fn fillet_rounds_every_corner_of_a_finished_rectangle() {
    let dir = TempDir::new("polygon");
    let mut engine = engine_with_pcb(&dir);
    engine.run_ok(
        "pcb.board_outline.add_rect",
        json!({"width": "30mm", "height": "20mm"}),
    );
    let (board, _) = outline(&engine);
    engine.run_ok(
        "pcb.polygon.fillet",
        json!({"object": board, "radius": "2mm"}),
    );
    let m = MM;
    // Each corner becomes a quarter arc between tangent points 2 mm along its edges.
    assert_eq!(
        outline(&engine).1,
        vec![
            arc(0, 2 * m, 90),
            v(2 * m, 0),
            arc(28 * m, 0, 90),
            v(30 * m, 2 * m),
            arc(30 * m, 18 * m, 90),
            v(28 * m, 20 * m),
            arc(2 * m, 20 * m, 90),
            v(0, 18 * m),
        ]
    );
}

#[test]
fn chamfer_bevels_only_the_named_corners() {
    let dir = TempDir::new("polygon");
    let mut engine = engine_with_pcb(&dir);
    let board = square_outline(&mut engine);
    engine.run_ok(
        "pcb.polygon.chamfer",
        json!({"object": board, "vertices": [2], "distance": "1mm"}),
    );
    let m = MM;
    assert_eq!(
        outline(&engine).1,
        vec![
            v(0, 0),
            v(10 * m, 0),
            v(10 * m, 9 * m),
            v(9 * m, 10 * m),
            v(0, 10 * m)
        ]
    );
}

#[test]
fn a_clockwise_corner_fillets_with_a_clockwise_arc() {
    let dir = TempDir::new("polygon");
    let mut engine = engine_with_pcb(&dir);
    let board = square_outline(&mut engine);
    // Clockwise: (0,0) → (0,10) → (10,10) → (10,0). Vertex 1 turns right.
    engine.run_ok(
        "pcb.polygon.set_vertices",
        json!({"object": board, "vertices": poly(&[(0, 0), (0, 10), (10, 10), (10, 0)])}),
    );
    engine.run_ok(
        "pcb.polygon.fillet",
        json!({"object": board, "vertices": [1], "radius": "3mm"}),
    );
    let m = MM;
    assert_eq!(
        outline(&engine).1,
        vec![
            v(0, 0),
            arc(0, 7 * m, -90),
            v(3 * m, 10 * m),
            v(10 * m, 10 * m),
            v(10 * m, 0)
        ]
    );
}

#[test]
fn corners_that_use_up_a_whole_edge_share_its_end() {
    let dir = TempDir::new("polygon");
    let mut engine = engine_with_pcb(&dir);
    engine.run_ok(
        "pcb.board_outline.add_rect",
        json!({"width": "10mm", "height": "4mm"}),
    );
    let (board, _) = outline(&engine);
    engine.run_ok(
        "pcb.polygon.fillet",
        json!({"object": board, "radius": "2mm"}),
    );
    // The short sides are all arc: a stadium of two half-circles and two straight sides.
    let m = MM;
    assert_eq!(
        outline(&engine).1,
        vec![
            arc(0, 2 * m, 90),
            v(2 * m, 0),
            arc(8 * m, 0, 90),
            arc(10 * m, 2 * m, 90),
            v(8 * m, 4 * m),
            arc(2 * m, 4 * m, 90),
        ]
    );
}

#[test]
fn a_radius_too_large_for_the_edges_is_refused() {
    let dir = TempDir::new("polygon");
    let mut engine = engine_with_pcb(&dir);
    let board = square_outline(&mut engine);
    let err = engine.run_err(
        "pcb.polygon.fillet",
        json!({"object": board, "radius": "6mm"}),
    );
    assert_eq!(err.code, ErrorCode::Rejected);
    assert!(err.message.contains("too short"), "{}", err.message);
    let err = engine.run_err(
        "pcb.polygon.chamfer",
        json!({"object": board, "vertices": [9], "distance": "1mm"}),
    );
    assert_eq!(err.code, ErrorCode::InvalidParams);
    assert_eq!(outline(&engine).1.len(), 4);
}

#[test]
fn a_chamfer_that_makes_the_shape_cross_itself_is_refused() {
    let dir = TempDir::new("polygon");
    let mut engine = engine_with_pcb(&dir);
    let board = square_outline(&mut engine);
    // A V notch from the top edge whose tip (9.5, 9) reaches into the top right corner.
    engine.run_ok(
        "pcb.polygon.set_vertices",
        json!({"object": board, "vertices": [
            {"x": 0, "y": 0}, {"x": "10mm", "y": 0}, {"x": "10mm", "y": "10mm"},
            {"x": "8mm", "y": "10mm"}, {"x": "9.5mm", "y": "9mm"},
            {"x": "7mm", "y": "10mm"}, {"x": 0, "y": "10mm"}
        ]}),
    );
    // A 1.9 mm bevel at the corner fits its edges but cuts across the notch's walls.
    let err = engine.run_err(
        "pcb.polygon.chamfer",
        json!({"object": board, "vertices": [2], "distance": "1.9mm"}),
    );
    assert_eq!(err.code, ErrorCode::Rejected);
    let at = &err.details.as_ref().unwrap()["crossing"];
    let (x, y) = (at["x"].as_i64().unwrap(), at["y"].as_i64().unwrap());
    assert_eq!(
        x + y,
        18_100_000,
        "the crossing lies on the bevel: ({x}, {y})"
    );
    assert_eq!(outline(&engine).1.len(), 7, "the outline is unchanged");
}

#[test]
fn every_polygon_object_takes_the_polygon_commands() {
    let dir = TempDir::new("polygon");
    let mut engine = engine_with_pcb(&dir);
    engine.run_ok(
        "pcb.board_outline.add",
        json!({"vertices": poly(&[(0, 0), (50, 0), (50, 50), (0, 50)])}),
    );
    let cutout = created(&engine.run_ok(
        "pcb.board_cutout.add",
        json!({"vertices": poly(&[(1, 1), (5, 1), (5, 5), (1, 5)])}),
    ));
    let zone = created(&engine.run_ok(
        "pcb.zone.add",
        json!({"layer": "copper_top", "vertices": poly(&[(10, 10), (20, 10), (20, 20), (10, 20)])}),
    ));
    let zone_cutout = created(&engine.run_ok(
        "pcb.zone_cutout.add",
        json!({"layer": "copper_top", "vertices": poly(&[(12, 12), (14, 12), (14, 14), (12, 14)])}),
    ));
    let keepout = created(&engine.run_ok(
        "pcb.keepout.add",
        json!({"shape": "polygon", "vertices": poly(&[(30, 30), (40, 30), (40, 40), (30, 40)])}),
    ));
    let square = |x: i64| {
        vec![
            v(x, x),
            v(x + 4 * MM, x),
            v(x + 4 * MM, x + 4 * MM),
            v(x, x + 4 * MM),
        ]
    };
    for (object, x) in [(cutout, 1), (zone, 10), (zone_cutout, 12), (keepout, 30)] {
        engine.run_ok(
            "pcb.polygon.set_vertices",
            json!({"object": object, "vertices": poly(&[(x, x), (x + 4, x), (x + 4, x + 4), (x, x + 4)])}),
        );
        engine.run_ok(
            "pcb.polygon.chamfer",
            json!({"object": object, "vertices": [0], "distance": "1mm"}),
        );
        engine.run_ok(
            "pcb.polygon.fillet",
            json!({"object": object, "vertices": [3], "radius": "1mm"}),
        );
        let mut want = square(x * MM);
        want.remove(0);
        let m = MM;
        let x = x * m;
        let mut expected = vec![v(x, x + m), v(x + m, x)];
        expected.push(want[0]);
        // Vertex 3 is now (x + 4, x + 4); its fillet's tangents lie 1 mm along each edge.
        expected.push(arc(x + 4 * m, x + 3 * m, 90));
        expected.push(v(x + 3 * m, x + 4 * m));
        expected.push(v(x, x + 4 * m));
        let pcb = pcb(&engine);
        let got = if object == cutout {
            pcb.cutout(object).unwrap().vertices.clone()
        } else if object == zone {
            pcb.zone(object).unwrap().vertices.clone()
        } else if object == zone_cutout {
            pcb.zone_cutout(object).unwrap().vertices.clone()
        } else {
            match &pcb.keepout(object).unwrap().shape {
                KeepoutShape::Polygon { vertices } => vertices.clone(),
                other => panic!("{other:?}"),
            }
        };
        assert_eq!(got, expected, "object {object}");
    }
}

#[test]
fn line_keepouts_and_zones_following_the_outline_are_refused() {
    let dir = TempDir::new("polygon");
    let mut engine = engine_with_pcb(&dir);
    square_outline(&mut engine);
    let line = created(&engine.run_ok(
        "pcb.keepout.add",
        json!({"shape": "line", "width": "1mm", "vertices": poly(&[(1, 1), (5, 1)])}),
    ));
    let err = engine.run_err(
        "pcb.polygon.fillet",
        json!({"object": line, "radius": "1mm"}),
    );
    assert_eq!(err.code, ErrorCode::Rejected);
    engine.run_ok(
        "pcb.zone.add_from_board_outline",
        json!({"layers": ["copper_top"]}),
    );
    let following = pcb(&engine)
        .zones()
        .find(|z| z.follows_outline)
        .unwrap()
        .uuid;
    let err = engine.run_err(
        "pcb.polygon.chamfer",
        json!({"object": following, "distance": "1mm"}),
    );
    assert_eq!(err.code, ErrorCode::Rejected);
    let stranger = Uuid::from_u128(0xdead);
    let err = engine.run_err(
        "pcb.polygon.set_vertices",
        json!({"object": stranger, "vertices": poly(&[(0, 0), (1, 0), (1, 1)])}),
    );
    assert_eq!(err.code, ErrorCode::InvalidParams);
}

#[test]
fn adding_a_self_crossing_polygon_is_refused_too() {
    let dir = TempDir::new("polygon");
    let mut engine = engine_with_pcb(&dir);
    let bow_tie = poly(&[(0, 0), (10, 10), (10, 0), (0, 10)]);
    for (id, extra) in [
        ("pcb.board_outline.add", json!({})),
        ("pcb.board_cutout.add", json!({})),
        ("pcb.zone.add", json!({"layer": "copper_top"})),
        ("pcb.zone_cutout.add", json!({"layer": "copper_top"})),
        ("pcb.keepout.add", json!({"shape": "polygon"})),
    ] {
        let mut params = extra;
        params["vertices"] = bow_tie.clone();
        let err = engine.run_err(id, params);
        assert_eq!(err.code, ErrorCode::Rejected, "{id}");
        assert_eq!(
            err.details.as_ref().unwrap()["crossing"],
            json!({"x": 5 * MM, "y": 5 * MM}),
            "{id}"
        );
    }
}

#[test]
fn a_loop_from_graphics_that_crosses_itself_is_refused() {
    let dir = TempDir::new("polygon");
    let mut engine = engine_with_pcb(&dir);
    // A bow tie of four lines whose ends all meet, but whose diagonals cross at (5, 5).
    let corners = [(0, 0), (10, 10), (10, 0), (0, 10)];
    let graphics: Vec<Uuid> = (0..4)
        .map(|i| {
            let (a, b) = (corners[i], corners[(i + 1) % 4]);
            created(&engine.run_ok(
                "pcb.graphic.add_line",
                json!({"layer": "mechanical", "width": 200_000,
                       "from": {"x": a.0 * MM, "y": a.1 * MM},
                       "to": {"x": b.0 * MM, "y": b.1 * MM}}),
            ))
        })
        .collect();
    let err = engine.run_err(
        "pcb.board_outline.add_from_graphics",
        json!({"graphics": graphics}),
    );
    assert_eq!(err.code, ErrorCode::Rejected);
    assert_eq!(
        err.details.as_ref().unwrap()["crossing"],
        json!({"x": 5 * MM, "y": 5 * MM})
    );
    assert_eq!(pcb(&engine).graphics().count(), 4, "the graphics stay");
}
