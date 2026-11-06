//! `schematic.selection.move_to_sheet`: moving a selection to another sheet keeps every
//! UUID and Designator, so Sync sees no Component change, and each sheet undoes on its own.

use std::path::{Path, PathBuf};

use reda_engine::command::ErrorCode;
use reda_testkit::{TempDir, TestEngine, test_engine_with_library};
use serde_json::{Value, json};

const SHEET: &str = "00000000-0000-4000-8000-000000000002";
const R_10K: &str = "a0000000-0000-4000-8000-000000000100";
const LM358: &str = "a0000000-0000-4000-8000-000000000120";

fn library() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../reda-schematic/tests/library/Test")
}

fn demo(dir: &TempDir) -> TestEngine {
    let mut engine = test_engine_with_library(&library());
    engine.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
    engine
}

fn at(x: i64, y: i64) -> Value {
    json!({"x": format!("{x}mil"), "y": format!("{y}mil")})
}

fn point(x: i64, y: i64) -> Value {
    json!({"x": x * 25_400, "y": y * 25_400})
}

fn add_sheet(engine: &mut TestEngine) -> String {
    let added = engine.run_ok("document.add", json!({"kind": "schematic"}));
    added["result"]["uuid"].as_str().unwrap().to_owned()
}

/// Places Part `part` on the first sheet; returns the place result.
fn place(engine: &mut TestEngine, part: &str, x: i64, y: i64) -> Value {
    engine.run_ok(
        "schematic.component.place",
        json!({"document": SHEET, "part": part, "at": at(x, y)}),
    )["result"]
        .clone()
}

fn get(engine: &mut TestEngine, uuid: &Value) -> Value {
    engine.run_ok("object.get", json!({"uuid": uuid}))
}

fn objects(engine: &mut TestEngine, document: &str, kind: &str) -> Vec<Value> {
    let out = engine.run_ok("objects.query", json!({"document": document, "type": kind}));
    out["objects"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["object"].clone())
        .collect()
}

fn sync(engine: &mut TestEngine) {
    let list = engine.run_ok("sync.preview", json!({}));
    engine.run_ok("sync.apply", json!({"hash": list["hash"]}));
    assert_eq!(engine.run_ok("sync.preview", json!({}))["count"], 0);
}

/// The kinds of the pending Sync changes, in list order.
fn pending(engine: &mut TestEngine) -> Vec<Value> {
    let list = engine.run_ok("sync.preview", json!({}));
    list["groups"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|g| g["changes"].as_array().unwrap().clone())
        .collect()
}

/// R1 and R2 joined by a Wire named SIG by a Net Label, synced to the PCB. Returns the
/// UUIDs of R1's Unit, R2's Unit, the Wire and the label.
fn two_resistors(engine: &mut TestEngine) -> [Value; 4] {
    let r1 = place(engine, R_10K, 0, 0)["unit"].clone();
    let r2 = place(engine, R_10K, 1000, 0)["unit"].clone();
    let wire = engine.run_ok(
        "schematic.wire.add",
        json!({"document": SHEET, "points": [at(0, 150), at(1000, 150)]}),
    )["result"]["wires"][0]
        .clone();
    let label = engine.run_ok(
        "schematic.net_label.add",
        json!({"document": SHEET, "name": "SIG", "at": at(500, 150)}),
    )["result"]["net_label"]
        .clone();
    sync(engine);
    [r1, r2, wire, label]
}

#[test]
fn moved_objects_keep_their_uuids_and_designators_so_sync_has_nothing_to_do() {
    let dir = TempDir::new("move-to-sheet");
    let mut engine = demo(&dir);
    let other = add_sheet(&mut engine);
    let selection = two_resistors(&mut engine);
    let components: Vec<Value> = selection[..2]
        .iter()
        .map(|u| get(&mut engine, u)["object"]["component"].clone())
        .collect();
    let entries = objects(&mut engine, SHEET, "part_entry");
    let copies = objects(&mut engine, SHEET, "footprint_copy");

    let out = engine.run_ok(
        "schematic.selection.move_to_sheet",
        json!({"objects": selection, "document": other, "at": at(2000, 1000)}),
    );
    assert_eq!(out["result"]["objects"], json!(selection));
    // Nothing changed connectivity: the Net is still SIG with both Pins.
    assert_eq!(out["result"]["nets"], json!([]));
    for uuid in &selection {
        assert_eq!(get(&mut engine, uuid)["document"], other.as_str());
    }
    let r1 = get(&mut engine, &selection[0])["object"].clone();
    assert_eq!(r1["designator"], "R1");
    assert_eq!(r1["component"], components[0]);
    // The lower-left corner of the selection's extent went to `at`.
    assert_eq!(r1["at"], point(2000, 1150));
    let r2 = get(&mut engine, &selection[1])["object"].clone();
    assert_eq!(r2["designator"], "R2");
    assert_eq!(r2["component"], components[1]);
    // The part table went along, UUIDs and all: nothing is left behind.
    assert!(objects(&mut engine, SHEET, "part_entry").is_empty());
    assert!(objects(&mut engine, SHEET, "footprint_copy").is_empty());
    assert_eq!(objects(&mut engine, &other, "part_entry"), entries);
    assert_eq!(objects(&mut engine, &other, "footprint_copy"), copies);
    assert_eq!(pending(&mut engine), Vec::<Value>::new());
}

