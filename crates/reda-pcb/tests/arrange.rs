//! Arranging Footprints: align, distribute, Arrange within rectangle, Arrange Outside
//! Board and Swap.
//!
//! The 0603 resistor Footprint's extent (courtyard) is 3 mm × 1.5 mm about its origin.

mod common;

use common::{at, engine, footprint, synced};
use reda_model::Side;
use reda_model::geom::Millideg;
use reda_command::ErrorCode;
use reda_model::geom::{Nm, Point};
use reda_testkit::TempDir;
use serde_json::json;

fn um(x: i64, y: i64) -> Point {
    Point::new(Nm::from_um(x), Nm::from_um(y))
}

#[test]
fn align_lines_up_edges_and_centres() {
    let dir = TempDir::new("arrange-align");
    let mut engine = engine(&dir);
    let r = synced(&mut engine, 2);
    engine.run_ok(
        "pcb.move",
        json!({"objects": [r[1]], "by": {"x": "10mm", "y": "5mm"}}),
    );
    let (a, b) = (at(&engine, r[0]), at(&engine, r[1]));

    engine.run_ok("pcb.align", json!({"objects": r, "to": "left"}));
    assert_eq!(at(&engine, r[1]), Point::new(a.x, b.y));
    assert_eq!(at(&engine, r[0]), a);

    engine.run_ok("pcb.align", json!({"objects": r, "to": "top"}));
    assert_eq!(
        at(&engine, r[0]),
        Point::new(a.x, b.y),
        "top edges on the topmost"
    );

    engine.run_ok(
        "pcb.move",
        json!({"objects": [r[1]], "by": {"x": "4mm", "y": 0}}),
    );
    engine.run_ok("pcb.align", json!({"objects": r, "to": "center_x"}));
    assert_eq!(at(&engine, r[0]).x, a.x + Nm::from_mm(2));
    assert_eq!(at(&engine, r[1]).x, a.x + Nm::from_mm(2));
}

#[test]
fn distribute_spaces_centres_evenly_between_the_outermost() {
    let dir = TempDir::new("arrange-distribute");
    let mut engine = engine(&dir);
    let r = synced(&mut engine, 3);
    engine.run_ok(
        "pcb.move",
        json!({"objects": [r[0]], "by": {"x": 0, "y": "3mm"}}),
    );
    engine.run_ok(
        "pcb.move",
        json!({"objects": [r[1]], "by": {"x": 0, "y": "1mm"}}),
    );
    let (a, b, c) = (at(&engine, r[0]), at(&engine, r[1]), at(&engine, r[2]));

    engine.run_ok(
        "pcb.distribute",
        json!({"objects": r, "direction": "vertical"}),
    );

    assert_eq!(at(&engine, r[0]), a);
    assert_eq!(at(&engine, r[2]), c);
    let middle = Nm((a.y.0 + c.y.0) / 2);
    assert_eq!(at(&engine, r[1]), Point::new(b.x, middle));
}

#[test]
fn arrange_within_rectangle_packs_rows_from_the_top_left() {
    let dir = TempDir::new("arrange-rect");
    let mut engine = engine(&dir);
    let r = synced(&mut engine, 4);

    engine.run_ok(
        "pcb.arrange_in_rect",
        json!({"objects": [r[3], r[1], r[0], r[2]],
               "from": {"x": "10mm", "y": 0}, "to": {"x": 0, "y": "10mm"}}),
    );

    // 1 mm apart, in Designator order: two 3 mm wide Footprints fit across 10 mm.
    assert_eq!(at(&engine, r[0]), um(1500, 9250));
    assert_eq!(at(&engine, r[1]), um(5500, 9250));
    assert_eq!(at(&engine, r[2]), um(1500, 6750));
    assert_eq!(at(&engine, r[3]), um(5500, 6750));
}

#[test]
fn arrange_within_rectangle_takes_a_spacing() {
    let dir = TempDir::new("arrange-rect-spacing");
    let mut engine = engine(&dir);
    let r = synced(&mut engine, 2);

    engine.run_ok(
        "pcb.arrange_in_rect",
        json!({"objects": r, "from": {"x": 0, "y": 0}, "to": {"x": "20mm", "y": "5mm"},
               "spacing": "0.5mm"}),
    );

    assert_eq!(at(&engine, r[0]), um(1500, 4250));
    assert_eq!(at(&engine, r[1]), um(5000, 4250));
}

#[test]
fn arrange_within_rectangle_refuses_when_they_do_not_fit() {
    let dir = TempDir::new("arrange-rect-full");
    let mut engine = engine(&dir);
    let r = synced(&mut engine, 2);
    let before = at(&engine, r[0]);

    for to in [
        json!({"x": "2mm", "y": "10mm"}),
        json!({"x": "10mm", "y": "1mm"}),
    ] {
        let err = engine.run_err(
            "pcb.arrange_in_rect",
            json!({"objects": r, "from": {"x": 0, "y": 0}, "to": to}),
        );
        assert_eq!(err.code, ErrorCode::Rejected, "{}", err.message);
    }
    assert_eq!(at(&engine, r[0]), before);
}

