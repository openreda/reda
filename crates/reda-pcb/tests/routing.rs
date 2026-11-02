//! Tracks and Vias, through Commands.

use reda_command::ErrorCode;
use reda_model::geom::{Nm, Point};
use reda_model::{LayerKind, PcbDocument, Side, TrackGeometry, Uuid};
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

fn add_net(engine: &mut TestEngine, name: &str) -> Uuid {
    uuid(&engine.run_ok("test.net.add", json!({"name": name}))["result"]["uuid"])
}

fn mm(x: i64, y: i64) -> Point {
    Point::new(Nm::from_mm(x), Nm::from_mm(y))
}

fn pointer(err: &reda_command::CommandError) -> &str {
    err.details
        .as_ref()
        .and_then(|d| d["pointer"].as_str())
        .unwrap_or("")
}

#[test]
fn a_route_adds_one_line_track_per_segment_with_the_net_class_width() {
    let dir = TempDir::new("track-add");
    let mut engine = engine_with_pcb(&dir);
    let net = add_net(&mut engine, "SIG");
    let out = engine.run_ok(
        "pcb.track.add",
        json!({"layer": "copper_top", "net": net.to_string(),
               "points": [{"x": 0, "y": 0}, {"x": "5mm", "y": 0}, {"x": "5mm", "y": "5mm"}]}),
    );
    let tracks = uuids(&out["result"]["tracks"]);
    assert_eq!(tracks.len(), 2);
    let pcb = pcb(&engine);
    let top = pcb
        .layers()
        .find(LayerKind::Copper, Some(Side::Top))
        .unwrap();
    let rules = pcb.setup().rules;
    let first = pcb.track(tracks[0]).unwrap();
    assert_eq!(first.layer, top.uuid);
    assert_eq!(first.net, Some(net));
    assert_eq!(
        first.width, rules.track_width,
        "the Default class has no override"
    );
    assert_eq!(
        first.geometry,
        TrackGeometry::Line {
            from: mm(0, 0),
            to: mm(5, 0)
        }
    );
    assert_eq!(
        pcb.track(tracks[1]).unwrap().geometry,
        TrackGeometry::Line {
            from: mm(5, 0),
            to: mm(5, 5)
        }
    );
}

#[test]
fn the_net_class_track_width_wins_over_the_rule_and_a_param_over_both() {
    let dir = TempDir::new("track-width");
    let mut engine = engine_with_pcb(&dir);
    let net = add_net(&mut engine, "VCC");
    let power = engine.run_ok(
        "pcb.net_class.add",
        json!({"name": "Power", "track_width": "0.5mm"}),
    );
    engine.run_ok(
        "pcb.net.set_class",
        json!({"nets": [net.to_string()], "class": power["result"]["uuid"]}),
    );
    let by_class = engine.run_ok(
        "pcb.track.add",
        json!({"layer": "copper_top", "net": net.to_string(),
               "points": [{"x": 0, "y": 0}, {"x": "5mm", "y": 0}]}),
    );
    let by_param = engine.run_ok(
        "pcb.track.add",
        json!({"layer": "copper_bottom", "net": net.to_string(), "width": "1mm",
               "points": [{"x": 0, "y": 0}, {"x": "5mm", "y": 0}]}),
    );
    let pcb = pcb(&engine);
    let width = |out: &Value| pcb.track(uuids(&out["result"]["tracks"])[0]).unwrap().width;
    assert_eq!(width(&by_class), Nm::from_um(500));
    assert_eq!(width(&by_param), Nm::from_mm(1));
}

#[test]
fn tracks_go_on_copper_layers_only_and_need_two_distinct_points() {
    let dir = TempDir::new("track-refused");
    let mut engine = engine_with_pcb(&dir);
    let err = engine.run_err(
        "pcb.track.add",
        json!({"layer": "silkscreen_top", "points": [{"x": 0, "y": 0}, {"x": "1mm", "y": 0}]}),
    );
    assert_eq!(err.code, ErrorCode::InvalidParams);
    assert_eq!(pointer(&err), "/layer");
    let err = engine.run_err(
        "pcb.track.add",
        json!({"layer": "copper_top", "points": [{"x": 0, "y": 0}]}),
    );
    assert_eq!(pointer(&err), "/points");
    let err = engine.run_err(
        "pcb.track.add",
        json!({"layer": "copper_top", "points": [{"x": 0, "y": 0}, {"x": 0, "y": 0}]}),
    );
    assert_eq!(pointer(&err), "/points/1");
}

