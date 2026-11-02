//! Placing Footprints on the board: move, rotate, flip side and lock.

mod common;

use common::{at, engine, footprint, pcb, synced_resistors};
use reda_command::ErrorCode;
use reda_model::Side;
use reda_model::geom::{Millideg, Nm, Point};
use reda_testkit::TempDir;
use serde_json::json;

fn mm(x: i64, y: i64) -> Point {
    Point::new(Nm::from_mm(x), Nm::from_mm(y))
}

#[test]
fn footprints_move_by_an_offset() {
    let dir = TempDir::new("placement-move");
    let mut engine = engine(&dir);
    let [r1, r2] = synced_resistors(&mut engine);
    let (a, b) = (at(&engine, r1), at(&engine, r2));

    engine.run_ok(
        "pcb.move",
        json!({"objects": [r1, r2], "by": {"x": "10mm", "y": "-5mm"}}),
    );

    assert_eq!(at(&engine, r1), a + mm(10, -5));
    assert_eq!(at(&engine, r2), b + mm(10, -5));
}

#[test]
fn a_locked_footprint_refuses_to_move() {
    let dir = TempDir::new("placement-lock");
    let mut engine = engine(&dir);
    let [r1, r2] = synced_resistors(&mut engine);
    let (a, b) = (at(&engine, r1), at(&engine, r2));

    engine.run_ok("pcb.lock", json!({"footprints": [r1], "locked": true}));
    assert!(footprint(&engine, r1).locked);

    for (id, params) in [
        (
            "pcb.move",
            json!({"objects": [r1, r2], "by": {"x": "1mm", "y": 0}}),
        ),
        ("pcb.rotate", json!({"objects": [r1]})),
        ("pcb.flip", json!({"objects": [r1]})),
    ] {
        let err = engine.run_err(id, params);
        assert_eq!(err.code, ErrorCode::Rejected, "{id}: {}", err.message);
        assert!(err.message.contains("locked"), "{id}: {}", err.message);
    }
    assert_eq!(at(&engine, r1), a);
    assert_eq!(at(&engine, r2), b, "nothing moves when one is locked");

    engine.run_ok("pcb.lock", json!({"footprints": [r1], "locked": false}));
    engine.run_ok(
        "pcb.move",
        json!({"objects": [r1], "by": {"x": "1mm", "y": 0}}),
    );
    assert_eq!(at(&engine, r1), a + mm(1, 0));
}

#[test]
fn rotating_one_footprint_turns_it_in_place() {
    let dir = TempDir::new("placement-rotate");
    let mut engine = engine(&dir);
    let [r1, _] = synced_resistors(&mut engine);
    let a = at(&engine, r1);

    engine.run_ok("pcb.rotate", json!({"objects": [r1]}));
    assert_eq!(footprint(&engine, r1).rotation, Millideg::from_deg(90));
    assert_eq!(at(&engine, r1), a);

    engine.run_ok("pcb.rotate", json!({"objects": [r1], "angle": "-135deg"}));
    assert_eq!(footprint(&engine, r1).rotation, Millideg::from_deg(315));
}

#[test]
fn rotating_several_footprints_turns_them_about_a_centre() {
    let dir = TempDir::new("placement-rotate-group");
    let mut engine = engine(&dir);
    let [r1, r2] = synced_resistors(&mut engine);
    let (a, b) = (at(&engine, r1), at(&engine, r2));

    engine.run_ok(
        "pcb.rotate",
        json!({"objects": [r1, r2], "angle": "90deg", "center": {"x": 0, "y": 0}}),
    );

    // A quarter turn counter-clockwise about the origin: (x, y) -> (-y, x).
    assert_eq!(at(&engine, r1), Point::new(-a.y, a.x));
    assert_eq!(at(&engine, r2), Point::new(-b.y, b.x));
    assert_eq!(footprint(&engine, r2).rotation, Millideg::from_deg(90));
}

#[test]
fn flipping_moves_a_footprint_to_the_other_side_in_place() {
    let dir = TempDir::new("placement-flip");
    let mut engine = engine(&dir);
    let [r1, _] = synced_resistors(&mut engine);
    let a = at(&engine, r1);
    engine.run_ok("pcb.rotate", json!({"objects": [r1], "angle": "30deg"}));

    engine.run_ok("pcb.flip", json!({"objects": [r1]}));

    let placed = footprint(&engine, r1);
    assert_eq!(placed.side, Side::Bottom);
    assert_eq!(placed.at, a);
    // Flipping left-right mirrors the angle too.
    assert_eq!(placed.rotation, Millideg::from_deg(330));

    engine.run_ok("pcb.flip", json!({"objects": [r1]}));
    assert_eq!(footprint(&engine, r1).side, Side::Top);
    assert_eq!(footprint(&engine, r1).rotation, Millideg::from_deg(30));
    assert!(pcb(&engine).footprint(r1).is_some());
}

