//! Clearing and tidying copper: `pcb.route.unroute` and `pcb.track.cleanup`.

use reda_command::ErrorCode;
use reda_model::geom::{Nm, Point};
use reda_model::{PcbDocument, TrackGeometry, Uuid};
use reda_testkit::{TempDir, TestEngine, probe_engine};
use serde_json::{Value, json};

fn uuid(value: &Value) -> Uuid {
    value.as_str().unwrap().parse().unwrap()
}

fn uuids(value: &Value) -> Vec<Uuid> {
    value.as_array().unwrap().iter().map(uuid).collect()
}

fn mm(x: f64, y: f64) -> Point {
    Point::new(Nm((x * 1e6) as i64), Nm((y * 1e6) as i64))
}

fn pcb(engine: &TestEngine) -> &PcbDocument {
    engine.project().unwrap().pcb().unwrap()
}

fn engine(dir: &TempDir) -> TestEngine {
    let mut engine = probe_engine();
    engine.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
    engine
}

fn net(engine: &mut TestEngine, name: &str) -> Uuid {
    uuid(&engine.run_ok("test.net.add", json!({"name": name}))["result"]["uuid"])
}

/// A one-Pad Footprint at (`x`, 0) mm on `net`.
fn pad(engine: &mut TestEngine, x: f64, net: Uuid) -> Uuid {
    let out = engine.run_ok(
        "test.footprint.add",
        json!({"at": {"x": format!("{x}mm"), "y": 0}, "designator": "P",
               "pads": [{"at": {"x": 0, "y": 0}, "width": "1mm", "height": "1mm", "net": net}]}),
    );
    uuid(&out["result"]["footprint"])
}

fn track(engine: &mut TestEngine, net: Uuid, width: &str, points: &[(f64, f64)]) -> Vec<Uuid> {
    let points: Vec<Value> = points
        .iter()
        .map(|(x, y)| json!({"x": format!("{x}mm"), "y": format!("{y}mm")}))
        .collect();
    let out = engine.run_ok(
        "pcb.track.add",
        json!({"layer": "copper_top", "width": width, "net": net, "points": points}),
    );
    uuids(&out["result"]["tracks"])
}

fn via(engine: &mut TestEngine, x: f64, y: f64, net: Uuid) -> Uuid {
    let out = engine.run_ok(
        "pcb.via.add",
        json!({"at": {"x": format!("{x}mm"), "y": format!("{y}mm")}, "net": net}),
    );
    uuid(&out["result"]["uuid"])
}

fn sorted(mut v: Vec<Uuid>) -> Vec<Uuid> {
    v.sort();
    v
}

/// SIG: P1 (0) - P2 (10) - P3 (20) routed, with a Via mid-way on the second hop.
/// PWR: one Track on its own.
struct Routed {
    engine: TestEngine,
    sig: Uuid,
    pwr: Uuid,
    p1: Uuid,
    first: Vec<Uuid>,
    second: Vec<Uuid>,
    via: Uuid,
    power: Vec<Uuid>,
    _dir: TempDir,
}

fn routed(label: &str) -> Routed {
    let dir = TempDir::new(label);
    let mut engine = engine(&dir);
    let (sig, pwr) = (net(&mut engine, "SIG"), net(&mut engine, "PWR"));
    let p1 = pad(&mut engine, 0.0, sig);
    pad(&mut engine, 10.0, sig);
    pad(&mut engine, 20.0, sig);
    let first = track(
        &mut engine,
        sig,
        "0.25mm",
        &[(0.0, 0.0), (5.0, 2.0), (10.0, 0.0)],
    );
    let second = track(
        &mut engine,
        sig,
        "0.25mm",
        &[(10.0, 0.0), (15.0, 2.0), (20.0, 0.0)],
    );
    let via = via(&mut engine, 15.0, 2.0, sig);
    let power = track(&mut engine, pwr, "0.5mm", &[(0.0, 10.0), (20.0, 10.0)]);
    Routed {
        engine,
        sig,
        pwr,
        p1,
        first,
        second,
        via,
        power,
        _dir: dir,
    }
}

#[test]
fn unroute_a_connection_takes_tracks_and_vias_up_to_the_next_pad() {
    let mut r = routed("unroute-connection");
    let out = r
        .engine
        .run_ok("pcb.route.unroute", json!({"connection": r.second[0]}));
    let mut want = r.second.clone();
    want.push(r.via);
    assert_eq!(uuids(&out["result"]["deleted"]), sorted(want));
    assert_eq!(
        pcb(&r.engine).tracks().count(),
        3,
        "the first hop and PWR stay"
    );
}