/// A two-Pad Footprint at `at`: 1 mm square SMD Pads 2 mm either side of its origin.
fn two_pads(engine: &mut TestEngine, at: (i64, i64), nets: [Option<Uuid>; 2]) -> (Uuid, [Uuid; 2]) {
    let pad = |dx: i64, net: Option<Uuid>| {
        json!({"at": {"x": format!("{dx}mm"), "y": 0}, "width": "1mm", "height": "1mm",
               "net": net.map(|n| n.to_string())})
    };
    let out = engine.run_ok(
        "test.footprint.add",
        json!({"at": {"x": format!("{}mm", at.0), "y": format!("{}mm", at.1)},
               "pads": [pad(-2, nets[0]), pad(2, nets[1])]}),
    );
    let pads = uuids(&out["result"]["pads"]);
    (uuid(&out["result"]["footprint"]), [pads[0], pads[1]])
}

fn route(engine: &mut TestEngine, layer: &str, points: &[(i64, i64)]) -> Value {
    let points: Vec<Value> = points
        .iter()
        .map(|(x, y)| json!({"x": format!("{x}mm"), "y": format!("{y}mm")}))
        .collect();
    engine.run_ok("pcb.track.add", json!({"layer": layer, "points": points}))["result"].clone()
}

#[test]
fn a_route_takes_the_net_of_the_pad_it_starts_on() {
    let dir = TempDir::new("track-infer");
    let mut engine = engine_with_pcb(&dir);
    let gnd = add_net(&mut engine, "GND");
    two_pads(&mut engine, (0, 0), [Some(gnd), None]);
    let from_pad = route(&mut engine, "copper_top", &[(-2, 0), (-2, 10)]);
    assert_eq!(uuid(&from_pad["net"]), gnd);
    let into_pad = route(&mut engine, "copper_top", &[(-10, 0), (-2, 0)]);
    assert_eq!(
        uuid(&into_pad["net"]),
        gnd,
        "falls back to the end it reaches"
    );
    let bottom = route(&mut engine, "copper_bottom", &[(-2, 0), (-2, 10)]);
    assert!(
        bottom.get("net").is_none(),
        "an SMD Pad has copper on its own side only"
    );
}

#[test]
fn a_through_via_spans_the_outer_copper_layers_and_sizes_from_the_net_class() {
    let dir = TempDir::new("via-add");
    let mut engine = engine_with_pcb(&dir);
    let net = add_net(&mut engine, "SIG");
    let out = engine.run_ok(
        "pcb.via.add",
        json!({"at": {"x": "1mm", "y": "1mm"}, "net": net.to_string()}),
    );
    let via = uuid(&out["result"]["uuid"]);
    let pcb = pcb(&engine);
    let via = pcb.via(via).unwrap();
    let copper = |side| {
        pcb.layers()
            .find(LayerKind::Copper, Some(side))
            .unwrap()
            .uuid
    };
    assert_eq!(
        (via.span.from, via.span.to),
        (copper(Side::Top), copper(Side::Bottom))
    );
    assert_eq!(via.net, Some(net));
    let rules = pcb.setup().rules;
    assert_eq!(
        (via.diameter, via.hole),
        (rules.via_diameter, rules.via_hole)
    );

    let err = engine.run_err(
        "pcb.via.add",
        json!({"at": {"x": 0, "y": 0}, "diameter": "0.3mm", "hole": "0.3mm"}),
    );
    assert_eq!(pointer(&err), "/diameter");
}

#[test]
fn a_stitching_via_takes_the_net_of_the_copper_under_it() {
    let dir = TempDir::new("via-infer");
    let mut engine = engine_with_pcb(&dir);
    let gnd = add_net(&mut engine, "GND");
    engine.run_ok(
        "pcb.track.add",
        json!({"layer": "copper_bottom", "net": gnd.to_string(),
               "points": [{"x": 0, "y": 0}, {"x": "10mm", "y": 0}]}),
    );
    let out = engine.run_ok("pcb.via.add", json!({"at": {"x": "5mm", "y": 0}}));
    assert_eq!(uuid(&out["result"]["net"]), gnd);
}

