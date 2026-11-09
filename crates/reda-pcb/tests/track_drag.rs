//! Editing routed copper: dragging a Track segment (neighbours follow, or it detaches) and
//! deleting Tracks and Vias together.

use reda_command::ErrorCode;
use reda_model::geom::{Nm, Point};
use reda_model::{PcbDocument, TrackGeometry, Uuid};
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

fn mm(x: i64, y: i64) -> Point {
    Point::new(Nm::from_mm(x), Nm::from_mm(y))
}

fn route(engine: &mut TestEngine, points: &[(i64, i64)]) -> Vec<Uuid> {
    let points: Vec<Value> = points
        .iter()
        .map(|(x, y)| json!({"x": format!("{x}mm"), "y": format!("{y}mm")}))
        .collect();
    let out = engine.run_ok(
        "pcb.track.add",
        json!({"layer": "copper_top", "points": points}),
    );
    uuids(&out["result"]["tracks"])
}

fn line(engine: &TestEngine, track: Uuid) -> (Point, Point) {
    pcb(engine).track(track).unwrap().geometry.ends()
}

fn drag(engine: &mut TestEngine, track: Uuid, by: (i64, i64), detach: bool) -> Value {
    engine.run_ok(
        "pcb.track.drag",
        json!({"track": track.to_string(),
               "by": {"x": format!("{}mm", by.0), "y": format!("{}mm", by.1)},
               "detach": detach}),
    )["result"]
        .clone()
}

#[test]
fn dragging_a_segment_stretches_its_neighbours_with_it() {
    let dir = TempDir::new("track-drag");
    let mut engine = engine_with_pcb(&dir);
    // A U: up, across, down.
    let t = route(&mut engine, &[(0, 0), (0, 10), (10, 10), (10, 0)]);

    let out = drag(&mut engine, t[1], (0, 5), false);
    assert_eq!(line(&engine, t[0]), (mm(0, 0), mm(0, 15)));
    assert_eq!(line(&engine, t[1]), (mm(0, 15), mm(10, 15)));
    assert_eq!(line(&engine, t[2]), (mm(10, 15), mm(10, 0)));
    let mut changed = t.clone();
    changed.sort();
    assert_eq!(uuids(&out["tracks"]), changed);
    assert!(uuids(&out["deleted"]).is_empty());

    // One undo entry puts all three back.
    let doc = pcb(&engine).uuid.to_string();
    engine.run_ok("edit.undo", json!({"document": doc}));
    assert_eq!(line(&engine, t[0]), (mm(0, 0), mm(0, 10)));
    assert_eq!(line(&engine, t[1]), (mm(0, 10), mm(10, 10)));
}

#[test]
fn mod_drag_detaches_the_segment() {
    let dir = TempDir::new("track-detach");
    let mut engine = engine_with_pcb(&dir);
    let t = route(&mut engine, &[(0, 0), (0, 10), (10, 10), (10, 0)]);

    let out = drag(&mut engine, t[1], (0, 5), true);
    assert_eq!(uuids(&out["tracks"]), vec![t[1]]);
    assert_eq!(line(&engine, t[0]), (mm(0, 0), mm(0, 10)));
    assert_eq!(line(&engine, t[1]), (mm(0, 15), mm(10, 15)));
}

#[test]
fn a_segment_dragged_off_a_pad_or_via_stays_attached_by_a_new_segment() {
    let dir = TempDir::new("track-bridge");
    let mut engine = engine_with_pcb(&dir);
    let net = uuid(&engine.run_ok("test.net.add", json!({"name": "SIG"}))["result"]["uuid"]);
    engine.run_ok(
        "test.footprint.add",
        json!({"at": {"x": 0, "y": 0}, "pads": [
            {"at": {"x": 0, "y": 0}, "width": "1mm", "height": "1mm", "net": net.to_string()}
        ]}),
    );
    engine.run_ok(
        "pcb.via.add",
        json!({"at": {"x": "10mm", "y": 0}, "net": net.to_string()}),
    );
    let t = route(&mut engine, &[(0, 0), (10, 0)]);

    let out = drag(&mut engine, t[0], (0, 3), false);
    assert_eq!(line(&engine, t[0]), (mm(0, 3), mm(10, 3)));
    let created: Vec<Uuid> = uuids(&out["tracks"])
        .into_iter()
        .filter(|u| *u != t[0])
        .collect();
    assert_eq!(created.len(), 2, "one bridge at each end");
    let mut bridges: Vec<(Point, Point)> = created.iter().map(|u| line(&engine, *u)).collect();
    bridges.sort_by_key(|(a, _)| a.x);
    assert_eq!(bridges, [(mm(0, 0), mm(0, 3)), (mm(10, 0), mm(10, 3))]);
    for u in created {
        let track = pcb(&engine).track(u).unwrap();
        assert_eq!(track.net, Some(net));
        assert_eq!(track.width, pcb(&engine).track(t[0]).unwrap().width);
    }
}

