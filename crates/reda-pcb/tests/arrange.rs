//! Arranging Footprints: align, distribute and Arrange within rectangle.
//!
//! The 0603 resistor Footprint's extent (courtyard) is 3 mm × 1.5 mm about its origin.

mod common;

use common::{at, engine, synced};
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
    engine.run_ok("pcb.lock", json!({"footprints": [r[1]], "locked": true}));

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
