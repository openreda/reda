//! Shared set-up for the placement tests: a Project on the repo's starter Library, with
//! Footprints synced from the schematic or placed as Mechanical Parts.

#![allow(dead_code, reason = "each test file uses its own subset")]

use std::path::{Path, PathBuf};

use reda_model::geom::Point;
use reda_model::{FootprintInstance, PcbDocument, Uuid};
use reda_testkit::{TempDir, TestEngine, test_engine_with_library};
use serde_json::json;

pub const SHEET: &str = "00000000-0000-4000-8000-000000000002";
pub const PCB: &str = "00000000-0000-4000-8000-000000000003";
/// The starter Library's Resistor (Standard, first Footprint RESC1005X40N).
pub const RESISTOR: &str = "84d5651a-bf4b-462e-9480-39e8f454a042";
/// The starter Library's 0603 resistor Footprint, the Resistor's second alternate.
pub const RESC1608: &str = "0691e5e7-9eb6-49f3-9f3e-d4159258693e";
/// The starter Library's MountingHole_M3 (Mechanical, no Symbol).
pub const MOUNTING_HOLE_M3: &str = "326ace02-ef44-4af4-8f9d-21d6c65a74ef";
/// The starter Library's Fiducial (Mechanical, no Symbol).
pub const FIDUCIAL: &str = "09708c06-dce5-40b8-8035-6db932661565";

pub fn starter_library() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../library/starter")
}

pub fn engine(dir: &TempDir) -> TestEngine {
    let mut engine = test_engine_with_library(&starter_library());
    engine.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
    engine
}

pub fn pcb(engine: &TestEngine) -> &PcbDocument {
    engine.project().unwrap().pcb().unwrap()
}

pub fn footprint(engine: &TestEngine, uuid: Uuid) -> &FootprintInstance {
    pcb(engine).footprint(uuid).expect("a Footprint instance")
}

pub fn at(engine: &TestEngine, uuid: Uuid) -> Point {
    footprint(engine, uuid).at
}

/// Places two 0603 Resistors on the sheet, syncs them to the board and returns their
/// Footprint instances, R1 then R2.
pub fn synced_resistors(engine: &mut TestEngine) -> [Uuid; 2] {
    let r = synced(engine, 2);
    [r[0], r[1]]
}

/// Places `n` 0603 Resistors on the sheet, syncs them to the board and returns their
/// Footprint instances, R1 to Rn.
pub fn synced(engine: &mut TestEngine, n: usize) -> Vec<Uuid> {
    for i in 0..n {
        engine.run_ok(
            "schematic.component.place",
            json!({"document": SHEET, "part": RESISTOR, "footprint": RESC1608,
                   "at": {"x": format!("{}mm", 20 * i), "y": 0}}),
        );
    }
    let hash = engine.run_ok("sync.preview", json!({}))["hash"].clone();
    engine.run_ok("sync.apply", json!({"hash": hash}));
    (1..=n)
        .map(|i| {
            pcb(engine)
                .footprints()
                .find(|f| f.designator == format!("R{i}"))
                .unwrap()
                .uuid
        })
        .collect()
}

/// The UUID a Command that creates one object returns.
pub fn created(out: &serde_json::Value) -> Uuid {
    out["result"]["uuid"].as_str().unwrap().parse().unwrap()
}
