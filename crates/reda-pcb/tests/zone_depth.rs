//! Zone fill depth: the `zone_connection` rule (per through-hole Pad, SMD Pad and Via,
//! with spoke count and angle, resolved Pad > Net Class > board-wide), `pour_over`, the
//! independent island settings, the `dead_copper` and `empty_zone` checks, per-Zone input
//! hashes, and the fixed arc tolerance.

use reda_model::geom::{Millideg, Nm, Point};
use reda_model::{
    ConnectionStyle, ConnectionTarget, FillShape, PcbDocument, RuleKind, Uuid, ZoneConnection,
    ZoneFills,
};
use reda_pcb::Copper;
use reda_pcb::zone::FillState;
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

fn add_net(engine: &mut TestEngine, name: &str) -> Uuid {
    uuid(&engine.run_ok("test.net.add", json!({"name": name}))["result"]["uuid"])
}

fn mm(v: f64) -> i64 {
    (v * 1e6).round() as i64
}

fn at(x: f64, y: f64) -> Point {
    Point::new(Nm(mm(x)), Nm(mm(y)))
}

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Value {
    let p = |x: f64, y: f64| json!({"x": mm(x), "y": mm(y)});
    json!([p(x0, y0), p(x1, y0), p(x1, y1), p(x0, y1)])
}

/// Adds a Zone over the rectangle on `layer`, with extra settings.
fn zone_on(
    engine: &mut TestEngine,
    layer: &str,
    net: Option<Uuid>,
    area: Value,
    extra: Value,
) -> Uuid {
    let mut params = json!({"layer": layer, "vertices": area,
                            "clearance": "0.5mm", "min_width": "0.2mm"});
    if let Some(net) = net {
        params["net"] = json!(net.to_string());
    }
    for (k, v) in extra.as_object().unwrap() {
        params[k] = v.clone();
    }
    uuid(&engine.run_ok("pcb.zone.add", params)["result"]["uuid"])
}

fn zone(engine: &mut TestEngine, net: Option<Uuid>, area: Value, extra: Value) -> Uuid {
    zone_on(engine, "copper_top", net, area, extra)
}

/// A square SMD Pad of `size` mm at (x, y) on the top copper, with an optional own Zone
/// connection.
fn pad(engine: &mut TestEngine, x: f64, y: f64, size: f64, net: Uuid, own: Value) -> Uuid {
    let mut pad = json!({"at": {"x": 0, "y": 0}, "width": mm(size), "height": mm(size),
                         "net": net.to_string()});
    if !own.is_null() {
        pad["zone_connection"] = own;
    }
    let out = engine.run_ok(
        "test.footprint.add",
        json!({"at": {"x": mm(x), "y": mm(y)}, "pads": [pad]}),
    );
    uuid(&out["result"]["footprint"])
}

/// A square plated through-hole Free Pad of `size` mm with a 1 mm hole.
fn th_pad(engine: &mut TestEngine, x: f64, y: f64, size: f64, net: Uuid) {
    engine.run_ok(
        "pcb.free_pad.add",
        json!({"kind": "plated_through_hole", "shape": "rect",
               "at": {"x": mm(x), "y": mm(y)}, "width": mm(size),
               "hole": {"width": "1mm"}, "net": net.to_string()}),
    );
}

fn via(engine: &mut TestEngine, x: f64, y: f64, net: Uuid) -> Uuid {
    uuid(
        &engine.run_ok(
            "pcb.via.add",
            json!({"at": {"x": mm(x), "y": mm(y)}, "net": net.to_string(),
                   "diameter": "1mm", "hole": "0.4mm"}),
        )["result"]["uuid"],
    )
}

fn track(engine: &mut TestEngine, net: Uuid, from: (f64, f64), to: (f64, f64), width: f64) {
    engine.run_ok(
        "pcb.track.add",
        json!({"layer": "copper_top", "net": net.to_string(), "width": mm(width),
               "points": [{"x": mm(from.0), "y": mm(from.1)}, {"x": mm(to.0), "y": mm(to.1)}]}),
    );
}

