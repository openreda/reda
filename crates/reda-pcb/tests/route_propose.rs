//! `propose()`: the Track Tool's preview as one pure function of the Copper index, tested
//! with no window. The board is a new Project's, with the clearance set to 0.2 mm and
//! Tracks 0.2 mm wide, so a Track's centreline keeps 0.3 mm from other-Net copper.

use reda_model::geom::{Nm, Point};
use reda_model::{LayerKind, PcbDocument, Side, Uuid};
use reda_pcb::Copper;
use reda_pcb::route::{
    CornerMode, Note, ObstacleMode, Posture, Request, Status, corner_path, propose,
};
use reda_testkit::{TempDir, TestEngine, probe_engine};
use serde_json::{Value, json};

fn engine(dir: &TempDir) -> TestEngine {
    let mut engine = probe_engine();
    engine.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
    engine.run_ok(
        "pcb.rules.set",
        json!({"kind": "clearance", "values": {"value": "0.2mm"}}),
    );
    engine
}

fn uuid(value: &Value) -> Uuid {
    value.as_str().unwrap().parse().unwrap()
}

fn net(engine: &mut TestEngine, name: &str) -> Uuid {
    uuid(&engine.run_ok("test.net.add", json!({"name": name}))["result"]["uuid"])
}

fn pcb(engine: &TestEngine) -> &PcbDocument {
    engine.project().unwrap().pcb().unwrap()
}

fn copper(engine: &TestEngine) -> std::sync::Arc<Copper> {
    Copper::of(engine.project().unwrap()).unwrap()
}

fn top(engine: &TestEngine) -> Uuid {
    pcb(engine)
        .layers()
        .find(LayerKind::Copper, Some(Side::Top))
        .unwrap()
        .uuid
}

fn mm(x: f64, y: f64) -> Point {
    Point::new(Nm((x * 1e6) as i64), Nm((y * 1e6) as i64))
}

/// A 1 mm square top Pad at (`x`, `y`) mm on `net`.
fn pad(engine: &mut TestEngine, x: f64, y: f64, net: Uuid) {
    engine.run_ok(
        "test.footprint.add",
        json!({"at": {"x": format!("{x}mm"), "y": format!("{y}mm")}, "designator": "P",
               "pads": [{"at": {"x": 0, "y": 0}, "width": "1mm", "height": "1mm", "net": net}]}),
    );
}

fn request(engine: &TestEngine, start: Point, cursor: Point, net: Option<Uuid>) -> Request {
    Request::new(start, cursor, top(engine), Nm::from_um(200), net)
}

#[test]
fn a_clear_path_proposes_its_first_segment_and_looks_ahead_at_the_second() {
    let dir = TempDir::new("propose-clear");
    let engine = engine(&dir);
    let r = request(&engine, mm(0.0, 0.0), mm(3.0, 1.0), None);
    let out = propose(&copper(&engine), pcb(&engine), &r);
    assert_eq!(out.place, [mm(0.0, 0.0), mm(2.0, 0.0)]);
    assert_eq!(out.look_ahead, [mm(2.0, 0.0), mm(3.0, 1.0)]);
    assert!(out.blocked.is_empty());
    assert_eq!(out.status, Status::Clear);
    assert!(!out.ends_route);
    assert!(out.removes.is_empty());
    assert_eq!(out.note, None);
    assert!(out.obstacles.is_empty());

    let off = Request {
        lookahead: false,
        ..r
    };
    let out = propose(&copper(&engine), pcb(&engine), &off);
    assert_eq!(out.place, [mm(0.0, 0.0), mm(2.0, 0.0), mm(3.0, 1.0)]);
    assert!(out.look_ahead.is_empty());
}

#[test]
fn corner_modes_and_postures_shape_the_path() {
    let (a, b) = (mm(0.0, 0.0), mm(3.0, 1.0));
    let path = |c, p| corner_path(a, b, c, p);
    assert_eq!(
        path(CornerMode::Deg45, Posture::Default),
        [a, mm(2.0, 0.0), b]
    );
    assert_eq!(
        path(CornerMode::Deg45, Posture::Flipped),
        [a, mm(1.0, 1.0), b]
    );
    assert_eq!(
        path(CornerMode::Deg90, Posture::Default),
        [a, mm(3.0, 0.0), b]
    );
    assert_eq!(
        path(CornerMode::Deg90, Posture::Flipped),
        [a, mm(0.0, 1.0), b]
    );
    assert_eq!(path(CornerMode::Any, Posture::Flipped), [a, b]);
    assert_eq!(CornerMode::Deg45.next(), CornerMode::Deg90);
    assert_eq!(CornerMode::Deg90.next(), CornerMode::Any);
    assert_eq!(CornerMode::Any.next(), CornerMode::Deg45);
}

