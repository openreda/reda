//! DRC over the board's copper: each check, its severities and its Waivers, through
//! `drc.run` and the other `drc.*` Commands.
//!
//! The board is the Generic preset's: clearance 0.16 mm, Track width 0.2 mm (0.25 mm
//! preferred), Via 0.7 mm with a 0.3 mm hole, annular ring 0.2 mm, hole-to-hole 0.5 mm, edge clearance 0.4 mm.

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

/// A straight 0.2 mm Track on the top copper from `(x0, y)` to `(x1, y)`, in mm.
fn track(engine: &mut TestEngine, net: Option<Uuid>, y: &str, x0: &str, x1: &str) -> Uuid {
    let mut params = json!({"layer": "copper_top", "width": "0.2mm",
        "points": [{"x": x0, "y": y}, {"x": x1, "y": y}]});
    if let Some(net) = net {
        params["net"] = json!(net);
    }
    uuid(&engine.run_ok("pcb.track.add", params)["result"]["tracks"][0])
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

#[test]
fn an_empty_board_has_no_violations() {
    let dir = TempDir::new("drc-empty");
    let mut engine = engine(&dir);
    let run = drc(&mut engine);
    assert_eq!(run["violations"], json!([]));
    assert_eq!(run["waived"], 0);
    assert_eq!(run["unmatched_waivers"], json!([]));
    assert!(run["design"].is_string());
}

#[test]
fn copper_of_different_nets_closer_than_the_clearance_is_reported() {
    let dir = TempDir::new("drc-clearance");
    let mut engine = engine(&dir);
    let a = net(&mut engine, "A");
    let b = net(&mut engine, "B");
    // Centrelines 0.3 mm apart, 0.2 mm wide: a 0.1 mm gap.
    let ta = track(&mut engine, Some(a), "0mm", "0mm", "5mm");
    let tb = track(&mut engine, Some(b), "0.3mm", "0mm", "5mm");
    // 0.4 mm gap: clear.
    track(&mut engine, Some(b), "-0.6mm", "0mm", "5mm");
    let run = drc(&mut engine);
    let found = of(&run, "clearance");
    assert_eq!(found.len(), 1, "{run:#}");
    assert_eq!(found[0]["severity"], "error");
    assert_eq!(objects(&found[0]), [ta, tb]);
    assert_eq!(found[0]["args"]["required"], 160_000);
    assert_eq!(found[0]["args"]["actual"], 100_000);
}

fn square(x0: &str, y0: &str, x1: &str, y1: &str) -> Value {
    json!([{"x": x0, "y": y0}, {"x": x1, "y": y0}, {"x": x1, "y": y1}, {"x": x0, "y": y1}])
}

fn class(engine: &mut TestEngine, params: Value, nets: &[Uuid]) -> Uuid {
    let class = uuid(&engine.run_ok("pcb.net_class.add", params)["result"]["uuid"]);
    engine.run_ok(
        "pcb.net.add_to_class",
        json!({"nets": nets, "class": class}),
    );
    class
}

#[test]
fn the_larger_net_class_clearance_applies() {
    let dir = TempDir::new("drc-class-clearance");
    let mut engine = engine(&dir);
    let a = net(&mut engine, "A");
    let hv = net(&mut engine, "HV");
    class(
        &mut engine,
        json!({"name": "HV", "rules": [{"kind": "clearance", "values": {"value": "0.5mm"}}]}),
        &[hv],
    );
    // A 0.3 mm gap: clear for the board rule, too close for the HV class.
    let ta = track(&mut engine, Some(a), "0mm", "0mm", "5mm");
    let th = track(&mut engine, Some(hv), "0.5mm", "0mm", "5mm");
    let run = drc(&mut engine);
    let found = of(&run, "clearance");
    assert_eq!(found.len(), 1, "{run:#}");
    assert_eq!(objects(&found[0]), [ta, th]);
    assert_eq!(found[0]["args"]["required"], 500_000);
    assert_eq!(found[0]["args"]["actual"], 300_000);
}

#[test]
fn touching_copper_of_different_nets_is_a_short_not_a_clearance_breach() {
    let dir = TempDir::new("drc-short");
    let mut engine = engine(&dir);
    let a = net(&mut engine, "A");
    let b = net(&mut engine, "B");
    let ta = track(&mut engine, Some(a), "0mm", "0mm", "5mm");
    let tb = track(&mut engine, Some(b), "0.1mm", "0mm", "3mm");
    // Same Net touching is a connection.
    track(&mut engine, Some(a), "0mm", "5mm", "8mm");
    let run = drc(&mut engine);
    assert_eq!(of(&run, "clearance"), Vec::<Value>::new());
    let shorts = of(&run, "short_circuit");
    assert_eq!(shorts.len(), 1, "{run:#}");
    assert_eq!(objects(&shorts[0]), [ta, tb]);
    assert_eq!(shorts[0]["args"]["net_a"], "A");
    assert_eq!(shorts[0]["args"]["net_b"], "B");
    assert_eq!(shorts[0]["at"], json!({"x":0,"y":50_000}));
    assert_eq!(shorts[0]["document"], PCB);
    assert_eq!(shorts[0]["layers"].as_array().unwrap().len(), 1);
    assert!(shorts[0].get("measured").is_none());
    assert!(shorts[0].get("rule").is_none());
}

#[test]
fn a_track_narrower_than_its_net_class_allows_is_reported() {
    let dir = TempDir::new("drc-track-width");
    let mut engine = engine(&dir);
    let a = net(&mut engine, "A");
    let power = net(&mut engine, "PWR");
    class(
        &mut engine,
        json!({"name": "Power", "rules": [{"kind": "track_width",
            "values": {"min": "0.5mm", "preferred": "0.5mm"}}]}),
        &[power],
    );
    let narrow = uuid(
        &engine.run_ok(
            "pcb.track.add",
            json!({"layer": "copper_top", "net": a, "width": "0.1mm",
                   "points": [{"x": 0, "y": 0}, {"x": "5mm", "y": 0}]}),
        )["result"]["tracks"][0],
    );
    let thin_power = uuid(
        &engine.run_ok(
            "pcb.track.add",
            json!({"layer": "copper_top", "net": power, "width": "0.3mm",
                   "points": [{"x": 0, "y": "5mm"}, {"x": "5mm", "y": "5mm"}]}),
        )["result"]["tracks"][0],
    );
    // 0.2 mm on Net A meets the board rule.
    track(&mut engine, Some(a), "-5mm", "0mm", "5mm");
    let run = drc(&mut engine);
    let found = of(&run, "track_width");
    let tracks: Vec<Vec<Uuid>> = found.iter().map(objects).collect();
    assert_eq!(tracks, [vec![narrow], vec![thin_power]], "{run:#}");
    assert_eq!(found[1]["args"]["minimum"], 500_000);
    assert_eq!(found[1]["args"]["actual"], 300_000);
}

#[test]
fn objects_of_a_kind_a_keepout_restricts_inside_it_are_reported() {
    let dir = TempDir::new("drc-keepout");
    let mut engine = engine(&dir);
    let a = net(&mut engine, "A");
    let copper = uuid(
        &engine.run_ok(
            "pcb.keepout.add",
            json!({"layer": "copper_top", "vertices": square("0mm", "0mm", "3mm", "3mm")}),
        )["result"]["uuid"],
    );
    let vias_only = uuid(
        &engine.run_ok(
            "pcb.keepout.add",
            json!({"restrict": ["vias"], "vertices": square("10mm", "0mm", "13mm", "3mm")}),
        )["result"]["uuid"],
    );
    let inside = track(&mut engine, Some(a), "1mm", "-2mm", "1mm");
    // A Track through the Via-only Keepout is allowed; a Via there is not.
    track(&mut engine, Some(a), "1mm", "9mm", "14mm");
    let via = uuid(
        &engine.run_ok(
            "pcb.via.add",
            json!({"at": {"x": "11mm", "y": "2mm"}, "net": a}),
        )["result"]["uuid"],
    );
    let placed = engine.run_ok(
        "test.footprint.add",
        json!({"at": {"x": "2mm", "y": "2mm"},
               "pads": [{"at": {"x": 0, "y": 0}, "width": "0.5mm", "height": "0.5mm"}]}),
    )["result"]
        .clone();
    let run = drc(&mut engine);
    let found: Vec<Vec<Uuid>> = of(&run, "keepout").iter().map(objects).collect();
    let footprint = uuid(&placed["footprint"]);
    let pad = uuid(&placed["pads"][0]);
    assert_eq!(
        found,
        [
            vec![footprint, pad, copper],
            vec![via, vias_only],
            vec![inside, copper],
        ],
        "{run:#}"
    );
}

#[test]
fn smd_and_through_hole_pads_are_restricted_separately() {
    let dir = TempDir::new("drc-keepout-pads");
    let mut engine = engine(&dir);
    let smd_only = uuid(
        &engine.run_ok(
            "pcb.keepout.add",
            json!({"restrict": ["smd_pads"], "vertices": square("0mm", "0mm", "10mm", "10mm")}),
        )["result"]["uuid"],
    );
    let placed = engine.run_ok(
        "test.footprint.add",
        json!({"at": {"x": "5mm", "y": "5mm"},
               "pads": [{"at": {"x": 0, "y": 0}, "width": "0.5mm", "height": "0.5mm"},
                        {"at": {"x": "2mm", "y": 0}, "width": "1.5mm", "height": "1.5mm",
                         "shape": "circle", "kind": "plated_through_hole",
                         "hole": {"width": 800000, "height": 800000}}]}),
    )["result"]
        .clone();
    let footprint = uuid(&placed["footprint"]);
    let [smd, tht] = [uuid(&placed["pads"][0]), uuid(&placed["pads"][1])];
    let found: Vec<Vec<Uuid>> = of(&drc(&mut engine), "keepout")
        .iter()
        .map(objects)
        .collect();
    assert_eq!(found, [vec![footprint, smd, smd_only]]);

    engine.run_ok(
        "pcb.keepout.set",
        json!({"keepout": smd_only, "restrict": ["tht_pads"]}),
    );
    let found: Vec<Vec<Uuid>> = of(&drc(&mut engine), "keepout")
        .iter()
        .map(objects)
        .collect();
    assert_eq!(found, [vec![footprint, tht, smd_only]]);
}

#[test]
fn copper_keeps_the_clearance_from_a_keepout_unless_zero_keepout_clearance() {
    let dir = TempDir::new("drc-keepout-clearance");
    let mut engine = engine(&dir);
    let keepout = uuid(
        &engine.run_ok(
            "pcb.keepout.add",
            json!({"layer": "copper_top", "vertices": square("0mm", "0mm", "3mm", "3mm")}),
        )["result"]["uuid"],
    );
    // A 0.2 mm Track whose edge is 0.1 mm above the Keepout: inside the 0.16 mm clearance.
    let near = track(&mut engine, None, "3.2mm", "0mm", "3mm");
    // One 0.2 mm above: clear.
    track(&mut engine, None, "-0.3mm", "0mm", "3mm");
    let run = drc(&mut engine);
    let found = of(&run, "keepout");
    assert_eq!(found.len(), 1, "{run:#}");
    let v = &found[0];
    assert_eq!(objects(v), [near, keepout]);
    assert_eq!(v["required"], json!({"length": 160_000}));
    assert_eq!(v["measured"], json!({"length": 100_000}));
    assert_eq!(v["args"]["required"], 160_000);
    assert_eq!(v["args"]["actual"], 100_000);
    let board =
        engine.run_ok("pcb.rules.set", json!({"kind": "clearance"}))["result"]["rule"]["uuid"]
            .clone();
    assert_eq!(v["rule"], board);

    engine.run_ok(
        "pcb.rules.set",
        json!({"kind": "clearance", "values": {"zero_keepout_clearance": true}}),
    );
    assert!(of(&drc(&mut engine), "keepout").is_empty());
}

fn via(engine: &mut TestEngine, x: &str, diameter: &str, hole: &str) -> Uuid {
    uuid(
        &engine.run_ok(
            "pcb.via.add",
            json!({"at": {"x": x, "y": 0}, "diameter": diameter, "hole": hole}),
        )["result"]["uuid"],
    )
}

/// The `(objects, actual)` of each Violation of `check`.
fn actuals(run: &Value, check: &str) -> Vec<(Vec<Uuid>, i64)> {
    of(run, check)
        .iter()
        .map(|v| (objects(v), v["args"]["actual"].as_i64().unwrap()))
        .collect()
}

#[test]
fn small_holes_thin_rings_and_crowded_holes_are_reported() {
    let dir = TempDir::new("drc-holes");
    let mut engine = engine(&dir);
    let small_hole = via(&mut engine, "0mm", "0.7mm", "0.2mm");
    let thin_ring = via(&mut engine, "5mm", "0.5mm", "0.3mm");
    via(&mut engine, "40mm", "0.7mm", "0.3mm");
    let placed = engine.run_ok(
        "test.footprint.add",
        json!({"at": {"x": "10mm", "y": 0}, "pads": [
            {"at": {"x": 0, "y": 0}, "width": "1mm", "height": "1mm", "shape": "circle",
             "kind": "plated_through_hole", "hole": {"width": 800_000, "height": 800_000}},
            {"at": {"x": "10mm", "y": 0}, "width": "1mm", "height": "1mm",
             "kind": "non_plated", "hole": {"width": 200_000, "height": 200_000}},
        ]}),
    )["result"]
        .clone();
    let footprint = uuid(&placed["footprint"]);
    let (pth, npth) = (uuid(&placed["pads"][0]), uuid(&placed["pads"][1]));
    let mounting = uuid(
        &engine.run_ok(
            "pcb.free_pad.add",
            json!({"kind": "non_plated", "shape": "circle", "at": {"x": "30mm", "y": 0},
                   "width": "1mm", "hole": {"width": "1mm"}}),
        )["result"]["uuid"],
    );
    let crowding = via(&mut engine, "30.9mm", "0.7mm", "0.3mm");
    let run = drc(&mut engine);
    assert_eq!(
        actuals(&run, "via_size"),
        [(vec![small_hole], 200_000), (vec![thin_ring], 500_000)],
        "{run:#}"
    );
    assert_eq!(of(&run, "via_size")[0]["args"]["minimum"], 300_000);
    assert_eq!(
        actuals(&run, "hole_size"),
        [(vec![footprint, npth], 200_000)],
        "{run:#}"
    );
    assert_eq!(of(&run, "hole_size")[0]["args"]["minimum"], 500_000);
    assert_eq!(
        actuals(&run, "annular_ring"),
        [(vec![thin_ring], 100_000), (vec![footprint, pth], 100_000)],
        "{run:#}"
    );
    assert_eq!(of(&run, "annular_ring")[0]["args"]["minimum"], 200_000);
    for (check, measured, required) in [
        ("via_size", 200_000, 300_000),
        ("hole_size", 200_000, 500_000),
        ("annular_ring", 100_000, 200_000),
        ("hole_to_hole", 250_000, 500_000),
    ] {
        let v = &of(&run, check)[0];
        assert_eq!(v["measured"], json!({"length": measured}), "{check}");
        assert_eq!(v["required"], json!({"length": required}), "{check}");
        assert_eq!(v["limit"], "min");
        assert_eq!(v["document"], PCB);
        assert!(v["rule"].is_string());
        assert!(v["at"].is_object());
        assert_eq!(v["layers"].as_array().unwrap().len(), 2);
    }
    let gap = &of(&run, "hole_to_hole")[0];
    assert_eq!(
        gap["points"],
        json!([{"x": 30_750_000, "y": 0}, {"x": 30_500_000, "y": 0}])
    );
    assert_eq!(gap["at"], json!({"x": 30_625_000, "y": 0}));

    // Edges 0.9 − 0.5 − 0.15 = 0.25 mm apart; Vias come before Pads.
    assert_eq!(
        actuals(&run, "hole_to_hole"),
        [(vec![crowding, mounting], 250_000)],
        "{run:#}"
    );
}

#[test]
fn copper_near_or_beyond_the_board_edge_is_reported() {
    let dir = TempDir::new("drc-edge");
    let mut engine = engine(&dir);
    let a = net(&mut engine, "A");
    let outline = uuid(
        &engine.run_ok(
            "pcb.board_outline.add_rect",
            json!({"width": "50mm", "height": "50mm"}),
        )["result"]["uuid"],
    );
    let cutout = uuid(
        &engine.run_ok(
            "pcb.board_cutout.add",
            json!({"vertices": square("20mm", "20mm", "30mm", "30mm")}),
        )["result"]["uuid"],
    );
    // 0.1 mm from the bottom edge.
    let near_outline = track(&mut engine, Some(a), "0.2mm", "5mm", "10mm");
    let outside = track(&mut engine, Some(a), "-10mm", "5mm", "10mm");
    // 0.1 mm below the cutout.
    let near_cutout = track(&mut engine, Some(a), "19.8mm", "22mm", "28mm");
    let in_cutout = track(&mut engine, Some(a), "25mm", "22mm", "28mm");
    track(&mut engine, Some(a), "10mm", "5mm", "10mm");
    let run = drc(&mut engine);
    assert_eq!(
        actuals(&run, "edge_clearance"),
        [
            (vec![near_outline, outline], 100_000),
            (vec![outside, outline], 0),
            (vec![near_cutout, cutout], 100_000),
            (vec![in_cutout, cutout], 0),
        ],
        "{run:#}"
    );
    assert_eq!(of(&run, "edge_clearance")[0]["args"]["required"], 400_000);
    let v = &of(&run, "edge_clearance")[0];
    assert_eq!(v["measured"], json!({"length": 100_000}));
    assert_eq!(v["required"], json!({"length": 400_000}));
    assert_eq!(v["limit"], "min");
    assert!(v["rule"].is_string());
    assert_eq!(
        v["points"],
        json!([{"x": 5_000_000, "y": 100_000}, {"x": 5_000_000, "y": 0}])
    );
    assert_eq!(v["at"], json!({"x": 5_000_000, "y": 50_000}));
    assert_eq!(v["document"], PCB);
    assert_eq!(v["layers"].as_array().unwrap().len(), 1);
}

/// A one-Pad SMD Footprint at `(x, 0)` mm on `net`: its instance and Pad.
fn test_point(engine: &mut TestEngine, x: &str, net: Option<Uuid>) -> (Uuid, Uuid) {
    let mut pad = json!({"at": {"x": 0, "y": 0}, "width": "1mm", "height": "1mm"});
    if let Some(net) = net {
        pad["net"] = json!(net);
    }
    let out = engine.run_ok(
        "test.footprint.add",
        json!({"at": {"x": x, "y": 0}, "pads": [pad]}),
    );
    (
        uuid(&out["result"]["footprint"]),
        uuid(&out["result"]["pads"][0]),
    )
}

#[test]
fn every_ratsnest_line_left_is_an_unrouted_connection() {
    let dir = TempDir::new("drc-unrouted");
    let mut engine = engine(&dir);
    let a = net(&mut engine, "A");
    let (f1, p1) = test_point(&mut engine, "0mm", Some(a));
    let (f2, p2) = test_point(&mut engine, "10mm", Some(a));
    let run = drc(&mut engine);
    let found = of(&run, "unrouted");
    assert_eq!(found.len(), 1, "{run:#}");
    assert_eq!(objects(&found[0]), [f1, p1, f2, p2]);
    assert_eq!(found[0]["args"]["net"], "A");
    assert_eq!(found[0]["severity"], "error");
    assert_eq!(found[0]["layers"].as_array().unwrap().len(), 1);
    assert_eq!(found[0]["document"], PCB);
    assert!(found[0]["at"].is_object());

    track(&mut engine, Some(a), "0mm", "0mm", "10mm");
    assert_eq!(of(&drc(&mut engine), "unrouted"), Vec::<Value>::new());
}

#[test]
fn tracks_vias_and_zones_on_no_net_are_reported_but_unconnected_pads_are_not() {
    let dir = TempDir::new("drc-no-net");
    let mut engine = engine(&dir);
    test_point(&mut engine, "20mm", None);
    let t = track(&mut engine, None, "0mm", "0mm", "5mm");
    let v =
        uuid(&engine.run_ok("pcb.via.add", json!({"at": {"x": "5mm", "y": 0}}))["result"]["uuid"]);
    let z = uuid(
        &engine.run_ok(
            "pcb.zone.add",
            json!({"layer": "copper_bottom", "vertices": square("30mm", "0mm", "40mm", "10mm")}),
        )["result"]["uuid"],
    );
    let run = drc(&mut engine);
    let found: Vec<Vec<Uuid>> = of(&run, "copper_without_net").iter().map(objects).collect();
    assert_eq!(found, [vec![v], vec![t], vec![z]], "{run:#}");
    assert_eq!(of(&run, "copper_without_net")[0]["severity"], "warning");
    let zone = of(&run, "copper_without_net")
        .into_iter()
        .find(|v| objects(v) == [z])
        .unwrap();
    assert_eq!(zone["at"], json!({"x":35_000_000,"y":5_000_000}));
    assert_eq!(zone["layers"].as_array().unwrap().len(), 1);
    let board = engine.run_ok("object.get", json!({"uuid":z}))["object"].clone();
    assert_eq!(zone["layers"], json!([board["layer"]]));
}

#[test]
fn track_ends_and_vias_that_connect_nothing_dangle() {
    let dir = TempDir::new("drc-dangling");
    let mut engine = engine(&dir);
    let a = net(&mut engine, "A");
    test_point(&mut engine, "0mm", Some(a));
    let routed = track(&mut engine, Some(a), "0mm", "0mm", "5mm");
    // From the Pad to nowhere: its far end dangles.
    let stub = uuid(
        &engine.run_ok(
            "pcb.track.add",
            json!({"layer": "copper_top", "net": a,
                   "points": [{"x": 0, "y": 0}, {"x": 0, "y": "-5mm"}]}),
        )["result"]["tracks"][0],
    );
    // A Via joining a top and a bottom Track is fine; a lone Via dangles.
    let joined = uuid(
        &engine.run_ok("pcb.via.add", json!({"at": {"x": "5mm", "y": 0}, "net": a}))["result"]["uuid"],
    );
    let lone = uuid(
        &engine.run_ok(
            "pcb.via.add",
            json!({"at": {"x": "20mm", "y": 0}, "net": a}),
        )["result"]["uuid"],
    );
    let bottom = uuid(
        &engine.run_ok(
            "pcb.track.add",
            json!({"layer": "copper_bottom", "net": a,
                   "points": [{"x": "5mm", "y": 0}, {"x": "5mm", "y": "5mm"}]}),
        )["result"]["tracks"][0],
    );
    let run = drc(&mut engine);
    let found = of(&run, "dangling");
    let dangling: Vec<(Vec<Uuid>, Value, Value)> = found
        .iter()
        .map(|v| (objects(v), v["args"]["x"].clone(), v["args"]["y"].clone()))
        .collect();
    assert_eq!(
        dangling,
        [
            (vec![lone], json!(20_000_000), json!(0)),
            (vec![stub], json!(0), json!(-5_000_000)),
            (vec![bottom], json!(5_000_000), json!(5_000_000)),
        ],
        "{run:#}"
    );
    assert!(
        !found
            .iter()
            .any(|v| objects(v) == [routed] || objects(v) == [joined])
    );
}

#[test]
fn stale_and_unfilled_zones_are_refilled_before_checking() {
    let dir = TempDir::new("drc-refill");
    let mut engine = engine(&dir);
    // The test Nets are on no schematic, so Sync would remove them, and the test
    // Footprints' Designators sit on their Pads.
    for check in ["out_of_sync", "silk_to_mask"] {
        engine.run_ok(
            "drc.set_severity",
            json!({"check": check, "severity": "off"}),
        );
    }
    let gnd = net(&mut engine, "GND");
    let sig = net(&mut engine, "SIG");
    test_point(&mut engine, "2mm", Some(gnd));
    test_point(&mut engine, "8mm", Some(gnd));
    engine.run_ok(
        "pcb.zone.add",
        json!({"layer": "copper_top", "net": gnd,
               "vertices": square("0mm", "-5mm", "10mm", "5mm")}),
    );
    // Unfilled: the fill made for the run joins the two Pads.
    let run = drc(&mut engine);
    assert_eq!(run["refilled"], true);
    assert_eq!(of(&run, "unrouted"), Vec::<Value>::new(), "{run:#}");
    assert!(
        engine
            .project()
            .unwrap()
            .zone_fills(PCB.parse().unwrap())
            .is_none(),
        "the run does not store its fill"
    );

    engine.run_ok("pcb.zone.fill", json!({}));
    let run = drc(&mut engine);
    assert_eq!(run["refilled"], false);
    assert_eq!(run["violations"], json!([]), "{run:#}");

    // A Track of another Net through the filled area: the stored fill would short it,
    // the refill makes way.
    track(&mut engine, Some(sig), "3mm", "1mm", "9mm");
    let run = drc(&mut engine);
    assert_eq!(run["refilled"], true);
    assert_eq!(of(&run, "short_circuit"), Vec::<Value>::new(), "{run:#}");
    assert_eq!(of(&run, "clearance"), Vec::<Value>::new(), "{run:#}");
}

const PCB: &str = "00000000-0000-4000-8000-000000000003";

/// A board with one clearance Violation between two Tracks: the engine and the Tracks.
fn one_clearance_violation(dir: &TempDir) -> (TestEngine, Uuid, Uuid) {
    let mut engine = engine(dir);
    let a = net(&mut engine, "A");
    let b = net(&mut engine, "B");
    let ta = track(&mut engine, Some(a), "0mm", "0mm", "5mm");
    let tb = track(&mut engine, Some(b), "0.3mm", "0mm", "5mm");
    (engine, ta, tb)
}

#[test]
fn a_check_set_to_warning_or_off_reports_accordingly() {
    let dir = TempDir::new("drc-severity");
    let (mut engine, _, _) = one_clearance_violation(&dir);
    engine.run_ok(
        "drc.set_severity",
        json!({"check": "clearance", "severity": "warning"}),
    );
    assert_eq!(of(&drc(&mut engine), "clearance")[0]["severity"], "warning");
    engine.run_ok(
        "drc.set_severity",
        json!({"check": "clearance", "severity": "off"}),
    );
    assert_eq!(of(&drc(&mut engine), "clearance"), Vec::<Value>::new());
    let err = engine.run_err(
        "drc.set_severity",
        json!({"check": "unconnected_pin", "severity": "off"}),
    );
    assert_eq!(err.code, reda_command::ErrorCode::InvalidParams);
}

#[test]
fn a_waived_violation_is_hidden_until_asked_for_and_its_waiver_is_saved() {
    let dir = TempDir::new("drc-waive");
    let (mut engine, ta, tb) = one_clearance_violation(&dir);
    let before = drc(&mut engine)["design"].clone();
    let out = engine.run_ok(
        "drc.waive",
        json!({"violations": [{"check": "clearance", "objects": [tb, ta]}], "reason": "net tie"}),
    );
    let waiver = uuid(&out["result"]["waivers"][0]);
    assert_eq!(
        engine.run_ok("object.get", json!({"uuid": waiver}))["document"],
        PCB
    );
    assert_eq!(
        engine.run_ok("drc.stale", json!({"design": before}))["stale"],
        true
    );

    let run = drc(&mut engine);
    assert_eq!(of(&run, "clearance"), Vec::<Value>::new());
    assert_eq!(run["waived"], 1);
    let run = engine.run_ok("drc.run", json!({"include_waived": true}));
    assert_eq!(of(&run, "clearance")[0]["waiver"], json!(waiver));
    assert_eq!(
        engine.run_ok("drc.stale", json!({"design": run["design"]}))["stale"],
        false
    );

    let again = engine.run_err(
        "drc.waive",
        json!({"violations": [{"check": "clearance", "objects": [ta, tb]}]}),
    );
    assert_eq!(again.code, reda_command::ErrorCode::Rejected);
    let none = engine.run_err(
        "drc.waive",
        json!({"violations": [{"check": "short_circuit", "objects": [ta, tb]}]}),
    );
    assert_eq!(none.code, reda_command::ErrorCode::Rejected);

    engine.run_ok("project.save", json!({}));
    let (_, text) = reda_testkit::read_tree(dir.path())
        .into_iter()
        .find(|(path, _)| path.ends_with(".redapcb"))
        .unwrap();
    let text = String::from_utf8(text).unwrap();
    assert!(text.contains(r#""waivers": ["#), "{text}");
    assert!(text.contains(r#""reason":"net tie""#), "{text}");
    let mut reopened = probe_engine();
    reopened.run_ok("project.open", json!({"path": dir.path().join("Demo")}));
    let run = drc(&mut reopened);
    assert_eq!(run["waived"], 1);
    assert_eq!(of(&run, "clearance"), Vec::<Value>::new());
}

#[test]
fn a_waiver_stops_matching_once_the_objects_are_clear_and_can_be_deleted() {
    let dir = TempDir::new("drc-unwaive");
    let (mut engine, ta, tb) = one_clearance_violation(&dir);
    let out = engine.run_ok(
        "drc.waive",
        json!({"violations": [{"check": "clearance", "objects": [ta, tb]}]}),
    );
    let waiver = uuid(&out["result"]["waivers"][0]);
    engine.run_ok("pcb.track.delete", json!({"tracks": [tb]}));
    let run = drc(&mut engine);
    assert_eq!(run["unmatched_waivers"], json!([waiver]));

    let err = engine.run_err("drc.unwaive", json!({"waivers": [waiver, ta]}));
    assert_eq!(err.details.unwrap()["pointer"], "/waivers/1");
    engine.run_ok("drc.unwaive", json!({"waivers": [waiver]}));
    assert_eq!(drc(&mut engine)["unmatched_waivers"], json!([]));
}

#[test]
fn clearance_reports_measurement_closest_points_and_the_winning_record() {
    let dir = TempDir::new("drc-structured-clearance");
    let (mut engine, ta, tb) = one_clearance_violation(&dir);
    let record =
        engine.run_ok("pcb.rules.set", json!({"kind": "clearance"}))["result"]["rule"]["uuid"]
            .clone();
    let run = drc(&mut engine);
    let v = &of(&run, "clearance")[0];
    assert_eq!(v["measured"], json!({"length": 100_000}));
    assert_eq!(v["required"], json!({"length": 160_000}));
    assert_eq!(v["limit"], "min");
    assert_eq!(v["rule"], record);
    assert_eq!(v["document"], PCB);
    assert_eq!(
        v["points"],
        json!([{"x": 0, "y": 100_000}, {"x": 0, "y": 200_000}])
    );
    assert_eq!(v["at"], json!({"x": 0, "y": 150_000}));
    assert_eq!(v["layers"].as_array().unwrap().len(), 1);
    assert_eq!(objects(v), [ta, tb]);
}

#[test]
fn track_width_reports_its_rule_location_and_layer() {
    let dir = TempDir::new("drc-width-details");
    let mut engine = engine(&dir);
    let t = uuid(&engine.run_ok("pcb.track.add", json!({"layer": "copper_top", "width": "0.1mm", "points": [{"x": "1mm", "y": "2mm"}, {"x": "5mm", "y": "2mm"}]}))["result"]["tracks"][0]);
    let rule =
        engine.run_ok("pcb.rules.set", json!({"kind": "track_width"}))["result"]["rule"]["uuid"]
            .clone();
    let run = drc(&mut engine);
    let v = &of(&run, "track_width")[0];
    assert_eq!(objects(v), [t]);
    assert_eq!(v["measured"], json!({"length": 100_000}));
    assert_eq!(v["required"], json!({"length": 200_000}));
    assert_eq!(v["limit"], "min");
    assert_eq!(v["rule"], rule);
    assert_eq!(v["at"], json!({"x": 3_000_000, "y": 2_000_000}));
    assert_eq!(v["document"], PCB);
    assert_eq!(v["layers"].as_array().unwrap().len(), 1);
    assert!(v.get("points").is_none());
}

#[test]
fn a_zone_with_empty_fill_keeps_its_layer_and_polygon_location() {
    let dir = TempDir::new("drc-empty-fill-marker");
    let mut e = engine(&dir);
    let vertices = square("30mm", "0mm", "40mm", "10mm");
    let z = e.run_ok(
        "pcb.zone.add",
        json!({"layer":"copper_bottom","vertices":vertices}),
    )["result"]["uuid"]
        .clone();
    e.run_ok(
        "pcb.keepout.add",
        json!({"layer":"copper_bottom","vertices":vertices}),
    );
    let run = drc(&mut e);
    let zone = of(&run, "copper_without_net")
        .into_iter()
        .find(|v| v["objects"] == json!([z]))
        .unwrap();
    let layer = e.run_ok("object.get", json!({"uuid":z}))["object"]["layer"].clone();
    assert_eq!(zone["layers"], json!([layer]));
    assert_eq!(zone["at"], json!({"x":35_000_000,"y":5_000_000}));
}

#[test]
fn equal_pair_limits_name_the_first_objects_class_record() {
    let dir = TempDir::new("drc-source-tie");
    let mut e = engine(&dir);
    let a = net(&mut e, "A");
    let b = net(&mut e, "B");
    let ca = class(&mut e, json!({"name":"Class A"}), &[a]);
    let cb = class(&mut e, json!({"name":"Class B"}), &[b]);
    let mut records = Vec::new();
    for class in [ca, cb] {
        records.push(
            e.run_ok(
                "pcb.rules.set",
                json!({"kind":"clearance","scope":{"net_class":class},"values":{"value":"0.5mm"}}),
            )["result"]["rule"]["uuid"]
                .clone(),
        );
    }
    let ta = track(&mut e, Some(a), "0mm", "0mm", "5mm");
    let tb = track(&mut e, Some(b), "0.3mm", "0mm", "5mm");
    let v = &of(&drc(&mut e), "clearance")[0];
    assert_eq!(objects(v), [ta, tb]);
    assert_eq!(v["rule"], records[0]);
    assert_eq!(v["required"], json!({"length":500_000}));
    e.run_ok(
        "pcb.rules.set",
        json!({"kind":"clearance","scope":{"net_class":cb},"values":{"value":"0.6mm"}}),
    );
    assert_eq!(of(&drc(&mut e), "clearance")[0]["rule"], records[1]);
}

#[test]
fn clearance_between_through_vias_names_both_violating_layers() {
    let dir = TempDir::new("drc-clearance-via-layers");
    let mut e = engine(&dir);
    for (name, x) in [("A", 0), ("B", 800_000)] {
        let n = net(&mut e, name);
        e.run_ok(
            "pcb.via.add",
            json!({"net":n,"at":{"x":x,"y":0},"diameter":"0.7mm","hole":"0.3mm"}),
        );
    }
    let v = &of(&drc(&mut e), "clearance")[0];
    assert_eq!(v["layers"].as_array().unwrap().len(), 2);
    assert_eq!(
        v["points"],
        json!([{"x":350_000,"y":0},{"x":450_000,"y":0}])
    );
}

#[test]
fn a_keepout_marker_names_only_the_layer_where_a_via_enters_it() {
    let dir = TempDir::new("drc-keepout-via-layer");
    let mut e = engine(&dir);
    e.run_ok(
        "pcb.keepout.add",
        json!({"layer":"copper_top","vertices":square("0mm","0mm","3mm","3mm")}),
    );
    e.run_ok("pcb.via.add", json!({"at":{"x":"1mm","y":"1mm"}}));
    let pcb = e.project().unwrap().pcb().unwrap();
    let top = pcb.copper_layers().next().unwrap().uuid;
    assert_eq!(of(&drc(&mut e), "keepout")[0]["layers"], json!([top]));
}