/// Sets values of the `zone_connection` rule, board-wide or for a Net Class.
fn connection(engine: &mut TestEngine, values: Value) {
    engine.run_ok(
        "pcb.rules.set",
        json!({"kind": "zone_connection", "values": values}),
    );
}

fn fill(engine: &TestEngine) -> ZoneFills {
    reda_pcb::zone::fill(pcb(engine), &mut |_, _| true).expect("not cancelled")
}

fn shapes(fills: &ZoneFills, zone: Uuid) -> &[FillShape] {
    &fills.zones[&zone].shapes
}

fn ring_contains(ring: &[Point], p: Point) -> bool {
    let (px, py) = (p.x.0 as f64, p.y.0 as f64);
    let mut inside = false;
    let n = ring.len();
    for i in 0..n {
        let (a, b) = (ring[i], ring[(i + 1) % n]);
        let (ax, ay, bx, by) = (a.x.0 as f64, a.y.0 as f64, b.x.0 as f64, b.y.0 as f64);
        if (ay > py) != (by > py) && px < ax + (py - ay) / (by - ay) * (bx - ax) {
            inside = !inside;
        }
    }
    inside
}

fn filled(shapes: &[FillShape], p: Point) -> bool {
    shapes
        .iter()
        .any(|s| ring_contains(&s.outer, p) && !s.holes.iter().any(|h| ring_contains(h, p)))
}

fn violations(engine: &mut TestEngine, check: &str) -> Vec<Value> {
    let run = engine.run_ok("drc.run", json!({}));
    run["violations"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| v["check"] == check)
        .cloned()
        .collect()
}

#[test]
fn a_new_board_seeds_the_zone_connection_and_dead_copper_rules() {
    let dir = TempDir::new("depth-seeds");
    let engine = engine_with_pcb(&dir);
    let pcb = pcb(&engine);
    let values = pcb.board_rule(RuleKind::ZoneConnection).values;
    let relief = ZoneConnection {
        style: Some(ConnectionStyle::Relief),
        spokes: Some(4),
        spoke_width: Some(Nm(250_000)),
        gap: Some(Nm(250_000)),
        angle: Some(Millideg(0)),
    };
    assert_eq!(values.through_hole, Some(relief));
    assert_eq!(values.smd, Some(relief));
    assert_eq!(
        values.via,
        Some(ZoneConnection {
            style: Some(ConnectionStyle::Solid),
            ..relief
        })
    );
    // 100 sq mil.
    assert_eq!(
        pcb.board_rule(RuleKind::DeadCopper).values.min_area,
        Some(64_516_000_000)
    );
}

#[test]
fn zone_connection_values_resolve_from_the_pad_then_the_net_class_then_the_board() {
    let dir = TempDir::new("depth-resolve");
    let mut engine = engine_with_pcb(&dir);
    let gnd = add_net(&mut engine, "GND");
    let sig = add_net(&mut engine, "SIG");
    let class = engine.run_ok(
        "pcb.net_class.add",
        json!({"name": "Power", "rules": [{"kind": "zone_connection",
               "values": {"smd": {"style": "solid", "gap": "0.4mm"}}}]}),
    )["result"]["uuid"]
        .clone();
    engine.run_ok(
        "pcb.net.add_to_class",
        json!({"nets": [gnd.to_string()], "class": class}),
    );
    let pcb = pcb(&engine);
    let resolve = |net, own: Option<&ZoneConnection>| {
        reda_pcb::rules::zone_connection(pcb, Some(net), ConnectionTarget::Smd, own)
    };
    // SIG takes the board-wide record.
    let board = resolve(sig, None);
    assert_eq!(board.style, ConnectionStyle::Relief);
    assert_eq!((board.gap, board.spoke_width), (Nm(250_000), Nm(250_000)));
    // GND's class sets style and gap; the rest come from the board-wide record.
    let class = resolve(gnd, None);
    assert_eq!(class.style, ConnectionStyle::Solid);
    assert_eq!(class.gap, Nm(400_000));
    assert_eq!((class.spokes, class.spoke_width), (4, Nm(250_000)));
    // A Pad's own value wins, value by value.
    let own = ZoneConnection {
        gap: Some(Nm(600_000)),
        spokes: Some(2),
        ..ZoneConnection::default()
    };
    let pad = resolve(gnd, Some(&own));
    assert_eq!(pad.style, ConnectionStyle::Solid, "still the class's");
    assert_eq!((pad.gap, pad.spokes), (Nm(600_000), 2));
    // Through-hole Pads and Vias have values of their own.
    let th = reda_pcb::rules::zone_connection(pcb, Some(gnd), ConnectionTarget::ThroughHole, None);
    assert_eq!(th.style, ConnectionStyle::Relief);
    let via = reda_pcb::rules::zone_connection(pcb, Some(gnd), ConnectionTarget::Via, None);
    assert_eq!(via.style, ConnectionStyle::Solid);
}

