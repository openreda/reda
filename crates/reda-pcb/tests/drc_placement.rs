//! DRC II: the placement, silkscreen and sync checks, through `drc.run`, on Footprints
//! synced from the repo's starter Library.
//!
//! The 0603 Resistor (RESC1608X55N) has two 0.925 × 1 mm Pads at x = ±0.7875 mm, a
//! 3 × 1.5 mm courtyard centred on its origin and its Designator 1.45 mm above it.

mod common;

use common::{SHEET, at, created, engine, pcb, synced, synced_resistors};
use reda_model::Uuid;
use reda_model::geom::Nm;
use reda_testkit::{TempDir, TestEngine};
use serde_json::{Value, json};

fn uuid(value: &Value) -> Uuid {
    value.as_str().unwrap().parse().unwrap()
}

fn drc(engine: &mut TestEngine) -> Value {
    engine.run_ok("drc.run", json!({}))
}

/// The object lists of one check's Violations.
fn objects_of(run: &Value, check: &str) -> Vec<Vec<Uuid>> {
    run["violations"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| v["check"] == check)
        .map(|v| v["objects"].as_array().unwrap().iter().map(uuid).collect())
        .collect()
}

fn of(run: &Value, check: &str) -> Vec<Value> {
    run["violations"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| v["check"] == check)
        .cloned()
        .collect()
}

/// Moves a Footprint so its origin is at `(x, y)` nm.
fn move_to(engine: &mut TestEngine, footprint: Uuid, x: i64, y: i64) {
    let now = at(engine, footprint);
    engine.run_ok(
        "pcb.move",
        json!({"objects": [footprint], "by": {"x": x - now.x.0, "y": y - now.y.0}}),
    );
}

const MM: i64 = 1_000_000;

fn outline(engine: &TestEngine) -> Uuid {
    pcb(engine).outline().unwrap().uuid
}

#[test]
fn footprints_partly_or_wholly_outside_the_board_outline_are_reported() {
    let dir = TempDir::new("drc-outside");
    let mut engine = engine(&dir);
    engine.run_ok(
        "pcb.board_outline.add_rect",
        json!({"width": "20mm", "height": "20mm"}),
    );
    let r = synced(&mut engine, 3);
    let edge = outline(&engine);
    move_to(&mut engine, r[0], 10 * MM, 10 * MM);
    // R2 stays in the cluster Sync placed beside the board; R3 straddles the left edge.
    move_to(&mut engine, r[2], MM, 10 * MM);
    let run = drc(&mut engine);
    let mut found = objects_of(&run, "footprint_outside_board");
    found.sort();
    let mut expected = vec![vec![r[1], edge], vec![r[2], edge]];
    expected.sort();
    assert_eq!(found, expected, "{run:#}");
    let first = &of(&run, "footprint_outside_board")[0];
    assert_eq!(first["severity"], "error");
    assert!(first["args"]["designator"].is_string());
}

#[test]
fn overlapping_courtyards_on_one_side_are_reported_but_touching_ones_are_not() {
    let dir = TempDir::new("drc-courtyard");
    let mut engine = engine(&dir);
    let r = synced(&mut engine, 4);
    move_to(&mut engine, r[0], 10 * MM, 10 * MM);
    // 2 mm to the right: the 3 mm courtyards overlap by 1 mm.
    move_to(&mut engine, r[1], 12 * MM, 10 * MM);
    // 1.5 mm up: the courtyards share an edge.
    move_to(&mut engine, r[2], 10 * MM, 10 * MM + 3 * MM / 2);
    // Right under R1, but on the bottom side.
    engine.run_ok("pcb.flip", json!({"objects": [r[3]]}));
    move_to(&mut engine, r[3], 10 * MM, 10 * MM);
    let run = drc(&mut engine);
    assert_eq!(
        objects_of(&run, "courtyard_overlap"),
        [vec![r[0], r[1]]],
        "{run:#}"
    );
    let found = &of(&run, "courtyard_overlap")[0];
    assert_eq!(found["severity"], "warning");
    assert_eq!(found["args"]["designator_a"], "R1");
    assert_eq!(found["args"]["designator_b"], "R2");
}

/// The Footprint's copy Pad numbered `number`.
fn pad(engine: &TestEngine, footprint: Uuid, number: &str) -> Uuid {
    let pcb = pcb(engine);
    let copy = pcb.footprint_copy(pcb.footprint(footprint).unwrap().footprint);
    let pads = &copy.unwrap().footprint.pads;
    pads.iter().find(|p| p.number == number).unwrap().uuid
}

#[test]
fn silkscreen_over_exposed_pads_on_its_side_is_reported() {
    let dir = TempDir::new("drc-silk");
    let mut engine = engine(&dir);
    let r = synced(&mut engine, 3);
    move_to(&mut engine, r[0], 10 * MM, 10 * MM);
    move_to(&mut engine, r[1], 20 * MM, 10 * MM);
    move_to(&mut engine, r[2], 30 * MM, 10 * MM);
    let line = |engine: &mut TestEngine, layer: &str, from: (&str, &str), to: (&str, &str)| {
        created(&engine.run_ok(
            "pcb.graphic.add_line",
            json!({"layer": layer, "from": {"x": from.0, "y": from.1},
                   "to": {"x": to.0, "y": to.1}, "width": "0.15mm"}),
        ))
    };
    // Across R1's Pad 1.
    let across = line(
        &mut engine,
        "silkscreen_top",
        ("9.2mm", "8mm"),
        ("9.2mm", "12mm"),
    );
    // Across R1's Pad 2, but on the bottom silkscreen, where the top SMD Pad is not.
    line(
        &mut engine,
        "silkscreen_bottom",
        ("10.8mm", "8mm"),
        ("10.8mm", "12mm"),
    );
    // Above both Pads, 0.425 mm clear.
    line(
        &mut engine,
        "silkscreen_top",
        ("8mm", "11mm"),
        ("12mm", "11mm"),
    );
    // R2's Designator moved onto its Pad 2.
    engine.run_ok(
        "pcb.designator_text.set",
        json!({"footprint": r[1], "at": {"x": "20.7875mm", "y": "10mm"}}),
    );
    // A text over R3's Pad 1.
    let text = created(&engine.run_ok(
        "pcb.text.add",
        json!({"layer": "silkscreen_top", "text": "X", "x": "29mm", "y": "9.6mm",
               "height": "0.8mm", "thickness": "0.15mm"}),
    ));
    let run = drc(&mut engine);
    let mut found = objects_of(&run, "silkscreen_over_pads");
    found.sort();
    let mut expected = vec![
        vec![across, r[0], pad(&engine, r[0], "1")],
        vec![r[1], pad(&engine, r[1], "2")],
        vec![text, r[2], pad(&engine, r[2], "1")],
    ];
    expected.sort();
    assert_eq!(found, expected, "{run:#}");
    assert_eq!(of(&run, "silkscreen_over_pads")[0]["severity"], "warning");
}
