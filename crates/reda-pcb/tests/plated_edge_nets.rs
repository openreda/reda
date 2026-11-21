//! Plated Edges and Nets (docs/spec/v0.2.5/plated-edges.md#net-and-copper): the DRC
//! checks on a Plated Edge's Net, connectivity through the plating, and the clearance
//! copper keeps from it.
//!
//! The board is the Generic preset's 40 × 30 mm rectangle: clearance 0.16 mm, edge
//! clearance 0.4 mm. Its edges, by the vertex that starts them: 0 bottom (y = 0), 1 right
//! (x = 40), 2 top (y = 30), 3 left (x = 0).

use reda_model::Uuid;
use reda_testkit::{TempDir, TestEngine, probe_engine};
use serde_json::{Value, json};

fn uuid(value: &Value) -> Uuid {
    value.as_str().unwrap().parse().unwrap()
}

/// A board with the 40 × 30 mm outline, no edge plated; the engine and the outline.
fn board(dir: &TempDir) -> (TestEngine, Uuid) {
    let mut engine = probe_engine();
    engine.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
    let out = engine.run_ok(
        "pcb.board_outline.add",
        json!({"vertices": [
            {"x": 0, "y": 0}, {"x": "40mm", "y": 0}, {"x": "40mm", "y": "30mm"}, {"x": 0, "y": "30mm"}
        ]}),
    );
    let outline = uuid(&out["result"]["uuid"]);
    (engine, outline)
}

fn net(engine: &mut TestEngine, name: &str) -> Uuid {
    uuid(&engine.run_ok("test.net.add", json!({"name": name}))["result"]["uuid"])
}

/// Plates edge `edge` of `contour`, with `net` or none.
fn plate(engine: &mut TestEngine, contour: Uuid, edge: usize, net: Option<Uuid>) {
    let plated = match net {
        Some(net) => json!({"net": net}),
        None => json!({}),
    };
    engine.run_ok(
        "pcb.contour.edge.set",
        json!({"contour": contour, "edge": edge, "plated": plated}),
    );
}

/// A straight 0.2 mm Track on `layer` through the given points, in mm.
fn track(engine: &mut TestEngine, layer: &str, net: Option<Uuid>, points: &[(f64, f64)]) -> Uuid {
    let points: Vec<Value> = points
        .iter()
        .map(|&(x, y)| json!({"x": format!("{x}mm"), "y": format!("{y}mm")}))
        .collect();
    let mut params = json!({"layer": layer, "width": "0.2mm", "points": points});
    if let Some(net) = net {
        params["net"] = json!(net);
    }
    uuid(&engine.run_ok("pcb.track.add", params)["result"]["tracks"][0])
}

/// A 1 mm round SMD Free Pad on the top copper at `(x, y)` mm.
fn smd(engine: &mut TestEngine, net: Option<Uuid>, x: f64, y: f64) -> Uuid {
    let mut params = json!({"kind": "smd", "shape": "circle", "width": "1mm",
        "at": {"x": format!("{x}mm"), "y": format!("{y}mm")}});
    if let Some(net) = net {
        params["net"] = json!(net);
    }
    uuid(&engine.run_ok("pcb.free_pad.add", params)["result"]["uuid"])
}

fn drc(engine: &mut TestEngine, checks: &[&str]) -> Value {
    engine.run_ok("drc.run", json!({"checks": checks}))
}