#[test]
fn the_rule_refuses_a_spoke_count_other_than_2_or_4_and_odd_angles() {
    let dir = TempDir::new("depth-refuse");
    let mut engine = engine_with_pcb(&dir);
    for values in [
        json!({"smd": {"spokes": 3}}),
        json!({"via": {"angle": "30deg"}}),
        json!({"through_hole": {"spoke_width": 0}}),
    ] {
        engine.run_err(
            "pcb.rules.set",
            json!({"kind": "zone_connection", "values": values}),
        );
    }
    engine.run_err(
        "pcb.rules.set",
        json!({"kind": "dead_copper", "values": {"min_area": null}}),
    );
    engine.run_ok(
        "pcb.rules.set",
        json!({"kind": "dead_copper", "values": {"min_area": "0.5mm2"}}),
    );
    assert_eq!(
        pcb(&engine)
            .board_rule(RuleKind::DeadCopper)
            .values
            .min_area,
        Some(500_000_000_000)
    );
}

#[test]
fn through_hole_pads_smd_pads_and_vias_each_take_their_own_connection() {
    let dir = TempDir::new("depth-targets");
    let mut engine = engine_with_pcb(&dir);
    let gnd = add_net(&mut engine, "GND");
    connection(
        &mut engine,
        json!({"through_hole": {"style": "solid"},
               "smd": {"gap": "0.5mm", "spoke_width": "0.3mm"}}),
    );
    let z = zone(
        &mut engine,
        Some(gnd),
        rect(0.0, 0.0, 30.0, 10.0),
        json!({"remove_unconnected": false}),
    );
    pad(&mut engine, 5.0, 5.0, 2.0, gnd, Value::Null);
    th_pad(&mut engine, 15.0, 5.0, 2.0, gnd);
    via(&mut engine, 25.0, 5.0, gnd);
    let fills = fill(&engine);
    let s = shapes(&fills, z);
    // The SMD Pad is relieved: a gap, crossed by a spoke.
    assert!(!filled(s, at(6.25, 5.5)), "SMD gap");
    assert!(filled(s, at(6.25, 5.0)), "SMD spoke");
    // The through-hole Pad is solid: the fill runs right up to it.
    assert!(filled(s, at(16.1, 5.5)), "through-hole solid");
    // The Via is solid by the seeds.
    assert!(filled(s, at(25.6, 5.0)), "Via solid");

    // Relieve Vias: a 0.25 mm gap opens around the Via, with spokes along its axes.
    connection(&mut engine, json!({"via": {"style": "relief"}}));
    let fills = fill(&engine);
    let s = shapes(&fills, z);
    assert!(!filled(s, at(25.6, 5.4)), "Via gap");
    assert!(filled(s, at(25.6, 5.0)), "Via spoke");
    assert!(filled(s, at(26.0, 5.0)), "past the gap");
}

