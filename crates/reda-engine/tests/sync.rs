//! Sync: `sync.preview` and `sync.apply` bring schematic Components, their data and their
//! Nets to the PCB Document's Footprint instances.

use std::path::{Path, PathBuf};

use reda_engine::command::ErrorCode;
use reda_testkit::{TempDir, TestEngine, test_engine_with_library};
use serde_json::{Value, json};

const SHEET: &str = "00000000-0000-4000-8000-000000000002";
const PCB: &str = "00000000-0000-4000-8000-000000000003";
const R_10K: &str = "a0000000-0000-4000-8000-000000000100";
const R_0603: &str = "a0000000-0000-4000-8000-000000000040";
const R_SHUNT: &str = "a0000000-0000-4000-8000-000000000150";
const R_KELVIN: &str = "a0000000-0000-4000-8000-000000000070";
/// A Net Tie Part (In BOM and In CPL both off) on the resistor's Symbol and 0603.
const NET_TIE: &str = "a0000000-0000-4000-8000-000000000170";

fn library() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../reda-schematic/tests/library/Test")
}

fn demo(dir: &TempDir) -> TestEngine {
    let mut engine = test_engine_with_library(&library());
    engine.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
    engine
}

/// Places a Component on the first sheet at (`x`, `y`) mm and returns its Component UUID.
fn place(engine: &mut TestEngine, part: &str, x: i64, y: i64) -> String {
    engine.run_ok(
        "schematic.component.place",
        json!({"document": SHEET, "part": part,
               "at": {"x": format!("{x}mm"), "y": format!("{y}mm")}}),
    )["result"]["component"]["uuid"]
        .as_str()
        .unwrap()
        .to_owned()
}

fn preview(engine: &mut TestEngine) -> Value {
    engine.run_ok("sync.preview", json!({}))
}

/// Every change of the preview, in list order.
fn changes(preview: &Value) -> Vec<Value> {
    preview["groups"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|g| g["changes"].as_array().unwrap().clone())
        .collect()
}

fn instances(engine: &mut TestEngine) -> Vec<Value> {
    engine.run_ok("objects.query", json!({"type": "footprint_instance"}))["objects"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["object"].clone())
        .collect()
}

