//! Cluster packing: where Sync puts new Footprints, one block per Schematic Sheet.
//!
//! The 0603 resistor Footprint's courtyard is 3 mm × 1.5 mm about its origin, so with the
//! 1 mm gap neighbours in a row are 4 mm apart and rows are 2.5 mm apart.

mod common;

use std::collections::BTreeSet;

use common::{RESC1608, RESISTOR, at, engine, footprint, pcb, synced};
use reda_model::geom::{Millideg, Nm, Point};
use reda_model::{Footprint, Side};
use reda_testkit::{TempDir, TestEngine};
use serde_json::json;

fn mm(x: f64, y: f64) -> Point {
    Point::new(Nm((x * 1e6) as i64), Nm((y * 1e6) as i64))
}

fn resistor_copy(engine: &TestEngine) -> Footprint {
    let instance = pcb(engine).footprints().next().unwrap();
    pcb(engine)
        .footprint_copy(instance.footprint)
        .unwrap()
        .footprint
        .clone()
}

#[test]
fn a_footprint_with_a_courtyard_is_boxed_by_its_courtyard() {
    let dir = TempDir::new("cluster-box");
    let mut engine = engine(&dir);
    synced(&mut engine, 1);
    let b = reda_pcb::cluster::footprint_box(&resistor_copy(&engine));
    assert_eq!((b.min, b.max), (mm(-1.5, -0.75), mm(1.5, 0.75)));
}

#[test]
fn a_footprint_without_a_courtyard_is_boxed_by_its_pads_and_graphics() {
    let dir = TempDir::new("cluster-box-bare");
    let mut engine = engine(&dir);
    synced(&mut engine, 1);
    let mut fp = resistor_copy(&engine);
    fp.graphics.retain(|g| !g.layer.is_courtyard());
    let b = reda_pcb::cluster::footprint_box(&fp);
    // RESC1608X55N: Pads 0.95 × 0.85 mm (rounded) at ±0.775 mm, inside the 1.6 × 0.8 mm
    // assembly outline, so the Pads set the box.
    assert!(
        b.max.x < Nm::from_um(1500) && b.max.x > Nm::from_um(1000),
        "{b:?}"
    );
    assert!(
        b.max.y < Nm::from_um(750) && b.max.y >= Nm::from_um(400),
        "{b:?}"
    );
}

#[test]
fn sync_packs_new_footprints_in_rows_one_mm_apart_from_the_origin() {
    let dir = TempDir::new("cluster-rows");
    let mut engine = engine(&dir);
    // Ten courtyards of 4.5 mm²: rows are √45 × 1.5 ≈ 10.06 mm wide, so two fit a row.
    let r = synced(&mut engine, 10);
    assert_eq!(at(&engine, r[0]), mm(1.5, -0.75));
    assert_eq!(at(&engine, r[1]), mm(5.5, -0.75));
    assert_eq!(at(&engine, r[2]), mm(1.5, -3.25));
    assert_eq!(at(&engine, r[9]), mm(5.5, -10.75));
    for uuid in r {
        let f = footprint(&engine, uuid);
        assert_eq!((f.rotation, f.side), (Millideg(0), Side::Top));
    }
}

#[test]
fn rows_follow_natural_designator_order() {
    let dir = TempDir::new("cluster-natural");
    let mut engine = engine(&dir);
    let r = synced(&mut engine, 10);
    // R10 is last, not third.
    let by_designator = |d: &str| {
        pcb(&engine)
            .footprints()
            .find(|f| f.designator == d)
            .unwrap()
            .at
    };
    assert_eq!(by_designator("R10"), at(&engine, r[9]));
    assert_eq!(by_designator("R2"), mm(5.5, -0.75));
}

#[test]
fn each_sheet_gets_its_own_block_stacked_below_the_last() {
    let dir = TempDir::new("cluster-sheets");
    let mut engine = engine(&dir);
    let sheet_2 =
        engine.run_ok("document.add", json!({"kind": "schematic"}))["result"]["uuid"].clone();
    for (sheet, x) in [
        (json!(common::SHEET), 0),
        (sheet_2, 0),
        (json!(common::SHEET), 20),
    ] {
        engine.run_ok(
            "schematic.component.place",
            json!({"document": sheet, "part": RESISTOR, "footprint": RESC1608,
                   "at": {"x": format!("{x}mm"), "y": 0}}),
        );
    }
    let hash = engine.run_ok("sync.preview", json!({}))["hash"].clone();
    engine.run_ok("sync.apply", json!({"hash": hash}));
    let by_designator = |d: &str| {
        pcb(&engine)
            .footprints()
            .find(|f| f.designator == d)
            .unwrap()
            .at
    };
    // Sheet 1's block (R1, R3): rows √9 × 1.5 = 4.5 mm wide, so one per row, 4 mm deep.
    // Sheet 2's block (R2) starts at the same left edge, 5 mm below it.
    assert_eq!(by_designator("R1"), mm(1.5, -0.75));
    assert_eq!(by_designator("R3"), mm(1.5, -3.25));
    assert_eq!(by_designator("R2"), mm(1.5, -9.75));
}

#[test]
fn a_block_stays_clear_of_the_board_and_placed_footprints_unless_they_move() {
    let dir = TempDir::new("cluster-clear");
    let mut engine = engine(&dir);
    engine.run_ok(
        "pcb.board_outline.add_rect",
        json!({"width": "50mm", "height": "30mm"}),
    );
    let r = synced(&mut engine, 1);
    let fp = resistor_copy(&engine);
    let r1 = at(&engine, r[0]);
    assert!(r1.x > Nm::from_mm(50) + Nm::from_um(1500), "{r1:?}");
    assert_eq!(r1.y, Nm::from_mm(30) - Nm::from_um(750));

    let blocks = [vec![&fp], vec![&fp, &fp]];
    let beside = reda_pcb::cluster::pack(pcb(&engine), &BTreeSet::new(), &blocks);
    // 5 mm right of R1's box, top-aligned with the board; the second block 5 mm below.
    let x = r1.x + Nm::from_um(1500 + 5000 + 1500);
    assert_eq!(beside[0], [Point::new(x, Nm::from_um(29_250))]);
    assert_eq!(
        beside[1],
        [
            Point::new(x, Nm::from_um(22_750)),
            Point::new(x, Nm::from_um(20_250))
        ]
    );

    // Re-clustering R1 itself ignores where it is now.
    let moving = BTreeSet::from([r[0]]);
    let again = reda_pcb::cluster::pack(pcb(&engine), &moving, &[vec![&fp]]);
    assert_eq!(again[0][0], r1);
}
