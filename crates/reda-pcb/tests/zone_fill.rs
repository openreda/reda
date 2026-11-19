//! Filling Zones: clearance, Zone connections, islands, priority, Keepouts, the board edge
//! and the minimum width.

use reda_model::geom::{Nm, Point};
use reda_model::{FillShape, PcbDocument, Uuid, ZoneFills};
use reda_pcb::{Copper, Item};
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

/// Adds a Zone on the top copper over the rectangle, with extra settings.
fn zone(engine: &mut TestEngine, net: Option<Uuid>, area: Value, extra: Value) -> Uuid {
    let mut params = json!({"layer": "copper_top", "vertices": area,
                            "clearance": "0.5mm", "min_width": "0.2mm"});
    if let Some(net) = net {
        params["net"] = json!(net.to_string());
    }
    for (k, v) in extra.as_object().unwrap() {
        params[k] = v.clone();
    }
    uuid(&engine.run_ok("pcb.zone.add", params)["result"]["uuid"])
}

/// A square SMD Pad of `size` mm at (x, y) on the top copper.
fn pad(engine: &mut TestEngine, x: f64, y: f64, size: f64, net: Option<Uuid>) -> Uuid {
    let out = engine.run_ok(
        "test.footprint.add",
        json!({"at": {"x": mm(x), "y": mm(y)},
               "pads": [{"at": {"x": 0, "y": 0}, "width": mm(size), "height": mm(size),
                         "net": net.map(|n| n.to_string())}]}),
    );
    uuid(&out["result"]["footprint"])
}

fn track(engine: &mut TestEngine, net: Uuid, from: (f64, f64), to: (f64, f64), width: f64) {
    engine.run_ok(
        "pcb.track.add",
        json!({"layer": "copper_top", "net": net.to_string(), "width": mm(width),
               "points": [{"x": mm(from.0), "y": mm(from.1)}, {"x": mm(to.0), "y": mm(to.1)}]}),
    );
}

/// Sets values of the board-wide `zone_connection` rule.
fn connection(engine: &mut TestEngine, values: Value) {
    engine.run_ok(
        "pcb.rules.set",
        json!({"kind": "zone_connection", "values": values}),
    );
}

fn fill(engine: &TestEngine) -> ZoneFills {
    reda_pcb::zone::fill(pcb(engine), &mut |_, _| true).expect("not cancelled")
}

fn ring_area(ring: &[Point]) -> f64 {
    let n = ring.len();
    (0..n)
        .map(|i| {
            let (a, b) = (ring[i], ring[(i + 1) % n]);
            a.x.0 as f64 * b.y.0 as f64 - b.x.0 as f64 * a.y.0 as f64
        })
        .sum::<f64>()
        / 2.0
}

