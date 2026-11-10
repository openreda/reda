//! Net Ties on the board: the one rule, based on the Net, that DRC and the route
//! clearance query share. A tie's Pads may touch; Tracks and Vias on one of its Nets need
//! no clearance to its other Pads but still short when they touch one; everything else,
//! Zones included, follows the normal rules, and the Nets stay separate.
//!
//! The board is the Generic preset's: clearance 0.16 mm, Via 0.7 mm.

use reda_model::Uuid;
use reda_testkit::{TempDir, TestEngine, probe_engine};
use serde_json::{Value, json};

const PCB: &str = "00000000-0000-4000-8000-000000000003";

fn engine(dir: &TempDir) -> TestEngine {
    let mut engine = probe_engine();
    engine.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
    // The test Nets are on no schematic, and the test Footprints' Designators sit on
    // their Pads.
    for check in ["out_of_sync", "silk_to_mask"] {
        engine.run_ok(
            "drc.set_severity",
            json!({"check": check, "severity": "off"}),
        );
    }
    engine
}

fn uuid(value: &Value) -> Uuid {
    value.as_str().unwrap().parse().unwrap()
}

fn net(engine: &mut TestEngine, name: &str) -> Uuid {
    uuid(&engine.run_ok("test.net.add", json!({"name": name}))["result"]["uuid"])
}

/// A two-Pad Footprint of `component_type` at the origin: 0.5 mm square Pads `1` on `a`
/// at x = -0.25 mm and `2` on `b` at x = `x2`, so at 0.25 mm they touch at x = 0. Its
/// instance and Pads.
fn tie(
    engine: &mut TestEngine,
    component_type: &str,
    a: Uuid,
    b: Uuid,
    x2: &str,
) -> (Uuid, [Uuid; 2]) {
    let pad = |x: &str, net: Uuid| json!({"at": {"x": x, "y": 0}, "width": "0.5mm", "height": "0.5mm", "net": net});
    let out = engine.run_ok(
        "test.footprint.add",
        json!({"at": {"x": 0, "y": 0}, "designator": "NT1", "component_type": component_type,
               "pads": [pad("-0.25mm", a), pad(x2, b)]}),
    );
    let pads = &out["result"]["pads"];
    (
        uuid(&out["result"]["footprint"]),
        [uuid(&pads[0]), uuid(&pads[1])],
    )
}

/// A 0.2 mm Track on the top copper through `points` (in mm).
fn track(engine: &mut TestEngine, net: Uuid, points: &[(&str, &str)]) -> Uuid {
    let points: Vec<Value> = points
        .iter()
        .map(|(x, y)| json!({"x": x, "y": y}))
        .collect();
    let out = engine.run_ok(
        "pcb.track.add",
        json!({"layer": "copper_top", "width": "0.2mm", "net": net, "points": points}),
    );
    uuid(&out["result"]["tracks"][0])
}

fn drc(engine: &mut TestEngine) -> Value {
    engine.run_ok("drc.run", json!({}))
}

/// The Violations of one check.
fn of(run: &Value, check: &str) -> Vec<Value> {
    run["violations"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| v["check"] == check)
        .cloned()
        .collect()
}

fn objects(violation: &Value) -> Vec<Uuid> {
    violation["objects"]
        .as_array()
        .unwrap()
        .iter()
        .map(uuid)
        .collect()
}