fn connected(engine: &mut TestEngine, params: Value) -> Vec<Value> {
    engine.run_ok("pcb.copper.connected", params)["items"]
        .as_array()
        .unwrap()
        .clone()
}

#[test]
fn select_connected_follows_touching_copper_through_vias_to_the_other_side() {
    let dir = TempDir::new("connected");
    let mut engine = engine_with_pcb(&dir);
    let (fp, pads) = two_pads(&mut engine, (0, 0), [None, None]);
    let top = route(&mut engine, "copper_top", &[(-2, 0), (-2, 5), (10, 5)]);
    engine.run_ok("pcb.via.add", json!({"at": {"x": "10mm", "y": "5mm"}}));
    let bottom = route(&mut engine, "copper_bottom", &[(10, 5), (20, 5)]);
    let apart = route(&mut engine, "copper_bottom", &[(30, 5), (40, 5)]);

    let items = connected(&mut engine, json!({"object": bottom["tracks"][0]}));
    assert_eq!(
        items.len(),
        5,
        "{items:?}: Pad, two top Tracks, Via, bottom Track"
    );
    assert!(
        items.contains(&json!({"kind": "pad", "footprint": fp.to_string(),
                                   "pad": pads[0].to_string()}))
    );
    assert!(items.contains(&json!({"kind": "track", "uuid": top["tracks"][1]})));
    assert!(!items.iter().any(|i| i["uuid"] == apart["tracks"][0]));

    let from_pad = connected(
        &mut engine,
        json!({"object": fp.to_string(), "pad": pads[0].to_string()}),
    );
    assert_eq!(from_pad, items);
    let other_pad = connected(
        &mut engine,
        json!({"object": fp.to_string(), "pad": pads[1].to_string()}),
    );
    assert_eq!(other_pad.len(), 1, "the second Pad is on its own");
}

fn ratsnest(engine: &mut TestEngine) -> Vec<Value> {
    engine.run_ok("pcb.ratsnest", json!({}))["lines"]
        .as_array()
        .unwrap()
        .clone()
}

#[test]
fn the_ratsnest_is_a_minimum_spanning_tree_that_copper_shrinks() {
    let dir = TempDir::new("ratsnest");
    let mut engine = engine_with_pcb(&dir);
    let sig = add_net(&mut engine, "SIG");
    // Three Pads of SIG in a row, 4 mm apart (x = -2 and 2 on the first Footprint, x = 6
    // on the second): the tree joins neighbours, never the far pair.
    two_pads(&mut engine, (0, 0), [Some(sig), Some(sig)]);
    two_pads(&mut engine, (8, 0), [Some(sig), None]);
    let lines = ratsnest(&mut engine);
    assert_eq!(lines.len(), 2, "{lines:?}");
    let spans: Vec<(i64, i64)> = lines
        .iter()
        .map(|l| {
            let a = l["from"]["x"].as_i64().unwrap();
            let b = l["to"]["x"].as_i64().unwrap();
            (a.min(b), a.max(b))
        })
        .collect();
    assert!(spans.contains(&(-2_000_000, 2_000_000)), "{spans:?}");
    assert!(spans.contains(&(2_000_000, 6_000_000)), "{spans:?}");
    assert!(lines.iter().all(|l| uuid(&l["net"]) == sig));

    route(&mut engine, "copper_top", &[(2, 0), (6, 0)]);
    assert_eq!(
        ratsnest(&mut engine).len(),
        1,
        "the routed connection drops out"
    );
    let only = engine.run_ok("pcb.ratsnest", json!({"nets": [sig.to_string()]}));
    assert_eq!(only["lines"].as_array().unwrap().len(), 1);

    route(
        &mut engine,
        "copper_top",
        &[(-2, 0), (-2, 3), (2, 3), (2, 0)],
    );
    assert!(ratsnest(&mut engine).is_empty());
}