#[test]
fn a_neighbour_squeezed_to_nothing_is_deleted() {
    let dir = TempDir::new("track-squeeze");
    let mut engine = engine_with_pcb(&dir);
    // Across, then up: dragging the upright left by its whole length folds the first away.
    let t = route(&mut engine, &[(0, 0), (10, 0), (10, 10)]);
    let out = drag(&mut engine, t[1], (-10, 0), false);
    assert_eq!(uuids(&out["deleted"]), vec![t[0]]);
    assert!(pcb(&engine).track(t[0]).is_none());
    assert_eq!(
        pcb(&engine).track(t[1]).unwrap().geometry,
        TrackGeometry::Line {
            from: mm(0, 0),
            to: mm(0, 10)
        }
    );
}

#[test]
fn a_drag_needs_a_track_and_a_distance() {
    let dir = TempDir::new("track-drag-refused");
    let mut engine = engine_with_pcb(&dir);
    let t = route(&mut engine, &[(0, 0), (10, 0)]);
    let err = engine.run_err(
        "pcb.track.drag",
        json!({"track": t[0].to_string(), "by": {"x": 0, "y": 0}}),
    );
    assert_eq!(err.code, ErrorCode::InvalidParams);
    let err = engine.run_err(
        "pcb.track.drag",
        json!({"track": Uuid::v4_from_bits(7).to_string(), "by": {"x": 1, "y": 0}}),
    );
    assert_eq!(err.code, ErrorCode::InvalidParams);
}

#[test]
fn deleting_copper_takes_tracks_and_vias_together_or_whole_tracks() {
    let dir = TempDir::new("copper-delete");
    let mut engine = engine_with_pcb(&dir);
    let t = route(&mut engine, &[(0, 0), (10, 0), (20, 0)]);
    let via = uuid(
        &engine.run_ok("pcb.via.add", json!({"at": {"x": "30mm", "y": "5mm"}}))["result"]["uuid"],
    );

    let out = engine.run_ok(
        "pcb.copper.delete",
        json!({"objects": [t[0].to_string(), via.to_string()]}),
    );
    let mut both = vec![t[0], via];
    both.sort();
    assert_eq!(uuids(&out["result"]["deleted"]), both);
    assert!(pcb(&engine).track(t[1]).is_some());
    let doc = pcb(&engine).uuid.to_string();
    engine.run_ok("edit.undo", json!({"document": doc}));

    let out = engine.run_ok(
        "pcb.copper.delete",
        json!({"objects": [t[1].to_string()], "whole": true}),
    );
    let mut whole = t.clone();
    whole.sort();
    assert_eq!(uuids(&out["result"]["deleted"]), whole);

    let footprint = engine.run_ok(
        "test.footprint.add",
        json!({"at": {"x": 0, "y": 0}, "pads": [{"at": {"x": 0, "y": 0}, "width": "1mm", "height": "1mm"}]}),
    )["result"]["footprint"]
        .clone();
    let err = engine.run_err("pcb.copper.delete", json!({"objects": [footprint]}));
    assert_eq!(err.code, ErrorCode::InvalidParams);
}

#[test]
fn a_drag_stops_at_a_locked_neighbour() {
    let dir = TempDir::new("track-drag-locked");
    let mut engine = engine_with_pcb(&dir);
    let t = route(&mut engine, &[(0, 0), (0, 10), (10, 10), (10, 0)]);
    engine.run_ok("pcb.lock", json!({"objects": [t[0]], "locked": true}));

    let out = drag(&mut engine, t[1], (0, 5), false);
    // The locked neighbour stays; a new segment joins it to the dragged end, as at a Pad.
    assert_eq!(line(&engine, t[0]), (mm(0, 0), mm(0, 10)));
    assert_eq!(line(&engine, t[1]), (mm(0, 15), mm(10, 15)));
    assert_eq!(line(&engine, t[2]), (mm(10, 15), mm(10, 0)));
    let added: Vec<Uuid> = uuids(&out["tracks"])
        .into_iter()
        .filter(|u| !t.contains(u))
        .collect();
    assert_eq!(added.len(), 1);
    assert_eq!(line(&engine, added[0]), (mm(0, 10), mm(0, 15)));

    // The locked one itself refuses a drag.
    let err = engine.run_err(
        "pcb.track.drag",
        json!({"track": t[0], "by": {"x": "1mm", "y": 0}}),
    );
    assert_eq!(err.code, ErrorCode::Rejected);
}
