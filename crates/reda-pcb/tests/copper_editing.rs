//! Editing routed copper by hand (docs/spec/v0.1/pcb.md#dragging-tracks and
//! #selecting-and-clearing-copper): a slide in `stop` stops at the last clear position,
//! and unwinding deletes an end object and hands on to its neighbour.

use reda_model::geom::Nm;
use reda_model::{PcbDocument, Uuid};
use reda_testkit::{TempDir, TestEngine, probe_engine};
use serde_json::{Value, json};

fn engine_with_pcb(dir: &TempDir) -> TestEngine {
    let mut engine = probe_engine();
    engine.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
    engine
}

fn pcb(engine: &TestEngine) -> &PcbDocument {
    engine.project().unwrap().pcb().unwrap()
}

fn uuid(value: &Value) -> Uuid {
    value.as_str().unwrap().parse().unwrap()
}

fn uuids(value: &Value) -> Vec<Uuid> {
    value.as_array().unwrap().iter().map(uuid).collect()
}

fn net(engine: &mut TestEngine, name: &str) -> Uuid {
    uuid(&engine.run_ok("test.net.add", json!({"name": name}))["result"]["uuid"])
}

fn route_on(engine: &mut TestEngine, layer: &str, net: Uuid, points: &[(i64, i64)]) -> Vec<Uuid> {
    let points: Vec<Value> = points
        .iter()
        .map(|(x, y)| json!({"x": format!("{x}mm"), "y": format!("{y}mm")}))
        .collect();
    let out = engine.run_ok(
        "pcb.track.add",
        json!({"layer": layer, "points": points, "net": net.to_string()}),
    );
    uuids(&out["result"]["tracks"])
}

fn route(engine: &mut TestEngine, net: Uuid, points: &[(i64, i64)]) -> Vec<Uuid> {
    route_on(engine, "copper_top", net, points)
}

fn via(engine: &mut TestEngine, net: Uuid, at: (i64, i64)) -> Uuid {
    uuid(
        &engine.run_ok(
            "pcb.via.add",
            json!({"at": {"x": format!("{}mm", at.0), "y": format!("{}mm", at.1)},
                   "net": net.to_string()}),
        )["result"]["uuid"],
    )
}

fn pad(engine: &mut TestEngine, net: Uuid, at: (i64, i64)) -> Uuid {
    uuid(
        &engine.run_ok(
            "test.footprint.add",
            json!({"at": {"x": format!("{}mm", at.0), "y": format!("{}mm", at.1)}, "pads": [
                {"at": {"x": 0, "y": 0}, "width": "1mm", "height": "1mm", "net": net.to_string()}
            ]}),
        )["result"]["footprint"],
    )
}

fn exists(engine: &TestEngine, object: Uuid) -> bool {
    pcb(engine).track(object).is_some() || pcb(engine).via(object).is_some()
}

#[test]
fn a_slide_in_stop_ends_at_the_last_clear_position() {
    let dir = TempDir::new("drag-stop");
    let mut engine = engine_with_pcb(&dir);
    let sig = net(&mut engine, "SIG");
    let other = net(&mut engine, "OTHER");
    let t = route(&mut engine, sig, &[(0, 0), (0, 5), (10, 5), (10, 0)]);
    route(&mut engine, other, &[(-5, 8), (15, 8)]);
    let width = pcb(&engine).track(t[1]).unwrap().width;

    let out = engine.run_ok(
        "pcb.track.drag",
        json!({"track": t[1].to_string(), "by": {"x": 0, "y": "5mm"}, "obstacle_mode": "stop"}),
    );
    let dy = out["result"]["by"]["y"].as_i64().unwrap();
    // Clear means the gap to OTHER's edge is at least the clearance; at most one nm off.
    let (a, _) = pcb(&engine).track(t[1]).unwrap().geometry.ends();
    assert_eq!(a.y, Nm::from_mm(5) + Nm(dy));
    let gap = Nm::from_mm(8).0 - a.y.0 - width.0;
    let clearance = reda_pcb::rules::net_rules(pcb(&engine), Some(sig))
        .clearance
        .unwrap()
        .0;
    assert!(
        gap >= clearance && gap <= clearance + 2,
        "stops at the clearance: gap {gap}, clearance {clearance}"
    );

    // `ignore` (the default) goes the whole way.
    let t2 = route(&mut engine, sig, &[(20, 0), (20, 5), (30, 5), (30, 0)]);
    route(&mut engine, other, &[(15, 8), (35, 8)]);
    let out = engine.run_ok(
        "pcb.track.drag",
        json!({"track": t2[1].to_string(), "by": {"x": 0, "y": "5mm"}}),
    );
    assert_eq!(out["result"]["by"]["y"], json!(Nm::from_mm(5).0));
}

