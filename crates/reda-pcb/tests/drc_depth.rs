//! DRC III: the checks that measure silkscreen, Vias under SMD Pads, poor connections,
//! courtyard clearance with the fallback box, the dangling tolerance and non-plated hole
//! edges, through `drc.run`.
//!
//! The board is the Generic preset's: clearance 0.16 mm, edge clearance 0.4 mm, Track
//! width 0.2 mm. The rules no preset seeds start at their defaults: `silk_to_edge`
//! 0.254 mm, `silk_to_mask` 0.10 mm over exposed copper, `courtyard_clearance` 0,
//! `dangling` 0 and `micro_segment` 0.01 mm.

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

fn drc(engine: &mut TestEngine) -> Value {
    engine.run_ok("drc.run", json!({}))
}

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

fn measured(violation: &Value) -> i64 {
    violation["measured"]["length"].as_i64().unwrap()
}

/// A route of 0.2 mm Tracks on the top copper through `points` (mm pairs).
fn route(engine: &mut TestEngine, net: Option<Uuid>, points: &[(&str, &str)]) -> Vec<Uuid> {
    let points: Vec<Value> = points
        .iter()
        .map(|(x, y)| json!({"x": x, "y": y}))
        .collect();
    let mut params = json!({"layer": "copper_top", "width": "0.2mm", "points": points});
    if let Some(net) = net {
        params["net"] = json!(net);
    }
    engine.run_ok("pcb.track.add", params)["result"]["tracks"]
        .as_array()
        .unwrap()
        .iter()
        .map(uuid)
        .collect()
}

/// A 1 mm square SMD Free Pad on the top copper at `(x, y)` mm.
fn smd(engine: &mut TestEngine, x: &str, y: &str, net: Option<Uuid>) -> Uuid {
    let mut params = json!({"kind": "smd", "shape": "rect", "at": {"x": x, "y": y},
                            "width": "1mm"});
    if let Some(net) = net {
        params["net"] = json!(net);
    }
    uuid(&engine.run_ok("pcb.free_pad.add", params)["result"]["uuid"])
}

fn silk_line(engine: &mut TestEngine, from: (&str, &str), to: (&str, &str), width: &str) -> Uuid {
    uuid(
        &engine.run_ok(
            "pcb.graphic.add_line",
            json!({"layer": "silkscreen_top", "from": {"x": from.0, "y": from.1},
                   "to": {"x": to.0, "y": to.1}, "width": width}),
        )["result"]["uuid"],
    )
}

fn set_rule(engine: &mut TestEngine, kind: &str, values: Value) {
    engine.run_ok("pcb.rules.set", json!({"kind": kind, "values": values}));
}

fn board_rule(engine: &TestEngine, kind: &str) -> String {
    let pcb = engine.project().unwrap().pcb().unwrap();
    pcb.rules()
        .find(|r| r.kind.id() == kind && r.scope.is_board_wide())
        .unwrap()
        .uuid
        .to_string()
}

#[test]
fn a_new_board_holds_the_rules_no_preset_seeds_at_their_defaults() {
    let dir = TempDir::new("drc-depth-defaults");
    let engine = engine(&dir);
    let pcb = engine.project().unwrap().pcb().unwrap();
    let values = |kind: &str| {
        let rule = pcb
            .rules()
            .find(|r| r.kind.id() == kind && r.scope.is_board_wide())
            .unwrap_or_else(|| panic!("no board-wide `{kind}` rule"));
        serde_json::to_value(rule.values).unwrap()
    };
    assert_eq!(values("silk_to_edge"), json!({"value": 254_000}));
    assert_eq!(
        values("silk_to_mask"),
        json!({"value": 100_000, "mode": "exposed_copper"})
    );
    assert_eq!(values("courtyard_clearance"), json!({"value": 0}));
    assert_eq!(values("dangling"), json!({"tolerance": 0}));
    assert_eq!(values("micro_segment"), json!({"max_length": 10_000}));
}