#[test]
fn a_plated_through_hole_pad_joins_copper_on_both_sides() {
    let dir = TempDir::new("pth");
    let mut engine = engine_with_pcb(&dir);
    let sig = add_net(&mut engine, "SIG");
    engine.run_ok(
        "test.footprint.add",
        json!({"at": {"x": 0, "y": 0}, "pads": [
            {"at": {"x": 0, "y": 0}, "width": "1.6mm", "height": "1.6mm", "shape": "circle",
             "kind": "plated_through_hole", "net": sig.to_string()},
            {"at": {"x": "10mm", "y": 0}, "width": "1mm", "height": "1mm",
             "net": sig.to_string()}]}),
    );
    route(&mut engine, "copper_bottom", &[(0, 0), (0, 5), (5, 5)]);
    engine.run_ok("pcb.via.add", json!({"at": {"x": "5mm", "y": "5mm"}}));
    assert_eq!(ratsnest(&mut engine).len(), 1);
    route(&mut engine, "copper_top", &[(5, 5), (10, 5), (10, 0)]);
    assert!(ratsnest(&mut engine).is_empty());
}

#[test]
fn rotated_and_flipped_footprints_put_their_pads_where_they_land() {
    let dir = TempDir::new("flipped");
    let mut engine = engine_with_pcb(&dir);
    let sig = add_net(&mut engine, "SIG");
    // Turned a quarter turn, the Pad at (2, 0) lands at (10, 12); flipped to the bottom
    // side first, the Pad at (2, 0) lands at (-2, 0) + (20, 0), on the bottom copper.
    engine.run_ok(
        "test.footprint.add",
        json!({"at": {"x": "10mm", "y": "10mm"}, "rotation": "90deg", "pads": [
            {"at": {"x": "2mm", "y": 0}, "width": "1mm", "height": "1mm",
             "net": sig.to_string()}]}),
    );
    engine.run_ok(
        "test.footprint.add",
        json!({"at": {"x": "20mm", "y": 0}, "side": "bottom", "pads": [
            {"at": {"x": "2mm", "y": 0}, "width": "1mm", "height": "1mm",
             "net": sig.to_string()}]}),
    );
    let top = route(&mut engine, "copper_top", &[(10, 12), (10, 20)]);
    assert_eq!(uuid(&top["net"]), sig);
    let bottom = route(&mut engine, "copper_bottom", &[(18, 0), (18, 8)]);
    assert_eq!(uuid(&bottom["net"]), sig);
    let missed = route(&mut engine, "copper_top", &[(18, 0), (18, -8)]);
    assert!(missed.get("net").is_none());
}

fn clearance(engine: &mut TestEngine, params: Value) -> Value {
    engine.run_ok("pcb.clearance.track", params)
}

fn at_y(net: Uuid, y: &str) -> Value {
    json!({"layer": "copper_top", "net": net.to_string(),
           "from": {"x": 0, "y": y}, "to": {"x": "10mm", "y": y}})
}

#[test]
fn the_clearance_query_flags_copper_of_other_nets_too_close() {
    let dir = TempDir::new("clearance");
    let mut engine = engine_with_pcb(&dir);
    let a = add_net(&mut engine, "A");
    let b = add_net(&mut engine, "B");
    let existing = engine.run_ok(
        "pcb.track.add",
        json!({"layer": "copper_top", "net": a.to_string(),
               "points": [{"x": 0, "y": 0}, {"x": "10mm", "y": 0}]}),
    );
    let existing = uuids(&existing["result"]["tracks"])[0];

    // Generic rules: 0.2 mm Tracks, 0.16 mm clearance. Centrelines 0.4 mm apart leave a
    // 0.2 mm gap; 0.3 mm apart leave 0.1 mm.
    let clear = clearance(&mut engine, at_y(b, "0.4mm"));
    assert_eq!(clear, json!({"clear": true, "hits": []}));
    let close = clearance(&mut engine, at_y(b, "0.3mm"));
    assert_eq!(close["clear"], json!(false));
    assert_eq!(
        close["hits"],
        json!([{"item": {"kind": "track", "uuid": existing.to_string()},
                "layer": pcb(&engine).track(existing).unwrap().layer.to_string(),
                "net": a.to_string(), "required": 160_000, "actual": 100_000}])
    );
    assert_eq!(
        clearance(&mut engine, at_y(a, "0.3mm"))["clear"],
        json!(true),
        "copper of its own Net never conflicts"
    );
    let mut ignoring = at_y(b, "0.3mm");
    ignoring["ignore"] = json!([existing.to_string()]);
    assert_eq!(clearance(&mut engine, ignoring)["clear"], json!(true));
    let mut other_layer = at_y(b, "0.3mm");
    other_layer["layer"] = json!("copper_bottom");
    assert_eq!(clearance(&mut engine, other_layer)["clear"], json!(true));
}