#[test]
fn spokes_come_in_twos_or_fours_turned_by_the_angle() {
    let dir = TempDir::new("depth-spokes");
    let mut engine = engine_with_pcb(&dir);
    let gnd = add_net(&mut engine, "GND");
    connection(
        &mut engine,
        json!({"smd": {"gap": "0.5mm", "spoke_width": "0.3mm"}}),
    );
    let z = zone(
        &mut engine,
        Some(gnd),
        rect(0.0, 0.0, 40.0, 10.0),
        json!({}),
    );
    // Four 2 mm Pads: edges 1 mm from each centre, gap out to 1.5 mm.
    pad(&mut engine, 5.0, 5.0, 2.0, gnd, json!({"spokes": 2}));
    pad(
        &mut engine,
        15.0,
        5.0,
        2.0,
        gnd,
        json!({"spokes": 2, "angle": 90_000}),
    );
    pad(&mut engine, 25.0, 5.0, 2.0, gnd, json!({"angle": 45_000}));
    pad(&mut engine, 35.0, 5.0, 2.0, gnd, Value::Null);
    let fills = fill(&engine);
    let s = shapes(&fills, z);
    let spoke = |cx: f64, dx: f64, dy: f64| filled(s, at(cx + dx, 5.0 + dy));
    // Two spokes at 0: along x only.
    assert!(spoke(5.0, 1.25, 0.0) && spoke(5.0, -1.25, 0.0));
    assert!(!spoke(5.0, 0.0, 1.25) && !spoke(5.0, 0.0, -1.25));
    // Two spokes at 90: along y only.
    assert!(spoke(15.0, 0.0, 1.25) && spoke(15.0, 0.0, -1.25));
    assert!(!spoke(15.0, 1.25, 0.0) && !spoke(15.0, -1.25, 0.0));
    // Four spokes at 45: through the corners, none along the axes.
    for (dx, dy) in [(1.25, 1.25), (-1.25, 1.25), (1.25, -1.25), (-1.25, -1.25)] {
        assert!(spoke(25.0, dx, dy), "diagonal ({dx}, {dy})");
    }
    assert!(!spoke(25.0, 1.25, 0.0) && !spoke(25.0, 0.0, 1.25));
    // The seeds: four spokes along the axes.
    for (dx, dy) in [(1.25, 0.0), (-1.25, 0.0), (0.0, 1.25), (0.0, -1.25)] {
        assert!(spoke(35.0, dx, dy), "axis ({dx}, {dy})");
    }
    assert!(!spoke(35.0, 1.25, 1.25));
}

/// The Ratsnest of a GND Zone holding a solid GND Pad and, at (5, 5), a GND Pad with
/// the Zone connection `own`, two spokes along x by the rule. Keepouts just past its gap
/// on either side leave spokes along x nothing to reach.
fn blocked_spokes(own: Value) -> Vec<reda_pcb::RatsnestLine> {
    let dir = TempDir::new("depth-no-spoke");
    let mut engine = engine_with_pcb(&dir);
    let gnd = add_net(&mut engine, "GND");
    connection(
        &mut engine,
        json!({"smd": {"gap": "0.5mm", "spoke_width": "0.3mm", "spokes": 2}}),
    );
    zone(
        &mut engine,
        Some(gnd),
        rect(0.0, 0.0, 20.0, 10.0),
        json!({}),
    );
    pad(&mut engine, 15.0, 5.0, 1.0, gnd, json!({"style": "solid"}));
    pad(&mut engine, 5.0, 5.0, 2.0, gnd, own);
    for x in [6.4, 2.4] {
        engine.run_ok(
            "pcb.keepout.add",
            json!({"vertices": rect(x, 4.0, x + 1.2, 6.0), "layers": ["copper_top"]}),
        );
    }
    let fills = fill(&engine);
    let mut copper = Copper::build(pcb(&engine));
    copper.set_zone_fills(pcb(&engine), Some(&fills));
    copper.ratsnest()
}

#[test]
fn a_relieved_pad_with_no_spoke_left_is_not_connected() {
    let lines = blocked_spokes(Value::Null);
    assert_eq!(lines.len(), 1, "the Pad stays on the Ratsnest: {lines:?}");
    // With 4 spokes, those along y reach the fill above and below.
    assert!(blocked_spokes(json!({"spokes": 4})).is_empty());
}