#[test]
fn copper_too_close_to_a_non_plated_hole_edge_breaks_the_edge_clearance() {
    let dir = TempDir::new("drc-depth-npth");
    let mut engine = engine(&dir);
    let a = net(&mut engine, "A");
    let hole = uuid(
        &engine.run_ok(
            "pcb.free_pad.add",
            json!({"kind": "non_plated", "at": {"x": "40mm", "y": 0}, "width": "3mm",
                   "hole": {"width": "3mm"}}),
        )["result"]["uuid"],
    );
    // Copper edge at y = 1.7 mm: 0.2 mm from the 1.5 mm hole edge.
    let near = route(
        &mut engine,
        Some(a),
        &[("35mm", "1.8mm"), ("45mm", "1.8mm")],
    )[0];
    // 0.5 mm clear.
    route(
        &mut engine,
        Some(a),
        &[("35mm", "-2.1mm"), ("45mm", "-2.1mm")],
    );
    let run = drc(&mut engine);
    let found = of(&run, "edge_clearance");
    assert_eq!(found.len(), 1, "{run:#}");
    assert_eq!(objects(&found[0]), [near, hole]);
    assert_eq!(measured(&found[0]), 200_000);
    assert_eq!(found[0]["required"], json!({"length": 400_000}));
    assert_eq!(found[0]["limit"], "min");
    assert_eq!(found[0]["rule"], board_rule(&engine, "edge_clearance"));
    assert_eq!(
        found[0]["points"],
        json!([{"x": 40_000_000, "y": 1_700_000}, {"x": 40_000_000, "y": 1_500_000}])
    );
}

#[test]
fn silkscreen_too_close_to_the_board_edge_is_reported() {
    let dir = TempDir::new("drc-depth-silk-edge");
    let mut engine = engine(&dir);
    let outline = uuid(
        &engine.run_ok(
            "pcb.board_outline.add_rect",
            json!({"width": "50mm", "height": "50mm"}),
        )["result"]["uuid"],
    );
    // A 0.1 mm line at y = 0.2 mm: its edge 0.15 mm from the bottom edge.
    let near = silk_line(&mut engine, ("5mm", "0.2mm"), ("10mm", "0.2mm"), "0.1mm");
    let outside = silk_line(&mut engine, ("5mm", "-5mm"), ("10mm", "-5mm"), "0.1mm");
    silk_line(&mut engine, ("5mm", "1mm"), ("10mm", "1mm"), "0.1mm");
    let run = drc(&mut engine);
    let found = of(&run, "silk_to_edge");
    let summary: Vec<Vec<Uuid>> = found.iter().map(objects).collect();
    assert_eq!(
        summary,
        [vec![near, outline], vec![outside, outline]],
        "{run:#}"
    );
    // The pen's round ends are polygons within 1 µm.
    assert!((measured(&found[0]) - 150_000).abs() <= 1_000, "{run:#}");
    assert_eq!(measured(&found[1]), 0);
    let v = &found[0];
    assert_eq!(v["severity"], "warning");
    assert_eq!(v["required"], json!({"length": 254_000}));
    assert_eq!(v["limit"], "min");
    assert_eq!(v["rule"], board_rule(&engine, "silk_to_edge"));
    assert_eq!(v["layers"].as_array().unwrap().len(), 1);
    assert_eq!(v["points"][1]["y"], 0);

    set_rule(&mut engine, "silk_to_edge", json!({"value": "0.1mm"}));
    let run = drc(&mut engine);
    assert_eq!(of(&run, "silk_to_edge").len(), 1, "{run:#}");
}