fn checks(run: &Value) -> Vec<String> {
    run["violations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["check"].as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn a_net_ties_pads_touch_with_no_short_and_any_other_parts_are_shorted_copper() {
    let dir = TempDir::new("tie-pads");
    let mut engine = engine(&dir);
    let (a, b) = (net(&mut engine, "A"), net(&mut engine, "B"));
    tie(&mut engine, "net_tie", a, b, "0.25mm");
    let run = drc(&mut engine);
    assert_eq!(checks(&run), Vec::<String>::new(), "{run:#}");

    engine.run_ok("edit.undo", json!({"document": PCB}));
    let (fp2, pads2) = tie(&mut engine, "standard", a, b, "0.25mm");
    let run = drc(&mut engine);
    let shorted = of(&run, "shorted_copper");
    assert_eq!(shorted.len(), 1, "{run:#}");
    assert_eq!(objects(&shorted[0]), [fp2, pads2[0], fp2, pads2[1]]);
    assert_eq!(shorted[0]["severity"], "error");
    assert_eq!(shorted[0]["args"]["pad_a"], "1");
    assert_eq!(shorted[0]["args"]["pad_b"], "2");
    assert_eq!(shorted[0]["args"]["footprint"], "Test");
    assert!(shorted[0]["at"].is_object());
    assert_eq!(of(&run, "short_circuit").len(), 1, "{run:#}");
}

#[test]
fn a_net_tie_whose_pads_do_not_touch_is_not_joined() {
    let dir = TempDir::new("tie-apart");
    let mut engine = engine(&dir);
    let (a, b) = (net(&mut engine, "A"), net(&mut engine, "B"));
    let (fp, _) = tie(&mut engine, "net_tie", a, b, "1mm");
    let run = drc(&mut engine);
    let found = of(&run, "net_tie_not_joined");
    assert_eq!(found.len(), 1, "{run:#}");
    assert_eq!(objects(&found[0]), [fp]);
    assert_eq!(found[0]["severity"], "error");
    assert_eq!(found[0]["args"]["reason"], "not_joined");
    assert_eq!(found[0]["args"]["footprint"], "Test");
    assert_eq!(of(&run, "shorted_copper"), Vec::<Value>::new());
}

#[test]
fn tracks_and_vias_on_a_ties_net_need_no_clearance_to_its_other_pads_but_still_short() {
    let dir = TempDir::new("tie-tracks");
    let mut engine = engine(&dir);
    let (a, b, c) = (
        net(&mut engine, "A"),
        net(&mut engine, "B"),
        net(&mut engine, "C"),
    );
    let (fp, pads) = tie(&mut engine, "net_tie", a, b, "0.25mm");
    // Leaving Pad 1 upwards 0.1 mm from Pad 2, and a Via 0.1 mm above Pad 2.
    track(&mut engine, a, &[("-0.2mm", "0mm"), ("-0.2mm", "2mm")]);
    engine.run_ok(
        "pcb.via.add",
        json!({"at": {"x": "0.25mm", "y": "0.7mm"}, "net": a}),
    );
    // Net C is no Net of the tie: it keeps its clearance as usual.
    let other = track(&mut engine, c, &[("0.25mm", "-0.45mm"), ("3mm", "-0.45mm")]);
    let run = drc(&mut engine);
    assert_eq!(of(&run, "short_circuit"), Vec::<Value>::new(), "{run:#}");
    let clearance = of(&run, "clearance");
    assert_eq!(clearance.len(), 1, "{run:#}");
    assert_eq!(objects(&clearance[0]), [fp, pads[1], other]);

    // Touching the other Pad is still a short.
    let touching = track(&mut engine, a, &[("-0.1mm", "0mm"), ("-0.1mm", "-2mm")]);
    let run = drc(&mut engine);
    let shorts = of(&run, "short_circuit");
    assert_eq!(shorts.len(), 1, "{run:#}");
    assert_eq!(objects(&shorts[0]), [fp, pads[1], touching]);
}

#[test]
fn the_route_clearance_query_follows_the_same_rule() {
    let dir = TempDir::new("tie-query");
    let mut engine = engine(&dir);
    let (a, b, c) = (
        net(&mut engine, "A"),
        net(&mut engine, "B"),
        net(&mut engine, "C"),
    );
    let (_, pads) = tie(&mut engine, "net_tie", a, b, "0.25mm");
    let query = |engine: &mut TestEngine, net: Uuid, x: &str| {
        engine.run_ok(
            "pcb.clearance.track",
            json!({"layer": "copper_top", "net": net, "width": "0.2mm",
                   "from": {"x": x, "y": "0mm"}, "to": {"x": x, "y": "2mm"}}),
        )
    };
    assert_eq!(query(&mut engine, a, "-0.2mm")["clear"], true);
    let touching = query(&mut engine, a, "-0.1mm");
    assert_eq!(touching["clear"], false);
    assert_eq!(touching["hits"][0]["item"]["pad"], json!(pads[1]));
    assert_eq!(touching["hits"][0]["actual"], 0);
    // Another Net near Pad 2: the normal clearance.
    let other = engine.run_ok(
        "pcb.clearance.track",
        json!({"layer": "copper_top", "net": c, "width": "0.2mm",
               "from": {"x": "0.25mm", "y": "-0.45mm"}, "to": {"x": "3mm", "y": "-0.45mm"}}),
    );
    assert_eq!(other["clear"], false);

    let via = |engine: &mut TestEngine, net: Uuid| {
        engine.run_ok(
            "pcb.clearance.via",
            json!({"at": {"x": "0.25mm", "y": "0.7mm"}, "net": net}),
        )["clear"]
            .clone()
    };
    assert_eq!(via(&mut engine, a), true);
    assert_eq!(via(&mut engine, c), false);
}

#[test]
fn zones_keep_their_clearance_and_the_nets_stay_separate() {
    let dir = TempDir::new("tie-zone");
    let mut engine = engine(&dir);
    let (a, b) = (net(&mut engine, "A"), net(&mut engine, "B"));
    tie(&mut engine, "net_tie", a, b, "0.25mm");
    // A Zone on Net A round the tie: its fill joins Pad 1 and keeps clear of Pad 2.
    engine.run_ok(
        "pcb.zone.add",
        json!({"layer": "copper_top", "net": a,
               "vertices": [{"x": "-3mm", "y": "-3mm"}, {"x": "3mm", "y": "-3mm"},
                            {"x": "3mm", "y": "3mm"}, {"x": "-3mm", "y": "3mm"}]}),
    );
    engine.run_ok("pcb.zone.fill", json!({}));
    let run = drc(&mut engine);
    assert_eq!(checks(&run), Vec::<String>::new(), "{run:#}");
    // A second Pad on Net B: the tie doesn't connect it through Net A.
    engine.run_ok(
        "test.footprint.add",
        json!({"at": {"x": "-2mm", "y": 0},
               "pads": [{"at": {"x": 0, "y": 0}, "width": "0.5mm", "height": "0.5mm", "net": b}]}),
    );
    let run = drc(&mut engine);
    let unrouted = of(&run, "unrouted");
    assert_eq!(unrouted.len(), 1, "{run:#}");
    assert_eq!(unrouted[0]["args"]["net"], "B");
}
