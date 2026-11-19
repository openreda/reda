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

/// A 0.2 mm Track on `layer` through `points`, in mm.
fn track_on(engine: &mut TestEngine, layer: &str, net: Uuid, points: Value) -> Uuid {
    uuid(
        &engine.run_ok(
            "pcb.track.add",
            json!({"layer": layer, "width": "0.2mm", "net": net, "points": points}),
        )["result"]["tracks"][0],
    )
}

#[test]
fn a_short_is_one_violation_per_net_pair_and_layer_listing_every_object() {
    let dir = TempDir::new("drc-short-pair");
    let mut engine = engine(&dir);
    let a = net(&mut engine, "A");
    let b = net(&mut engine, "B");
    // One Track of A crossing three Tracks of B on the top copper.
    let ta = track_on(
        &mut engine,
        "copper_top",
        a,
        json!([{"x": "2mm", "y": "-1mm"}, {"x": "2mm", "y": "5mm"}]),
    );
    let tb: Vec<Uuid> = ["0mm", "2mm", "4mm"]
        .into_iter()
        .map(|y| track(&mut engine, Some(b), y, "0mm", "5mm"))
        .collect();
    let run = drc(&mut engine);
    let shorts = of(&run, "short_circuit");
    assert_eq!(shorts.len(), 1, "{run:#}");
    let mut found = objects(&shorts[0]);
    found.sort();
    let mut expected = [vec![ta], tb.clone()].concat();
    expected.sort();
    assert_eq!(found, expected);
    assert_eq!(shorts[0]["layers"].as_array().unwrap().len(), 1);

    // The same two Nets touching on the bottom copper too: a second Violation.
    let ba = track_on(
        &mut engine,
        "copper_bottom",
        a,
        json!([{"x": "10mm", "y": "0mm"}, {"x": "15mm", "y": "0mm"}]),
    );
    let bb = track_on(
        &mut engine,
        "copper_bottom",
        b,
        json!([{"x": "12mm", "y": "-1mm"}, {"x": "12mm", "y": "1mm"}]),
    );
    let run = drc(&mut engine);
    let shorts = of(&run, "short_circuit");
    assert_eq!(shorts.len(), 2, "{run:#}");
    let layers: Vec<_> = shorts.iter().map(|v| v["layers"].clone()).collect();
    assert_ne!(layers[0], layers[1]);
    assert!(shorts.iter().any(|v| objects(v) == [ba, bb]), "{run:#}");
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

#[test]
fn annular_ring_measures_each_inner_land_against_the_shared_hole() {
    let dir = TempDir::new("pad-stack-ring");
    let mut engine = probe_engine();
    engine.run_ok(
        "project.new",
        json!({"name":"Demo","path":dir.path(),"layers":4}),
    );
    let pcb = engine.project().unwrap().pcb().unwrap();
    let inner: Vec<_> = pcb
        .copper_layers()
        .filter(|l| l.side.is_none())
        .map(|l| l.uuid)
        .collect();
    engine.run_ok(
        "pcb.free_pad.add",
        json!({"kind":"plated_through_hole","width":"3mm","at":{"x":"5mm","y":"5mm"},
        "hole":{"width":"1mm"},"middle":{"shape":"rect","width":"1.1mm","height":"3mm"}}),
    );
    let violations = of(&drc(&mut engine), "annular_ring");
    assert_eq!(violations.len(), 1, "{violations:?}");
    assert_eq!(violations[0]["measured"]["length"], 50000);
    assert_eq!(violations[0]["layers"], json!(inner));
}

#[test]
fn a_different_inner_via_land_is_not_duplicate_copper() {
    let dir = TempDir::new("pad-stack-duplicate");
    let mut engine = probe_engine();
    engine.run_ok(
        "project.new",
        json!({"name":"Demo","path":dir.path(),"layers":4}),
    );
    for middle in ["0.7mm", "1.4mm"] {
        engine.run_ok(
            "pcb.via.add",
            json!({"at":{"x":"5mm","y":"5mm"},"hole":"0.3mm",
            "diameter":{"start":"0.7mm","middle":middle}}),
        );
    }
    assert!(of(&drc(&mut engine), "duplicate_copper").is_empty());
}

#[test]
fn only_the_oversized_inner_lands_break_board_edge_clearance() {
    let dir = TempDir::new("pad-stack-edge");
    let mut engine = probe_engine();
    engine.run_ok("project.new",json!({"name":"Demo","path":dir.path(),"layers":4,"board":{"width":"20mm","height":"10mm"}}));
    let inner: Vec<_> = engine
        .project()
        .unwrap()
        .pcb()
        .unwrap()
        .copper_layers()
        .filter(|l| l.side.is_none())
        .map(|l| l.uuid)
        .collect();
    engine.run_ok("pcb.free_pad.add",json!({"kind":"plated_through_hole","width":"1mm","at":{"x":"1.5mm","y":"5mm"},"hole":{"width":"0.5mm"},
        "middle":{"shape":"rect","width":"2.4mm","height":"1mm"}}));
    let violations = of(&drc(&mut engine), "edge_clearance");
    assert_eq!(violations.len(), 1, "{violations:?}");
    assert_eq!(violations[0]["measured"]["length"], 300000);
    assert_eq!(violations[0]["layers"], json!(inner));
}

#[test]
fn via_span_distinguishes_missing_types_class_permissions_and_stretched_micro_types() {
    let dir = TempDir::new("via-span-reasons");
    let mut engine = probe_engine();
    engine.run_ok(
        "project.new",
        json!({"path":dir.path(),"name":"Demo","layers":6}),
    );
    let pcb = engine.project().unwrap().pcb().unwrap();
    let doc = pcb.uuid;
    let l: Vec<_> = pcb.copper_layers().map(|l| l.uuid).collect();
    let laser = uuid(
        &engine.run_ok(
            "pcb.via_type.add",
            json!({"document":doc,"from":l[0],"to":l[1],"micro":true}),
        )["result"]["uuid"],
    );
    let signal = net(&mut engine, "SIG");
    let class=uuid(&engine.run_ok("pcb.net_class.add",json!({"name":"Laser","nets":[signal],"rules":[{"kind":"via_span","values":{"allowed":[laser]}}]}))["result"]["uuid"]);
    let mut place = |x, from, to| {
        uuid(
            &engine.run_ok(
                "pcb.via.add",
                json!({"at":{"x":x,"y":0},"from":from,"to":to,"net":signal}),
            )["result"]["uuid"],
        )
    };
    let valid = place(1000000, l[0], l[1]);
    let disallowed = place(2000000, l[0], l[5]);
    let unmatched = place(3000000, l[2], l[3]);
    let pcb = engine.project().unwrap().pcb().unwrap();
    let class_rule = pcb
        .class_rule(reda_model::RuleKind::ViaSpan, class)
        .unwrap()
        .uuid;
    let found = of(&drc(&mut engine), "via_span");
    assert_eq!(found.len(), 2, "{found:?}");
    let forbidden = found.iter().find(|v| objects(v) == [disallowed]).unwrap();
    assert_eq!(forbidden["args"]["reason"], "not_allowed");
    assert_eq!(forbidden["rule"], json!(class_rule));
    let none = found.iter().find(|v| objects(v) == [unmatched]).unwrap();
    assert_eq!(none["args"]["reason"], "no_type");
    assert_eq!(none["layers"], json!([l[2], l[3]]));
    engine.run_ok("project.save", json!({}));
    let project = dir.path().join("Demo");
    let file = project.join("Demo.redapcb");
    let mut saved: Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    // This is the stored result of inserting inner Layers into a formerly adjacent laser span.
    let kind = saved["stackups"][0]["via_types"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|v| v["uuid"] == json!(laser))
        .unwrap();
    kind["to"] = json!(l[3]);
    let via = saved["vias"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|v| v["uuid"] == json!(valid))
        .unwrap();
    via["span"]["to"] = json!(l[3]);
    std::fs::write(file, serde_json::to_vec(&saved).unwrap()).unwrap();
    let mut reopened = probe_engine();
    reopened.run_ok("project.open", json!({"path":project.join("Demo.reda")}));
    let found = of(&drc(&mut reopened), "via_span");
    let stretched = found.iter().find(|v| objects(v) == [valid]).unwrap();
    assert_eq!(stretched["args"]["reason"], "micro_too_long");
    assert_eq!(stretched["layers"], json!(&l[..4]));
}

#[test]
fn hole_spacing_exempts_end_to_end_vias_of_one_net_at_the_same_centre() {
    let dir = TempDir::new("stack-hole-spacing");
    let mut engine = probe_engine();
    engine.run_ok(
        "project.new",
        json!({"path":dir.path(),"name":"Demo","layers":6}),
    );
    let layers: Vec<_> = engine
        .project()
        .unwrap()
        .pcb()
        .unwrap()
        .copper_layers()
        .map(|l| l.uuid)
        .collect();
    let a = net(&mut engine, "A");
    let b = net(&mut engine, "B");
    let mut via = |x: i64, signal, from: usize, to: usize| {
        uuid(
            &engine.run_ok(
                "pcb.via.add",
                json!({
        "at":{"x":x,"y":0},"net":signal,"from":layers[from-1],"to":layers[to-1]}),
            )["result"]["uuid"],
        )
    };
    via(1_000_000, a, 2, 1);
    via(1_000_000, a, 3, 2);
    via(1_000_000, a, 4, 3);
    let overlap = vec![via(4_000_000, a, 1, 3), via(4_000_000, a, 2, 4)];
    let different_nets = vec![via(8_000_000, a, 1, 2), via(8_000_000, b, 2, 3)];
    let offset = vec![via(12_000_000, a, 1, 2), via(12_050_000, a, 2, 3)];
    let found = of(&drc(&mut engine), "hole_to_hole");
    let pairs: Vec<_> = found.iter().map(objects).collect();
    assert_eq!(pairs, [overlap, different_nets, offset]);
    assert_eq!(found[0]["layers"], json!(&layers[1..3]));
    assert_eq!(found[1]["layers"], json!([layers[1]]));
    assert_eq!(found[2]["layers"], json!([layers[1]]));
}

#[test]
fn via_size_checks_each_physical_diameter_with_the_actual_drilling_method() {
    let dir = TempDir::new("via-size-groups");
    let mut engine = probe_engine();
    engine.run_ok(
        "project.new",
        json!({"path":dir.path(),"name":"Demo","layers":6}),
    );
    let pcb = engine.project().unwrap().pcb().unwrap();
    let doc = pcb.uuid;
    let layers: Vec<_> = pcb.copper_layers().map(|l| l.uuid).collect();
    engine.run_ok(
        "pcb.via_type.add",
        json!({"document":doc,"from":layers[2],"to":layers[0],"micro":true}),
    );
    engine.run_ok("pcb.rules.set",json!({"kind":"microvia_size","values":{"diameter":{"min":"0.25mm","max":"0.35mm"},"hole":{"max":"0.11mm"}}}));
    let laser = uuid(
        &engine.run_ok(
            "pcb.via.add",
            json!({"at":{"x":"2mm","y":0},"from":layers[0],"to":layers[2],
        "diameter":{"start":"0.28mm","middle":"0.46mm","end":"0.55mm"},"hole":"0.12mm"}),
        )["result"]["uuid"],
    );
    let drilled = uuid(
        &engine.run_ok(
            "pcb.via.add",
            json!({"at":{"x":"7mm","y":0},"from":layers[2],"to":layers[5],
        "diameter":{"start":"0.8mm","middle":"0.45mm","end":"0.5mm"},"hole":"0.3mm"}),
        )["result"]["uuid"],
    );
    engine.run_ok(
        "pcb.rules.set",
        json!({"kind":"annular_ring","values":{"min":"0.2mm"}}),
    );
    let run = drc(&mut engine);
    let mut rings: Vec<_> = of(&run, "annular_ring")
        .into_iter()
        .filter(|v| objects(v) == [drilled])
        .map(|v| {
            (
                v["measured"]["length"].as_i64().unwrap(),
                v["layers"].clone(),
            )
        })
        .collect();
    rings.sort_by_key(|(actual, _)| *actual);
    assert_eq!(
        rings,
        [
            (75_000, json!(&layers[3..5])),
            (100_000, json!([layers[5]]))
        ]
    );
    let sizes = |id, check| {
        let mut result: Vec<_> = of(&run, check)
            .into_iter()
            .map(|v| {
                assert_eq!(objects(&v), [id]);
                (
                    v["measured"]["length"].as_i64().unwrap(),
                    v["layers"].clone(),
                )
            })
            .collect();
        result.sort_by_key(|(measured, _)| *measured);
        result
    };
    assert_eq!(
        sizes(laser, "microvia_size"),
        [
            (120_000, json!(&layers[..3])),
            (460_000, json!([layers[1]])),
            (550_000, json!([layers[0]]))
        ]
    );
    assert_eq!(
        sizes(drilled, "via_size"),
        [
            (450_000, json!(&layers[3..5])),
            (500_000, json!([layers[5]]))
        ]
    );
}

#[test]
fn via_aspect_ratio_uses_each_span_depth_and_max_stack_counts_only_laser_holes() {
    let dir = TempDir::new("drc-via-hdi");
    let mut engine = probe_engine();
    engine.run_ok(
        "project.new",
        json!({"path":dir.path(),"name":"Demo","layers":6}),
    );
    let pcb = engine.project().unwrap().pcb().unwrap();
    let document = pcb.uuid;
    let layers: Vec<_> = pcb.copper_layers().map(|l| l.uuid).collect();
    // Five equal blank sheets share 1,390,001 nm. L1:L2 includes both 35 µm copper foils.
    let sheets: Vec<_> = pcb
        .stackup()
        .dielectrics()
        .map(|d| json!({"uuid":d.uuid, "thickness":null}))
        .collect();
    engine.run_ok(
        "pcb.stackup.set",
        json!({"thickness":1600001, "dielectrics":sheets}),
    );
    let signal = net(&mut engine, "HDI");
    for (a, b, micro) in [(0, 1, true), (1, 2, true), (2, 3, false), (3, 4, true)] {
        engine.run_ok(
            "pcb.via_type.add",
            json!({"document":document,"from":layers[a],"to":layers[b],"micro":micro}),
        );
    }
    let mut place = |x: i64, a: usize, b: usize| {
        uuid(
            &engine.run_ok(
                "pcb.via.add",
                json!({
                    "at":{"x":x,"y":0},"net":signal,"from":layers[a],"to":layers[b],
                    "diameter":"0.6mm","hole":"0.2mm"
                }),
            )["result"]["uuid"],
        )
    };
    let first = place(1_000_000, 0, 1);
    let second = place(1_000_000, 1, 2);
    let bridge = place(1_000_000, 2, 3);
    let last = place(1_000_000, 3, 4);
    let independent = place(5_000_000, 4, 3);
    // An unlisted mechanical span between two lasers does not join their stacks.
    let unconnected_first = place(9_000_000, 0, 1);
    place(9_000_000, 1, 3);
    let unconnected_last = place(9_000_000, 3, 4);
    let clean = drc(&mut engine);
    assert!(of(&clean, "aspect_ratio").is_empty());
    assert!(of(&clean, "max_stack").is_empty());
    engine.run_ok(
        "pcb.rules.set",
        json!({"kind":"microvia_size","values":{"aspect_ratio":{"max":1740},"max_stack":1}}),
    );
    let run = drc(&mut engine);
    let ratios = of(&run, "aspect_ratio");
    // The fractional share puts the ratio just over 1.740:1, with no nm rounding.
    let mut actual: Vec<_> = ratios.iter().map(|v| objects(v)[0]).collect();
    actual.sort();
    let mut expected = vec![
        first,
        second,
        last,
        independent,
        unconnected_first,
        unconnected_last,
    ];
    expected.sort();
    assert_eq!(actual, expected);
    assert!(
        ratios
            .iter()
            .all(|v| v["limit"] == "max" && !objects(v).contains(&bridge))
    );
    let stacks = of(&run, "max_stack");
    assert_eq!(stacks.len(), 1);
    assert_eq!(stacks[0]["args"]["actual"], 3);
    assert_eq!(stacks[0]["args"]["maximum"], 1);
    assert_eq!(stacks[0]["layers"], json!(&layers[..5]));
    engine.run_ok(
        "pcb.rules.set",
        json!({"kind":"microvia_size","values":{"aspect_ratio":{"max":null},"max_stack":null}}),
    );
    let cleared = drc(&mut engine);
    assert!(of(&cleared, "aspect_ratio").is_empty());
    assert!(of(&cleared, "max_stack").is_empty());
}

#[test]
fn only_retained_lands_have_annular_ring_checks_but_an_omitted_layer_keeps_hole_clearance() {
    let dir = TempDir::new("unused-ring-and-hole");
    let mut e = probe_engine();
    e.run_ok(
        "project.new",
        json!({"path":dir.path(),"name":"Demo","layers":4}),
    );
    let layers: Vec<_> = e
        .project()
        .unwrap()
        .pcb()
        .unwrap()
        .copper_layers()
        .map(|l| l.uuid)
        .collect();
    let sig = net(&mut e, "SIG");
    let via = e.run_ok(
        "pcb.via.add",
        json!({"net":sig,"at":{"x":"5mm","y":"5mm"},
        "hole":"0.4mm","diameter":"0.5mm","unused_layers":"remove_keep_ends"}),
    )["result"]["uuid"]
        .clone();
    e.run_ok(
        "pcb.rules.set",
        json!({"kind":"clearance","values":{"hole":{"via":"0.4mm"}}}),
    );
    e.run_ok("pcb.track.add",json!({"layer":layers[1],"net":sig,"width":"0.1mm","points":[{"x":"4mm","y":"5.5mm"},{"x":"6mm","y":"5.5mm"}]}));
    e.run_ok("pcb.track.add",json!({"layer":layers[2],"net":sig,"width":"0.1mm","points":[{"x":"5mm","y":"5mm"},{"x":"6mm","y":"5mm"}]}));
    let report = drc(&mut e);
    let rings = of(&report, "annular_ring");
    assert_eq!(rings.len(), 1);
    assert_eq!(rings[0]["objects"], json!([via]));
    assert_eq!(rings[0]["layers"], json!([layers[0], layers[2], layers[3]]));
    let hits = of(&report, "clearance");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0]["args"]["part"], "hole.via");
    assert_eq!(hits[0]["layers"], json!([layers[1]]));
    assert_eq!(hits[0]["measured"]["length"], 250000);
}