#[test]
fn silk_to_mask_measures_to_exposed_copper_or_to_the_mask_opening() {
    let dir = TempDir::new("drc-depth-silk-mask");
    let mut engine = engine(&dir);
    set_rule(&mut engine, "mask_expansion", json!({"top": "0.1mm"}));
    let pad = smd(&mut engine, "0mm", "0mm", None);
    // Its lower edge at y = 0.625 mm: 0.125 mm from the copper, 0.025 mm from the opening.
    let near = silk_line(
        &mut engine,
        ("-0.3mm", "0.7mm"),
        ("0.3mm", "0.7mm"),
        "0.15mm",
    );
    // Across the Pad.
    let across = silk_line(&mut engine, ("-1mm", "0mm"), ("1mm", "0mm"), "0.15mm");
    let run = drc(&mut engine);
    let found = of(&run, "silk_to_mask");
    assert_eq!(
        found.iter().map(objects).collect::<Vec<_>>(),
        [vec![across, pad]],
        "{run:#}"
    );
    assert_eq!(measured(&found[0]), 0);
    assert_eq!(found[0]["required"], json!({"length": 100_000}));
    assert_eq!(found[0]["rule"], board_rule(&engine, "silk_to_mask"));
    assert_eq!(found[0]["args"]["mode"], "exposed_copper");

    set_rule(&mut engine, "silk_to_mask", json!({"mode": "mask_opening"}));
    let run = drc(&mut engine);
    let found = of(&run, "silk_to_mask");
    let mut summary: Vec<(Vec<Uuid>, i64)> =
        found.iter().map(|v| (objects(v), measured(v))).collect();
    summary.sort();
    let mut expected = vec![(vec![near, pad], 25_000), (vec![across, pad], 0)];
    expected.sort();
    // The opening's round corners are polygons within 1 µm.
    assert_eq!(summary.len(), 2, "{run:#}");
    for ((got, at), (want, close)) in summary.iter().zip(&expected) {
        assert_eq!(got, want);
        assert!((at - close).abs() <= 1_000, "{at} vs {close}");
    }
    assert!(
        found.iter().all(|v| v["args"]["mode"] == "mask_opening"),
        "{run:#}"
    );
}

#[test]
fn a_via_inside_an_smd_pad_is_reported() {
    let dir = TempDir::new("drc-depth-via-smd");
    let mut engine = engine(&dir);
    let a = net(&mut engine, "A");
    let pad = smd(&mut engine, "10mm", "10mm", Some(a));
    let inside = uuid(
        &engine.run_ok(
            "pcb.via.add",
            json!({"at": {"x": "10.2mm", "y": "10mm"}, "net": a}),
        )["result"]["uuid"],
    );
    engine.run_ok(
        "pcb.via.add",
        json!({"at": {"x": "15mm", "y": "10mm"}, "net": a}),
    );
    let run = drc(&mut engine);
    let found = of(&run, "via_under_smd");
    assert_eq!(found.len(), 1, "{run:#}");
    assert_eq!(objects(&found[0]), [pad, inside]);
    assert_eq!(found[0]["severity"], "warning");
    assert_eq!(found[0]["at"], json!({"x": 10_200_000, "y": 10_000_000}));
    assert_eq!(found[0]["layers"].as_array().unwrap().len(), 1);

    engine.run_ok(
        "drc.set_severity",
        json!({"check": "via_under_smd", "severity": "off"}),
    );
    assert_eq!(of(&drc(&mut engine), "via_under_smd"), Vec::<Value>::new());
}

#[test]
fn a_track_end_touching_copper_off_its_centre_is_a_poor_connection() {
    let dir = TempDir::new("drc-depth-poor");
    let mut engine = engine(&dir);
    let a = net(&mut engine, "A");
    let pad = smd(&mut engine, "20mm", "0mm", Some(a));
    // Ends 0.05 mm short of the Pad's edge: its round end overlaps the Pad.
    let poor = route(&mut engine, Some(a), &[("17mm", "0mm"), ("19.45mm", "0mm")])[0];
    // Ends on the Pad's centre.
    let good_pad = smd(&mut engine, "20mm", "5mm", Some(a));
    route(&mut engine, Some(a), &[("17mm", "5mm"), ("20mm", "5mm")]);
    let run = drc(&mut engine);
    let found = of(&run, "poor_connection");
    assert_eq!(found.len(), 1, "{run:#}");
    assert_eq!(objects(&found[0]), [poor, pad]);
    assert_eq!(found[0]["at"], json!({"x": 19_450_000, "y": 0}));
    assert_eq!(found[0]["severity"], "warning");
    // The end touches copper, so it does not dangle.
    assert!(
        !of(&run, "dangling")
            .iter()
            .any(|v| v["args"]["x"] == 19_450_000),
        "{run:#}"
    );
    assert!(!objects(&found[0]).contains(&good_pad));
}