#[test]
fn a_slide_that_starts_too_close_can_still_move_away_in_stop() {
    let dir = TempDir::new("drag-stop-blocked");
    let mut engine = engine_with_pcb(&dir);
    let sig = net(&mut engine, "SIG");
    let other = net(&mut engine, "OTHER");
    let t = route(&mut engine, sig, &[(0, 0), (0, 5), (10, 5), (10, 0)]);
    engine.run_ok(
        "pcb.track.add",
        json!({"layer": "copper_top", "net": other.to_string(), "points": [
            {"x": "-5mm", "y": "5.3mm"}, {"x": "15mm", "y": "5.3mm"}
        ]}),
    );
    // The violation it starts with doesn't count, so it can move away from it.
    let out = engine.run_ok(
        "pcb.track.drag",
        json!({"track": t[1].to_string(), "by": {"x": 0, "y": "-2mm"}, "obstacle_mode": "stop"}),
    );
    assert_eq!(out["result"]["by"]["y"], json!(Nm::from_mm(-2).0));
}

#[test]
fn unwinding_deletes_an_end_object_and_selects_its_neighbour_through_vias_to_a_pad() {
    let dir = TempDir::new("unwind");
    let mut engine = engine_with_pcb(&dir);
    let sig = net(&mut engine, "SIG");
    pad(&mut engine, sig, (0, 0));
    let bottom = route_on(&mut engine, "copper_bottom", sig, &[(0, 0), (5, 0)]);
    let v = via(&mut engine, sig, (5, 0));
    let top = route(&mut engine, sig, &[(5, 0), (5, 5), (10, 5)]);

    let step = |engine: &mut TestEngine, object: Uuid| -> Value {
        engine.run_ok(
            "pcb.copper.unwind",
            json!({"objects": [object.to_string()]}),
        )["result"]
            .clone()
    };
    let out = step(&mut engine, top[1]);
    assert_eq!(uuids(&out["deleted"]), [top[1]]);
    assert_eq!(out["next"], json!(top[0].to_string()));
    let out = step(&mut engine, top[0]);
    assert_eq!(out["next"], json!(v.to_string()), "on to the Via");
    let out = step(&mut engine, v);
    assert_eq!(out["next"], json!(bottom[0].to_string()), "through the Via");
    let out = step(&mut engine, bottom[0]);
    assert_eq!(out["next"], Value::Null, "stops at the Pad");
    assert!(pcb(&engine).tracks().next().is_none());
    assert!(pcb(&engine).vias().next().is_none());
}

#[test]
fn unwinding_an_object_at_a_branch_selects_nothing_and_both_ways_runs_to_pads_and_branches() {
    let dir = TempDir::new("unwind-branch");
    let mut engine = engine_with_pcb(&dir);
    let sig = net(&mut engine, "SIG");
    // A T: a stem from (5, 0) up to (5, 5), and a bar (0, 5)-(5, 5)-(10, 5).
    let bar = route(&mut engine, sig, &[(0, 5), (5, 5), (10, 5)]);
    let stem = route(&mut engine, sig, &[(5, 0), (5, 5)]);
    let out = engine.run_ok(
        "pcb.copper.unwind",
        json!({"objects": [bar[1].to_string()]}),
    )["result"]
        .clone();
    assert_eq!(out["next"], Value::Null, "it touched two objects");
    assert!(!exists(&engine, bar[1]));

    // Both ways: from the middle of a chain between two Pads, everything up to the Pads
    // goes, but a branch stays.
    pad(&mut engine, sig, (20, 0));
    pad(&mut engine, sig, (40, 0));
    let chain = route(
        &mut engine,
        sig,
        &[(20, 0), (25, 0), (30, 0), (35, 0), (40, 0)],
    );
    let out = engine.run_ok(
        "pcb.copper.unwind",
        json!({"objects": [chain[2].to_string()], "both": true}),
    )["result"]
        .clone();
    let mut deleted = uuids(&out["deleted"]);
    deleted.sort();
    let mut want = chain.clone();
    want.sort();
    assert_eq!(deleted, want);
    assert_eq!(out["next"], Value::Null);
    assert!(exists(&engine, bar[0]) && exists(&engine, stem[0]));

    // From one arm of a whole T, both ways stops at the branch.
    let bar = route(&mut engine, sig, &[(50, 5), (55, 5), (60, 5)]);
    let stem = route(&mut engine, sig, &[(55, 0), (55, 5)]);
    let out = engine.run_ok(
        "pcb.copper.unwind",
        json!({"objects": [bar[0].to_string()], "both": true}),
    )["result"]
        .clone();
    assert_eq!(uuids(&out["deleted"]), [bar[0]]);
    assert!(exists(&engine, stem[0]) && exists(&engine, bar[1]));
}

#[test]
fn unwinding_needs_one_track_or_via() {
    let dir = TempDir::new("unwind-refuse");
    let mut engine = engine_with_pcb(&dir);
    let sig = net(&mut engine, "SIG");
    let t = route(&mut engine, sig, &[(0, 0), (5, 0), (10, 0)]);
    let err = engine.run_err(
        "pcb.copper.unwind",
        json!({"objects": [t[0].to_string(), t[1].to_string()]}),
    );
    assert_eq!(err.code, reda_command::ErrorCode::InvalidParams);
}