#[test]
fn flipping_several_footprints_mirrors_their_positions_about_a_vertical_line() {
    let dir = TempDir::new("placement-flip-group");
    let mut engine = engine(&dir);
    let [r1, r2] = synced_resistors(&mut engine);
    let (a, b) = (at(&engine, r1), at(&engine, r2));

    engine.run_ok(
        "pcb.flip",
        json!({"objects": [r1, r2], "center": {"x": "100mm", "y": 0}}),
    );

    let mirror = |p: Point| Point::new(Nm::from_mm(200) - p.x, p.y);
    assert_eq!(at(&engine, r1), mirror(a));
    assert_eq!(at(&engine, r2), mirror(b));
    assert_eq!(footprint(&engine, r2).side, Side::Bottom);
}

#[test]
fn placement_edits_undo_as_one_step() {
    let dir = TempDir::new("placement-undo");
    let mut engine = engine(&dir);
    let [r1, r2] = synced_resistors(&mut engine);
    let (a, b) = (at(&engine, r1), at(&engine, r2));

    engine.run_ok(
        "pcb.move",
        json!({"objects": [r1, r2], "by": {"x": "3mm", "y": 0}}),
    );
    engine.run_ok("edit.undo", json!({"document": common::PCB}));

    assert_eq!(at(&engine, r1), a);
    assert_eq!(at(&engine, r2), b);
}

#[test]
fn unknown_or_unplaceable_objects_are_refused() {
    let dir = TempDir::new("placement-unknown");
    let mut engine = engine(&dir);
    let [r1, _] = synced_resistors(&mut engine);
    let net_class = pcb(&engine).setup().default_net_class;

    let err = engine.run_err(
        "pcb.move",
        json!({"objects": [r1, net_class], "by": {"x": 0, "y": 0}}),
    );
    assert_eq!(err.code, ErrorCode::InvalidParams);
    assert_eq!(err.details.unwrap()["pointer"], "/objects/1");

    let err = engine.run_err("pcb.move", json!({"objects": [], "by": {"x": 0, "y": 0}}));
    assert_eq!(err.code, ErrorCode::InvalidParams);
}

#[test]
fn a_flipped_footprint_mirrors_its_pads_and_drawing_onto_the_bottom() {
    use reda_model::FootprintLayer;
    use reda_pcb::placed::{placed_designator, placed_graphics, placed_pads};

    let dir = TempDir::new("placement-flip-geometry");
    let mut engine = engine(&dir);
    let [r1, _] = synced_resistors(&mut engine);
    let a = at(&engine, r1);
    let pad = |engine: &reda_testkit::TestEngine, number: &str| {
        placed_pads(pcb(engine), footprint(engine, r1))
            .into_iter()
            .find(|p| p.number == number)
            .unwrap()
    };
    // RESC1608X55N: Pad 1 at (-0.7875 mm, 0), Pad 2 at (+0.7875 mm, 0), SMD on top.
    let half = Nm(787_500);
    assert_eq!(pad(&engine, "1").at, a + Point::new(-half, Nm(0)));
    assert_eq!(pad(&engine, "1").side, Some(Side::Top));

    engine.run_ok("pcb.flip", json!({"objects": [r1]}));

    assert_eq!(pad(&engine, "1").at, a + Point::new(half, Nm(0)));
    assert_eq!(pad(&engine, "2").at, a + Point::new(-half, Nm(0)));
    assert_eq!(pad(&engine, "1").side, Some(Side::Bottom));
    let layers: Vec<FootprintLayer> = placed_graphics(pcb(&engine), footprint(&engine, r1))
        .iter()
        .map(|g| g.layer)
        .collect();
    assert_eq!(
        layers,
        [
            FootprintLayer::AssemblyBottom,
            FootprintLayer::CourtyardBottom
        ]
    );
    let designator = placed_designator(pcb(&engine), footprint(&engine, r1)).unwrap();
    assert_eq!(designator.layer, FootprintLayer::SilkscreenBottom);
    assert_eq!(designator.at, a + Point::new(Nm(0), Nm(1_450_000)));

    // On the bottom, turning is still counter-clockwise seen from the top.
    engine.run_ok("pcb.rotate", json!({"objects": [r1]}));
    assert_eq!(pad(&engine, "1").at, a + Point::new(Nm(0), half));
    assert_eq!(pad(&engine, "1").rotation, Millideg::from_deg(90));
}
