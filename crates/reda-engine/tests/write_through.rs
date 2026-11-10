//! Write-through edits: Component data edited from the PCB side (`pcb.footprint.set`)
//! updates every Unit and the Footprint instance together, without leaving a pending Sync
//! change behind. Re-annotate from the board is in `reannotate_board.rs`.

use std::path::{Path, PathBuf};

use reda_engine::command::ErrorCode;
use reda_testkit::{TempDir, TestEngine, test_engine_with_library};
use serde_json::{Value, json};

const SHEET: &str = "00000000-0000-4000-8000-000000000002";
const PCB: &str = "00000000-0000-4000-8000-000000000003";
const R_10K: &str = "a0000000-0000-4000-8000-000000000100";
const LM358: &str = "a0000000-0000-4000-8000-000000000120";
const R_SHUNT: &str = "a0000000-0000-4000-8000-000000000150";
const R_KELVIN: &str = "a0000000-0000-4000-8000-000000000070";

fn library() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../reda-schematic/tests/library/Test")
}

fn demo(dir: &TempDir) -> TestEngine {
    let mut engine = test_engine_with_library(&library());
    engine.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
    engine
}

/// Places a Component on `sheet` at (`x`, `y`) mm and returns its Component UUID.
fn place_on(engine: &mut TestEngine, sheet: &str, part: &str, x: i64, y: i64) -> String {
    engine.run_ok(
        "schematic.component.place",
        json!({"document": sheet, "part": part,
               "at": {"x": format!("{x}mm"), "y": format!("{y}mm")}}),
    )["result"]["component"]["uuid"]
        .as_str()
        .unwrap()
        .to_owned()
}

fn place(engine: &mut TestEngine, part: &str, x: i64, y: i64) -> String {
    place_on(engine, SHEET, part, x, y)
}

fn wire(engine: &mut TestEngine, points: &[(f64, f64)]) {
    let points: Vec<Value> = points
        .iter()
        .map(|(x, y)| json!({"x": format!("{x}mm"), "y": format!("{y}mm")}))
        .collect();
    engine.run_ok(
        "schematic.wire.add",
        json!({"document": SHEET, "points": points}),
    );
}

fn sync(engine: &mut TestEngine) {
    let list = engine.run_ok("sync.preview", json!({}));
    engine.run_ok("sync.apply", json!({"hash": list["hash"]}));
}

fn pending(engine: &mut TestEngine) -> Value {
    engine.run_ok("sync.preview", json!({}))["count"].clone()
}

fn objects(engine: &mut TestEngine, kind: &str) -> Vec<Value> {
    engine.run_ok("objects.query", json!({"type": kind}))["objects"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["object"].clone())
        .collect()
}

/// The Footprint instance of a Component.
fn instance_of(engine: &mut TestEngine, component: &str) -> Value {
    objects(engine, "footprint_instance")
        .into_iter()
        .find(|i| i["component"]["uuid"] == component)
        .expect("a synced Component")
}

/// Every Unit of a Component.
fn units_of(engine: &mut TestEngine, component: &str) -> Vec<Value> {
    objects(engine, "unit")
        .into_iter()
        .filter(|u| u["component"]["uuid"] == component)
        .collect()
}

fn touched(done: &Value) -> Vec<String> {
    done["changes"]
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect()
}

#[test]
fn in_bom_and_in_cpl_edited_on_the_pcb_reach_every_unit_and_the_footprint() {
    let dir = TempDir::new("write-through-flags");
    let mut engine = demo(&dir);
    let u1 = place(&mut engine, LM358, 0, 0);
    engine.run_ok(
        "schematic.unit.place",
        json!({"document": SHEET, "component": u1, "at": {"x": "30mm", "y": "0mm"}}),
    );
    sync(&mut engine);
    let footprint = instance_of(&mut engine, &u1)["uuid"].clone();

    engine.run_ok(
        "pcb.footprint.set",
        json!({"footprints": [footprint], "in_bom": false, "in_cpl": false}),
    );

    let units = units_of(&mut engine, &u1);
    assert_eq!(units.len(), 2);
    for unit in &units {
        assert_eq!(
            (&unit["in_bom"], &unit["in_cpl"]),
            (&json!(false), &json!(false))
        );
    }
    let instance = instance_of(&mut engine, &u1);
    assert_eq!(
        (&instance["in_bom"], &instance["in_cpl"]),
        (&json!(false), &json!(false))
    );
    assert_eq!(pending(&mut engine), 0);
}