#[test]
fn arranging_refuses_locked_footprints() {
    let dir = TempDir::new("arrange-locked");
    let mut engine = engine(&dir);
    let r = synced(&mut engine, 3);
    engine.run_ok("pcb.lock", json!({"objects": [r[1]], "locked": true}));

    for (id, params) in [
        ("pcb.align", json!({"objects": r, "to": "bottom"})),
        (
            "pcb.distribute",
            json!({"objects": r, "direction": "horizontal"}),
        ),
        (
            "pcb.arrange_in_rect",
            json!({"objects": r, "from": {"x": 0, "y": 0}, "to": {"x": "50mm", "y": "50mm"}}),
        ),
    ] {
        let err = engine.run_err(id, params);
        assert_eq!(err.code, ErrorCode::Rejected, "{id}");
        assert_eq!(err.details.unwrap()["locked"][0], r[1].to_string());
    }
}

#[test]
fn arrange_outside_board_reclusters_as_a_fresh_sync_would() {
    let dir = TempDir::new("arrange-outside");
    let mut engine = engine(&dir);
    let r = synced(&mut engine, 3);
    // Where Sync put them is where a fresh cluster goes: nothing else is on the board.
    let synced_at: Vec<Point> = r.iter().map(|u| at(&engine, *u)).collect();
    engine.run_ok(
        "pcb.move",
        json!({"objects": [r[0]], "by": {"x": "-40mm", "y": "30mm"}}),
    );
    engine.run_ok("pcb.rotate", json!({"objects": [r[1]]}));
    engine.run_ok("pcb.flip", json!({"objects": [r[2]]}));

    // Given out of order: the rows still follow natural Designator order.
    let out = engine.run_ok(
        "pcb.arrange_outside_board",
        json!({"footprints": [r[2], r[0], r[1]]}),
    );
    assert_eq!(out["result"]["objects"].as_array().unwrap().len(), 3);
    for (u, want) in r.iter().zip(&synced_at) {
        let f = footprint(&engine, *u);
        assert_eq!(f.at, *want, "{}", f.designator);
        assert_eq!((f.rotation, f.side), (Millideg(0), Side::Top));
    }
}

#[test]
fn arrange_outside_board_keeps_clear_of_footprints_left_in_place() {
    let dir = TempDir::new("arrange-outside-clear");
    let mut engine = engine(&dir);
    let r = synced(&mut engine, 2);
    let r2 = at(&engine, r[1]);
    engine.run_ok(
        "pcb.move",
        json!({"objects": [r[0]], "by": {"x": "-30mm", "y": 0}}),
    );
    engine.run_ok("pcb.arrange_outside_board", json!({"footprints": [r[0]]}));
    // R2 stays and R1 goes to its right: courtyards 3 mm wide, with the cluster gap.
    assert_eq!(at(&engine, r[1]), r2);
    assert!(at(&engine, r[0]).x > r2.x + Nm::from_mm(3), "{:?}", at(&engine, r[0]));
}

#[test]
fn swap_exchanges_positions_and_each_keeps_its_turn_and_side() {
    let dir = TempDir::new("arrange-swap");
    let mut engine = engine(&dir);
    let r = synced(&mut engine, 2);
    engine.run_ok(
        "pcb.move",
        json!({"objects": [r[1]], "by": {"x": "10mm", "y": "5mm"}}),
    );
    engine.run_ok("pcb.rotate", json!({"objects": [r[0]], "angle": "90deg"}));
    engine.run_ok("pcb.flip", json!({"objects": [r[1]]}));
    let (a, b) = (footprint(&engine, r[0]).clone(), footprint(&engine, r[1]).clone());

    engine.run_ok("pcb.swap", json!({"a": r[0], "b": r[1]}));

    let (a2, b2) = (footprint(&engine, r[0]), footprint(&engine, r[1]));
    assert_eq!((a2.at, b2.at), (b.at, a.at));
    assert_eq!((a2.rotation, a2.side), (a.rotation, a.side));
    assert_eq!((b2.rotation, b2.side), (b.rotation, b.side));

    let err = engine.run_err("pcb.swap", json!({"a": r[0], "b": r[0]}));
    assert_eq!(err.code, ErrorCode::InvalidParams);
}

#[test]
fn swap_and_arrange_outside_refuse_locked_footprints_unless_included() {
    let dir = TempDir::new("arrange-outside-locked");
    let mut engine = engine(&dir);
    let r = synced(&mut engine, 2);
    engine.run_ok(
        "pcb.move",
        json!({"objects": [r[1]], "by": {"x": "10mm", "y": 0}}),
    );
    engine.run_ok("pcb.lock", json!({"objects": [r[1]], "locked": true}));
    for (id, params) in [
        ("pcb.swap", json!({"a": r[0], "b": r[1]})),
        ("pcb.arrange_outside_board", json!({"footprints": r})),
    ] {
        let b = at(&engine, r[1]);
        let err = engine.run_err(id, params.clone());
        assert_eq!(err.code, ErrorCode::Rejected, "{id}");
        assert_eq!(err.details.unwrap()["locked"][0], r[1].to_string(), "{id}");
        assert_eq!(at(&engine, r[1]), b, "{id}");
        let mut params = params;
        params["include_locked"] = json!(true);
        engine.run_ok(id, params);
    }
    assert!(footprint(&engine, r[1]).locked, "the lock stays");
}
