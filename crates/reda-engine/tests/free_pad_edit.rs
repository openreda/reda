//! `pcb.free_pad.edit`: a Free Pad takes every field of the Pad model after it is placed,
//! as Free Pad Properties edits it (library.md#pad-model).

use reda_engine::command::ErrorCode;
use reda_testkit::{TempDir, TestEngine, test_engine};
use serde_json::{Value, json};

const PCB: &str = "00000000-0000-4000-8000-000000000003";

fn board(dir: &TempDir) -> TestEngine {
    let mut engine = test_engine();
    engine.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
    engine
}

fn free_pad(engine: &mut TestEngine, uuid: &Value) -> Value {
    engine.run_ok(
        "objects.query",
        json!({"type": "free_pad", "document": PCB}),
    )["objects"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["object"].clone())
        .find(|o| o["uuid"] == *uuid)
        .unwrap()
}

#[test]
fn every_pad_field_can_be_changed_and_cleared_in_one_undo_step() {
    let dir = TempDir::new("free-pad-edit");
    let mut engine = board(&dir);
    let uuid = engine.run_ok(
        "pcb.free_pad.add",
        json!({"kind": "smd", "at": {"x": "5mm", "y": "5mm"}, "width": "1mm"}),
    )["result"]["uuid"]
        .clone();
    engine.run_ok(
        "pcb.free_pad.edit",
        json!({"pad": uuid, "shape": "chamfered_rect", "width": "2mm", "height": "1mm",
               "chamfer_ratio": 25000, "corner_ratio": 10000, "corners": ["top_left"],
               "offset": {"x": "0.1mm", "y": 0}, "side": "bottom",
               "mask": {"bottom": "tent"}, "paste": {"percent": -10},
               "zone_connection": {"style": "solid"}}),
    );
    let pad = free_pad(&mut engine, &uuid);
    assert_eq!(pad["shape"], "chamfered_rect");
    assert_eq!(pad["width"], 2_000_000);
    assert_eq!(pad["chamfer_ratio"], 25000);
    assert_eq!(pad["corner_ratio"], 10000);
    assert_eq!(pad["corners"], json!(["top_left"]));
    assert_eq!(pad["offset"], json!({"x": 100000, "y": 0}));
    assert_eq!(pad["side"], "bottom");
    assert_eq!(pad["mask"], json!({"bottom": "tent"}));
    assert_eq!(pad["paste"], json!({"percent": -10}));
    assert_eq!(pad["zone_connection"], json!({"style": "solid"}));
    // The anchor stays where it was.
    assert_eq!(pad["at"], json!({"x": 5_000_000, "y": 5_000_000}));

    engine.run_ok(
        "pcb.free_pad.edit",
        json!({"pad": uuid, "offset": null, "paste": null, "side": null,
               "mask": null, "zone_connection": null}),
    );
    let pad = free_pad(&mut engine, &uuid);
    for gone in ["offset", "paste", "side", "mask", "zone_connection"] {
        assert!(pad.get(gone).is_none(), "{gone} stays: {pad}");
    }
    engine.run_ok("edit.undo", json!({"document": PCB}));
    assert_eq!(free_pad(&mut engine, &uuid)["side"], "bottom");
}

#[test]
fn a_free_pad_changes_kind_with_its_hole_and_bad_values_are_refused() {
    let dir = TempDir::new("free-pad-edit-refuse");
    let mut engine = board(&dir);
    let uuid = engine.run_ok(
        "pcb.free_pad.add",
        json!({"kind": "plated_through_hole", "at": {"x": 0, "y": 0}, "width": "2mm",
               "hole": {"width": "1mm"}}),
    )["result"]["uuid"]
        .clone();
    // An SMD Pad can have no hole.
    let refused = engine.run_err("pcb.free_pad.edit", json!({"pad": uuid, "kind": "smd"}));
    assert_eq!(refused.code, ErrorCode::InvalidParams);
    engine.run_ok(
        "pcb.free_pad.edit",
        json!({"pad": uuid, "kind": "smd", "hole": null}),
    );
    assert_eq!(free_pad(&mut engine, &uuid)["kind"], "smd");
    let unknown = engine.run_err(
        "pcb.free_pad.edit",
        json!({"pad": "00000000-0000-4000-8000-0000000000ff", "width": "1mm"}),
    );
    assert_eq!(unknown.code, ErrorCode::NotFound);
}