#[test]
fn a_new_component_becomes_a_footprint_instance() {
    let dir = TempDir::new("sync-add");
    let mut engine = demo(&dir);
    let r1 = place(&mut engine, R_10K, 0, 0);

    let list = preview(&mut engine);
    assert_eq!(list["count"], 1);
    assert_eq!(list["groups"][0]["group"], "components");
    let add = &changes(&list)[0];
    assert_eq!(add["kind"], "add_component");
    assert_eq!(add["component"]["uuid"], r1.as_str());
    assert_eq!(add["designator"], "R1");
    assert_eq!(add["footprint"], R_0603);

    engine.run_ok("sync.apply", json!({"hash": list["hash"]}));

    let placed = instances(&mut engine);
    assert_eq!(placed.len(), 1);
    assert_eq!(placed[0]["component"]["uuid"], r1.as_str());
    assert_eq!(placed[0]["designator"], "R1");
    assert_eq!(placed[0]["fields"]["value"], "10k");
    assert_eq!(placed[0]["pads"].as_array().unwrap().len(), 2);
    assert_eq!(preview(&mut engine)["count"], 0);
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

fn label(engine: &mut TestEngine, name: &str, x: f64, y: f64) -> String {
    engine.run_ok(
        "schematic.net_label.add",
        json!({"document": SHEET, "name": name,
               "at": {"x": format!("{x}mm"), "y": format!("{y}mm")}}),
    )["result"]["net_label"]
        .as_str()
        .unwrap()
        .to_owned()
}

fn sync(engine: &mut TestEngine) -> Value {
    let list = preview(engine);
    engine.run_ok("sync.apply", json!({"hash": list["hash"]}))
}

fn of_kind(preview: &Value, kind: &str) -> Vec<Value> {
    changes(preview)
        .into_iter()
        .filter(|c| c["kind"] == kind)
        .collect()
}

fn nets(engine: &mut TestEngine) -> Vec<Value> {
    engine.run_ok("objects.query", json!({"type": "net"}))["objects"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["object"].clone())
        .collect()
}

/// R1 at the origin and R2 10 mm to the right, their pin 2s wired together and labelled
/// SIG. Pin 1 is 3.81 mm above a resistor's origin and pin 2 3.81 mm below.
fn two_wired_resistors(engine: &mut TestEngine) -> (String, String) {
    let r1 = place(engine, R_10K, 0, 0);
    let r2 = place(engine, R_10K, 10, 0);
    wire(engine, &[(0.0, -3.81), (10.0, -3.81)]);
    label(engine, "SIG", 5.0, -3.81);
    (r1, r2)
}

#[test]
fn a_wired_net_becomes_a_pcb_net_in_the_default_class_and_its_pads_join_it() {
    let dir = TempDir::new("sync-net");
    let mut engine = demo(&dir);
    two_wired_resistors(&mut engine);

    let list = preview(&mut engine);
    let groups: Vec<&str> = list["groups"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| g["group"].as_str().unwrap())
        .collect();
    assert_eq!(groups, ["components", "nets", "pad_nets"]);
    let add_net = &of_kind(&list, "add_net")[0];
    assert_eq!(add_net["name"], "SIG");
    // Lone unwired Pins carry no PCB Net: only the two pin 2 Pads move.
    let pads = of_kind(&list, "pad_net");
    assert_eq!(pads.len(), 2);
    for pad in &pads {
        assert_eq!(pad["number"], "2");
        assert_eq!(pad["to"]["name"], "SIG");
        assert!(pad["requires"].as_array().unwrap().contains(&add_net["id"]));
    }

    sync(&mut engine);

    let nets = nets(&mut engine);
    assert_eq!(nets.len(), 1);
    assert_eq!(nets[0]["name"], "SIG");
    let default_class = engine.run_ok("objects.query", json!({"type": "net_class"}))["objects"][0]
        ["object"]["uuid"]
        .clone();
    assert_eq!(nets[0]["class"], default_class);
    for instance in instances(&mut engine) {
        let on_sig = instance["pads"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|p| p["net"] == nets[0]["uuid"])
            .count();
        assert_eq!(on_sig, 1);
    }
}

#[test]
fn unticking_a_change_unticks_the_changes_that_depend_on_it() {
    let dir = TempDir::new("sync-cascade");
    let mut engine = demo(&dir);
    two_wired_resistors(&mut engine);
    let list = preview(&mut engine);
    let add_net = of_kind(&list, "add_net")[0]["id"].clone();

    let done = engine.run_ok(
        "sync.apply",
        json!({"hash": list["hash"], "exclude": [add_net]}),
    )["result"]
        .clone();

    assert_eq!(done["left_out"].as_array().unwrap().len(), 3);
    assert!(nets(&mut engine).is_empty());
    assert_eq!(instances(&mut engine).len(), 2);
    // What was left out shows up again.
    let again = preview(&mut engine);
    assert_eq!(again["count"], 3);
    assert_eq!(of_kind(&again, "add_net")[0]["id"], add_net);
}

#[test]
fn every_change_has_a_stable_id_and_the_list_a_hash() {
    let dir = TempDir::new("sync-ids");
    let mut engine = demo(&dir);
    two_wired_resistors(&mut engine);
    let first = preview(&mut engine);
    assert_eq!(preview(&mut engine), first);

    // Another Component changes the list's hash but not the ids already there.
    place(&mut engine, R_10K, 20, 0);
    let second = preview(&mut engine);
    assert_ne!(second["hash"], first["hash"]);
    let ids = |p: &Value| -> Vec<Value> { changes(p).iter().map(|c| c["id"].clone()).collect() };
    let second_ids = ids(&second);
    for id in ids(&first) {
        assert!(second_ids.contains(&id));
    }
}

#[test]
fn apply_refuses_a_stale_preview_and_unknown_ids() {
    let dir = TempDir::new("sync-conflict");
    let mut engine = demo(&dir);
    place(&mut engine, R_10K, 0, 0);
    let stale = preview(&mut engine);
    place(&mut engine, R_10K, 10, 0);

    let err = engine.run_err("sync.apply", json!({"hash": stale["hash"]}));
    assert_eq!(err.code, ErrorCode::Conflict);
    assert!(instances(&mut engine).is_empty());

    let fresh = preview(&mut engine);
    let err = engine.run_err(
        "sync.apply",
        json!({"hash": fresh["hash"], "exclude": ["nope"]}),
    );
    assert_eq!(err.code, ErrorCode::InvalidParams);
}

#[test]
fn sync_needs_a_pcb_document() {
    let dir = TempDir::new("sync-no-pcb");
    let mut engine = demo(&dir);
    engine.run_ok("document.delete", json!({"document": PCB}));
    let err = engine.run_err("sync.preview", json!({}));
    assert_eq!(err.code, ErrorCode::Unavailable);
}

fn rename_label(engine: &mut TestEngine, label: &str, x: f64, y: f64, name: &str) -> String {
    engine.run_ok("schematic.delete", json!({"objects": [label]}));
    self::label(engine, name, x, y)
}

#[test]
fn a_renamed_net_keeps_its_uuid() {
    let dir = TempDir::new("sync-rename");
    let mut engine = demo(&dir);
    place(&mut engine, R_10K, 0, 0);
    place(&mut engine, R_10K, 10, 0);
    wire(&mut engine, &[(0.0, -3.81), (10.0, -3.81)]);
    let sig = label(&mut engine, "SIG", 5.0, -3.81);
    sync(&mut engine);
    let before = nets(&mut engine)[0]["uuid"].clone();

    rename_label(&mut engine, &sig, 5.0, -3.81, "DATA");
    let list = preview(&mut engine);
    assert_eq!(list["count"], 1);
    let rename = &of_kind(&list, "rename_net")[0];
    assert_eq!(rename["net"], before);
    assert_eq!(rename["from"], "SIG");
    assert_eq!(rename["to"], "DATA");

    sync(&mut engine);
    let after = nets(&mut engine);
    assert_eq!(after.len(), 1);
    assert_eq!(after[0]["uuid"], before);
    assert_eq!(after[0]["name"], "DATA");
}

#[test]
fn an_auto_named_net_follows_its_designator_as_a_rename() {
    let dir = TempDir::new("sync-auto-name");
    let mut engine = demo(&dir);
    let r1 = place(&mut engine, R_10K, 0, 0);
    place(&mut engine, R_10K, 10, 0);
    wire(&mut engine, &[(0.0, -3.81), (10.0, -3.81)]);
    sync(&mut engine);
    assert_eq!(nets(&mut engine)[0]["name"], "Net-(R1-2)");

    engine.run_ok(
        "schematic.component.set",
        json!({"components": [r1], "designator": "R7"}),
    );
    let list = preview(&mut engine);
    assert_eq!(of_kind(&list, "rename_net")[0]["to"], "Net-(R2-2)");
    assert_eq!(of_kind(&list, "designator")[0]["to"], "R7");
    assert!(of_kind(&list, "add_net").is_empty());
}

#[test]
fn a_split_net_keeps_its_uuid_on_the_larger_part() {
    let dir = TempDir::new("sync-split");
    let mut engine = demo(&dir);
    // Three resistors on SIG through their pin 2s.
    for x in [0, 10, 20] {
        place(&mut engine, R_10K, x, 0);
    }
    wire(&mut engine, &[(0.0, -3.81), (10.0, -3.81)]);
    wire(&mut engine, &[(10.0, -3.81), (20.0, -3.81)]);
    label(&mut engine, "SIG", 5.0, -3.81);
    sync(&mut engine);
    let sig = nets(&mut engine)[0]["uuid"].clone();

    // Cut R3 off; it joins R4 on a new Net.
    let wires = engine.run_ok("objects.query", json!({"type": "wire"}))["objects"].clone();
    let right = wires
        .as_array()
        .unwrap()
        .iter()
        .find(|w| {
            (w["object"]["vertices"].as_array().unwrap().iter()).any(|v| v["x"] == 20_000_000)
        })
        .unwrap()["object"]["uuid"]
        .clone();
    engine.run_ok("schematic.delete", json!({"objects": [right]}));
    place(&mut engine, R_10K, 30, 0);
    wire(&mut engine, &[(20.0, -3.81), (30.0, -3.81)]);

    let list = preview(&mut engine);
    assert!(of_kind(&list, "rename_net").is_empty());
    assert!(of_kind(&list, "remove_net").is_empty());
    assert_eq!(of_kind(&list, "add_net")[0]["name"], "Net-(R3-2)");
    sync(&mut engine);
    let after = nets(&mut engine);
    assert_eq!(after.len(), 2);
    assert!(after.iter().any(|n| n["uuid"] == sig && n["name"] == "SIG"));
}

#[test]
fn merged_nets_keep_the_one_with_the_largest_overlap() {
    let dir = TempDir::new("sync-merge");
    let mut engine = demo(&dir);
    for x in [0, 10, 20, 30, 40] {
        place(&mut engine, R_10K, x, 0);
    }
    // BIG on R1–R3, SMALL on R4–R5.
    wire(&mut engine, &[(0.0, -3.81), (20.0, -3.81)]);
    label(&mut engine, "BIG", 5.0, -3.81);
    wire(&mut engine, &[(30.0, -3.81), (40.0, -3.81)]);
    label(&mut engine, "SMALL", 35.0, -3.81);
    sync(&mut engine);
    let before = nets(&mut engine);
    let big = before.iter().find(|n| n["name"] == "BIG").unwrap()["uuid"].clone();
    let small = before.iter().find(|n| n["name"] == "SMALL").unwrap()["uuid"].clone();

    // Copper on SMALL moves to BIG when the Nets merge (Sync never deletes copper).
    let track = engine.run_ok(
        "pcb.track.add",
        json!({"layer": "copper_top", "net": small, "points": [
            {"x": "100mm", "y": 0}, {"x": "110mm", "y": 0}]}),
    )["result"]["tracks"][0]
        .clone();
    let via = engine.run_ok(
        "pcb.via.add",
        json!({"at": {"x": "110mm", "y": 0}, "net": small}),
    )["result"]["uuid"]
        .clone();
    let zone = engine.run_ok(
        "pcb.zone.add",
        json!({"layer": "copper_bottom", "net": small, "vertices": [
            {"x": "100mm", "y": 0}, {"x": "110mm", "y": 0}, {"x": "110mm", "y": "10mm"}]}),
    )["result"]["uuid"]
        .clone();

    wire(&mut engine, &[(20.0, -3.81), (30.0, -3.81)]);
    let list = preview(&mut engine);
    let removed = &of_kind(&list, "remove_net")[0];
    assert_eq!(removed["net"], small);
    assert_eq!(removed["merge_into"], big);
    // The removal waits for the Pads to leave the Net.
    assert_eq!(removed["requires"].as_array().unwrap().len(), 2);

    sync(&mut engine);
    let after = nets(&mut engine);
    assert_eq!(after.len(), 1);
    assert_eq!(after[0]["uuid"], big);
    for object in [track, via, zone] {
        let got = engine.run_ok("object.get", json!({"uuid": object}));
        assert_eq!(got["object"]["net"], big, "{got}");
    }
}

#[test]
fn pads_are_found_through_the_pin_pad_map() {
    let dir = TempDir::new("sync-pin-map");
    let mut engine = demo(&dir);
    // R_Shunt on its Kelvin Footprint: pin 1 → Pads 1 and 3, pin 2 → Pads 2 and 4.
    let shunt = place(&mut engine, R_SHUNT, 0, 0);
    engine.run_ok(
        "schematic.component.set",
        json!({"components": [shunt], "footprint": R_KELVIN}),
    );
    place(&mut engine, R_10K, 10, 0);
    wire(&mut engine, &[(0.0, -3.81), (10.0, -3.81)]);
    label(&mut engine, "SENSE", 5.0, -3.81);

    let list = preview(&mut engine);
    let mut numbers: Vec<String> = of_kind(&list, "pad_net")
        .iter()
        .filter(|p| p["designator"] == "R1" && p["to"]["name"] == "SENSE")
        .map(|p| p["number"].as_str().unwrap().to_owned())
        .collect();
    numbers.sort();
    assert_eq!(numbers, ["2", "4"]);
    // The unwired pin 1 still ties its two Pads together on a Net of their own.
    let pin_1 = &of_kind(&list, "add_net")
        .into_iter()
        .find(|n| n["name"] != "SENSE")
        .unwrap()["name"];
    assert_eq!(pin_1, "Net-(R1-1)");
}

#[test]
fn field_and_not_fitted_changes_propagate() {
    let dir = TempDir::new("sync-data");
    let mut engine = demo(&dir);
    let r1 = place(&mut engine, R_10K, 0, 0);
    sync(&mut engine);

    engine.run_ok(
        "schematic.component.set",
        json!({"components": [r1], "value": "4k7", "not_fitted": true,
               "fields": {"LCSC": "C23162"}}),
    );
    let list = preview(&mut engine);
    assert_eq!(list["groups"][0]["group"], "component_data");
    let fields = &of_kind(&list, "fields")[0];
    assert_eq!(fields["from"]["value"], "10k");
    assert_eq!(fields["to"]["value"], "4k7");
    assert_eq!(of_kind(&list, "not_fitted")[0]["to"], true);

    sync(&mut engine);
    let instance = &instances(&mut engine)[0];
    assert_eq!(instance["fields"]["value"], "4k7");
    assert_eq!(instance["fields"]["custom"][0]["value"], "C23162");
    assert_eq!(instance["not_fitted"], true);
}

#[test]
fn component_type_in_bom_and_in_cpl_are_carried_to_the_board() {
    let dir = TempDir::new("sync-flags");
    let mut engine = demo(&dir);
    let r1 = place(&mut engine, R_10K, 0, 0);
    place(&mut engine, NET_TIE, 20, 0);
    sync(&mut engine);
    let by_designator = |instances: &[Value], d: &str| {
        instances
            .iter()
            .find(|i| i["designator"] == d)
            .unwrap()
            .clone()
    };
    let all = instances(&mut engine);
    let (r, nt) = (by_designator(&all, "R1"), by_designator(&all, "R2"));
    assert_eq!(
        (&r["component_type"], &r["in_bom"], &r["in_cpl"]),
        (&json!("standard"), &json!(true), &json!(true))
    );
    assert_eq!(
        (&nt["component_type"], &nt["in_bom"], &nt["in_cpl"]),
        (&json!("net_tie"), &json!(false), &json!(false))
    );

    engine.run_ok(
        "schematic.component.set",
        json!({"components": [r1], "in_bom": false, "in_cpl": false}),
    );
    let list = preview(&mut engine);
    assert_eq!(list["count"], 2, "the board is out of sync: {list}");
    let in_bom = &of_kind(&list, "in_bom")[0];
    assert_eq!(
        (&in_bom["from"], &in_bom["to"]),
        (&json!(true), &json!(false))
    );
    assert_eq!(of_kind(&list, "in_cpl")[0]["to"], false);

    sync(&mut engine);
    let r = by_designator(&instances(&mut engine), "R1");
    assert_eq!((&r["in_bom"], &r["in_cpl"]), (&json!(false), &json!(false)));
    assert_eq!(preview(&mut engine)["count"], 0);
}

#[test]
fn only_a_mechanical_part_is_placed_straight_onto_the_board() {
    let dir = TempDir::new("sync-net-tie-board");
    let mut engine = demo(&dir);
    for part in [NET_TIE, R_10K] {
        let err = engine.run_err(
            "pcb.mechanical.place",
            json!({"part": part, "at": {"x": "0mm", "y": "0mm"}}),
        );
        assert_eq!(err.code, ErrorCode::Rejected, "{}", err.message);
        assert!(err.message.contains("through Sync"), "{}", err.message);
    }
}

#[test]
fn a_net_tie_refuses_not_fitted() {
    let dir = TempDir::new("sync-net-tie-not-fitted");
    let mut engine = demo(&dir);
    let tie = place(&mut engine, NET_TIE, 0, 0);
    let err = engine.run_err(
        "schematic.component.set",
        json!({"components": [tie], "not_fitted": true}),
    );
    assert_eq!(err.code, ErrorCode::Rejected, "{}", err.message);
    assert!(err.message.contains("Net Tie"), "{}", err.message);
    engine.run_ok(
        "schematic.component.set",
        json!({"components": [tie], "not_fitted": false}),
    );
}

#[test]
fn a_footprint_change_swaps_in_place_and_reassigns_pad_nets() {
    let dir = TempDir::new("sync-footprint");
    let mut engine = demo(&dir);
    let shunt = place(&mut engine, R_SHUNT, 0, 0);
    place(&mut engine, R_10K, 10, 0);
    wire(&mut engine, &[(0.0, -3.81), (10.0, -3.81)]);
    label(&mut engine, "SENSE", 5.0, -3.81);
    sync(&mut engine);
    let before = instances(&mut engine)
        .into_iter()
        .find(|i| i["designator"] == "R1")
        .unwrap();

    engine.run_ok(
        "schematic.component.set",
        json!({"components": [shunt], "footprint": R_KELVIN}),
    );
    let list = preview(&mut engine);
    let swap = &of_kind(&list, "change_footprint")[0];
    assert_eq!(swap["from"]["name"], "R_0603");
    assert_eq!(swap["to"]["name"], "R_2512_Kelvin");
    // Every Pad Net of the new Footprint waits for the swap.
    for pad in of_kind(&list, "pad_net") {
        assert!(pad["requires"].as_array().unwrap().contains(&swap["id"]));
    }

    sync(&mut engine);
    let after = instances(&mut engine)
        .into_iter()
        .find(|i| i["designator"] == "R1")
        .unwrap();
    assert_eq!(after["uuid"], before["uuid"]);
    assert_eq!(after["at"], before["at"]);
    let on_net = after["pads"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|p| p.get("net").is_some())
        .count();
    assert_eq!(on_net, 4);
    // Only the Footprints in use stay in the PCB's part table.
    let copies = engine.run_ok(
        "objects.query",
        json!({"type": "footprint_copy", "document": PCB}),
    )["objects"]
        .as_array()
        .unwrap()
        .len();
    assert_eq!(copies, 2);
    assert_eq!(preview(&mut engine)["count"], 0);
}

#[test]
fn a_deleted_component_is_proposed_for_removal() {
    let dir = TempDir::new("sync-remove");
    let mut engine = demo(&dir);
    place(&mut engine, R_10K, 0, 0);
    place(&mut engine, R_10K, 10, 0);
    wire(&mut engine, &[(0.0, -3.81), (10.0, -3.81)]);
    sync(&mut engine);
    let net = nets(&mut engine)[0]["uuid"].clone();
    let track = engine.run_ok(
        "pcb.track.add",
        json!({"layer": "copper_top", "net": net, "points": [
            {"x": "100mm", "y": 0}, {"x": "110mm", "y": 0}]}),
    )["result"]["tracks"][0]
        .clone();

    let units = engine.run_ok("objects.query", json!({"type": "unit"}))["objects"].clone();
    let r2 = units
        .as_array()
        .unwrap()
        .iter()
        .find(|u| u["object"]["designator"] == "R2")
        .unwrap()["object"]["uuid"]
        .clone();
    engine.run_ok("schematic.delete", json!({"objects": [r2]}));

    let list = preview(&mut engine);
    let removal = &of_kind(&list, "remove_component")[0];
    assert_eq!(removal["designator"], "R2");
    // The unnamed Net now reaches one Pad only and goes; R1's Pad leaves it first.
    let net = &of_kind(&list, "remove_net")[0];
    assert!(net["requires"].as_array().unwrap().contains(&removal["id"]));

    sync(&mut engine);
    let left = instances(&mut engine);
    assert_eq!(left.len(), 1);
    assert_eq!(left[0]["designator"], "R1");
    assert!(nets(&mut engine).is_empty());
    let got = engine.run_ok("object.get", json!({"uuid": track}));
    assert_eq!(got["object"]["type"], "track");
    assert!(
        got["object"].get("net").is_none(),
        "copper on a removed Net stays, on no Net: {got}"
    );
}

#[test]
fn new_footprints_cluster_outside_the_board_by_sheet_then_designator() {
    let dir = TempDir::new("sync-place");
    let mut engine = test_engine_with_library(&library());
    engine.run_ok(
        "project.new",
        json!({"path": dir.path(), "name": "Demo",
               "board": {"width": "50mm", "height": "30mm"}}),
    );
    let sheet_2 =
        engine.run_ok("document.add", json!({"kind": "schematic"}))["result"]["uuid"].clone();
    place(&mut engine, R_10K, 0, 0); // R1
    engine.run_ok(
        "schematic.component.place",
        json!({"document": sheet_2, "part": R_10K, "at": {"x": 0, "y": 0}}),
    ); // R2, on sheet 2
    place(&mut engine, R_10K, 10, 0); // R3
    sync(&mut engine);

    let at = |engine: &mut TestEngine, designator: &str| -> (i64, i64) {
        let i = instances(engine)
            .into_iter()
            .find(|i| i["designator"] == designator)
            .unwrap();
        (
            i["at"]["x"].as_i64().unwrap(),
            i["at"]["y"].as_i64().unwrap(),
        )
    };
    let (r1, r2, r3) = (
        at(&mut engine, "R1"),
        at(&mut engine, "R2"),
        at(&mut engine, "R3"),
    );
    // Right of the 50 mm board, sheet 1's block (R1 then R3) above sheet 2's (R2), all
    // from one left edge. Two small boxes make a row narrower than both, so R3 wraps.
    assert!(r1.0 > 50_000_000);
    assert_eq!(r3.0, r1.0);
    assert!(r3.1 < r1.1);
    assert!(r2.1 < r3.1);
    assert_eq!(r2.0, r1.0);

    // The next Sync's cluster goes beside this one.
    place(&mut engine, R_10K, 20, 0); // R4
    sync(&mut engine);
    assert!(at(&mut engine, "R4").0 > r3.0);
}

#[test]
fn without_an_outline_the_first_cluster_starts_at_the_origin() {
    let dir = TempDir::new("sync-origin");
    let mut engine = demo(&dir);
    place(&mut engine, R_10K, 0, 0);
    sync(&mut engine);
    let at = &instances(&mut engine)[0]["at"];
    // R_0603's Pads reach 1.2 mm left of and 0.475 mm above its origin, plus margin.
    assert!(at["x"].as_i64().unwrap() > 0 && at["x"].as_i64().unwrap() < 2_000_000);
    assert!(at["y"].as_i64().unwrap() < 0 && at["y"].as_i64().unwrap() > -2_000_000);
}

#[test]
fn the_pending_count_is_available_to_the_ui_and_drc() {
    let dir = TempDir::new("sync-count");
    let mut engine = demo(&dir);
    two_wired_resistors(&mut engine);
    let project = engine.project().unwrap();
    assert_eq!(reda_engine::sync::pending_count(project), 5);
    let violation = reda_engine::sync::out_of_sync_violation(project).unwrap();
    assert_eq!(violation.check, "out_of_sync");
    assert_eq!(violation.severity, reda_engine::model::Severity::Warning);

    sync(&mut engine);
    let project = engine.project().unwrap();
    assert_eq!(reda_engine::sync::pending_count(project), 0);
    assert!(reda_engine::sync::out_of_sync_violation(project).is_none());
}

#[test]
fn net_links_pair_each_synced_schematic_net_with_its_pcb_net() {
    let dir = TempDir::new("sync-links");
    let mut engine = demo(&dir);
    two_wired_resistors(&mut engine);
    let project = engine.project().unwrap();
    // Nothing is linked before the board has the Net.
    assert!(reda_engine::sync::net_links(project).is_empty());

    sync(&mut engine);
    let sig = nets(&mut engine)[0]["uuid"].as_str().unwrap().to_owned();
    let wire =
        engine.run_ok("objects.query", json!({"type": "wire"}))["objects"][0]["object"]["uuid"]
            .as_str()
            .unwrap()
            .to_owned();
    let net_label = engine.run_ok("objects.query", json!({"type": "net_label"}))["objects"][0]
        ["object"]["uuid"]
        .as_str()
        .unwrap()
        .to_owned();
    let project = engine.project().unwrap();
    let links = reda_engine::sync::net_links(project);
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].pcb.to_string(), sig);
    assert_eq!(links[0].name, "SIG");
    let mut objects: Vec<String> = links[0].objects.iter().map(|u| u.to_string()).collect();
    objects.sort();
    // The Units with a Pin on the Net come with it.
    let units = engine.run_ok("objects.query", json!({"type": "unit"}))["objects"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["object"]["uuid"].as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    assert_eq!(units.len(), 2);
    let mut expected = vec![wire, net_label];
    expected.extend(units);
    expected.sort();
    assert_eq!(objects, expected);
}