#[test]
fn courtyards_closer_than_courtyard_clearance_are_reported_with_the_fallback_box() {
    let dir = TempDir::new("drc-depth-courtyard");
    let mut engine = engine(&dir);
    let footprint = |engine: &mut TestEngine, x: &str| {
        let out = engine.run_ok(
            "test.footprint.add",
            json!({"at": {"x": x, "y": 0}, "pads": [
                {"at": {"x": 0, "y": 0}, "width": "1mm", "height": "1mm"}]}),
        );
        uuid(&out["result"]["footprint"])
    };
    // No courtyards: each uses the box around its Pad. Boxes 0.3 mm apart.
    let a = footprint(&mut engine, "0mm");
    let b = footprint(&mut engine, "1.3mm");
    // Exactly touching b's box.
    let c = footprint(&mut engine, "2.3mm");
    let run = drc(&mut engine);
    assert_eq!(
        of(&run, "courtyard_overlap"),
        Vec::<Value>::new(),
        "{run:#}"
    );

    set_rule(
        &mut engine,
        "courtyard_clearance",
        json!({"value": "0.5mm"}),
    );
    let run = drc(&mut engine);
    let found = of(&run, "courtyard_overlap");
    let summary: Vec<(Vec<Uuid>, i64)> = found.iter().map(|v| (objects(v), measured(v))).collect();
    assert_eq!(summary, [(vec![a, b], 300_000), (vec![b, c], 0)], "{run:#}");
    let v = &found[0];
    assert_eq!(v["required"], json!({"length": 500_000}));
    assert_eq!(v["limit"], "min");
    assert_eq!(v["rule"], board_rule(&engine, "courtyard_clearance"));
    assert_eq!(v["points"][0]["x"], 500_000);
    assert_eq!(v["points"][1]["x"], 800_000);
    assert_eq!(v["layers"].as_array().unwrap().len(), 1);
}

#[test]
fn stubs_up_to_the_dangling_tolerance_pass() {
    let dir = TempDir::new("drc-depth-dangling");
    let mut engine = engine(&dir);
    let a = net(&mut engine, "A");
    smd(&mut engine, "30mm", "0mm", Some(a));
    smd(&mut engine, "40mm", "0mm", Some(a));
    // A 0.8 mm stub in two Tracks, and a 1.6 mm one in two.
    let short = route(
        &mut engine,
        Some(a),
        &[("30mm", "0mm"), ("30mm", "0.4mm"), ("30mm", "0.8mm")],
    );
    let long = route(
        &mut engine,
        Some(a),
        &[("40mm", "0mm"), ("40mm", "0.8mm"), ("40mm", "1.6mm")],
    );
    let dangling =
        |run: &Value| -> Vec<Uuid> { of(run, "dangling").iter().flat_map(objects).collect() };
    let run = drc(&mut engine);
    assert_eq!(dangling(&run), [short[1], long[1]], "{run:#}");
    let v = &of(&run, "dangling")[0];
    assert_eq!(v["rule"], board_rule(&engine, "dangling"));
    assert_eq!(v["layers"].as_array().unwrap().len(), 1);

    set_rule(&mut engine, "dangling", json!({"tolerance": "1mm"}));
    let run = drc(&mut engine);
    assert_eq!(dangling(&run), [long[1]], "{run:#}");
}