fn violations(run: &Value) -> Vec<Value> {
    run["violations"].as_array().unwrap().clone()
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
fn plated_edges_meeting_at_a_corner_with_different_nets_are_an_error_at_the_corner() {
    let dir = TempDir::new("plated-nets-differ");
    let (mut engine, outline) = board(&dir);
    let (gnd, vcc) = (net(&mut engine, "GND"), net(&mut engine, "VCC"));
    plate(&mut engine, outline, 0, Some(gnd));
    plate(&mut engine, outline, 1, Some(vcc));
    // Edges 2 and 3 meet with one Net: that corner is fine.
    plate(&mut engine, outline, 2, Some(gnd));
    plate(&mut engine, outline, 3, Some(gnd));
    let run = drc(&mut engine, &["plated_edge_nets_differ"]);
    let found = violations(&run);
    // Edges 0 (GND) and 1 (VCC) meet at (40, 0), edges 1 and 2 at (40, 30).
    assert_eq!(found.len(), 2, "{run:#}");
    for (v, at) in found.iter().zip([[40_000_000, 0], [40_000_000, 30_000_000]]) {
        assert_eq!(v["severity"], "error", "{v:#}");
        assert_eq!(v["at"], json!({"x": at[0], "y": at[1]}), "{v:#}");
        assert_eq!(objects(v), [outline], "{v:#}");
    }
    assert_eq!(found[0]["args"]["net_a"], "GND");
    assert_eq!(found[0]["args"]["net_b"], "VCC");
}

/// Two GND Pads on the top copper, each joined to the left edge: the first by a top Track,
/// the second by a top Track to a Via and a bottom Track on from it. The Pads and the
/// two Tracks touching the edge.
fn two_pads_run_to_the_left_edge(engine: &mut TestEngine, gnd: Uuid) -> [Uuid; 4] {
    let a = smd(engine, Some(gnd), 5.0, 10.0);
    let ta = track(engine, "copper_top", Some(gnd), &[(5.0, 10.0), (0.1, 10.0)]);
    let b = smd(engine, Some(gnd), 5.0, 20.0);
    track(engine, "copper_top", Some(gnd), &[(5.0, 20.0), (3.0, 20.0)]);
    engine.run_ok(
        "pcb.via.add",
        json!({"at": {"x": "3mm", "y": "20mm"}, "net": gnd}),
    );
    let tb = track(engine, "copper_bottom", Some(gnd), &[(3.0, 20.0), (0.1, 20.0)]);
    [a, b, ta, tb]
}

#[test]
fn copper_of_the_net_touching_a_plated_edge_on_any_layer_is_connected_through_it() {
    let dir = TempDir::new("plated-connects");
    let (mut engine, outline) = board(&dir);
    let gnd = net(&mut engine, "GND");
    let [a, b, ..] = two_pads_run_to_the_left_edge(&mut engine, gnd);
    let lines = |engine: &mut TestEngine| {
        engine.run_ok("pcb.ratsnest", json!({}))["lines"]
            .as_array()
            .unwrap()
            .len()
    };
    let unrouted = |engine: &mut TestEngine| violations(&drc(engine, &["unrouted"])).len();
    let connected = |engine: &mut TestEngine| -> Vec<Value> {
        engine.run_ok("pcb.copper.connected", json!({"object": a}))["items"]
            .as_array()
            .unwrap()
            .clone()
    };
    assert_eq!(lines(&mut engine), 1);
    assert_eq!(unrouted(&mut engine), 1);

    plate(&mut engine, outline, 3, Some(gnd));
    assert_eq!(lines(&mut engine), 0);
    assert_eq!(unrouted(&mut engine), 0);
    let items = connected(&mut engine);
    assert!(
        items.contains(&json!({"kind": "free_pad", "uuid": b})),
        "{items:#?}"
    );
    assert!(
        items.contains(&json!({"kind": "plated_edge", "contour": outline, "edge": 3})),
        "{items:#?}"
    );

    // A Plated Edge of another Net joins nothing.
    let vcc = net(&mut engine, "VCC");
    plate(&mut engine, outline, 3, Some(vcc));
    assert_eq!(lines(&mut engine), 1);
}

#[test]
fn a_plated_edge_no_copper_of_its_net_touches_on_the_outer_layers_is_a_warning() {
    let dir = TempDir::new("plated-unconnected");
    let (mut engine, outline) = board(&dir);
    let (gnd, vcc) = (net(&mut engine, "GND"), net(&mut engine, "VCC"));
    // A GND Track touches the left edge; the top edge meets it at a corner, so it is
    // the same metal and joined too.
    track(&mut engine, "copper_top", Some(gnd), &[(5.0, 10.0), (0.1, 10.0)]);
    plate(&mut engine, outline, 3, Some(gnd));
    plate(&mut engine, outline, 2, Some(gnd));
    // A plated triangular cutout that only copper of another Net touches.
    let cutout = uuid(
        &engine.run_ok(
            "pcb.board_cutout.add",
            json!({"vertices": [
                {"x": "20mm", "y": "10mm"}, {"x": "25mm", "y": "10mm"}, {"x": "22mm", "y": "14mm"}
            ]}),
        )["result"]["uuid"],
    );
    engine.run_ok(
        "pcb.contour.edge.set_all",
        json!({"contour": cutout, "plated": {"net": gnd}}),
    );
    track(&mut engine, "copper_top", Some(vcc), &[(22.0, 5.0), (22.0, 9.9)]);
    let run = drc(&mut engine, &["plated_edge_unconnected"]);
    let found = violations(&run);
    assert_eq!(found.len(), 3, "{run:#}");
    for (v, edge) in found.iter().zip(0..) {
        assert_eq!(v["severity"], "warning");
        assert_eq!(objects(v), [cutout]);
        assert_eq!(v["args"]["edge"], edge);
        assert_eq!(v["args"]["net"], "GND");
    }
    // A GND Track on the bottom copper joins the cutout's plating.
    track(&mut engine, "copper_bottom", Some(gnd), &[(22.0, 5.0), (22.0, 9.9)]);
    let run = drc(&mut engine, &["plated_edge_unconnected"]);
    assert_eq!(violations(&run), Vec::<Value>::new(), "{run:#}");
}

/// A Footprint of two 1 × 2 mm SMD Pads made Edge Connector Pads, touching the bottom
/// edge at x = 10 and x = 30. The instance and its Pads.
fn fingers_on_the_bottom_edge(engine: &mut TestEngine) -> (Uuid, Vec<Uuid>) {
    let out = engine.run_ok(
        "test.footprint.add",
        json!({"at": {"x": "20mm", "y": "1mm"}, "designator": "J1", "pads": [
            {"at": {"x": "-10mm", "y": 0}, "width": "1mm", "height": "2mm"},
            {"at": {"x": "10mm", "y": 0}, "width": "1mm", "height": "2mm"}]}),
    )["result"]
        .clone();
    let instance = uuid(&out["footprint"]);
    let pads: Vec<Uuid> = out["pads"].as_array().unwrap().iter().map(uuid).collect();
    for pad in &pads {
        engine.run_ok(
            "pcb.pad.edit",
            json!({"instance": instance, "pad": pad, "role": "edge_connector"}),
        );
    }
    (instance, pads)
}

#[test]
fn an_edge_connector_pad_on_a_plated_edge_is_an_error() {
    let dir = TempDir::new("plated-fingers");
    let (mut engine, outline) = board(&dir);
    let (instance, pads) = fingers_on_the_bottom_edge(&mut engine);
    let run = drc(&mut engine, &["edge_connector_on_plated_edge"]);
    assert_eq!(violations(&run), Vec::<Value>::new(), "{run:#}");

    // Plate the bottom edge from x = 0 to x = 20 only, by splitting it there.
    engine.run_ok(
        "pcb.polygon.set_vertices",
        json!({"object": outline, "vertices": [
            {"x": 0, "y": 0, "plated": {}}, {"x": "20mm", "y": 0}, {"x": "40mm", "y": 0},
            {"x": "40mm", "y": "30mm"}, {"x": 0, "y": "30mm"}
        ]}),
    );
    let run = drc(&mut engine, &["edge_connector_on_plated_edge"]);
    let found = violations(&run);
    assert_eq!(found.len(), 1, "{run:#}");
    assert_eq!(found[0]["severity"], "error");
    assert_eq!(objects(&found[0]), [instance, pads[0], outline]);
    assert_eq!(found[0]["args"]["edge"], 0);
}

#[test]
fn copper_of_the_edges_own_net_may_touch_a_plated_edge_but_not_cross_it() {
    let dir = TempDir::new("plated-own-net");
    let (mut engine, outline) = board(&dir);
    let gnd = net(&mut engine, "GND");
    plate(&mut engine, outline, 3, Some(gnd));
    // Touches the left edge (its round end reaches x = 0).
    track(&mut engine, "copper_top", Some(gnd), &[(5.0, 10.0), (0.1, 10.0)]);
    let run = drc(&mut engine, &["edge_clearance"]);
    assert_eq!(violations(&run), Vec::<Value>::new(), "{run:#}");

    // Runs 0.4 mm past it.
    let across = track(&mut engine, "copper_top", Some(gnd), &[(5.0, 20.0), (-0.3, 20.0)]);
    let run = drc(&mut engine, &["edge_clearance"]);
    let found = violations(&run);
    assert_eq!(found.len(), 1, "{run:#}");
    assert_eq!(objects(&found[0]), [across, outline]);
    assert_eq!(found[0]["args"]["actual"], 0);
}

#[test]
fn copper_of_another_net_keeps_the_larger_of_clearance_and_edge_clearance_from_a_plated_edge()
{
    let dir = TempDir::new("plated-other-net");
    let (mut engine, outline) = board(&dir);
    let (gnd, vcc) = (net(&mut engine, "GND"), net(&mut engine, "VCC"));
    plate(&mut engine, outline, 3, Some(gnd));
    // 0.5 mm from the plated left edge: clear of the 0.4 mm edge clearance.
    let beside = track(&mut engine, "copper_top", Some(vcc), &[(5.0, 10.0), (0.6, 10.0)]);
    let run = drc(&mut engine, &["edge_clearance", "clearance"]);
    assert_eq!(violations(&run), Vec::<Value>::new(), "{run:#}");

    // A 0.6 mm clearance now governs, and is reported once, as edge clearance.
    engine.run_ok(
        "pcb.rules.set",
        json!({"kind": "clearance", "values": {"value": "0.6mm"}}),
    );
    let run = drc(&mut engine, &["edge_clearance", "clearance"]);
    let found = violations(&run);
    assert_eq!(found.len(), 1, "{run:#}");
    assert_eq!(found[0]["check"], "edge_clearance");
    assert_eq!(objects(&found[0]), [beside, outline]);
    assert_eq!(found[0]["args"]["required"], 600_000);
    assert_eq!(found[0]["args"]["actual"], 500_000);

    // The unplated bottom edge still asks only the edge clearance.
    track(&mut engine, "copper_top", Some(vcc), &[(20.0, 5.0), (20.0, 0.6)]);
    let run = drc(&mut engine, &["edge_clearance"]);
    assert_eq!(violations(&run).len(), 1, "{run:#}");
}

/// The fill of the only Zone, as its polygons' points.
fn fill_points(engine: &mut TestEngine) -> Vec<(i64, i64)> {
    let fills = engine.run_ok("pcb.zone.fills", json!({}));
    let mut points = Vec::new();
    collect_points(&fills, &mut points);
    points
}

fn collect_points(value: &Value, out: &mut Vec<(i64, i64)>) {
    match value {
        Value::Object(map) => {
            if let (Some(x), Some(y)) = (map.get("x"), map.get("y"))
                && let (Some(x), Some(y)) = (x.as_i64(), y.as_i64())
            {
                out.push((x, y));
            }
            map.values().for_each(|v| collect_points(v, out));
        }
        Value::Array(items) => items.iter().for_each(|v| collect_points(v, out)),
        _ => {}
    }
}

/// A Zone over the whole board on the top copper that keeps unconnected islands, filled.
fn board_zone(engine: &mut TestEngine, net: Uuid) {
    engine.run_ok(
        "pcb.zone.add",
        json!({"layer": "copper_top", "net": net, "remove_unconnected": false, "vertices": [
            {"x": 0, "y": 0}, {"x": "40mm", "y": 0}, {"x": "40mm", "y": "30mm"}, {"x": 0, "y": "30mm"}
        ]}),
    );
    engine.run_ok("pcb.zone.fill", json!({}));
}

#[test]
fn a_zone_of_the_edges_net_fills_up_to_it_and_joins_it() {
    let dir = TempDir::new("plated-zone-own");
    let (mut engine, outline) = board(&dir);
    let gnd = net(&mut engine, "GND");
    plate(&mut engine, outline, 3, Some(gnd));
    board_zone(&mut engine, gnd);
    let points = fill_points(&mut engine);
    let min_x = points.iter().map(|p| p.0).min().unwrap();
    let min_y = points.iter().map(|p| p.1).min().unwrap();
    // Up to the plated left edge; held back the edge clearance from the bottom one.
    assert_eq!(min_x, 0, "{points:?}");
    assert!((400_000..=401_000).contains(&min_y), "{points:?}");
    let run = drc(&mut engine, &["plated_edge_unconnected", "edge_clearance"]);
    assert_eq!(violations(&run), Vec::<Value>::new(), "{run:#}");
}

#[test]
fn a_zone_of_another_net_keeps_the_larger_of_clearance_and_edge_clearance_from_it() {
    let dir = TempDir::new("plated-zone-other");
    let (mut engine, outline) = board(&dir);
    let (gnd, vcc) = (net(&mut engine, "GND"), net(&mut engine, "VCC"));
    plate(&mut engine, outline, 3, Some(gnd));
    engine.run_ok(
        "pcb.rules.set",
        json!({"kind": "clearance", "values": {"value": "0.6mm"}}),
    );
    board_zone(&mut engine, vcc);
    let points = fill_points(&mut engine);
    let min_x = points.iter().map(|p| p.0).min().unwrap();
    let min_y = points.iter().map(|p| p.1).min().unwrap();
    assert!((599_000..=601_000).contains(&min_x), "{min_x}");
    assert!((400_000..=401_000).contains(&min_y), "{points:?}");
}

#[test]
fn a_plated_edge_with_no_net_is_a_warning_at_its_middle() {
    let dir = TempDir::new("plated-no-net");
    let (mut engine, outline) = board(&dir);
    let gnd = net(&mut engine, "GND");
    plate(&mut engine, outline, 0, None);
    plate(&mut engine, outline, 2, Some(gnd));
    let run = drc(&mut engine, &["plated_edge_without_net"]);
    let found = violations(&run);
    assert_eq!(found.len(), 1, "{run:#}");
    assert_eq!(found[0]["severity"], "warning");
    assert_eq!(found[0]["at"], json!({"x": 20_000_000, "y": 0}));
    assert_eq!(found[0]["args"]["edge"], 0);
    assert_eq!(objects(&found[0]), [outline]);
}
