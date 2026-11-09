//! Editing routed copper (docs/spec/v0.1/pcb.md#dragging-tracks): sliding a Track
//! segment by the slide rules (or detached, or without angle keeping), dragging Track
//! ends, corners, Vias and Free Pads, breaking a segment, and deleting Tracks and Vias
//! together.

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

fn by(x: i64, y: i64) -> Value {
    json!({"x": format!("{x}mm"), "y": format!("{y}mm")})
}

fn drag(engine: &mut TestEngine, track: Uuid, to: (i64, i64), detach: bool) -> Value {
    drag_with(engine, track, to, json!({"detach": detach}))
}

/// `pcb.track.drag` with extra params.
fn drag_with(engine: &mut TestEngine, track: Uuid, to: (i64, i64), extra: Value) -> Value {
    let mut params = json!({"track": track.to_string(), "by": by(to.0, to.1)});
    for (k, v) in extra.as_object().unwrap() {
        params[k] = v.clone();
    }
    engine.run_ok("pcb.track.drag", params)["result"].clone()
}

fn added(out: &Value, old: &[Uuid]) -> Vec<Uuid> {
    uuids(&out["tracks"])
        .into_iter()
        .filter(|u| !old.contains(u))
        .collect()
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
fn a_segment_dragged_off_a_pad_or_via_stays_attached_by_a_square_connector_without_miters() {
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

    let out = drag_with(&mut engine, t[0], (0, 3), json!({"miter": false}));
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
    // The locked neighbour stays; a 45° connector joins it to the dragged end, as at a Pad.
    assert_eq!(line(&engine, t[0]), (mm(0, 0), mm(0, 10)));
    assert_eq!(line(&engine, t[1]), (mm(5, 15), mm(10, 15)));
    assert_eq!(line(&engine, t[2]), (mm(10, 15), mm(10, 0)));
    let added = added(&out, &t);
    assert_eq!(added.len(), 1);
    assert_eq!(line(&engine, added[0]), (mm(0, 10), mm(5, 15)));

    // The locked one itself refuses a drag.
    let err = engine.run_err(
        "pcb.track.drag",
        json!({"track": t[0], "by": {"x": "1mm", "y": 0}}),
    );
    assert_eq!(err.code, ErrorCode::Rejected);
}

fn pt(value: &Value) -> Point {
    Point::new(
        Nm(value["x"].as_i64().unwrap()),
        Nm(value["y"].as_i64().unwrap()),
    )
}

fn via_at(engine: &mut TestEngine, x: i64, y: i64) -> Uuid {
    uuid(
        &engine.run_ok(
            "pcb.via.add",
            json!({"at": {"x": format!("{x}mm"), "y": format!("{y}mm")}}),
        )["result"]["uuid"],
    )
}

fn pad_at(engine: &mut TestEngine, x: i64, y: i64) {
    engine.run_ok(
        "test.footprint.add",
        json!({"at": {"x": format!("{x}mm"), "y": format!("{y}mm")}, "pads": [
            {"at": {"x": 0, "y": 0}, "width": "1mm", "height": "1mm"}
        ]}),
    );
}

fn drag_end(
    engine: &mut TestEngine,
    track: Uuid,
    end: &str,
    to: (i64, i64),
    extra: Value,
) -> Value {
    let mut params = json!({"track": track, "end": end, "by": by(to.0, to.1)});
    for (k, v) in extra.as_object().unwrap() {
        params[k] = v.clone();
    }
    engine.run_ok("pcb.track.drag_end", params)["result"].clone()
}

#[test]
fn a_slide_keeps_its_neighbours_angles_and_moves_only_across_the_segment() {
    let dir = TempDir::new("track-slide-45");
    let mut engine = engine_with_pcb(&dir);
    // Up at 45°, across, down at 45°.
    let t = route(&mut engine, &[(0, 0), (10, 10), (20, 10), (30, 0)]);

    // The part of the drag along the segment is dropped.
    let out = drag(&mut engine, t[1], (3, 2), false);
    assert_eq!(pt(&out["by"]), mm(0, 2));
    assert_eq!(line(&engine, t[0]), (mm(0, 0), mm(12, 12)));
    assert_eq!(line(&engine, t[1]), (mm(12, 12), mm(18, 12)));
    assert_eq!(line(&engine, t[2]), (mm(18, 12), mm(30, 0)));
}

#[test]
fn without_preserve_angle_the_segment_moves_freely_and_neighbours_turn() {
    let dir = TempDir::new("track-slide-free");
    let mut engine = engine_with_pcb(&dir);
    let t = route(&mut engine, &[(0, 0), (0, 10), (10, 10), (10, 0)]);

    let out = drag_with(&mut engine, t[1], (3, 5), json!({"preserve_angle": false}));
    assert_eq!(pt(&out["by"]), mm(3, 5));
    assert_eq!(line(&engine, t[0]), (mm(0, 0), mm(3, 15)));
    assert_eq!(line(&engine, t[1]), (mm(3, 15), mm(13, 15)));
    assert_eq!(line(&engine, t[2]), (mm(13, 15), mm(10, 0)));
}

#[test]
fn dragging_past_a_neighbours_end_stops_there_and_merges_the_segment_beyond() {
    let dir = TempDir::new("track-slide-merge");
    let mut engine = engine_with_pcb(&dir);
    // A step: across, up 5, across again.
    let t = route(&mut engine, &[(0, 0), (10, 0), (10, 5), (20, 5)]);

    let out = drag(&mut engine, t[0], (0, 8), false);
    assert_eq!(
        pt(&out["by"]),
        mm(0, 5),
        "the drag stops where the upright runs out"
    );
    let mut gone = vec![t[1], t[2]];
    gone.sort();
    assert_eq!(uuids(&out["deleted"]), gone);
    assert_eq!(line(&engine, t[0]), (mm(0, 5), mm(20, 5)));
    assert_eq!(pcb(&engine).tracks().count(), 1, "no new corner is built");

    // Merge Collinear off: the upright still goes and the drag still stops, but the two
    // segments stay apart.
    let doc = pcb(&engine).uuid.to_string();
    engine.run_ok("edit.undo", json!({"document": doc}));
    let out = drag_with(&mut engine, t[0], (0, 8), json!({"merge_collinear": false}));
    assert_eq!(uuids(&out["deleted"]), vec![t[1]]);
    assert_eq!(line(&engine, t[0]), (mm(0, 5), mm(10, 5)));
    assert_eq!(line(&engine, t[2]), (mm(10, 5), mm(20, 5)));
}

#[test]
fn a_fixed_end_gets_a_45_degree_connector_with_miters_and_a_square_one_without() {
    let dir = TempDir::new("track-slide-miter");
    let mut engine = engine_with_pcb(&dir);
    // Two segments in line: the second is a parallel neighbour, which can't follow.
    let t = route(&mut engine, &[(0, 0), (10, 0), (20, 0)]);

    let out = drag(&mut engine, t[0], (0, 2), false);
    let c = added(&out, &t);
    assert_eq!(c.len(), 1);
    assert_eq!(line(&engine, c[0]), (mm(10, 0), mm(8, 2)));
    assert_eq!(line(&engine, t[0]), (mm(0, 2), mm(8, 2)));
    assert_eq!(line(&engine, t[1]), (mm(10, 0), mm(20, 0)));

    let doc = pcb(&engine).uuid.to_string();
    engine.run_ok("edit.undo", json!({"document": doc}));
    let out = drag_with(&mut engine, t[0], (0, 2), json!({"miter": false}));
    let c = added(&out, &t);
    assert_eq!(line(&engine, c[0]), (mm(10, 0), mm(10, 2)));
    assert_eq!(line(&engine, t[0]), (mm(0, 2), mm(10, 2)));
}

#[test]
fn a_branch_is_a_fixed_end() {
    let dir = TempDir::new("track-slide-branch");
    let mut engine = engine_with_pcb(&dir);
    let t = route(&mut engine, &[(0, 0), (10, 0), (20, 0)]);
    let up = route(&mut engine, &[(10, 0), (10, 10)]);
    let out = drag_with(&mut engine, t[0], (0, -2), json!({"miter": false}));
    let c = added(&out, &t);
    assert_eq!(line(&engine, c[0]), (mm(10, 0), mm(10, -2)));
    assert_eq!(line(&engine, up[0]), (mm(10, 0), mm(10, 10)));
}

#[test]
fn a_free_end_or_one_on_a_footprint_pad_moves_alone() {
    let dir = TempDir::new("track-end-free");
    let mut engine = engine_with_pcb(&dir);
    pad_at(&mut engine, 0, 0);
    let t = route(&mut engine, &[(0, 0), (10, 0), (10, 10)]);

    let out = drag_end(&mut engine, t[1], "to", (2, 0), json!({}));
    assert_eq!(uuids(&out["tracks"]), vec![t[1]]);
    assert_eq!(line(&engine, t[1]), (mm(10, 0), mm(12, 10)));

    // On a Footprint Pad the end detaches and leaves the Pad.
    drag_end(&mut engine, t[0], "from", (0, 3), json!({}));
    assert_eq!(line(&engine, t[0]), (mm(0, 3), mm(10, 0)));
    assert_eq!(pcb(&engine).tracks().count(), 2);
}

#[test]
fn a_shared_vertex_moves_both_segments_and_merges_when_they_line_up() {
    let dir = TempDir::new("track-end-deform");
    let mut engine = engine_with_pcb(&dir);
    let t = route(&mut engine, &[(0, 0), (10, 0), (10, 10)]);

    drag_end(&mut engine, t[0], "to", (0, -2), json!({}));
    assert_eq!(line(&engine, t[0]), (mm(0, 0), mm(10, -2)));
    assert_eq!(line(&engine, t[1]), (mm(10, -2), mm(10, 10)));

    // Onto the line between the far ends: one straight segment is left.
    let doc = pcb(&engine).uuid.to_string();
    engine.run_ok("edit.undo", json!({"document": doc}));
    let out = drag_end(&mut engine, t[0], "to", (-5, 5), json!({}));
    assert_eq!(uuids(&out["deleted"]), vec![t[1]]);
    assert_eq!(line(&engine, t[0]), (mm(0, 0), mm(10, 10)));
}

#[test]
fn a_scale_corner_keeps_its_shape_and_its_far_neighbours_angles() {
    let dir = TempDir::new("track-end-scale");
    let mut engine = engine_with_pcb(&dir);
    // Across, up, across, up: drag the corner between the middle two.
    let t = route(
        &mut engine,
        &[(0, 0), (10, 0), (10, 10), (20, 10), (20, 20)],
    );

    let out = drag_end(&mut engine, t[1], "to", (2, 3), json!({"corner": "scale"}));
    assert_eq!(pt(&out["by"]), mm(2, 3));
    assert_eq!(line(&engine, t[0]), (mm(0, 0), mm(12, 0)));
    assert_eq!(line(&engine, t[1]), (mm(12, 0), mm(12, 13)));
    assert_eq!(line(&engine, t[2]), (mm(12, 13), mm(20, 13)));
    assert_eq!(line(&engine, t[3]), (mm(20, 13), mm(20, 20)));
}

#[test]
fn a_t_junction_end_is_refused() {
    let dir = TempDir::new("track-end-tee");
    let mut engine = engine_with_pcb(&dir);
    let t = route(&mut engine, &[(0, 0), (10, 0), (20, 0)]);
    route(&mut engine, &[(10, 0), (10, 10)]);
    let err = engine.run_err(
        "pcb.track.drag_end",
        json!({"track": t[0], "end": "to", "by": by(0, 1)}),
    );
    assert_eq!(err.code, ErrorCode::Rejected);
}

#[test]
fn an_end_on_a_via_drags_the_via_with_all_its_tracks() {
    let dir = TempDir::new("track-end-via");
    let mut engine = engine_with_pcb(&dir);
    pad_at(&mut engine, 0, 0);
    let via = via_at(&mut engine, 10, 0);
    let a = route(&mut engine, &[(0, 0), (10, 0)]);
    let b = route(&mut engine, &[(10, 0), (10, 10)]);

    let out = drag_end(&mut engine, a[0], "to", (0, 2), json!({}));
    assert_eq!(uuids(&out["moved"]), vec![via]);
    assert_eq!(pcb(&engine).via(via).unwrap().at, mm(10, 2));
    // Both Tracks stay on the Via; the one from the Pad keeps its angle behind a miter.
    assert_eq!(line(&engine, a[0]), (mm(2, 2), mm(10, 2)));
    assert_eq!(line(&engine, b[0]), (mm(10, 2), mm(10, 12)));
    let c = added(&out, &[a[0], b[0]]);
    assert_eq!(line(&engine, c[0]), (mm(0, 0), mm(2, 2)));
}

#[test]
fn a_via_or_free_pad_drag_keeps_every_track_attached() {
    let dir = TempDir::new("copper-drag");
    let mut engine = engine_with_pcb(&dir);
    pad_at(&mut engine, 0, 0);
    let via = via_at(&mut engine, 10, 0);
    let t = route(&mut engine, &[(0, 0), (10, 0)]);

    // Without angle keeping the Track's end follows and it turns.
    engine.run_ok(
        "pcb.copper.drag",
        json!({"object": via, "by": by(0, 2), "preserve_angle": false}),
    );
    assert_eq!(pcb(&engine).via(via).unwrap().at, mm(10, 2));
    assert_eq!(line(&engine, t[0]), (mm(0, 0), mm(10, 2)));

    // Detached it moves alone.
    engine.run_ok(
        "pcb.copper.drag",
        json!({"object": via, "by": by(5, 0), "detach": true}),
    );
    assert_eq!(pcb(&engine).via(via).unwrap().at, mm(15, 2));
    assert_eq!(line(&engine, t[0]), (mm(0, 0), mm(10, 2)));

    let pad = uuid(
        &engine.run_ok(
            "pcb.free_pad.add",
            json!({"at": {"x": "30mm", "y": 0}, "kind": "smd", "width": "1mm"}),
        )["result"]["uuid"],
    );
    let s = route(&mut engine, &[(30, 0), (30, 10)]);
    let out = engine.run_ok("pcb.copper.drag", json!({"object": pad, "by": by(3, 0)}));
    assert_eq!(uuids(&out["result"]["moved"]), vec![pad]);
    assert_eq!(pcb(&engine).free_pad(pad).unwrap().at, mm(33, 0));
    assert_eq!(line(&engine, s[0]), (mm(33, 0), mm(33, 10)));

    // A locked Track on it refuses the drag.
    engine.run_ok("pcb.lock", json!({"objects": [s[0]], "locked": true}));
    let err = engine.run_err("pcb.copper.drag", json!({"object": pad, "by": by(3, 0)}));
    assert_eq!(err.code, ErrorCode::Rejected);
}

#[test]
fn a_break_adds_a_vertex_and_can_drag_it_in_one_step() {
    let dir = TempDir::new("track-break");
    let mut engine = engine_with_pcb(&dir);
    let t = route(&mut engine, &[(0, 0), (10, 0)]);

    // The nearest point on the segment.
    let out = engine.run_ok("pcb.track.break", json!({"track": t[0], "at": by(4, 1)}));
    let halves = uuids(&out["result"]["tracks"]);
    assert_eq!(halves.len(), 2);
    let second = *halves.iter().find(|u| **u != t[0]).unwrap();
    assert_eq!(line(&engine, t[0]), (mm(0, 0), mm(4, 0)));
    assert_eq!(line(&engine, second), (mm(4, 0), mm(10, 0)));
    let doc = pcb(&engine).uuid.to_string();
    engine.run_ok("edit.undo", json!({"document": doc}));
    assert_eq!(pcb(&engine).tracks().count(), 1);

    let out = engine.run_ok(
        "pcb.track.break",
        json!({"track": t[0], "at": by(5, 0), "by": by(0, 3)}),
    );
    let second = *uuids(&out["result"]["tracks"])
        .iter()
        .find(|u| **u != t[0])
        .unwrap();
    assert_eq!(line(&engine, t[0]), (mm(0, 0), mm(5, 3)));
    assert_eq!(line(&engine, second), (mm(5, 3), mm(10, 0)));

    let err = engine.run_err("pcb.track.break", json!({"track": t[0], "at": by(0, 0)}));
    assert_eq!(err.code, ErrorCode::InvalidParams);
}