#[test]
fn objects_keep_their_coordinates_without_at() {
    let dir = TempDir::new("move-to-sheet-in-place");
    let mut engine = demo(&dir);
    let other = add_sheet(&mut engine);
    let r1 = place(&mut engine, R_10K, 300, 700)["unit"].clone();
    engine.run_ok(
        "schematic.selection.move_to_sheet",
        json!({"objects": [r1], "document": other}),
    );
    assert_eq!(get(&mut engine, &r1)["object"]["at"], point(300, 700));
    // Moving onto the sheet the objects are already on is refused.
    let err = engine.run_err(
        "schematic.selection.move_to_sheet",
        json!({"objects": [r1], "document": other}),
    );
    assert_eq!(err.code, ErrorCode::InvalidParams);
}

#[test]
fn a_moved_net_label_leaves_only_net_renames_for_sync() {
    let dir = TempDir::new("move-to-sheet-label");
    let mut engine = demo(&dir);
    let other = add_sheet(&mut engine);
    let [_, _, _, label] = two_resistors(&mut engine);

    let out = engine.run_ok(
        "schematic.selection.move_to_sheet",
        json!({"objects": [label], "document": other}),
    );
    // A Net Label is sheet-local: SIG's Pins are now on an auto-named Net.
    let nets = out["result"]["nets"].as_array().unwrap().clone();
    assert_eq!(nets.len(), 1);
    assert_eq!(nets[0]["from"], "SIG");
    let to = nets[0]["to"].as_str().unwrap().to_owned();
    assert!(to.starts_with("Net-(R1-"), "{to}");
    let changes = pending(&mut engine);
    assert_eq!(changes.len(), 1, "{changes:#?}");
    assert_eq!(changes[0]["kind"], "rename_net");
    assert_eq!(changes[0]["from"], "SIG");
    assert_eq!(changes[0]["to"], to.as_str());
}

#[test]
fn a_single_unit_moves_and_its_part_entry_is_copied_while_the_source_still_needs_it() {
    let dir = TempDir::new("move-to-sheet-unit");
    let mut engine = demo(&dir);
    let other = add_sheet(&mut engine);
    let a = place(&mut engine, LM358, 0, 0);
    let b = engine.run_ok(
        "schematic.unit.place",
        json!({"document": SHEET, "component": a["component"]["uuid"], "at": at(2000, 0)}),
    )["result"]["unit"]
        .clone();
    sync(&mut engine);
    let entry = objects(&mut engine, SHEET, "part_entry")[0].clone();

    engine.run_ok(
        "schematic.selection.move_to_sheet",
        json!({"objects": [b], "document": other}),
    );
    let moved = get(&mut engine, &b)["object"].clone();
    assert_eq!(moved["designator"], "U1");
    assert_eq!(moved["component"], a["component"]);
    // Unit A still uses its entry, so the target gets a copy under a fresh UUID.
    assert_eq!(
        objects(&mut engine, SHEET, "part_entry"),
        std::slice::from_ref(&entry)
    );
    let copied = objects(&mut engine, &other, "part_entry");
    assert_eq!(copied.len(), 1);
    assert_ne!(copied[0]["uuid"], entry["uuid"]);
    assert_eq!(moved["part"], copied[0]["uuid"]);
    let components: Vec<Value> = pending(&mut engine)
        .into_iter()
        .filter(|c| c["kind"].as_str().unwrap().ends_with("component"))
        .collect();
    assert_eq!(components, Vec::<Value>::new());
}

#[test]
fn each_sheet_undoes_on_its_own_and_undo_refuses_to_restore_a_uuid_now_elsewhere() {
    let dir = TempDir::new("move-to-sheet-undo");
    let mut engine = demo(&dir);
    let added = engine.run_ok(
        "document.add",
        json!({"kind": "schematic", "title": "Power"}),
    );
    let other = added["result"]["uuid"].as_str().unwrap().to_owned();
    let r1 = place(&mut engine, R_10K, 0, 0)["unit"].clone();
    let before = get(&mut engine, &r1);
    engine.run_ok(
        "schematic.selection.move_to_sheet",
        json!({"objects": [r1], "document": other, "at": at(1000, 1000)}),
    );
    let moved = get(&mut engine, &r1);
    // Undoing on the source sheet first would bring R1 back while it is on the other one.
    let err = engine.run_err("edit.undo", json!({"document": SHEET}));
    assert_eq!(err.code, ErrorCode::Conflict);
    assert!(err.message.contains("\"Power\""), "{}", err.message);
    assert!(err.message.contains("undo on"), "{}", err.message);
    assert_eq!(err.details.unwrap()["first"], other.as_str());
    assert_eq!(get(&mut engine, &r1), moved);

    engine.run_ok("edit.undo", json!({"document": other}));
    assert!(objects(&mut engine, &other, "unit").is_empty());
    engine.run_ok("edit.undo", json!({"document": SHEET}));
    assert_eq!(get(&mut engine, &r1), before);

    // Redo goes the other way: the source sheet first.
    let err = engine.run_err("edit.redo", json!({"document": other}));
    assert_eq!(err.code, ErrorCode::Conflict);
    assert!(err.message.contains("redo on"), "{}", err.message);
    assert_eq!(get(&mut engine, &r1), before);
    engine.run_ok("edit.redo", json!({"document": SHEET}));
    engine.run_ok("edit.redo", json!({"document": other}));
    assert_eq!(get(&mut engine, &r1), moved);
}
