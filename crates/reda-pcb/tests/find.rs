//! `pcb.find`: Designators, values, Net names and Pads such as `U1-6`, ranked exact, then
//! contains, then ignoring case.

mod common;

use common::{engine, pcb, synced};
use reda_testkit::{TempDir, probe_engine};
use serde_json::{Value, json};

fn texts(out: &Value) -> Vec<(String, String, String)> {
    out["matches"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| {
            let s = |k: &str| m[k].as_str().unwrap().to_owned();
            (s("rank"), s("field"), s("text"))
        })
        .collect()
}

fn row(rank: &str, field: &str, text: &str) -> (String, String, String) {
    (rank.into(), field.into(), text.into())
}

#[test]
fn a_pad_is_found_by_designator_and_number() {
    let dir = TempDir::new("pcb-find-pad");
    let mut engine = engine(&dir);
    let r = synced(&mut engine, 2);
    let out = engine.run_ok("pcb.find", json!({"text": "R2-1"}));
    let matches = out["matches"].as_array().unwrap();
    assert_eq!(matches.len(), 1, "{out}");
    let pad = pcb(&engine).footprint(r[1]).unwrap().pads.iter().find(|p| {
        let copy = pcb(&engine).footprint_copy(pcb(&engine).footprint(r[1]).unwrap().footprint);
        copy.unwrap()
            .footprint
            .pads
            .iter()
            .any(|q| q.uuid == p.pad && q.number == "1")
    });
    assert_eq!(
        matches[0],
        json!({"field": "pad", "rank": "exact", "text": "R2-1", "footprint": r[1],
               "pad": {"footprint": r[1], "pad": pad.unwrap().pad}})
    );
    // Ignoring case, still the whole name.
    let out = engine.run_ok("pcb.find", json!({"text": "r2-1"}));
    assert_eq!(texts(&out), vec![row("ignoring_case", "pad", "R2-1")]);
    // Part of a Pad name finds no Pad.
    let out = engine.run_ok("pcb.find", json!({"text": "R2", "fields": ["pad"]}));
    assert_eq!(texts(&out), vec![]);
}

#[test]
fn results_rank_exact_then_contains_then_ignoring_case() {
    let dir = TempDir::new("pcb-find-rank");
    let mut engine = engine(&dir);
    synced(&mut engine, 12);
    let out = engine.run_ok("pcb.find", json!({"text": "R1", "fields": ["designator"]}));
    assert_eq!(
        texts(&out),
        vec![
            row("exact", "designator", "R1"),
            row("contains", "designator", "R10"),
            row("contains", "designator", "R11"),
            row("contains", "designator", "R12"),
        ]
    );
    let out = engine.run_ok("pcb.find", json!({"text": "r1", "fields": ["designator"]}));
    assert_eq!(texts(&out)[0], row("ignoring_case", "designator", "R1"));
    assert_eq!(texts(&out).len(), 4);
    let out = engine.run_ok(
        "pcb.find",
        json!({"text": "r1", "fields": ["designator"], "match_case": true}),
    );
    assert_eq!(texts(&out), vec![]);
    let out = engine.run_ok(
        "pcb.find",
        json!({"text": "r1", "fields": ["designator"], "exact": true}),
    );
    assert_eq!(texts(&out), vec![row("ignoring_case", "designator", "R1")]);
}

#[test]
fn values_are_found() {
    let dir = TempDir::new("pcb-find-value");
    let mut engine = engine(&dir);
    let r = synced(&mut engine, 1);
    let value = pcb(&engine).footprint(r[0]).unwrap().fields.value.clone();
    let out = engine.run_ok("pcb.find", json!({"text": value, "fields": ["value"]}));
    assert_eq!(out["matches"][0]["footprint"], json!(r[0]), "{out}");
}

#[test]
fn a_net_is_found_with_its_copper_and_pads() {
    let dir = TempDir::new("pcb-find-net");
    let mut engine = probe_engine();
    engine.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
    let id = |v: &Value| v.as_str().unwrap().to_owned();
    let net = id(&engine.run_ok("test.net.add", json!({"name": "VBUS"}))["result"]["uuid"]);
    engine.run_ok("test.net.add", json!({"name": "SDA"}));
    let out = engine.run_ok(
        "test.footprint.add",
        json!({"at": {"x": 0, "y": 0},
               "pads": [{"at": {"x": 0, "y": 0}, "width": "1mm", "height": "1mm", "net": net}]}),
    );
    let footprint = id(&out["result"]["footprint"]);
    let pad = id(&out["result"]["pads"][0]);
    let out = engine.run_ok(
        "pcb.track.add",
        json!({"layer": "copper_top", "net": net,
               "points": [{"x": 0, "y": 0}, {"x": "5mm", "y": 0}]}),
    );
    let track = id(&out["result"]["tracks"][0]);
    let out = engine.run_ok("pcb.find", json!({"text": "vbus"}));
    assert_eq!(
        out["matches"],
        json!([{"field": "net", "rank": "ignoring_case", "text": "VBUS", "net": net,
                "objects": [track], "pads": [{"footprint": footprint, "pad": pad}]}])
    );
}

#[test]
fn empty_text_is_refused() {
    let dir = TempDir::new("pcb-find-empty");
    let mut engine = engine(&dir);
    let err = engine.run_err("pcb.find", json!({"text": ""}));
    assert_eq!(err.details, Some(json!({"pointer": "/text"})));
}