#[test]
fn pour_over_decides_whether_same_net_tracks_and_zones_merge() {
    let dir = TempDir::new("depth-pour-over");
    let mut engine = engine_with_pcb(&dir);
    let gnd = add_net(&mut engine, "GND");
    let z = zone(
        &mut engine,
        Some(gnd),
        rect(0.0, 0.0, 10.0, 10.0),
        json!({"remove_unconnected": false}),
    );
    // A GND Track from a Pad outside the Zone, ending inside it.
    pad(&mut engine, 15.0, 5.0, 1.0, gnd, Value::Null);
    pad(&mut engine, 2.0, 2.0, 1.0, gnd, Value::Null);
    track(&mut engine, gnd, (15.0, 5.0), (5.0, 5.0), 0.25);

    // zones_only (the default): the Track keeps the clearance, and isn't joined.
    let fills = fill(&engine);
    let s = shapes(&fills, z);
    assert!(!filled(s, at(7.0, 5.0)), "under the Track");
    assert!(!filled(s, at(7.0, 5.5)), "within the clearance");
    assert!(filled(s, at(7.0, 5.7)));
    let mut copper = Copper::build(pcb(&engine));
    copper.set_zone_fills(pcb(&engine), Some(&fills));
    assert_eq!(
        copper.ratsnest().len(),
        1,
        "the Track's end isn't connected"
    );

    // all: the Track merges with the fill and joins it.
    engine.run_ok("pcb.zone.set", json!({"zone": z, "pour_over": "all"}));
    let fills = fill(&engine);
    let s = shapes(&fills, z);
    assert!(filled(s, at(7.0, 5.0)) && filled(s, at(7.0, 5.5)));
    let mut copper = Copper::build(pcb(&engine));
    copper.set_zone_fills(pcb(&engine), Some(&fills));
    assert!(copper.ratsnest().is_empty());
}

#[test]
fn with_pour_over_none_same_net_zones_keep_clear() {
    let dir = TempDir::new("depth-pour-none");
    let mut engine = engine_with_pcb(&dir);
    let gnd = add_net(&mut engine, "GND");
    let low = zone(
        &mut engine,
        Some(gnd),
        rect(0.0, 0.0, 10.0, 10.0),
        json!({"remove_unconnected": false}),
    );
    zone(
        &mut engine,
        Some(gnd),
        rect(5.0, 0.0, 15.0, 10.0),
        json!({"remove_unconnected": false, "priority": 1}),
    );
    // zones_only: the two GND fills merge.
    assert!(filled(shapes(&fill(&engine), low), at(7.0, 5.0)));

    engine.run_ok("pcb.zone.set", json!({"zone": low, "pour_over": "none"}));
    let fills = fill(&engine);
    let s = shapes(&fills, low);
    assert!(filled(s, at(4.4, 5.0)));
    assert!(
        !filled(s, at(4.6, 5.0)),
        "0.5 mm clear of the other GND fill"
    );
    assert!(!filled(s, at(7.0, 5.0)));
}

/// A GND Zone split by a SIG Track at y = 7: the lower piece (about 64 mm²) holds a GND
/// Pad, the upper one (about 24 mm²) touches nothing.
fn split_zone(engine: &mut TestEngine, gnd: Uuid, extra: Value) -> Uuid {
    let sig = add_net(engine, "SIG");
    let z = zone(engine, Some(gnd), rect(0.0, 0.0, 10.0, 10.0), extra);
    track(engine, sig, (-1.0, 7.0), (11.0, 7.0), 0.2);
    pad(engine, 5.0, 2.0, 1.0, gnd, json!({"style": "solid"}));
    z
}

