//! Filling Zones: clearance, thermal relief, islands, priority, Keepouts, the board edge
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
    &fills.zones[&zone]
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
        json!({"remove_islands": false}),
    );
    let fills = fill(&engine);
    let s = shapes(&fills, z);
    assert_eq!(s.len(), 1);
    assert!((area(s) - 100.0).abs() < 0.01, "{}", area(s));
    assert_eq!(fills.inputs, reda_pcb::zone::inputs_hash(pcb(&engine)));
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
        json!({"remove_islands": false}),
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
        json!({"remove_islands": false}),
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
    let z = zone(
        &mut engine,
        Some(gnd),
        rect(0.0, 0.0, 10.0, 10.0),
        json!({"pad_connection": "solid"}),
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
        json!({"zone": z.to_string(), "remove_islands": false}),
    );
    assert_eq!(shapes(&fill(&engine), z).len(), 2);
}

#[test]
fn thermal_relief_leaves_a_gap_crossed_by_four_spokes() {
    let dir = TempDir::new("fill-thermal");
    let mut engine = engine_with_pcb(&dir);
    let gnd = add_net(&mut engine, "GND");
    let z = zone(
        &mut engine,
        Some(gnd),
        rect(0.0, 0.0, 10.0, 10.0),
        json!({"thermal_gap": "0.5mm", "thermal_spoke_width": "0.3mm"}),
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
    let z = zone(
        &mut engine,
        Some(gnd),
        rect(0.0, 0.0, 10.0, 10.0),
        json!({"pad_connection": "solid"}),
    );
    pad(&mut engine, 5.0, 5.0, 2.0, Some(gnd));
    let fills = fill(&engine);
    let s = shapes(&fills, z);
    assert!(filled(s, at(6.25, 5.5)));
    assert!(filled(s, at(6.3, 6.3)));
    assert!((area(s) - 100.0).abs() < 0.01);
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
        json!({"remove_islands": false}),
    );
    let high = zone(
        &mut engine,
        Some(vcc),
        rect(5.0, 0.0, 15.0, 10.0),
        json!({"remove_islands": false, "priority": 1}),
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
        json!({"remove_islands": false}),
    );
    let b = zone(
        &mut engine,
        Some(gnd),
        rect(5.0, 0.0, 15.0, 10.0),
        json!({"remove_islands": false, "priority": 1}),
    );
    let fills = fill(&engine);
    assert!((area(shapes(&fills, a)) - 100.0).abs() < 0.01);
    assert!((area(shapes(&fills, b)) - 100.0).abs() < 0.01);
}

#[test]
fn a_fill_leaves_copper_keepouts_empty() {
    let dir = TempDir::new("fill-keepout");
    let mut engine = engine_with_pcb(&dir);
    let gnd = add_net(&mut engine, "GND");
    let z = zone(
        &mut engine,
        Some(gnd),
        rect(0.0, 0.0, 10.0, 10.0),
        json!({"remove_islands": false}),
    );
    engine.run_ok(
        "pcb.keepout.add",
        json!({"vertices": rect(2.0, 2.0, 4.0, 4.0)}),
    );
    // A Via-only Keepout and one on the other Layer leave the fill alone.
    engine.run_ok(
        "pcb.keepout.add",
        json!({"vertices": rect(6.0, 6.0, 8.0, 8.0), "copper": false}),
    );
    engine.run_ok(
        "pcb.keepout.add",
        json!({"vertices": rect(6.0, 2.0, 8.0, 4.0), "layers": ["copper_bottom"]}),
    );
    let fills = fill(&engine);
    let s = shapes(&fills, z);
    assert!(!filled(s, at(3.0, 3.0)));
    assert!(filled(s, at(7.0, 7.0)));
    assert!(filled(s, at(7.0, 3.0)));
    assert!((area(s) - 96.0).abs() < 0.01);
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
        json!({"remove_islands": false}),
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
        json!({"remove_islands": false, "min_width": "0.5mm"}),
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
        json!({"remove_islands": false}),
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
        json!({"remove_islands": false}),
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
fn free_pads_get_thermal_relief_and_bare_holes_keep_their_clearance() {
    let dir = TempDir::new("fill-free-pads");
    let mut engine = engine_with_pcb(&dir);
    let gnd = add_net(&mut engine, "GND");
    let z = zone(
        &mut engine,
        Some(gnd),
        rect(0.0, 0.0, 20.0, 10.0),
        json!({"thermal_gap": "0.5mm", "thermal_spoke_width": "0.3mm"}),
    );
    let before = reda_pcb::zone::inputs_hash(pcb(&engine));
    // A 2 mm GND test point at (5, 5) and a 3 mm bare mounting hole at (15, 5).
    engine.run_ok(
        "pcb.free_pad.add",
        json!({"kind": "smd", "at": {"x": mm(5.0), "y": mm(5.0)}, "width": mm(2.0),
               "shape": "rect", "net": gnd.to_string()}),
    );
    assert_ne!(reda_pcb::zone::inputs_hash(pcb(&engine)), before);
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