#[test]
fn footprint_instances_survive_save_and_open() {
    let dir = TempDir::new("sync-save");
    let mut engine = demo(&dir);
    two_wired_resistors(&mut engine);
    sync(&mut engine);
    engine.run_ok("project.save", json!({}));
    let saved = instances(&mut engine);

    let mut other = test_engine_with_library(&library());
    other.run_ok("project.open", json!({"path": dir.path().join("Demo")}));
    assert_eq!(instances(&mut other), saved);
    assert_eq!(preview(&mut other)["count"], 0);
}

#[test]
fn applying_is_one_undo_entry_on_the_pcb_document() {
    let dir = TempDir::new("sync-undo");
    let mut engine = demo(&dir);
    two_wired_resistors(&mut engine);
    let done = sync(&mut engine);
    let scopes: Vec<&String> = done["changes"].as_object().unwrap().keys().collect();
    assert_eq!(scopes, [PCB]);

    engine.run_ok("edit.undo", json!({"document": PCB}));
    assert!(instances(&mut engine).is_empty());
    assert!(nets(&mut engine).is_empty());
    assert_eq!(preview(&mut engine)["count"], 5);
}

#[test]
fn removing_a_component_deletes_its_footprint_but_leaves_its_copper() {
    let dir = TempDir::new("sync-remove-copper");
    let mut engine = demo(&dir);
    // R1–R3 on SIG through their pin 2s; R3 is then deleted, SIG lives on.
    for x in [0, 10, 20] {
        place(&mut engine, R_10K, x, 0);
    }
    wire(&mut engine, &[(0.0, -3.81), (20.0, -3.81)]);
    label(&mut engine, "SIG", 5.0, -3.81);
    sync(&mut engine);
    let sig = nets(&mut engine)
        .into_iter()
        .find(|n| n["name"] == "SIG")
        .unwrap()["uuid"]
        .clone();
    let r3 = instances(&mut engine)
        .into_iter()
        .find(|i| i["designator"] == "R3")
        .unwrap();
    // Copper routed up to R3's Footprint.
    let at = &r3["at"];
    let track = engine.run_ok(
        "pcb.track.add",
        json!({"layer": "copper_top", "net": sig, "points": [
            {"x": at["x"], "y": at["y"]}, {"x": at["x"], "y": 50_000_000}]}),
    )["result"]["tracks"][0]
        .clone();
    let via = engine.run_ok(
        "pcb.via.add",
        json!({"at": {"x": at["x"], "y": 50_000_000}, "net": sig}),
    )["result"]["uuid"]
        .clone();

    let unit = engine.run_ok("objects.query", json!({"type": "unit"}))["objects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|u| u["object"]["designator"] == "R3")
        .unwrap()["object"]["uuid"]
        .clone();
    engine.run_ok("schematic.delete", json!({"objects": [unit]}));
    let list = preview(&mut engine);
    assert_eq!(of_kind(&list, "remove_component").len(), 1);
    assert!(of_kind(&list, "remove_net").is_empty(), "{list}");

    sync(&mut engine);
    let left: Vec<Value> = instances(&mut engine)
        .iter()
        .map(|i| i["designator"].clone())
        .collect();
    assert_eq!(left, ["R1", "R2"]);
    let gone = engine.run_err("object.get", json!({"uuid": r3["uuid"]}));
    assert_eq!(gone.code, ErrorCode::NotFound);
    for object in [track, via] {
        let got = engine.run_ok("object.get", json!({"uuid": object}));
        assert_eq!(got["object"]["net"], sig, "attached copper stays: {got}");
    }
}

#[test]
fn duplicate_designators_do_not_block_sync() {
    let dir = TempDir::new("sync-duplicates");
    let mut engine = demo(&dir);
    let r1 = place(&mut engine, R_10K, 0, 0);
    let r2 = place(&mut engine, R_10K, 10, 0);
    wire(&mut engine, &[(0.0, -3.81), (10.0, -3.81)]);
    label(&mut engine, "SIG", 5.0, -3.81);
    // Two Components named R1, as a hand edit or a merge would leave them.
    let mut engine = hand_edit_sheet(&mut engine, &dir, |text| {
        text.replace("\"designator\":\"R2\"", "\"designator\":\"R1\"")
    });

    let list = preview(&mut engine);
    assert_eq!(of_kind(&list, "add_component").len(), 2);
    sync(&mut engine);
    let placed = instances(&mut engine);
    let mut components: Vec<&str> = placed
        .iter()
        .map(|i| i["component"]["uuid"].as_str().unwrap())
        .collect();
    components.sort_unstable();
    let mut expected = [r1.as_str(), r2.as_str()];
    expected.sort_unstable();
    assert_eq!(components, expected);
    assert!(placed.iter().all(|i| i["designator"] == "R1"));
    assert_eq!(nets(&mut engine).len(), 1);
    assert_eq!(preview(&mut engine)["count"], 0);
}

/// BIG on R1–R3 and SMALL on R4–R5, synced; returns (BIG, SMALL). Wiring R3 to R4 then
/// merges SMALL into BIG.
fn big_and_small(engine: &mut TestEngine) -> (Value, Value) {
    for x in [0, 10, 20, 30, 40] {
        place(engine, R_10K, x, 0);
    }
    wire(engine, &[(0.0, -3.81), (20.0, -3.81)]);
    label(engine, "BIG", 5.0, -3.81);
    wire(engine, &[(30.0, -3.81), (40.0, -3.81)]);
    label(engine, "SMALL", 35.0, -3.81);
    sync(engine);
    let nets = nets(engine);
    let uuid = |name: &str| nets.iter().find(|n| n["name"] == name).unwrap()["uuid"].clone();
    (uuid("BIG"), uuid("SMALL"))
}

/// A Mechanical Part (no Component link) with one Pad on `net`; returns (instance, Pad).
fn mechanical(engine: &mut TestEngine, net: &Value) -> (Value, Value) {
    let added = engine.run_ok(
        "test.footprint.add",
        json!({"at": {"x": "200mm", "y": 0}, "pads": [
            {"at": {"x": 0, "y": 0}, "width": "1mm", "height": "1mm", "net": net}]}),
    )["result"]
        .clone();
    (added["footprint"].clone(), added["pads"][0].clone())
}

fn pad_net(engine: &mut TestEngine, instance: &Value, pad: &Value) -> Value {
    let got = engine.run_ok("object.get", json!({"uuid": instance}));
    got["object"]["pads"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["pad"] == *pad)
        .unwrap()
        .get("net")
        .cloned()
        .unwrap_or(Value::Null)
}

/// Like [`demo`], plus the test-only Commands (`test.footprint.add`).
fn demo_with_test_commands(dir: &TempDir) -> TestEngine {
    let mut engine = TestEngine(
        reda_engine::Engine::builder()
            .ids(reda_engine::command::SeededIds::new(0))
            .clock(reda_engine::command::FixedClock::EPOCH)
            .user_dirs(None)
            .starter_library(Some(library()))
            .commands(reda_testkit::probe_commands)
            .build(),
    );
    engine.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
    engine
}

#[test]
fn sync_leaves_mechanical_parts_alone_but_moves_their_pads_with_a_merge() {
    let dir = TempDir::new("sync-mechanical");
    let mut engine = demo_with_test_commands(&dir);
    let (big, small) = big_and_small(&mut engine);
    let (part, pad) = mechanical(&mut engine, &small);
    // A Mechanical Part has no Component, so Sync proposes nothing for it.
    assert_eq!(preview(&mut engine)["count"], 0);

    wire(&mut engine, &[(20.0, -3.81), (30.0, -3.81)]);
    let list = preview(&mut engine);
    assert!(of_kind(&list, "remove_component").is_empty(), "{list}");
    let removed = &of_kind(&list, "remove_net")[0];
    assert_eq!(removed["net"], small);
    assert_eq!(removed["requires"].as_array().unwrap().len(), 2, "{list}");

    sync(&mut engine);
    assert_eq!(pad_net(&mut engine, &part, &pad), big);
    assert_eq!(instances(&mut engine).len(), 6);
}

#[test]
fn free_pads_follow_a_merge_and_lose_a_net_with_no_merge_target() {
    let dir = TempDir::new("sync-free-pads");
    let mut engine = demo(&dir);
    let (big, small) = big_and_small(&mut engine);
    let free_pad = |engine: &mut TestEngine, net: &Value| {
        engine.run_ok(
            "pcb.free_pad.add",
            json!({"kind": "smd", "at": {"x": "200mm", "y": 0}, "width": "1mm", "net": net}),
        )["result"]["uuid"]
            .clone()
    };
    let on_small = free_pad(&mut engine, &small);
    let on_big = free_pad(&mut engine, &big);
    let net_of = |engine: &mut TestEngine, pad: &Value| {
        let got = engine.run_ok("object.get", json!({"uuid": pad}));
        got["object"].get("net").cloned().unwrap_or(Value::Null)
    };

    // SMALL merges into BIG.
    wire(&mut engine, &[(20.0, -3.81), (30.0, -3.81)]);
    sync(&mut engine);
    assert_eq!(net_of(&mut engine, &on_small), big);

    // BIG goes with no merge target once every Resistor is deleted.
    let units: Vec<Value> = engine.run_ok("objects.query", json!({"type": "unit"}))["objects"]
        .as_array()
        .unwrap()
        .iter()
        .map(|u| u["object"]["uuid"].clone())
        .collect();
    engine.run_ok("schematic.delete", json!({"objects": units}));
    let list = preview(&mut engine);
    assert!(of_kind(&list, "remove_net")[0].get("merge_into").is_none());
    sync(&mut engine);
    assert_eq!(net_of(&mut engine, &on_small), Value::Null);
    assert_eq!(net_of(&mut engine, &on_big), Value::Null);
}

#[test]
fn a_footprint_swap_keeps_placement_lock_and_designator_text() {
    let dir = TempDir::new("sync-swap-placement");
    let mut engine = demo(&dir);
    let shunt = place(&mut engine, R_SHUNT, 0, 0);
    sync(&mut engine);
    let r1 = instances(&mut engine)[0]["uuid"].clone();
    engine.run_ok(
        "pcb.move",
        json!({"objects": [r1], "by": {"x": "12mm", "y": "-7mm"}}),
    );
    engine.run_ok("pcb.rotate", json!({"objects": [r1], "angle": "90deg"}));
    engine.run_ok("pcb.flip", json!({"objects": [r1]}));
    engine.run_ok(
        "pcb.designator_text.set",
        json!({"footprint": r1, "at": {"x": "20mm", "y": "-3mm"}, "size": "1.5mm"}),
    );
    engine.run_ok("pcb.lock", json!({"footprints": [r1], "locked": true}));
    let before = instances(&mut engine)[0].clone();
    assert_eq!(before["side"], "bottom");
    assert_eq!(before["locked"], true);
    assert!(before.get("text_overrides").is_some(), "{before}");

    engine.run_ok(
        "schematic.component.set",
        json!({"components": [shunt], "footprint": R_KELVIN}),
    );
    assert_eq!(of_kind(&preview(&mut engine), "change_footprint").len(), 1);
    sync(&mut engine);

    let after = instances(&mut engine)[0].clone();
    assert_ne!(after["footprint"], before["footprint"]);
    for key in ["uuid", "at", "rotation", "side", "locked"] {
        assert_eq!(after[key], before[key], "{key} is kept");
    }
    let mut placement_before = before["text_overrides"][0].clone();
    let mut placement_after = after["text_overrides"][0].clone();
    placement_before.as_object_mut().unwrap().remove("text");
    placement_after.as_object_mut().unwrap().remove("text");
    assert_eq!(placement_after, placement_before);
    assert_eq!(after["pads"].as_array().unwrap().len(), 4);
}

#[test]
fn copper_on_a_removed_net_with_no_merge_target_becomes_unassigned() {
    let dir = TempDir::new("sync-unassigned");
    let mut engine = demo_with_test_commands(&dir);
    let (_, small) = big_and_small(&mut engine);
    let (part, pad) = mechanical(&mut engine, &small);
    let track = engine.run_ok(
        "pcb.track.add",
        json!({"layer": "copper_top", "net": small, "points": [
            {"x": "100mm", "y": 0}, {"x": "110mm", "y": 0}]}),
    )["result"]["tracks"][0]
        .clone();
    let via = engine.run_ok(
        "pcb.via.add",
        json!({"at": {"x": "110mm", "y": 0}, "net": small}),
    )["result"]["uuid"]
        .clone();
    let zone = engine.run_ok(
        "pcb.zone.add",
        json!({"layer": "copper_bottom", "net": small, "vertices": [
            {"x": "100mm", "y": 0}, {"x": "110mm", "y": 0}, {"x": "110mm", "y": "10mm"}]}),
    )["result"]["uuid"]
        .clone();

    // SMALL's label and Wire go: its Pads are left on no Net, so nothing absorbs it.
    for object in engine.run_ok("objects.query", json!({"type": "wire"}))["objects"]
        .as_array()
        .unwrap()
        .clone()
    {
        if object["object"]["vertices"][0]["x"].as_i64().unwrap() >= 30_000_000 {
            engine.run_ok(
                "schematic.delete",
                json!({"objects": [object["object"]["uuid"]]}),
            );
        }
    }
    let list = preview(&mut engine);
    let removed = &of_kind(&list, "remove_net")[0];
    assert_eq!(removed["net"], small);
    assert!(removed.get("merge_into").is_none(), "{list}");

    sync(&mut engine);
    assert_eq!(pad_net(&mut engine, &part, &pad), Value::Null);
    for object in [track, via, zone] {
        let got = engine.run_ok("object.get", json!({"uuid": object}));
        assert!(got["object"].get("net").is_none(), "{got}");
    }
}

/// Saves, rewrites the sheet file with `edit` and opens the Project in a new engine whose
/// UUIDs do not collide with the first one's.
fn hand_edit_sheet(
    engine: &mut TestEngine,
    dir: &TempDir,
    edit: impl Fn(String) -> String,
) -> TestEngine {
    engine.run_ok("project.save", json!({}));
    let (rel, bytes) = reda_testkit::read_tree(dir.path())
        .into_iter()
        .find(|(path, _)| path.ends_with(".redasch"))
        .unwrap();
    let text = String::from_utf8(bytes).unwrap();
    std::fs::write(dir.path().join(rel), edit(text)).unwrap();
    let mut reopened = TestEngine(
        reda_engine::Engine::builder()
            .ids(reda_engine::command::SeededIds::new(1))
            .clock(reda_engine::command::FixedClock::EPOCH)
            .user_dirs(None)
            .starter_library(Some(library()))
            .build(),
    );
    reopened.run_ok("project.open", json!({"path": dir.path().join("Demo")}));
    reopened
}