#[test]
fn a_value_edited_on_the_pcb_updates_the_units_and_the_footprint_with_an_undo_entry_each() {
    let dir = TempDir::new("write-through-value");
    let mut engine = demo(&dir);
    let r1 = place(&mut engine, R_10K, 0, 0);
    sync(&mut engine);
    let footprint = instance_of(&mut engine, &r1)["uuid"].clone();

    let done = engine.run_ok(
        "pcb.footprint.set",
        json!({"footprints": [footprint], "value": "4k7"}),
    );

    let mut scopes = touched(&done);
    scopes.sort();
    assert_eq!(scopes, [SHEET, PCB]);
    assert_eq!(units_of(&mut engine, &r1)[0]["fields"]["value"], "4k7");
    assert_eq!(instance_of(&mut engine, &r1)["fields"]["value"], "4k7");
    assert_eq!(pending(&mut engine), 0);

    // Each Document undoes its own half.
    engine.run_ok("edit.undo", json!({"document": PCB}));
    assert_eq!(instance_of(&mut engine, &r1)["fields"]["value"], "10k");
    assert_eq!(units_of(&mut engine, &r1)[0]["fields"]["value"], "4k7");
    engine.run_ok("edit.undo", json!({"document": SHEET}));
    assert_eq!(units_of(&mut engine, &r1)[0]["fields"]["value"], "10k");
    assert_eq!(pending(&mut engine), 0);
}

fn add_sheet(engine: &mut TestEngine) -> String {
    engine.run_ok("document.add", json!({"kind": "schematic"}))["result"]["uuid"]
        .as_str()
        .unwrap()
        .to_owned()
}

