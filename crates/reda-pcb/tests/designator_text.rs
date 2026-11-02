//! Each Footprint's Designator text can be moved and turned on its own.

mod common;

use common::{at, engine, footprint, pcb, synced_resistors};
use reda_command::ErrorCode;
use reda_model::FootprintLayer;
use reda_model::geom::{Millideg, Nm, Point};
use reda_pcb::placed::placed_designator;
use reda_testkit::{TempDir, TestEngine};
use serde_json::json;

fn mm(x: i64, y: i64) -> Point {
    Point::new(Nm::from_mm(x), Nm::from_mm(y))
}

fn designator(engine: &TestEngine, uuid: reda_model::Uuid) -> reda_model::FootprintText {
    placed_designator(pcb(engine), footprint(engine, uuid)).unwrap()
}

#[test]
fn the_designator_text_moves_and_turns_in_board_coordinates() {
    let dir = TempDir::new("designator-move");
    let mut engine = engine(&dir);
    let [r1, _] = synced_resistors(&mut engine);
    engine.run_ok("pcb.rotate", json!({"objects": [r1]}));
    let a = at(&engine, r1);
    // RESC1608X55N puts it 1.45 mm above the origin; the Footprint is turned a quarter.
    assert_eq!(
        designator(&engine, r1).at,
        a + Point::new(Nm(-1_450_000), Nm(0))
    );
    assert_eq!(designator(&engine, r1).rotation, Millideg::from_deg(90));

    let target = a + mm(2, 0);
    engine.run_ok(
        "pcb.designator_text.set",
        json!({"footprint": r1, "at": {"x": target.x.0, "y": target.y.0}, "rotation": 0}),
    );

    let text = designator(&engine, r1);
    assert_eq!(text.at, target);
    assert_eq!(text.rotation, Millideg::ZERO);
    assert_eq!(text.layer, FootprintLayer::SilkscreenTop);
    // Stored in the Footprint's own frame, so it follows the Footprint.
    let stored = footprint(&engine, r1).designator_text.unwrap();
    assert_eq!(stored.at, Point::new(Nm(0), Nm::from_mm(-2)));
    assert_eq!(stored.rotation, Millideg::from_deg(270));

    engine.run_ok(
        "pcb.move",
        json!({"objects": [r1], "by": {"x": "1mm", "y": "1mm"}}),
    );
    assert_eq!(designator(&engine, r1).at, target + mm(1, 1));
}

#[test]
fn the_designator_text_can_be_resized_hidden_and_reset() {
    let dir = TempDir::new("designator-reset");
    let mut engine = engine(&dir);
    let [r1, _] = synced_resistors(&mut engine);
    let original = designator(&engine, r1);

    engine.run_ok(
        "pcb.designator_text.set",
        json!({"footprint": r1, "size": "0.8mm", "visible": false}),
    );
    let text = designator(&engine, r1);
    assert_eq!(text.size, Nm::from_um(800));
    assert!(!text.visible);
    assert_eq!(text.at, original.at);

    engine.run_ok(
        "pcb.designator_text.set",
        json!({"footprint": r1, "reset": true}),
    );
    assert_eq!(footprint(&engine, r1).designator_text, None);
    assert_eq!(designator(&engine, r1), original);
}

#[test]
fn a_locked_footprints_designator_text_still_moves() {
    let dir = TempDir::new("designator-locked");
    let mut engine = engine(&dir);
    let [r1, _] = synced_resistors(&mut engine);
    engine.run_ok("pcb.lock", json!({"footprints": [r1], "locked": true}));

    engine.run_ok(
        "pcb.designator_text.set",
        json!({"footprint": r1, "rotation": "45deg"}),
    );
    assert_eq!(designator(&engine, r1).rotation, Millideg::from_deg(45));
}

#[test]
fn the_designator_text_needs_a_footprint_and_a_positive_size() {
    let dir = TempDir::new("designator-bad");
    let mut engine = engine(&dir);
    let [r1, _] = synced_resistors(&mut engine);

    let err = engine.run_err(
        "pcb.designator_text.set",
        json!({"footprint": pcb(&engine).setup().uuid}),
    );
    assert_eq!(err.code, ErrorCode::InvalidParams);
    let err = engine.run_err(
        "pcb.designator_text.set",
        json!({"footprint": r1, "size": 0}),
    );
    assert_eq!(err.details.unwrap()["pointer"], "/size");
    let err = engine.run_err(
        "pcb.designator_text.set",
        json!({"footprint": r1, "reset": true, "size": "1mm"}),
    );
    assert_eq!(err.code, ErrorCode::InvalidParams);
}
