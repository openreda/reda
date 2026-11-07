//! Explaining the rules: `pcb.rules.applicable` for one or two objects and
//! `pcb.rules.affected` for a record or a Net Class.
//!
//! The board is the Generic preset's: clearance 0.16 mm, Track width 0.20 mm min and
//! 0.25 mm preferred.

use reda_model::Uuid;
use reda_testkit::{TempDir, TestEngine, probe_engine};
use serde_json::{Value, json};

fn engine(dir: &TempDir) -> TestEngine {
    let mut engine = probe_engine();
    engine.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
    engine
}

fn uuid(value: &Value) -> Uuid {
    value.as_str().unwrap().parse().unwrap()
}

fn net(engine: &mut TestEngine, name: &str) -> Uuid {
    uuid(&engine.run_ok("test.net.add", json!({"name": name}))["result"]["uuid"])
}

fn class(engine: &mut TestEngine, name: &str, nets: &[Uuid]) -> Uuid {
    uuid(&engine.run_ok("pcb.net_class.add", json!({"name": name, "nets": nets}))["result"]["uuid"])
}

fn set(engine: &mut TestEngine, params: Value) -> Uuid {
    uuid(&engine.run_ok("pcb.rules.set", params)["result"]["rule"]["uuid"])
}

fn track(engine: &mut TestEngine, net: Option<Uuid>, y: &str) -> Uuid {
    let mut params = json!({"layer": "copper_top",
        "points": [{"x": "0mm", "y": y}, {"x": "5mm", "y": y}]});
    if let Some(net) = net {
        params["net"] = json!(net);
    }
    uuid(&engine.run_ok("pcb.track.add", params)["result"]["tracks"][0])
}

fn kind<'a>(out: &'a Value, kind: &str) -> &'a Value {
    out["kinds"]
        .as_array()
        .unwrap()
        .iter()
        .find(|k| k["kind"] == kind)
        .unwrap_or_else(|| panic!("no {kind} in {out:#}"))
}

/// The winning `(field, value, rule)` rows of one object.
fn winners(rules: &Value) -> Vec<(String, Value, Uuid)> {
    rules["values"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| {
            (
                w["field"].as_str().unwrap().to_owned(),
                w["value"].clone(),
                uuid(&w["rule"]),
            )
        })
        .collect()
}

#[test]
fn one_object_shows_winners_losers_and_disabled_records_per_kind() {
    let dir = TempDir::new("applicable-one");
    let mut engine = engine(&dir);
    let vcc = net(&mut engine, "VCC");
    let power = class(&mut engine, "Power", &[vcc]);
    let wide = class(&mut engine, "Wide", &[vcc]);
    let board = set(&mut engine, json!({"kind": "track_width"}));
    let power_rule = set(
        &mut engine,
        json!({"kind": "track_width", "scope": {"net_class": power}, "values": {"min": "0.22mm"}}),
    );
    let wide_rule = set(
        &mut engine,
        json!({"kind": "track_width", "scope": {"net_class": wide},
               "values": {"min": "0.4mm", "preferred": "0.5mm"}}),
    );
    let off = set(
        &mut engine,
        json!({"kind": "clearance", "scope": {"net_class": wide}, "values": {"value": "1mm"},
               "enabled": false}),
    );
    let t = track(&mut engine, Some(vcc), "0mm");
    let out = engine.run_ok("pcb.rules.applicable", json!({"objects": [t]}));
    assert_eq!(out["nets"], json!([vcc]));
    let width = kind(&out, "track_width");
    assert_eq!(width["severity"], "error");
    assert!(width.get("between").is_none());
    let mine = &width["objects"][0];
    assert_eq!(
        winners(mine),
        [
            ("min".into(), json!(220_000), power_rule),
            ("preferred".into(), json!(500_000), wide_rule),
        ],
        "{out:#}"
    );
    assert_eq!(
        mine["losers"],
        json!([
            {"rule": wide_rule, "net_class": wide, "fields": ["min"]},
            {"rule": board, "fields": ["min", "preferred"]},
        ]),
        "{out:#}"
    );
    assert_eq!(mine["disabled"], json!([]));
    let clearance = &kind(&out, "clearance")["objects"][0];
    assert_eq!(
        clearance["disabled"],
        json!([{"rule": off, "net_class": wide}])
    );
    assert_eq!(winners(clearance)[0].1, json!(160_000));
    assert!(kind(&out, "mask_expansion").get("severity").is_none());
}

#[test]
fn two_objects_show_each_clearance_and_which_applied() {
    let dir = TempDir::new("applicable-two");
    let mut engine = engine(&dir);
    let a = net(&mut engine, "A");
    let hv = net(&mut engine, "HV");
    let class = class(&mut engine, "HV", &[hv]);
    let hv_rule = set(
        &mut engine,
        json!({"kind": "clearance", "scope": {"net_class": class}, "values": {"value": "0.5mm"}}),
    );
    let ta = track(&mut engine, Some(a), "0mm");
    let th = track(&mut engine, Some(hv), "2mm");
    let out = engine.run_ok("pcb.rules.applicable", json!({"objects": [ta, th]}));
    let clearance = kind(&out, "clearance");
    assert_eq!(winners(&clearance["objects"][0])[0].1, json!(160_000));
    assert_eq!(winners(&clearance["objects"][1])[0].1, json!(500_000));
    assert_eq!(
        clearance["between"],
        json!({"field": "value", "value": 500_000, "rule": hv_rule, "object": 1})
    );
    assert_eq!(kind(&out, "hole_to_hole")["between"]["object"], 0);
    assert!(kind(&out, "track_width").get("between").is_none());

    let err = engine.run_err("pcb.rules.applicable", json!({"objects": [ta, th, ta]}));
    assert_eq!(err.code, reda_command::ErrorCode::InvalidParams);
}

#[test]
fn affected_leaves_out_objects_another_record_overrides() {
    let dir = TempDir::new("affected");
    let mut engine = engine(&dir);
    let gnd = net(&mut engine, "GND");
    let hv = net(&mut engine, "HV");
    let hv_class = class(&mut engine, "HV", &[hv]);
    let hv_rule = set(
        &mut engine,
        json!({"kind": "clearance", "scope": {"net_class": hv_class}, "values": {"value": "0.5mm"}}),
    );
    let board = set(&mut engine, json!({"kind": "clearance"}));
    let on_gnd = track(&mut engine, Some(gnd), "0mm");
    let on_hv = track(&mut engine, Some(hv), "2mm");
    let loose = track(&mut engine, None, "4mm");

    let by_class = engine.run_ok("pcb.rules.affected", json!({"net_class": hv_class}));
    assert_eq!(by_class["nets"], json!([hv]));
    assert_eq!(by_class["objects"], json!([on_hv]));
    let by_rule = engine.run_ok("pcb.rules.affected", json!({"rule": hv_rule}));
    assert_eq!(by_rule["objects"], json!([on_hv]));
    // The board-wide clearance record is overridden on HV, and applies on no Net.
    let by_board = engine.run_ok("pcb.rules.affected", json!({"rule": board}));
    assert_eq!(by_board["nets"], json!([gnd]));
    assert_eq!(by_board["objects"], json!([on_gnd, loose]));
    assert_eq!(by_board["pads"], json!([]));
}