#[test]
fn the_larger_net_class_clearance_wins() {
    let dir = TempDir::new("clearance-class");
    let mut engine = engine_with_pcb(&dir);
    let a = add_net(&mut engine, "HV");
    let b = add_net(&mut engine, "B");
    let hv = engine.run_ok(
        "pcb.net_class.add",
        json!({"name": "HV", "clearance": "0.5mm"}),
    );
    engine.run_ok(
        "pcb.net.set_class",
        json!({"nets": [a.to_string()], "class": hv["result"]["uuid"]}),
    );
    engine.run_ok(
        "pcb.track.add",
        json!({"layer": "copper_top", "net": b.to_string(),
               "points": [{"x": 0, "y": 0}, {"x": "10mm", "y": 0}]}),
    );
    let hit = clearance(&mut engine, at_y(a, "0.4mm"));
    assert_eq!(
        hit["hits"][0]["required"],
        json!(500_000),
        "the HV side asks more"
    );
    assert_eq!(hit["hits"][0]["actual"], json!(200_000));
    assert_eq!(
        clearance(&mut engine, at_y(b, "0.4mm"))["clear"],
        json!(true)
    );

    let via = engine.run_ok(
        "pcb.clearance.via",
        json!({"at": {"x": "5mm", "y": "0.6mm"}, "net": a.to_string()}),
    );
    // A 0.7 mm Via 0.6 mm off the Track's centre: a 0.15 mm gap, under HV's 0.5 mm.
    assert_eq!(via["hits"][0]["actual"], json!(150_000));
}

#[test]
fn deleting_a_segment_or_the_whole_track_up_to_pads_vias_and_branches() {
    let dir = TempDir::new("track-delete");
    let mut engine = engine_with_pcb(&dir);
    // A route of three segments from (0,0) to a Via at (30,0), a branch off its end at
    // (20,0), and a route on past the Via.
    let main = route(
        &mut engine,
        "copper_top",
        &[(0, 0), (10, 0), (20, 0), (30, 0)],
    );
    engine.run_ok("pcb.via.add", json!({"at": {"x": "30mm", "y": 0}}));
    let past = route(&mut engine, "copper_top", &[(30, 0), (40, 0)]);
    let branch = route(&mut engine, "copper_top", &[(20, 0), (20, 10)]);
    let main = uuids(&main["tracks"]);

    let out = engine.run_ok(
        "pcb.track.delete",
        json!({"tracks": [main[0].to_string()], "whole": true}),
    );
    let mut first_two = vec![main[0], main[1]];
    first_two.sort();
    assert_eq!(
        uuids(&out["result"]["deleted"]),
        first_two,
        "stops at the branch"
    );
    engine.run_ok(
        "edit.undo",
        json!({"document": pcb(&engine).uuid.to_string()}),
    );

    let out = engine.run_ok(
        "pcb.track.delete",
        json!({"tracks": [main[2].to_string()], "whole": true}),
    );
    assert_eq!(
        uuids(&out["result"]["deleted"]),
        vec![main[2]],
        "branch to Via"
    );
    assert!(pcb(&engine).track(uuids(&past["tracks"])[0]).is_some());
    assert!(pcb(&engine).track(uuids(&branch["tracks"])[0]).is_some());

    let out = engine.run_ok("pcb.track.delete", json!({"tracks": [main[1].to_string()]}));
    assert_eq!(uuids(&out["result"]["deleted"]), vec![main[1]]);
    assert!(pcb(&engine).track(main[0]).is_some());

    let via = pcb(&engine).vias().next().unwrap().uuid;
    engine.run_ok("pcb.via.delete", json!({"vias": [via.to_string()]}));
    assert!(pcb(&engine).via(via).is_none());
    let err = engine.run_err("pcb.via.delete", json!({"vias": [via.to_string()]}));
    assert_eq!(pointer(&err), "/vias/0");
}