fn net_names(engine: &mut TestEngine) -> Vec<String> {
    objects(engine, "net")
        .iter()
        .map(|n| n["name"].as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn a_designator_edit_reaches_every_unit_and_renames_auto_named_nets_on_the_board() {
    let dir = TempDir::new("write-through-designator");
    let mut engine = demo(&dir);
    let r1 = place(&mut engine, R_10K, 0, 0);
    place(&mut engine, R_10K, 10, 0);
    wire(&mut engine, &[(0.0, -3.81), (10.0, -3.81)]);
    let op_amp = place(&mut engine, LM358, 50, 0);
    let second = add_sheet(&mut engine);
    engine.run_ok(
        "schematic.unit.place",
        json!({"document": second, "component": op_amp, "at": {"x": 0, "y": 0}}),
    );
    sync(&mut engine);
    assert!(net_names(&mut engine).contains(&"Net-(R1-2)".to_owned()));

    let footprint = instance_of(&mut engine, &r1)["uuid"].clone();
    let done = engine.run_ok(
        "pcb.footprint.set",
        json!({"footprints": [footprint], "designator": "R7"}),
    );
    assert_eq!(done["result"]["footprints"], json!([footprint]));
    assert_eq!(instance_of(&mut engine, &r1)["designator"], "R7");
    assert!(net_names(&mut engine).contains(&"Net-(R2-2)".to_owned()));
    assert_eq!(pending(&mut engine), 0);

    let footprint = instance_of(&mut engine, &op_amp)["uuid"].clone();
    let done = engine.run_ok(
        "pcb.footprint.set",
        json!({"footprints": [footprint], "designator": "U5"}),
    );
    assert_eq!(done["result"]["units"].as_array().unwrap().len(), 2);
    let units = units_of(&mut engine, &op_amp);
    assert_eq!(units.len(), 2);
    assert!(units.iter().all(|u| u["designator"] == "U5"));
    assert_eq!(instance_of(&mut engine, &op_amp)["designator"], "U5");
    assert_eq!(pending(&mut engine), 0);

    let taken = engine.run_err(
        "pcb.footprint.set",
        json!({"footprints": [footprint], "designator": "R7"}),
    );
    assert_eq!(taken.code, ErrorCode::Conflict);
}

#[test]
fn a_footprint_edited_on_the_pcb_is_swapped_in_place_as_sync_would() {
    let dir = TempDir::new("write-through-footprint");
    let mut engine = demo(&dir);
    let shunt = place(&mut engine, R_SHUNT, 0, 0);
    place(&mut engine, R_10K, 10, 0);
    wire(&mut engine, &[(0.0, -3.81), (10.0, -3.81)]);
    sync(&mut engine);
    let before = instance_of(&mut engine, &shunt);
    engine.run_ok(
        "pcb.move",
        json!({"objects": [before["uuid"]], "by": {"x": "3mm", "y": "1mm"}}),
    );
    let moved = instance_of(&mut engine, &shunt);

    let done = engine.run_ok(
        "pcb.footprint.set",
        json!({"footprints": [before["uuid"]], "footprint": R_KELVIN}),
    );

    let mut scopes = touched(&done);
    scopes.sort();
    assert_eq!(scopes, [SHEET, PCB]);
    assert_eq!(units_of(&mut engine, &shunt)[0]["footprint"], R_KELVIN);
    let after = instance_of(&mut engine, &shunt);
    assert_eq!(after["uuid"], before["uuid"]);
    assert_eq!(after["at"], moved["at"]);
    assert_ne!(after["footprint"], before["footprint"]);
    let pads = after["pads"].as_array().unwrap();
    assert_eq!(pads.len(), 4);
    assert_eq!(pads.iter().filter(|p| p.get("net").is_some()).count(), 4);
    assert_eq!(pending(&mut engine), 0);
    // Only the Footprints in use stay in the PCB's part table.
    let copies = engine.run_ok(
        "objects.query",
        json!({"type": "footprint_copy", "document": PCB}),
    )["objects"]
        .as_array()
        .unwrap()
        .len();
    assert_eq!(copies, 2);

    let not_an_alternate = engine.run_err(
        "pcb.footprint.set",
        json!({"footprints": [before["uuid"]], "footprint": SHEET}),
    );
    assert_eq!(not_an_alternate.code, ErrorCode::InvalidParams);
}

#[test]
fn changes_already_pending_stay_pending() {
    let dir = TempDir::new("write-through-pending");
    let mut engine = demo(&dir);
    let r1 = place(&mut engine, R_10K, 0, 0);
    let r2 = place(&mut engine, R_10K, 10, 0);
    sync(&mut engine);
    engine.run_ok(
        "schematic.component.set",
        json!({"components": [r2], "not_fitted": true}),
    );
    place(&mut engine, R_10K, 20, 0);
    assert_eq!(pending(&mut engine), 2);

    let footprint = instance_of(&mut engine, &r1)["uuid"].clone();
    engine.run_ok(
        "pcb.footprint.set",
        json!({"footprints": [footprint], "fields": {"LCSC": "C1"}}),
    );

    assert_eq!(pending(&mut engine), 2);
    assert_eq!(
        instance_of(&mut engine, &r1)["fields"]["custom"][0]["value"],
        "C1"
    );
    assert!(instance_of(&mut engine, &r2).get("not_fitted").is_none());
}

/// The repo's starter Library, which has Mechanical Parts with Footprints.
fn starter(dir: &TempDir) -> TestEngine {
    let library = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../library/starter");
    let mut engine = test_engine_with_library(&library);
    engine.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
    engine
}

#[test]
fn mechanical_parts_are_edited_on_the_pcb_alongside_synced_footprints() {
    const RESISTOR: &str = "84d5651a-bf4b-462e-9480-39e8f454a042";
    const MOUNTING_HOLE_M3: &str = "326ace02-ef44-4af4-8f9d-21d6c65a74ef";
    let dir = TempDir::new("write-through-mechanical");
    let mut engine = starter(&dir);
    let r1 = place(&mut engine, RESISTOR, 0, 0);
    sync(&mut engine);
    let synced = instance_of(&mut engine, &r1)["uuid"].clone();
    let hole = engine.run_ok(
        "pcb.mechanical.place",
        json!({"part": MOUNTING_HOLE_M3, "at": {"x": "-10mm", "y": 0}}),
    )["result"]["footprint"]
        .clone();

    let done = engine.run_ok(
        "pcb.footprint.set",
        json!({"footprints": [synced, hole], "not_fitted": true}),
    );

    assert_eq!(done["result"]["footprints"], json!([synced, hole]));
    let all = objects(&mut engine, "footprint_instance");
    assert!(all.iter().all(|f| f["not_fitted"] == true));
    assert_eq!(units_of(&mut engine, &r1)[0]["not_fitted"], true);
    assert_eq!(pending(&mut engine), 0);

    let swap = engine.run_err(
        "pcb.footprint.set",
        json!({"footprints": [hole], "footprint": RESISTOR}),
    );
    assert_eq!(swap.code, ErrorCode::InvalidParams);
    let taken = engine.run_err(
        "pcb.footprint.set",
        json!({"footprints": [synced], "designator": "H1"}),
    );
    assert_eq!(taken.code, ErrorCode::Conflict);
    let two = engine.run_err(
        "pcb.footprint.set",
        json!({"footprints": [synced, hole], "designator": "X1"}),
    );
    assert_eq!(two.code, ErrorCode::InvalidParams);
    let missing = engine.run_err("pcb.footprint.set", json!({"footprints": [SHEET]}));
    assert_eq!(missing.code, ErrorCode::InvalidParams);
    assert_eq!(missing.details.unwrap()["pointer"], "/footprints/0");
}