#[test]
fn unroute_nets_footprints_and_all() {
    let mut r = routed("unroute-nets");
    let out = r
        .engine
        .run_ok("pcb.route.unroute", json!({"nets": [r.pwr]}));
    assert_eq!(uuids(&out["result"]["deleted"]), sorted(r.power.clone()));

    let out = r
        .engine
        .run_ok("pcb.route.unroute", json!({"footprints": [r.p1]}));
    assert_eq!(uuids(&out["result"]["deleted"]), sorted(r.first.clone()));

    let out = r.engine.run_ok("pcb.route.unroute", json!({"all": true}));
    let mut rest = r.second.clone();
    rest.push(r.via);
    assert_eq!(uuids(&out["result"]["deleted"]), sorted(rest));
    assert_eq!(pcb(&r.engine).tracks().count(), 0);
    assert_eq!(pcb(&r.engine).vias().count(), 0);
    assert!(pcb(&r.engine).net(r.sig).is_some(), "Nets stay");
}

#[test]
fn unroute_needs_something_to_unroute() {
    let mut r = routed("unroute-nothing");
    let err = r.engine.run_err("pcb.route.unroute", json!({}));
    assert_eq!(err.code, ErrorCode::InvalidParams);
    let err = r
        .engine
        .run_err("pcb.route.unroute", json!({"connection": r.p1}));
    assert_eq!(
        err.code,
        ErrorCode::InvalidParams,
        "a Footprint is no connection"
    );
    let ok = r.engine.run_ok(
        "pcb.route.unroute",
        json!({"nets": [r.sig], "include_locked": true}),
    );
    assert_eq!(uuids(&ok["result"]["deleted"]).len(), 5);
}

fn lines(engine: &TestEngine) -> Vec<(Point, Point, Nm)> {
    let mut out: Vec<_> = pcb(engine)
        .tracks()
        .map(|t| match t.geometry {
            TrackGeometry::Line { from, to } => (from, to, t.width),
            _ => unreachable!(),
        })
        .collect();
    out.sort_by_key(|(a, b, _)| (a.x, a.y, b.x, b.y));
    out
}

#[test]
fn cleanup_removes_stacked_duplicates_keeping_the_widest() {
    let dir = TempDir::new("cleanup-dupes");
    let mut engine = engine(&dir);
    let sig = net(&mut engine, "SIG");
    let thin = track(&mut engine, sig, "0.2mm", &[(0.0, 0.0), (5.0, 5.0)]);
    track(&mut engine, sig, "0.4mm", &[(5.0, 5.0), (0.0, 0.0)]);
    let out = engine.run_ok("pcb.track.cleanup", json!({}));
    assert_eq!(uuids(&out["result"]["duplicates"]), thin);
    assert_eq!(
        lines(&engine),
        [(mm(5.0, 5.0), mm(0.0, 0.0), Nm::from_um(400))]
    );
}

#[test]
fn cleanup_merges_collinear_segments_with_nothing_at_the_joint() {
    let dir = TempDir::new("cleanup-merge");
    let mut engine = engine(&dir);
    let (sig, other) = (net(&mut engine, "SIG"), net(&mut engine, "OTHER"));
    track(
        &mut engine,
        sig,
        "0.25mm",
        &[(0.0, 0.0), (2.0, 0.0), (5.0, 0.0), (5.0, 3.0)],
    );
    // A Via at a joint holds it apart; so does a third Track.
    track(
        &mut engine,
        other,
        "0.25mm",
        &[(0.0, 10.0), (4.0, 10.0), (8.0, 10.0)],
    );
    via(&mut engine, 4.0, 10.0, other);
    let out = engine.run_ok("pcb.track.cleanup", json!({}));
    assert_eq!(uuids(&out["result"]["merged"]).len(), 1);
    assert_eq!(uuids(&out["result"]["lengthened"]).len(), 1);
    assert_eq!(
        lines(&engine),
        [
            (mm(0.0, 0.0), mm(5.0, 0.0), Nm::from_um(250)),
            (mm(0.0, 10.0), mm(4.0, 10.0), Nm::from_um(250)),
            (mm(4.0, 10.0), mm(8.0, 10.0), Nm::from_um(250)),
            (mm(5.0, 0.0), mm(5.0, 3.0), Nm::from_um(250)),
        ]
    );
}

#[test]
fn cleanup_can_be_limited_to_some_nets() {
    let dir = TempDir::new("cleanup-nets");
    let mut engine = engine(&dir);
    let (sig, other) = (net(&mut engine, "SIG"), net(&mut engine, "OTHER"));
    track(
        &mut engine,
        sig,
        "0.25mm",
        &[(0.0, 0.0), (2.0, 0.0), (5.0, 0.0)],
    );
    track(
        &mut engine,
        other,
        "0.25mm",
        &[(0.0, 10.0), (2.0, 10.0), (5.0, 10.0)],
    );
    let out = engine.run_ok("pcb.track.cleanup", json!({"nets": [sig]}));
    assert_eq!(uuids(&out["result"]["merged"]).len(), 1);
    assert_eq!(pcb(&engine).tracks().count(), 3);
}