#[test]
fn reaching_same_net_copper_ends_the_route_and_places_both_segments() {
    let dir = TempDir::new("propose-ends");
    let mut engine = engine(&dir);
    let sig = net(&mut engine, "SIG");
    pad(&mut engine, 0.0, 0.0, sig);
    pad(&mut engine, 3.0, 1.0, sig);
    let r = request(&engine, mm(0.0, 0.0), mm(3.0, 1.0), Some(sig));
    let out = propose(&copper(&engine), pcb(&engine), &r);
    assert!(out.ends_route);
    assert_eq!(out.place, [mm(0.0, 0.0), mm(2.0, 0.0), mm(3.0, 1.0)]);
    assert!(out.look_ahead.is_empty(), "a click that ends places both");

    let manual = Request {
        auto_terminate: false,
        ..r
    };
    let out = propose(&copper(&engine), pcb(&engine), &manual);
    assert!(!out.ends_route, "auto-terminate off");
}

#[test]
fn stop_clips_at_the_last_clear_point_and_keeps_it_off_grid() {
    let dir = TempDir::new("propose-stop");
    let mut engine = engine(&dir);
    let (sig, other) = (net(&mut engine, "SIG"), net(&mut engine, "OTHER"));
    pad(&mut engine, 5.0, 0.0, other);
    let r = request(&engine, mm(0.0, 0.0), mm(10.0, 0.0), Some(sig));
    let out = propose(&copper(&engine), pcb(&engine), &r);
    assert_eq!(out.status, Status::Clipped);
    // The Pad's edge is at 4.5 mm; 0.2 mm clearance and a 0.1 mm half width: 4.2 mm.
    let clip = out.place[1];
    assert_eq!(clip.y, Nm::ZERO);
    assert!(
        (clip.x.0 - 4_200_000).abs() <= 2,
        "clipped at {} nm, off the 0.5 mm grid",
        clip.x.0
    );
    assert_eq!(out.blocked, [clip, mm(10.0, 0.0)]);
    assert_eq!(out.obstacles.copper.len(), 1, "names what stopped it");
    assert!(!out.ends_route);
}

#[test]
fn ignore_goes_anywhere_marked_violating_and_highlight_names_the_obstacles() {
    let dir = TempDir::new("propose-ignore");
    let mut engine = engine(&dir);
    let (sig, other) = (net(&mut engine, "SIG"), net(&mut engine, "OTHER"));
    pad(&mut engine, 5.0, 0.0, other);
    let r = Request {
        mode: ObstacleMode::Ignore,
        ..request(&engine, mm(0.0, 0.0), mm(10.0, 0.0), Some(sig))
    };
    let out = propose(&copper(&engine), pcb(&engine), &r);
    assert_eq!(out.place, [mm(0.0, 0.0), mm(10.0, 0.0)]);
    assert_eq!(out.status, Status::Violating);
    assert!(out.blocked.is_empty());
    assert!(out.obstacles.is_empty(), "ignore names nothing");

    let highlight = Request {
        mode: ObstacleMode::Highlight,
        ..r
    };
    let out = propose(&copper(&engine), pcb(&engine), &highlight);
    assert_eq!(out.place, [mm(0.0, 0.0), mm(10.0, 0.0)]);
    assert_eq!(out.status, Status::Violating);
    assert_eq!(out.obstacles.copper.len(), 1);
    assert_eq!(out.obstacles.copper[0].net, Some(other));
    assert_eq!(ObstacleMode::Stop.next(), ObstacleMode::Ignore);
    assert_eq!(ObstacleMode::Ignore.next(), ObstacleMode::Highlight);
    assert_eq!(ObstacleMode::Highlight.next(), ObstacleMode::Stop);
}

#[test]
fn the_obstacle_modes_have_the_spec_names() {
    for (mode, name) in [
        (ObstacleMode::Ignore, "ignore"),
        (ObstacleMode::Stop, "stop"),
        (ObstacleMode::Highlight, "highlight"),
    ] {
        assert_eq!(serde_json::to_value(mode).unwrap(), json!(name));
    }
    assert_eq!(ObstacleMode::default(), ObstacleMode::Stop);
}