#[test]
fn remove_unconnected_and_min_island_area_are_independent() {
    let dir = TempDir::new("depth-islands");
    let mut engine = engine_with_pcb(&dir);
    let gnd = add_net(&mut engine, "GND");
    let z = split_zone(&mut engine, gnd, json!({"remove_unconnected": false}));
    assert_eq!(shapes(&fill(&engine), z).len(), 2, "both pieces stay");

    engine.run_ok(
        "pcb.zone.set",
        json!({"zone": z, "remove_unconnected": true}),
    );
    let fills = fill(&engine);
    assert_eq!(shapes(&fills, z).len(), 1);
    assert!(filled(shapes(&fills, z), at(5.0, 4.0)));

    // An area alone drops the small piece, though unconnected pieces are kept...
    engine.run_ok(
        "pcb.zone.set",
        json!({"zone": z, "remove_unconnected": false, "min_island_area": "30mm2"}),
    );
    let fills = fill(&engine);
    assert_eq!(shapes(&fills, z).len(), 1);
    assert!(filled(shapes(&fills, z), at(5.0, 4.0)));
    // ...and drops a connected piece smaller than it too.
    engine.run_ok(
        "pcb.zone.set",
        json!({"zone": z, "min_island_area": "70mm2"}),
    );
    assert!(shapes(&fill(&engine), z).is_empty());
}

#[test]
fn drc_warns_about_dead_copper_above_the_rule_area() {
    let dir = TempDir::new("depth-dead");
    let mut engine = engine_with_pcb(&dir);
    let gnd = add_net(&mut engine, "GND");
    let z = split_zone(&mut engine, gnd, json!({"remove_unconnected": false}));
    let dead = violations(&mut engine, "dead_copper");
    assert_eq!(dead.len(), 1, "only the upper piece: {dead:#?}");
    let v = &dead[0];
    assert_eq!(v["severity"], "warning");
    assert_eq!(v["objects"], json!([z.to_string()]));
    assert_eq!(v["required"], json!({"area": 64_516_000_000_i64}));
    assert_eq!(v["limit"], "max");
    let measured = v["measured"]["area"].as_i64().unwrap() as f64 / 1e12;
    // 10 mm × (10 - 7 - 0.1 - 0.5) mm.
    assert!((measured - 24.0).abs() < 0.1, "{measured} mm²");
    let rule = pcb(&engine).board_rule(RuleKind::DeadCopper).uuid;
    assert_eq!(v["rule"], rule.to_string());

    // Above the area it passes.
    engine.run_ok(
        "pcb.rules.set",
        json!({"kind": "dead_copper", "values": {"min_area": "30mm2"}}),
    );
    assert!(violations(&mut engine, "dead_copper").is_empty());
}

#[test]
fn zones_with_no_net_are_exempt_from_dead_copper_and_empty_zone() {
    let dir = TempDir::new("depth-no-net");
    let mut engine = engine_with_pcb(&dir);
    zone(
        &mut engine,
        None,
        rect(0.0, 0.0, 10.0, 10.0),
        json!({"remove_unconnected": false}),
    );
    engine.run_ok(
        "pcb.keepout.add",
        json!({"vertices": rect(20.0, 0.0, 30.0, 10.0)}),
    );
    zone(&mut engine, None, rect(21.0, 1.0, 29.0, 9.0), json!({}));
    assert!(violations(&mut engine, "dead_copper").is_empty());
    assert!(violations(&mut engine, "empty_zone").is_empty());
}

#[test]
fn drc_warns_about_a_zone_with_nothing_left_to_fill() {
    let dir = TempDir::new("depth-empty");
    let mut engine = engine_with_pcb(&dir);
    let gnd = add_net(&mut engine, "GND");
    // Nothing of the Net to touch, so the whole fill is removed.
    let lonely = zone(
        &mut engine,
        Some(gnd),
        rect(0.0, 0.0, 10.0, 10.0),
        json!({}),
    );
    let kept = zone(
        &mut engine,
        Some(gnd),
        rect(20.0, 0.0, 30.0, 10.0),
        json!({}),
    );
    pad(&mut engine, 25.0, 5.0, 1.0, gnd, Value::Null);
    let empty = violations(&mut engine, "empty_zone");
    assert_eq!(empty.len(), 1, "{empty:#?}");
    assert_eq!(empty[0]["objects"], json!([lonely.to_string()]));
    assert_eq!(empty[0]["severity"], "warning");
    assert_ne!(empty[0]["objects"], json!([kept.to_string()]));
}