/// The filled area in mm².
fn area(shapes: &[FillShape]) -> f64 {
    shapes
        .iter()
        .map(|s| {
            ring_area(&s.outer).abs() - s.holes.iter().map(|h| ring_area(h).abs()).sum::<f64>()
        })
        .sum::<f64>()
        / 1e12
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

/// Whether the fill covers `p`.
fn filled(shapes: &[FillShape], p: Point) -> bool {
    shapes
        .iter()
        .any(|s| ring_contains(&s.outer, p) && !s.holes.iter().any(|h| ring_contains(h, p)))
}

fn shapes(fills: &ZoneFills, zone: Uuid) -> &[FillShape] {
    &fills.zones[&zone].shapes
}

#[test]
fn an_empty_zone_fills_its_whole_outline() {
    let dir = TempDir::new("fill-empty");
    let mut engine = engine_with_pcb(&dir);
    let gnd = add_net(&mut engine, "GND");
    let z = zone(
        &mut engine,
        Some(gnd),
        rect(0.0, 0.0, 10.0, 10.0),
        json!({"remove_unconnected": false}),
    );
    let fills = fill(&engine);
    let s = shapes(&fills, z);
    assert_eq!(s.len(), 1);
    assert!((area(s) - 100.0).abs() < 0.01, "{}", area(s));
    assert_eq!(
        fills.zones[&z].inputs,
        reda_pcb::zone::input_hashes(pcb(&engine))[&z]
    );
}

#[test]
fn a_fill_keeps_its_clearance_from_other_nets_copper() {
    let dir = TempDir::new("fill-clearance");
    let mut engine = engine_with_pcb(&dir);
    let gnd = add_net(&mut engine, "GND");
    let sig = add_net(&mut engine, "SIG");
    let z = zone(
        &mut engine,
        Some(gnd),
        rect(0.0, 0.0, 10.0, 10.0),
        json!({"remove_unconnected": false}),
    );
    // A 0.2 mm SIG Track across the middle at y = 5: the fill must stay 0.5 mm clear of
    // its edge, so 0.6 mm from its centreline.
    track(&mut engine, sig, (-1.0, 5.0), (11.0, 5.0), 0.2);
    let fills = fill(&engine);
    let s = shapes(&fills, z);
    assert_eq!(s.len(), 2, "the Track splits the fill");
    assert!(!filled(s, at(5.0, 5.55)));
    assert!(!filled(s, at(5.0, 4.45)));
    assert!(filled(s, at(5.0, 5.65)));
    assert!(filled(s, at(5.0, 4.35)));
    for p in s.iter().flat_map(|s| &s.outer) {
        assert!((p.y.0 - mm(5.0)).abs() >= mm(0.6) - 1, "{p:?}");
    }
}

#[test]
fn a_larger_net_class_clearance_wins_over_the_zone_clearance() {
    let dir = TempDir::new("fill-class");
    let mut engine = engine_with_pcb(&dir);
    let gnd = add_net(&mut engine, "GND");
    let hv = add_net(&mut engine, "HV");
    let class = engine.run_ok(
        "pcb.net_class.add",
        json!({"name": "HV", "rules": [{"kind": "clearance", "values": {"value": "1mm"}}]}),
    )["result"]["uuid"]
        .clone();
    engine.run_ok(
        "pcb.net.add_to_class",
        json!({"nets": [hv.to_string()], "class": class}),
    );
    let z = zone(
        &mut engine,
        Some(gnd),
        rect(0.0, 0.0, 10.0, 10.0),
        json!({"remove_unconnected": false}),
    );
    track(&mut engine, hv, (-1.0, 5.0), (11.0, 5.0), 0.2);
    let fills = fill(&engine);
    let s = shapes(&fills, z);
    assert!(!filled(s, at(5.0, 6.05)));
    assert!(filled(s, at(5.0, 6.15)));
}

#[test]
fn islands_touching_no_copper_of_the_net_are_removed() {
    let dir = TempDir::new("fill-islands");
    let mut engine = engine_with_pcb(&dir);
    let gnd = add_net(&mut engine, "GND");
    let sig = add_net(&mut engine, "SIG");
    connection(&mut engine, json!({"smd": {"style": "solid"}}));
    let z = zone(
        &mut engine,
        Some(gnd),
        rect(0.0, 0.0, 10.0, 10.0),
        json!({}),
    );
    track(&mut engine, sig, (-1.0, 5.0), (11.0, 5.0), 0.2);
    pad(&mut engine, 5.0, 2.0, 1.0, Some(gnd));
    let fills = fill(&engine);
    let s = shapes(&fills, z);
    assert_eq!(s.len(), 1, "only the lower half holds a GND Pad");
    assert!(filled(s, at(5.0, 3.5)));
    assert!(!filled(s, at(5.0, 8.0)));

    engine.run_ok(
        "pcb.zone.set",
        json!({"zone": z.to_string(), "remove_unconnected": false}),
    );
    assert_eq!(shapes(&fill(&engine), z).len(), 2);
}

#[test]
fn relief_leaves_a_gap_crossed_by_four_spokes() {
    let dir = TempDir::new("fill-thermal");
    let mut engine = engine_with_pcb(&dir);
    let gnd = add_net(&mut engine, "GND");
    connection(
        &mut engine,
        json!({"smd": {"gap": "0.5mm", "spoke_width": "0.3mm"}}),
    );
    let z = zone(
        &mut engine,
        Some(gnd),
        rect(0.0, 0.0, 10.0, 10.0),
        json!({}),
    );
    // A 2 mm Pad at the centre: its edges at 4 and 6 mm, the gap out to 3.5 and 6.5 mm.
    pad(&mut engine, 5.0, 5.0, 2.0, Some(gnd));
    let fills = fill(&engine);
    let s = shapes(&fills, z);
    assert_eq!(s.len(), 1);
    // Spokes cross the gap along the Pad's axes...
    for p in [at(6.25, 5.0), at(3.75, 5.0), at(5.0, 6.25), at(5.0, 3.75)] {
        assert!(filled(s, p), "spoke at {p:?}");
    }
    // ...and the rest of the gap is empty.
    for p in [at(6.25, 5.5), at(6.25, 4.5), at(4.5, 6.25), at(6.3, 6.3)] {
        assert!(!filled(s, p), "gap at {p:?}");
    }
    assert!(filled(s, at(7.0, 7.0)));
}

#[test]
fn a_solid_connection_fills_right_up_to_the_pad() {
    let dir = TempDir::new("fill-solid");
    let mut engine = engine_with_pcb(&dir);
    let gnd = add_net(&mut engine, "GND");
    connection(&mut engine, json!({"smd": {"style": "solid"}}));
    let z = zone(
        &mut engine,
        Some(gnd),
        rect(0.0, 0.0, 10.0, 10.0),
        json!({}),
    );
    pad(&mut engine, 5.0, 5.0, 2.0, Some(gnd));
    let fills = fill(&engine);
    let s = shapes(&fills, z);
    assert!(filled(s, at(6.25, 5.5)));
    assert!(filled(s, at(6.3, 6.3)));
    assert!((area(s) - 100.0).abs() < 0.01);
}

#[test]
fn a_pads_own_zone_connection_wins_over_the_rule() {
    let dir = TempDir::new("fill-pad-connection");
    let mut engine = engine_with_pcb(&dir);
    let gnd = add_net(&mut engine, "GND");
    connection(
        &mut engine,
        json!({"smd": {"gap": "0.5mm", "spoke_width": "0.3mm"}}),
    );
    let z = zone(
        &mut engine,
        Some(gnd),
        rect(0.0, 0.0, 20.0, 10.0),
        json!({}),
    );
    let pad_with = |engine: &mut TestEngine, x: f64, connection: Value| {
        engine.run_ok(
            "test.footprint.add",
            json!({"at": {"x": mm(x), "y": mm(5.0)},
                   "pads": [{"at": {"x": 0, "y": 0}, "width": mm(2.0), "height": mm(2.0),
                             "net": gnd.to_string(), "zone_connection": connection}]}),
        );
    };
    // Solid where the rule relieves: the fill runs up to the Pad.
    pad_with(&mut engine, 5.0, json!({"style": "solid"}));
    // Relieved with its own 1 mm gap: the gap reaches 1 mm past the Pad's edge.
    pad_with(&mut engine, 15.0, json!({"gap": 1_000_000}));
    let fills = fill(&engine);
    let s = shapes(&fills, z);
    assert!(filled(s, at(6.25, 5.5)), "solid");
    assert!(filled(s, at(6.3, 6.3)), "solid");
    assert!(!filled(s, at(16.75, 5.5)), "inside the Pad's own gap");
    assert!(filled(s, at(16.75, 5.0)), "a spoke");
    assert!(filled(s, at(17.2, 7.2)), "past the gap");
}

#[test]
fn a_pad_with_no_zone_connection_keeps_its_gap_and_is_not_joined() {
    let dir = TempDir::new("fill-pad-none");
    let mut engine = engine_with_pcb(&dir);
    let gnd = add_net(&mut engine, "GND");
    connection(
        &mut engine,
        json!({"smd": {"style": "solid", "gap": "0.5mm"}}),
    );
    let z = zone(
        &mut engine,
        Some(gnd),
        rect(0.0, 0.0, 10.0, 10.0),
        json!({"remove_unconnected": false}),
    );
    engine.run_ok(
        "test.footprint.add",
        json!({"at": {"x": mm(5.0), "y": mm(5.0)},
               "pads": [{"at": {"x": 0, "y": 0}, "width": mm(2.0), "height": mm(2.0),
                         "net": gnd.to_string(), "zone_connection": {"style": "none"}}]}),
    );
    let fills = fill(&engine);
    let s = shapes(&fills, z);
    for p in [at(6.25, 5.0), at(6.25, 5.5), at(5.0, 3.75)] {
        assert!(!filled(s, p), "gap at {p:?}");
    }
    assert!(filled(s, at(7.0, 7.0)));
}

#[test]
fn a_higher_priority_zone_fills_first_and_the_other_keeps_clear_of_it() {
    let dir = TempDir::new("fill-priority");
    let mut engine = engine_with_pcb(&dir);
    let gnd = add_net(&mut engine, "GND");
    let vcc = add_net(&mut engine, "VCC");
    let low = zone(
        &mut engine,
        Some(gnd),
        rect(0.0, 0.0, 10.0, 10.0),
        json!({"remove_unconnected": false}),
    );
    let high = zone(
        &mut engine,
        Some(vcc),
        rect(5.0, 0.0, 15.0, 10.0),
        json!({"remove_unconnected": false, "priority": 1}),
    );
    let fills = fill(&engine);
    assert!((area(shapes(&fills, high)) - 100.0).abs() < 0.01);
    let s = shapes(&fills, low);
    assert!(filled(s, at(4.4, 5.0)));
    assert!(
        !filled(s, at(4.6, 5.0)),
        "0.5 mm clear of the VCC fill at x = 5"
    );

    // Swapping the priorities swaps who gives way.
    engine.run_ok(
        "pcb.zone.set",
        json!({"zone": low.to_string(), "priority": 2}),
    );
    let fills = fill(&engine);
    assert!((area(shapes(&fills, low)) - 100.0).abs() < 0.01);
    assert!(!filled(shapes(&fills, high), at(10.4, 5.0)));
}

#[test]
fn zones_of_one_net_may_overlap() {
    let dir = TempDir::new("fill-same-net");
    let mut engine = engine_with_pcb(&dir);
    let gnd = add_net(&mut engine, "GND");
    let a = zone(
        &mut engine,
        Some(gnd),
        rect(0.0, 0.0, 10.0, 10.0),
        json!({"remove_unconnected": false}),
    );
    let b = zone(
        &mut engine,
        Some(gnd),
        rect(5.0, 0.0, 15.0, 10.0),
        json!({"remove_unconnected": false, "priority": 1}),
    );
    let fills = fill(&engine);
    assert!((area(shapes(&fills, a)) - 100.0).abs() < 0.01);
    assert!((area(shapes(&fills, b)) - 100.0).abs() < 0.01);
}

#[test]
fn a_fill_keeps_the_clearance_from_keepouts_restricting_zones() {
    let dir = TempDir::new("fill-keepout");
    let mut engine = engine_with_pcb(&dir);
    engine.run_ok(
        "pcb.rules.set",
        json!({"kind": "clearance", "values": {"value": "0.5mm"}}),
    );
    let gnd = add_net(&mut engine, "GND");
    let z = zone(
        &mut engine,
        Some(gnd),
        rect(0.0, 0.0, 10.0, 10.0),
        json!({"remove_unconnected": false}),
    );
    engine.run_ok(
        "pcb.keepout.add",
        json!({"vertices": rect(2.0, 2.0, 4.0, 4.0)}),
    );
    // A Keepout not restricting Zones, and one on the other Layer, leave the fill alone.
    engine.run_ok(
        "pcb.keepout.add",
        json!({"vertices": rect(6.0, 6.0, 8.0, 8.0), "restrict": ["tracks", "vias"]}),
    );
    engine.run_ok(
        "pcb.keepout.add",
        json!({"vertices": rect(6.0, 2.0, 8.0, 4.0), "layer": "copper_bottom"}),
    );
    let fills = fill(&engine);
    let s = shapes(&fills, z);
    assert!(!filled(s, at(3.0, 3.0)));
    assert!(!filled(s, at(1.6, 3.0)), "within the clearance");
    assert!(filled(s, at(1.4, 3.0)));
    assert!(filled(s, at(7.0, 7.0)));
    assert!(filled(s, at(7.0, 3.0)));
    // The 2 mm square grown by 0.5 mm with round corners: 4 + 4 × 1 + π × 0.25 mm².
    // Round edges are polygons containing the arcs (within 1 µm), so a little more goes.
    let cut = 8.0 + std::f64::consts::PI * 0.25;
    assert!((area(s) - (100.0 - cut)).abs() < 0.03, "{}", area(s));

    // Edge-exact under zero_keepout_clearance.
    engine.run_ok(
        "pcb.rules.set",
        json!({"kind": "clearance", "values": {"zero_keepout_clearance": true}}),
    );
    let fills = fill(&engine);
    let s = shapes(&fills, z);
    assert!(filled(s, at(1.9, 3.0)));
    assert!((area(s) - 96.0).abs() < 0.01);
}

#[test]
fn a_line_keepout_cuts_the_fill_along_its_width() {
    let dir = TempDir::new("fill-keepout-line");
    let mut engine = engine_with_pcb(&dir);
    engine.run_ok(
        "pcb.rules.set",
        json!({"kind": "clearance", "values": {"zero_keepout_clearance": true}}),
    );
    let gnd = add_net(&mut engine, "GND");
    let z = zone(
        &mut engine,
        Some(gnd),
        rect(0.0, 0.0, 10.0, 10.0),
        json!({"remove_unconnected": false}),
    );
    engine.run_ok(
        "pcb.keepout.add",
        json!({"shape": "line", "width": "1mm",
               "vertices": [{"x": "-1mm", "y": "5mm"}, {"x": "11mm", "y": "5mm"}]}),
    );
    let fills = fill(&engine);
    let s = shapes(&fills, z);
    assert!(!filled(s, at(5.0, 5.4)));
    assert!(filled(s, at(5.0, 5.6)));
    assert!(filled(s, at(5.0, 4.4)));
    // The line's sides are within 1 µm of exact.
    assert!((area(s) - 90.0).abs() < 0.03, "{}", area(s));
}

#[test]
fn a_fill_stays_inside_the_board_by_the_edge_clearance() {
    let dir = TempDir::new("fill-edge");
    let mut engine = engine_with_pcb(&dir);
    let gnd = add_net(&mut engine, "GND");
    engine.run_ok(
        "pcb.rules.set",
        json!({"kind": "edge_clearance", "values": {"value": "0.5mm"}}),
    );
    engine.run_ok(
        "pcb.board_outline.add_rect",
        json!({"width": "8mm", "height": "8mm"}),
    );
    engine.run_ok(
        "pcb.board_cutout.add",
        json!({"vertices": rect(3.0, 3.0, 5.0, 5.0)}),
    );
    let z = zone(
        &mut engine,
        Some(gnd),
        rect(-5.0, -5.0, 20.0, 20.0),
        json!({"remove_unconnected": false}),
    );
    let fills = fill(&engine);
    let s = shapes(&fills, z);
    assert!(filled(s, at(0.6, 0.6)));
    assert!(!filled(s, at(0.4, 4.0)));
    assert!(!filled(s, at(7.6, 4.0)));
    assert!(!filled(s, at(9.0, 4.0)), "off the board");
    assert!(!filled(s, at(4.0, 4.0)), "in the cutout");
    assert!(
        !filled(s, at(5.4, 4.0)),
        "within the edge clearance of the cutout"
    );
    assert!(filled(s, at(5.6, 4.0)));
}

#[test]
fn necks_narrower_than_the_minimum_width_are_removed() {
    let dir = TempDir::new("fill-neck");
    let mut engine = engine_with_pcb(&dir);
    let gnd = add_net(&mut engine, "GND");
    // Two 4 mm squares joined by a 0.3 mm neck.
    let p = |x: f64, y: f64| json!({"x": mm(x), "y": mm(y)});
    let outline = json!([
        p(0.0, 0.0),
        p(4.0, 0.0),
        p(4.0, 1.85),
        p(6.0, 1.85),
        p(6.0, 0.0),
        p(10.0, 0.0),
        p(10.0, 4.0),
        p(6.0, 4.0),
        p(6.0, 2.15),
        p(4.0, 2.15),
        p(4.0, 4.0),
        p(0.0, 4.0)
    ]);
    let z = zone(
        &mut engine,
        Some(gnd),
        outline,
        json!({"remove_unconnected": false, "min_width": "0.5mm"}),
    );
    let fills = fill(&engine);
    let s = shapes(&fills, z);
    assert_eq!(s.len(), 2);
    assert!(!filled(s, at(5.0, 2.0)));

    engine.run_ok(
        "pcb.zone.set",
        json!({"zone": z.to_string(), "min_width": "0.2mm"}),
    );
    assert_eq!(shapes(&fill(&engine), z).len(), 1);
}

#[test]
fn a_cancelled_fill_returns_nothing_and_reports_progress() {
    let dir = TempDir::new("fill-cancel");
    let mut engine = engine_with_pcb(&dir);
    let gnd = add_net(&mut engine, "GND");
    zone(
        &mut engine,
        Some(gnd),
        rect(0.0, 0.0, 10.0, 10.0),
        json!({}),
    );
    zone(
        &mut engine,
        Some(gnd),
        rect(20.0, 0.0, 30.0, 10.0),
        json!({}),
    );
    let mut seen = Vec::new();
    let out = reda_pcb::zone::fill(pcb(&engine), &mut |done, total| {
        seen.push((done, total));
        done < 1
    });
    assert!(out.is_none());
    assert_eq!(seen, [(0, 2), (1, 2)]);
}

#[test]
fn a_filled_island_connects_the_pads_it_touches() {
    let dir = TempDir::new("fill-ratsnest");
    let mut engine = engine_with_pcb(&dir);
    let gnd = add_net(&mut engine, "GND");
    let a = pad(&mut engine, 2.0, 2.0, 1.0, Some(gnd));
    let b = pad(&mut engine, 8.0, 8.0, 1.0, Some(gnd));
    let z = zone(
        &mut engine,
        Some(gnd),
        rect(0.0, 0.0, 10.0, 10.0),
        json!({}),
    );
    let pcb = pcb(&engine);
    let mut copper = Copper::build(pcb);
    assert_eq!(copper.ratsnest().len(), 1);

    let fills = fill(&engine);
    copper.set_zone_fills(pcb, Some(&fills));
    assert!(copper.ratsnest().is_empty());
    let island = Item::Zone { zone: z, island: 0 };
    assert_eq!(copper.net(island), Some(gnd));
    let connected: Vec<Uuid> = copper
        .connected(island)
        .iter()
        .map(|i| i.object())
        .collect();
    assert!(connected.contains(&a) && connected.contains(&b));

    // Built afresh with the same fills, the index is the same.
    let mut fresh = Copper::build(pcb);
    fresh.set_zone_fills(pcb, Some(&fills));
    assert_eq!(fresh, copper);
    copper.set_zone_fills(pcb, None);
    assert_eq!(copper.ratsnest().len(), 1);
}

#[test]
fn routing_clearance_ignores_zone_fills_but_a_track_end_on_a_fill_takes_its_net() {
    let dir = TempDir::new("fill-routing");
    let mut engine = engine_with_pcb(&dir);
    let gnd = add_net(&mut engine, "GND");
    let sig = add_net(&mut engine, "SIG");
    zone(
        &mut engine,
        Some(gnd),
        rect(0.0, 0.0, 10.0, 10.0),
        json!({"remove_unconnected": false}),
    );
    let pcb_doc = pcb(&engine);
    let layer = pcb_doc.copper_layers().next().unwrap().uuid;
    let mut copper = Copper::build(pcb_doc);
    copper.set_zone_fills(pcb_doc, Some(&fill(&engine)));
    assert_eq!(copper.net_at(layer, at(5.0, 5.0)), Some(gnd));
    let hits = copper.clearance(
        pcb_doc,
        &reda_pcb::Proposal::Track {
            layer,
            geometry: reda_model::TrackGeometry::Line {
                from: at(1.0, 5.0),
                to: at(9.0, 5.0),
            },
            width: Nm(mm(0.2)),
            net: Some(sig),
        },
        &[],
    );
    assert!(
        hits.is_empty(),
        "the fill makes way when it is refilled: {hits:?}"
    );
}

#[test]
fn the_index_follows_zone_changes_with_the_fills_it_holds() {
    let dir = TempDir::new("fill-apply");
    let mut engine = engine_with_pcb(&dir);
    let gnd = add_net(&mut engine, "GND");
    let vcc = add_net(&mut engine, "VCC");
    let z = zone(
        &mut engine,
        Some(gnd),
        rect(0.0, 0.0, 10.0, 10.0),
        json!({"remove_unconnected": false}),
    );
    let fills = fill(&engine);
    let mut copper = Copper::build(pcb(&engine));
    copper.set_zone_fills(pcb(&engine), Some(&fills));
    let doc = pcb(&engine).uuid;
    let mut follow = |engine: &mut TestEngine, id: &str, params: Value| {
        engine.run_ok(id, params);
        let pcb = engine.project().unwrap().pcb().unwrap();
        let entry = engine
            .history()
            .next_undo(reda_model::Scope::Document(doc))
            .unwrap();
        copper.apply(pcb, &entry.changes);
        let mut fresh = Copper::build(pcb);
        fresh.set_zone_fills(pcb, Some(&fills));
        assert_eq!(copper, fresh);
        copper.net(Item::Zone { zone: z, island: 0 })
    };
    let net = follow(
        &mut engine,
        "pcb.zone.set",
        json!({"zone": z.to_string(), "net": vcc.to_string()}),
    );
    assert_eq!(net, Some(vcc), "the island takes the Zone's new Net");
    let net = follow(
        &mut engine,
        "pcb.zone.delete",
        json!({"zones": [z.to_string()]}),
    );
    assert_eq!(net, None, "a deleted Zone's fill is no copper");
}

#[test]
fn free_pads_get_relief_and_bare_holes_keep_their_clearance() {
    let dir = TempDir::new("fill-free-pads");
    let mut engine = engine_with_pcb(&dir);
    let gnd = add_net(&mut engine, "GND");
    connection(
        &mut engine,
        json!({"smd": {"gap": "0.5mm", "spoke_width": "0.3mm"}}),
    );
    let z = zone(
        &mut engine,
        Some(gnd),
        rect(0.0, 0.0, 20.0, 10.0),
        json!({}),
    );
    let before = reda_pcb::zone::input_hashes(pcb(&engine))[&z].clone();
    // A 2 mm GND test point at (5, 5) and a 3 mm bare mounting hole at (15, 5).
    engine.run_ok(
        "pcb.free_pad.add",
        json!({"kind": "smd", "at": {"x": mm(5.0), "y": mm(5.0)}, "width": mm(2.0),
               "shape": "rect", "net": gnd.to_string()}),
    );
    assert_ne!(reda_pcb::zone::input_hashes(pcb(&engine))[&z], before);
    engine.run_ok(
        "pcb.free_pad.add",
        json!({"kind": "non_plated", "at": {"x": mm(15.0), "y": mm(5.0)}, "width": mm(3.0),
               "hole": {"width": mm(3.0)}}),
    );
    let fills = fill(&engine);
    let s = shapes(&fills, z);
    assert_eq!(s.len(), 1, "the test point keeps the fill");
    assert!(filled(s, at(6.25, 5.0)), "spoke");
    assert!(!filled(s, at(6.25, 5.5)), "gap");
    assert!(!filled(s, at(15.0, 5.0)), "hole");
    assert!(!filled(s, at(16.9, 5.0)), "0.5 mm clear of the hole");
    assert!(filled(s, at(17.1, 5.0)));
}

#[test]
fn inner_relief_spokes_reach_the_fill_across_the_inner_land_gap() {
    let dir = TempDir::new("pad-stack-relief");
    let mut engine = probe_engine();
    engine.run_ok(
        "project.new",
        json!({"name":"Demo","path":dir.path(),"layers":4}),
    );
    let layer = pcb(&engine).copper_layers().nth(1).unwrap().uuid;
    let gnd = add_net(&mut engine, "GND");
    let z = zone(
        &mut engine,
        Some(gnd),
        rect(0.0, 0.0, 15.0, 10.0),
        json!({"layer":layer}),
    );
    engine.run_ok("pcb.free_pad.add",json!({"kind":"plated_through_hole","width":"1mm","at":{"x":"5mm","y":"5mm"},
        "net":gnd,"hole":{"width":"0.5mm"},"middle":{"shape":"rect","width":"4mm","height":"1mm"},
        "zone_connection":{"style":"relief","gap":"0.5mm","angle":"0deg","spokes":2,"spoke_width":"0.3mm"}}));
    let fills = fill(&engine);
    let s = shapes(&fills, z);
    assert!(filled(s, at(7.25, 5.0)), "spoke to the wide inner land");
    assert!(!filled(s, at(7.25, 5.3)), "relief gap off the spoke");
}

#[test]
fn edits_to_free_copper_drawings_refill_only_their_layer() {
    let dir = TempDir::new("zone-copper-drawing");
    let mut engine = engine_with_pcb(&dir);
    let z = zone(
        &mut engine,
        None,
        rect(0., 0., 20., 15.),
        json!({"remove_unconnected":false}),
    );
    let before = fill(&engine);
    let rect = engine.run_ok("pcb.graphic.add_rect", json!({
        "layer":"copper_top","from":{"x":"4mm","y":"3mm"},"to":{"x":"12mm","y":"10mm"},"width":"0.4mm"
    }))["result"]["uuid"].clone();
    let after = reda_pcb::zone::current(pcb(&engine), Some(&before));
    let copper = shapes(&after, z);
    assert!(
        !filled(copper, at(6., 10.6)),
        "0.4 mm from the graphic's edge is inside its clearance"
    );
    assert!(filled(copper, at(6., 10.8)));
    assert!(
        filled(copper, at(8., 6.)),
        "the rectangle is only an outline"
    );
    engine.run_ok("pcb.graphic.delete", json!({"uuid":rect}));
    assert!(reda_pcb::zone::status(pcb(&engine), Some(&after)).needs_refill);
    let text = engine.run_ok(
        "pcb.text.add",
        json!({
            "layer":"copper_bottom","font":"Noto Sans","text":"O","x":"4mm","y":"3mm","height":"7mm"
        }),
    )["result"]["uuid"]
        .clone();
    assert!(!reda_pcb::zone::status(pcb(&engine), Some(&before)).needs_refill);
    engine.run_ok("pcb.text.set", json!({"uuid":text,"layer":"copper_top"}));
    let after = reda_pcb::zone::current(pcb(&engine), Some(&before));
    assert!(
        !filled(shapes(&after, z), at(4.8, 6.5)),
        "the left side of the O clears the fill"
    );
    assert!(
        filled(shapes(&after, z), at(7.8, 6.5)),
        "the counter still fills"
    );
}

#[test]
fn an_installed_copper_font_changes_clearance_and_invalidates_the_fallback_fill() {
    let dir = TempDir::new("zone-installed-font");
    let mut engine = engine_with_pcb(&dir);
    let z = zone(
        &mut engine,
        None,
        rect(0., 0., 16., 15.),
        json!({"remove_unconnected":false}),
    );
    engine.run_ok(
        "pcb.text.add",
        json!({"layer":"copper_top","x":0,"y":0,
        "text":"I","height":"10mm","font":"Board Mono"}),
    );
    let probe = json!({"layer":"copper_top","from":{"x":"4.1mm","y":"4mm"},
        "to":{"x":"4.1mm","y":"6mm"},"width":"0.1mm"});
    // At a 10 mm cap height, the centre stroke is at x=2.37 mm for the fallback
    // Noto Sans I and x=4.11 mm for JetBrains Mono.
    assert_eq!(
        engine.run_ok("pcb.clearance.track", probe.clone())["clear"],
        true
    );
    engine.run_ok("pcb.zone.fill", json!({}));
    assert!(filled(
        shapes(
            engine
                .project()
                .unwrap()
                .zone_fills(pcb(&engine).uuid)
                .unwrap(),
            z
        ),
        at(4.1, 5.)
    ));
    let previous = engine.run_ok("drc.run", json!({"checks":["clearance"]}))["design"].clone();
    engine.fonts_mut().add(
        "Board Mono",
        &include_bytes!("../../reda-ui/fonts/jetbrains-mono/JetBrainsMonoNL-Regular.ttf")[..],
    );
    assert_eq!(
        engine.run_ok("drc.stale", json!({"design":previous}))["stale"],
        true
    );
    assert_eq!(engine.run_ok("pcb.clearance.track", probe)["clear"], false);
    assert_eq!(
        engine.run_ok("pcb.zone.fill_status", json!({}))["needs_refill"],
        true
    );
    engine.run_ok("pcb.zone.fill", json!({}));
    let stored = engine
        .project()
        .unwrap()
        .zone_fills(pcb(&engine).uuid)
        .unwrap();
    assert!(!filled(shapes(stored, z), at(4.1, 5.)));
    assert!(filled(shapes(stored, z), at(2.37, 5.)));
    assert_eq!(
        engine.run_ok("pcb.zone.fill_status", json!({}))["needs_refill"],
        false
    );
}

#[test]
fn supplied_zone_copper_touching_a_graphic_is_a_clearance_violation() {
    let dir = TempDir::new("drc-zone-free-drawing");
    let mut engine = engine_with_pcb(&dir);
    let z = zone(
        &mut engine,
        None,
        rect(0., 0., 12., 12.),
        json!({"remove_unconnected":false}),
    );
    let before = fill(&engine);
    let graphic = uuid(
        &engine.run_ok(
            "pcb.graphic.add_arc",
            json!({
                "layer":"copper_top","from":{"x":"3mm","y":"5mm"},"to":{"x":"9mm","y":"5mm"},
                "sweep":"180deg","width":"0.2mm"
            }),
        )["result"]["uuid"],
    );
    let pcb = pcb(&engine);
    let reports = |fills| {
        reda_pcb::drc::check_only(
            pcb,
            fills,
            engine.fonts(),
            Some(&[reda_model::DrcCheck::Clearance]),
        )
    };
    let hits = reports(&before);
    assert_eq!(hits.len(), 1, "{hits:?}");
    assert!(hits[0].objects.contains(&z) && hits[0].objects.contains(&graphic));
    let fresh = reda_pcb::zone::current(pcb, Some(&before));
    assert!(reports(&fresh).is_empty());
    assert!(!filled(shapes(&fresh, z), at(6., 2.)));
    assert!(filled(shapes(&fresh, z), at(6., 5.)));
}

#[test]
fn restoring_a_via_land_from_a_lower_zone_invalidates_the_upper_fill() {
    let dir = TempDir::new("fill-unused-lower-zone");
    let mut e = engine_with_pcb(&dir);
    let ground = add_net(&mut e, "GND");
    let power = add_net(&mut e, "VCC");
    e.run_ok(
        "pcb.via.add",
        json!({"at":{"x":"5mm","y":"5mm"},"net":power,
        "hole":"0.4mm","diameter":"2mm","unused_layers":"remove"}),
    );
    e.run_ok(
        "pcb.rules.set",
        json!({"kind":"clearance","values":{"hole":{"via":"0.3mm"}}}),
    );
    let high = zone(
        &mut e,
        Some(ground),
        rect(0., 0., 10., 10.),
        json!({"priority":2,"remove_unconnected":false}),
    );
    let initial = fill(&e);
    assert!(
        !filled(shapes(&initial, high), at(5.45, 5.)),
        "0.3 mm hole clearance"
    );
    assert!(
        filled(shapes(&initial, high), at(5.7, 5.)),
        "only the drill blocks this Layer"
    );
    zone(
        &mut e,
        Some(power),
        rect(1., 1., 9., 9.),
        json!({"priority":1}),
    );
    let current = reda_pcb::zone::current(pcb(&e), Some(&initial));
    assert!(
        !filled(shapes(&current, high), at(5.7, 5.)),
        "the restored land keeps copper clear"
    );
}

#[test]
fn a_disconnected_same_net_zone_measures_its_gap_from_the_omitted_lands_hole() {
    let dir = TempDir::new("fill-omitted-hole-gap");
    let mut e = probe_engine();
    e.run_ok(
        "project.new",
        json!({"path":dir.path(),"name":"Demo","layers":4}),
    );
    let inner = pcb(&e).copper_layers().nth(1).unwrap().uuid;
    let net = add_net(&mut e, "GND");
    e.run_ok(
        "pcb.free_pad.add",
        json!({"kind":"plated_through_hole","width":"2mm",
        "at":{"x":"5mm","y":"5mm"},"hole":{"width":"0.4mm"},"unused_layers":"remove","net":net,
        "zone_connection":{"style":"none","gap":"0.6mm"}}),
    );
    let z = zone(
        &mut e,
        Some(net),
        rect(0., 0., 10., 10.),
        json!({"layer":inner,"remove_unconnected":false}),
    );
    let fills = fill(&e);
    let s = shapes(&fills, z);
    assert!(
        !filled(s, at(5.75, 5.)),
        "0.2 mm bore radius plus the 0.6 mm connection gap"
    );
    assert!(
        filled(s, at(5.9, 5.)),
        "the removed 1 mm land radius adds no gap"
    );
}

#[test]
fn a_zero_clearance_zone_does_not_touch_netless_artwork() {
    let dir = TempDir::new("zone-artwork-contact");
    let mut engine = engine_with_pcb(&dir);
    engine.run_ok(
        "pcb.rules.set",
        json!({"kind":"clearance","values":{"value":0}}),
    );
    let z = zone(
        &mut engine,
        None,
        rect(0., 0., 12., 12.),
        json!({
            "clearance":0, "min_width":1, "remove_unconnected":false
        }),
    );
    engine.run_ok(
        "pcb.graphic.add_rect",
        json!({
            "layer":"copper_top","from":{"x":"4mm","y":"4mm"},"to":{"x":"8mm","y":"8mm"},
            "filled":true, "width":0
        }),
    );
    let fills = fill(&engine);
    assert!(filled(shapes(&fills, z), at(3.99, 6.)));
    assert!(!filled(shapes(&fills, z), at(6., 6.)));
    let hits = reda_pcb::drc::check_only(
        pcb(&engine),
        &fills,
        engine.fonts(),
        Some(&[reda_model::DrcCheck::Clearance]),
    );
    assert!(hits.is_empty(), "{hits:?}");
}

#[test]
fn a_zone_reaching_the_edge_of_a_land_restores_its_copper() {
    let dir = TempDir::new("zone-reaching-land-edge");
    let mut engine = engine_with_pcb(&dir);
    let net = add_net(&mut engine, "GND");
    let via = uuid(
        &engine.run_ok(
            "pcb.via.add",
            json!({"at":at(5.,5.), "net":net, "hole":"0.4mm", "diameter":"2mm",
               "unused_layers":"remove","zone_connection":{"style":"solid"}}),
        )["result"]["uuid"],
    );
    let z = zone(&mut engine, Some(net), rect(5.5, 4., 9., 6.), json!({}));
    let layer = pcb(&engine).zone(z).unwrap().layer;
    let has_land = |engine: &TestEngine| {
        Copper::build(pcb(engine))
            .items_at(layer, at(5., 5.))
            .contains(&Item::Via { uuid: via })
    };
    assert!(has_land(&engine));
    assert!(
        filled(shapes(&fill(&engine), z), at(8., 5.)),
        "the Zone connects through that edge"
    );
    engine.run_ok(
        "pcb.zone_cutout.add",
        json!({"layer":layer,"vertices":rect(4.,3.,6.1,7.)}),
    );
    assert!(!has_land(&engine));
    assert!(shapes(&fill(&engine), z).is_empty());
}