#[test]
fn keepouts_and_the_board_edge_stop_a_route_too() {
    let dir = TempDir::new("propose-edge");
    let mut engine = engine(&dir);
    engine.run_ok(
        "pcb.board_outline.add_rect",
        json!({"width": "20mm", "height": "20mm"}),
    );
    let r = request(&engine, mm(5.0, 5.0), mm(25.0, 5.0), None);
    let out = propose(&copper(&engine), pcb(&engine), &r);
    assert_eq!(out.status, Status::Clipped);
    assert_eq!(out.obstacles.edges.len(), 1, "the Board Outline");
    assert!(out.place[1].x < mm(20.0, 0.0).x);

    engine.run_ok(
        "pcb.keepout.add",
        json!({"layer": "copper_top", "vertices": [{"x": "8mm", "y": "8mm"}, {"x": "12mm", "y": "8mm"},
                            {"x": "12mm", "y": "12mm"}, {"x": "8mm", "y": "12mm"}]}),
    );
    let r = request(&engine, mm(5.0, 10.0), mm(15.0, 10.0), None);
    let out = propose(&copper(&engine), pcb(&engine), &r);
    assert_eq!(out.status, Status::Clipped);
    assert_eq!(out.obstacles.keepouts.len(), 1);
}

#[test]
fn the_net_tie_rule_is_honoured() {
    let dir = TempDir::new("propose-tie");
    let mut engine = engine(&dir);
    let (a, b) = (net(&mut engine, "A"), net(&mut engine, "B"));
    // A Net Tie: Pad 1 on A at x = -0.25 mm and Pad 2 on B at x = 0.25 mm, touching.
    let tie_pad = |x: &str, net: Uuid| json!({"at": {"x": x, "y": 0}, "width": "0.5mm", "height": "0.5mm", "net": net});
    engine.run_ok(
        "test.footprint.add",
        json!({"at": {"x": 0, "y": 0}, "designator": "NT1", "component_type": "net_tie",
               "pads": [tie_pad("-0.25mm", a), tie_pad("0.25mm", b)]}),
    );
    // A Track on A passing 0.1 mm from Pad 2 (on B) is clear: a tie's other Pads need
    // no clearance from its Nets' copper.
    let r = request(&engine, mm(-0.25, 0.0), mm(-0.25, 3.0), Some(a));
    let near = Request {
        start: mm(0.65, -3.0),
        cursor: mm(0.65, 3.0),
        ..r.clone()
    };
    let out = propose(&copper(&engine), pcb(&engine), &near);
    assert_eq!(out.status, Status::Clear, "{:?}", out.obstacles);
    // Another Net there keeps the normal clearance.
    let c = net(&mut engine, "C");
    let other = Request {
        net: Some(c),
        ..near.clone()
    };
    let out = propose(&copper(&engine), pcb(&engine), &other);
    assert_eq!(out.status, Status::Clipped);
    // Touching Pad 2 is still a short.
    let touching = Request {
        start: mm(0.4, -3.0),
        cursor: mm(0.4, 3.0),
        ..near
    };
    let out = propose(&copper(&engine), pcb(&engine), &touching);
    assert_eq!(out.status, Status::Clipped);
}

#[test]
fn a_cursor_in_a_zones_only_zone_of_the_net_says_it_is_not_connected() {
    let dir = TempDir::new("propose-zone");
    let mut engine = engine(&dir);
    let gnd = net(&mut engine, "GND");
    engine.run_ok(
        "pcb.board_outline.add_rect",
        json!({"width": "30mm", "height": "30mm"}),
    );
    engine.run_ok(
        "pcb.zone.add",
        json!({"layer": "copper_top", "net": gnd,
               "vertices": [{"x": "10mm", "y": "5mm"}, {"x": "20mm", "y": "5mm"},
                            {"x": "20mm", "y": "15mm"}, {"x": "10mm", "y": "15mm"}]}),
    );
    // A Pad of the Net keeps the island from being removed as unconnected.
    pad(&mut engine, 18.0, 10.0, gnd);
    engine.run_ok("pcb.zone.fill", json!({}));
    let r = request(&engine, mm(2.0, 10.0), mm(15.0, 10.0), Some(gnd));
    let out = propose(&copper(&engine), pcb(&engine), &r);
    assert!(!out.ends_route, "Zones don't end a route");
    assert_eq!(out.note, Some(Note::ZoneNotConnected));
}