#[test]
fn each_zone_goes_stale_only_when_its_own_inputs_change() {
    let dir = TempDir::new("depth-stale");
    let mut engine = engine_with_pcb(&dir);
    let gnd = add_net(&mut engine, "GND");
    let sig = add_net(&mut engine, "SIG");
    let opts = json!({"remove_unconnected": false});
    let top_high = zone(
        &mut engine,
        Some(gnd),
        rect(0.0, 0.0, 10.0, 10.0),
        json!({"remove_unconnected": false, "priority": 2}),
    );
    let top_low = zone(
        &mut engine,
        Some(sig),
        rect(5.0, 0.0, 15.0, 10.0),
        opts.clone(),
    );
    let top_apart = zone(
        &mut engine,
        Some(sig),
        rect(20.0, 0.0, 30.0, 10.0),
        opts.clone(),
    );
    let bottom = zone_on(
        &mut engine,
        "copper_bottom",
        Some(gnd),
        rect(0.0, 0.0, 10.0, 10.0),
        opts,
    );
    let stored = fill(&engine);
    let state = |engine: &TestEngine| {
        let status = reda_pcb::zone::status(pcb(engine), Some(&stored));
        let of = |z: Uuid| status.zones.iter().find(|s| s.zone == z).unwrap().state;
        [of(top_high), of(top_low), of(top_apart), of(bottom)]
    };
    use FillState::{Filled, Stale};
    assert_eq!(state(&engine), [Filled; 4]);

    // A Track on the bottom stales only the bottom Zone.
    engine.run_ok(
        "pcb.track.add",
        json!({"layer": "copper_bottom", "net": sig.to_string(), "width": "0.2mm",
               "points": [{"x": "2mm", "y": "2mm"}, {"x": "4mm", "y": "2mm"}]}),
    );
    assert_eq!(state(&engine), [Filled, Filled, Filled, Stale]);
    let refilled = reda_pcb::zone::current(pcb(&engine), Some(&stored));
    assert_eq!(
        refilled,
        fill(&engine),
        "a partial refill matches a full one"
    );

    // Changing a higher-priority Zone stales the lower-priority Zones on its Layer.
    engine.run_ok("pcb.zone.set", json!({"zone": top_high, "priority": 3}));
    assert_eq!(state(&engine), [Stale, Stale, Stale, Stale]);
    let refilled = reda_pcb::zone::current(pcb(&engine), Some(&stored));
    assert_eq!(refilled, fill(&engine));
}

#[test]
fn arcs_are_flattened_to_chords_within_a_micrometre() {
    let dir = TempDir::new("depth-arcs");
    let mut engine = engine_with_pcb(&dir);
    // A 10 mm radius circle about the origin, as two half arcs.
    let r = mm(10.0) as f64;
    let circle = json!([
        {"x": "10mm", "y": 0, "sweep": "180deg"},
        {"x": "-10mm", "y": 0, "sweep": "180deg"}
    ]);
    let z = zone(&mut engine, None, circle, json!({"min_width": "0.01mm"}));
    let fills = fill(&engine);
    let s = shapes(&fills, z);
    assert_eq!(s.len(), 1);
    let ring = &s[0].outer;
    let dist = |x: f64, y: f64| x.hypot(y);
    let mut worst: f64 = 0.0;
    for i in 0..ring.len() {
        let (a, b) = (ring[i], ring[(i + 1) % ring.len()]);
        let (ax, ay, bx, by) = (a.x.0 as f64, a.y.0 as f64, b.x.0 as f64, b.y.0 as f64);
        assert!(dist(ax, ay) <= r + 2.0, "outside the arc at {a:?}");
        worst = worst.max(r - dist((ax + bx) / 2.0, (ay + by) / 2.0));
    }
    assert_eq!(reda_pcb::zone::ARC_TOLERANCE, Nm(1_000));
    assert!(worst <= 1_000.0 + 2.0, "a chord {worst} nm inside the arc");
    // Not needlessly finer either: the chords use the tolerance.
    assert!(worst >= 500.0, "chords {worst} nm from the arc");
}
