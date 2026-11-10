//! Footprint moves in `avoid` and the live overlap (docs/spec/v0.1/pcb.md#moving-footprints):
//! pure checks over the Courtyard, or else the fallback box, and `courtyard_clearance`.
//!
//! Test Footprints have one 1 × 1 mm Pad and no Courtyard, so each one's fallback box is
//! that Pad: 1 mm wide about its origin.

use reda_model::Uuid;
use reda_model::geom::{Nm, Point};
use reda_pcb::place::{Overlap, alignment_guides, avoid, overlaps, pad_to_pad, selection_outlines};
use reda_testkit::{TempDir, TestEngine, probe_engine};
use serde_json::{Value, json};

fn engine(dir: &TempDir) -> TestEngine {
    let mut engine = probe_engine();
    engine.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
    engine
}

fn footprint(engine: &mut TestEngine, x: &str) -> Uuid {
    let out = engine.run_ok(
        "test.footprint.add",
        json!({"at": {"x": x, "y": 0}, "pads": [
            {"at": {"x": 0, "y": 0}, "width": "1mm", "height": "1mm"}]}),
    );
    out["result"]["footprint"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap()
}

fn clearance(engine: &mut TestEngine, value: Value) {
    engine.run_ok(
        "pcb.rules.set",
        json!({"kind": "courtyard_clearance", "values": {"value": value}}),
    );
}

fn um(x: i64, y: i64) -> Point {
    Point::new(Nm::from_um(x), Nm::from_um(y))
}

fn pcb(engine: &TestEngine) -> &reda_model::PcbDocument {
    engine.project().unwrap().pcb().unwrap()
}

#[test]
fn the_live_overlap_finds_boxes_closer_than_courtyard_clearance() {
    let dir = TempDir::new("place-overlap");
    let mut engine = engine(&dir);
    // Boxes 2 mm apart: a spans -0.5..0.5, b 2.5..3.5.
    let a = footprint(&mut engine, "0mm");
    let b = footprint(&mut engine, "3mm");
    assert_eq!(overlaps(pcb(&engine), &[a], Point::default()), []);
    // Touching passes at 0.
    assert_eq!(overlaps(pcb(&engine), &[a], um(2000, 0)), []);
    assert_eq!(
        overlaps(pcb(&engine), &[a], um(2001, 0)),
        [Overlap { moved: a, other: b }]
    );

    clearance(&mut engine, json!("0.5mm"));
    assert_eq!(overlaps(pcb(&engine), &[a], um(1500, 0)), []);
    assert_eq!(
        overlaps(pcb(&engine), &[a], um(1600, 0)),
        [Overlap { moved: a, other: b }]
    );
    // Moved together, they never meet each other.
    assert_eq!(overlaps(pcb(&engine), &[a, b], um(1600, 0)), []);
}

#[test]
fn avoid_stops_at_the_last_position_that_keeps_courtyard_clearance() {
    let dir = TempDir::new("place-avoid");
    let mut engine = engine(&dir);
    let a = footprint(&mut engine, "0mm");
    let _b = footprint(&mut engine, "3mm");
    clearance(&mut engine, json!("0.5mm"));

    // Towards b and past it: it stops 0.5 mm short of b's box, to within 1 µm.
    let got = avoid(pcb(&engine), &[a], um(10_000, 0));
    assert!(
        got.y == Nm::ZERO && got.x <= Nm::from_um(1500) && got.x >= Nm::from_um(1499),
        "{got:?}"
    );
    assert!(overlaps(pcb(&engine), &[a], got).is_empty());

    // A clear path goes all the way.
    assert_eq!(avoid(pcb(&engine), &[a], um(0, 8000)), um(0, 8000));
    assert_eq!(avoid(pcb(&engine), &[a], um(-5000, 0)), um(-5000, 0));
}

#[test]
fn avoid_lets_a_part_out_of_an_overlap_it_starts_in() {
    let dir = TempDir::new("place-avoid-out");
    let mut engine = engine(&dir);
    let a = footprint(&mut engine, "0mm");
    let _b = footprint(&mut engine, "0.5mm");
    assert_eq!(avoid(pcb(&engine), &[a], um(-4000, 0)), um(-4000, 0));
}

#[test]
fn alignment_guides_line_up_pad_centres_and_courtyard_edges_with_the_neighbours() {
    let dir = TempDir::new("place-guides");
    let mut engine = engine(&dir);
    // a's box spans -0.5..0.5 about its origin; b stands at (3, 7) mm.
    let a = footprint(&mut engine, "0mm");
    let b = engine.run_ok(
        "test.footprint.add",
        json!({"at": {"x": "3mm", "y": "7mm"}, "pads": [
            {"at": {"x": 0, "y": 0}, "width": "1mm", "height": "1mm"}]}),
    )["result"]["footprint"]
        .as_str()
        .unwrap()
        .parse::<Uuid>()
        .unwrap();
    let _ = b;
    // Held by a point 0.2 mm right of its origin.
    let guides = alignment_guides(pcb(&engine), &[a], um(200, 0));
    // Pad centre on pad centre: reference at 3.2 / 7; box edges on box edges: the left
    // edge (-0.5 → 2.5) and right edge (0.5 → 3.5) put it at 3.2 too, and abutting edges
    // (left on right: 3.5 + 0.5 + 0.2 = 4.2; right on left: 2.5 - 0.5 + 0.2 = 2.2).
    assert_eq!(
        guides.x,
        [um(2200, 0).x, um(3200, 0).x, um(4200, 0).x]
            .into_iter()
            .collect()
    );
    assert_eq!(
        guides.y,
        [Nm::from_um(6000), Nm::from_um(7000), Nm::from_um(8000)]
            .into_iter()
            .collect()
    );
    // Moving everything leaves no neighbours.
    assert!(
        alignment_guides(pcb(&engine), &[a, b], um(0, 0))
            .x
            .is_empty()
    );
    // The outline checked is the fallback box.
    let outline = &selection_outlines(pcb(&engine), a)[0];
    assert!(
        outline.contains(&um(-500, -500)) && outline.contains(&um(500, 500)),
        "{outline:?}"
    );
}

#[test]
fn pad_to_pad_draws_a_line_from_each_moved_pad_to_the_nearest_pad_on_its_net() {
    let dir = TempDir::new("place-pad-to-pad");
    let mut engine = engine(&dir);
    let net = engine.run_ok("test.net.add", json!({"name": "SIG"}))["result"]["uuid"].clone();
    let add = |engine: &mut TestEngine, x: &str| -> Uuid {
        engine.run_ok(
            "test.footprint.add",
            json!({"at": {"x": x, "y": 0}, "pads": [
                {"at": {"x": 0, "y": 0}, "width": "1mm", "height": "1mm", "net": net}]}),
        )["result"]["footprint"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap()
    };
    let a = add(&mut engine, "0mm");
    let _near = add(&mut engine, "4mm");
    let _far = add(&mut engine, "9mm");
    let _other = footprint(&mut engine, "1mm");
    let lines = pad_to_pad(pcb(&engine), &[a]);
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert_eq!((lines[0].from, lines[0].to), (um(0, 0), um(4000, 0)));
}
