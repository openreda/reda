//! Loop removal: a route that joins two points already joined on its Net takes the old
//! path in the same `batch`, through `commit()`, run as one Command.
//!
//! The board: Pads P1 at (0, 0) and P2 at (10, 0) mm on SIG, joined by an old path that
//! goes up and over: (0, 0) - (0, 5) - (10, 5) - (10, 0), 0.25 mm wide. The new route goes
//! straight from P1 to P2.

use reda_model::geom::{Nm, Point};
use reda_model::{LayerKind, PcbDocument, Side, TrackGeometry, Uuid};
use reda_pcb::Copper;
use reda_pcb::route::{Leg, LoopOptions, Note, Step, commit};
use reda_testkit::{TempDir, TestEngine, probe_engine};
use serde_json::{Value, json};

struct Board {
    engine: TestEngine,
    net: Uuid,
    old: Vec<Uuid>,
    _dir: TempDir,
}

fn uuid(value: &Value) -> Uuid {
    value.as_str().unwrap().parse().unwrap()
}

fn mm(x: f64, y: f64) -> Point {
    Point::new(Nm((x * 1e6) as i64), Nm((y * 1e6) as i64))
}

fn pcb(engine: &TestEngine) -> &PcbDocument {
    engine.project().unwrap().pcb().unwrap()
}

fn layer(engine: &TestEngine, side: Side) -> Uuid {
    pcb(engine)
        .layers()
        .find(LayerKind::Copper, Some(side))
        .unwrap()
        .uuid
}

fn pad(engine: &mut TestEngine, x: f64, y: f64, net: Uuid) {
    engine.run_ok(
        "test.footprint.add",
        json!({"at": {"x": format!("{x}mm"), "y": format!("{y}mm")}, "designator": "P",
               "pads": [{"at": {"x": 0, "y": 0}, "width": "1mm", "height": "1mm", "net": net}]}),
    );
}

fn track(
    engine: &mut TestEngine,
    net: Uuid,
    layer: &str,
    width: &str,
    points: &[(f64, f64)],
) -> Vec<Uuid> {
    let points: Vec<Value> = points
        .iter()
        .map(|(x, y)| json!({"x": format!("{x}mm"), "y": format!("{y}mm")}))
        .collect();
    let out = engine.run_ok(
        "pcb.track.add",
        json!({"layer": layer, "width": width, "net": net, "points": points}),
    );
    out["result"]["tracks"]
        .as_array()
        .unwrap()
        .iter()
        .map(uuid)
        .collect()
}

fn board(label: &str) -> Board {
    let dir = TempDir::new(label);
    let mut engine = probe_engine();
    engine.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
    let net = uuid(&engine.run_ok("test.net.add", json!({"name": "SIG"}))["result"]["uuid"]);
    pad(&mut engine, 0.0, 0.0, net);
    pad(&mut engine, 10.0, 0.0, net);
    let old = track(
        &mut engine,
        net,
        "copper_top",
        "0.25mm",
        &[(0.0, 0.0), (0.0, 5.0), (10.0, 5.0), (10.0, 0.0)],
    );
    Board {
        engine,
        net,
        old,
        _dir: dir,
    }
}

fn straight(b: &Board, width: Nm) -> Vec<Step> {
    vec![Step::Leg(Leg {
        layer: layer(&b.engine, Side::Top),
        width,
        points: vec![mm(0.0, 0.0), mm(10.0, 0.0)],
    })]
}

fn plan(b: &Board, steps: &[Step], options: LoopOptions) -> reda_pcb::route::Commit {
    let copper = Copper::of(b.engine.project().unwrap()).unwrap();
    commit(&copper, pcb(&b.engine), Some(b.net), steps, &options)
}

/// Runs the commit as the Tool does and returns the Journal entries it added.
fn run(b: &mut Board, c: &reda_pcb::route::Commit) -> usize {
    let before = b.engine.journal().len();
    let (id, params) = c.command().unwrap();
    b.engine.run_ok(&id, params);
    b.engine.journal().len() - before
}

fn lines(engine: &TestEngine) -> Vec<(Point, Point)> {
    let mut out: Vec<(Point, Point)> = pcb(engine)
        .tracks()
        .map(|t| match t.geometry {
            TrackGeometry::Line { from, to } => (from, to),
            _ => unreachable!(),
        })
        .collect();
    out.sort_by_key(|(a, b)| (a.x, a.y, b.x, b.y));
    out
}

#[test]
fn a_reroute_deletes_the_old_path_in_one_batch_and_one_undo_step() {
    let mut b = board("loops-basic");
    let c = plan(&b, &straight(&b, Nm::from_um(250)), LoopOptions::default());
    let mut old = b.old.clone();
    old.sort();
    assert_eq!(c.removes.objects, old);
    assert_eq!(c.removes.removed.len(), 3, "the red preview");
    assert_eq!(c.note, None);
    assert_eq!(c.steps[0]["id"], "pcb.copper.delete", "deletes come first");
    assert_eq!(c.command().unwrap().0, "batch");
    assert_eq!(run(&mut b, &c), 1, "one Journal entry");
    assert_eq!(lines(&b.engine), [(mm(0.0, 0.0), mm(10.0, 0.0))]);

    let document = pcb(&b.engine).uuid;
    b.engine
        .run_ok("edit.undo", json!({"document": document.to_string()}));
    assert_eq!(
        lines(&b.engine).len(),
        3,
        "one undo brings the old path back"
    );
}

#[test]
fn a_branch_keeps_the_whole_old_path_with_a_note() {
    let mut b = board("loops-branch");
    let net = b.net;
    pad(&mut b.engine, 5.0, 10.0, net);
    track(
        &mut b.engine,
        net,
        "copper_top",
        "0.25mm",
        &[(5.0, 5.0), (5.0, 10.0)],
    );
    let c = plan(&b, &straight(&b, Nm::from_um(250)), LoopOptions::default());
    assert!(c.removes.is_empty());
    assert_eq!(c.note, Some(Note::LoopBranch));
    assert_eq!(
        c.command().unwrap().0,
        "pcb.track.add",
        "just the new Track"
    );
}

#[test]
fn a_dangling_stub_goes_too_unless_antennas_are_kept() {
    let mut b = board("loops-antenna");
    let net = b.net;
    let stub = track(
        &mut b.engine,
        net,
        "copper_top",
        "0.25mm",
        &[(5.0, 5.0), (5.0, 8.0)],
    );
    let c = plan(&b, &straight(&b, Nm::from_um(250)), LoopOptions::default());
    assert!(c.removes.objects.contains(&stub[0]));
    // The old middle Track is split at the stub's end and both halves go.
    assert!(c.removes.keep.is_empty());
    run(&mut b, &c);
    assert_eq!(lines(&b.engine), [(mm(0.0, 0.0), mm(10.0, 0.0))]);

    let b = {
        let mut b = board("loops-antenna-kept");
        let net = b.net;
        track(
            &mut b.engine,
            net,
            "copper_top",
            "0.25mm",
            &[(5.0, 5.0), (5.0, 8.0)],
        );
        b
    };
    let options = LoopOptions {
        antennas: false,
        ..LoopOptions::default()
    };
    let c = plan(&b, &straight(&b, Nm::from_um(250)), options);
    let mut old = b.old.clone();
    old.sort();
    assert_eq!(c.removes.objects, old, "the stub stays");
}

#[test]
fn a_narrower_route_keeps_the_old_path() {
    let b = board("loops-narrow");
    let c = plan(&b, &straight(&b, Nm::from_um(200)), LoopOptions::default());
    assert!(c.removes.is_empty());
    assert_eq!(c.note, Some(Note::LoopNarrower));
}

#[test]
fn loop_removal_off_or_off_for_the_net_keeps_the_loop() {
    let mut b = board("loops-off");
    let off = LoopOptions {
        enabled: false,
        ..LoopOptions::default()
    };
    let c = plan(&b, &straight(&b, Nm::from_um(250)), off);
    assert!(c.removes.is_empty());
    assert_eq!(c.note, None);

    let net = b.net;
    b.engine.run_ok(
        "pcb.net.set_routing",
        json!({"nets": [net], "loop_removal": false}),
    );
    let c = plan(&b, &straight(&b, Nm::from_um(250)), LoopOptions::default());
    assert!(c.removes.is_empty(), "the Net's own switch");
}

#[test]
fn a_path_through_vias_goes_with_its_vias_unless_turned_off() {
    let dir = TempDir::new("loops-vias");
    let mut engine = probe_engine();
    engine.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
    let net = uuid(&engine.run_ok("test.net.add", json!({"name": "SIG"}))["result"]["uuid"]);
    pad(&mut engine, 0.0, 0.0, net);
    pad(&mut engine, 10.0, 0.0, net);
    let mut old = track(
        &mut engine,
        net,
        "copper_top",
        "0.25mm",
        &[(0.0, 0.0), (0.0, 5.0)],
    );
    old.extend(track(
        &mut engine,
        net,
        "copper_bottom",
        "0.25mm",
        &[(0.0, 5.0), (10.0, 5.0)],
    ));
    old.extend(track(
        &mut engine,
        net,
        "copper_top",
        "0.25mm",
        &[(10.0, 5.0), (10.0, 0.0)],
    ));
    for x in ["0mm", "10mm"] {
        let v = engine.run_ok(
            "pcb.via.add",
            json!({"at": {"x": x, "y": "5mm"}, "net": net}),
        );
        old.push(uuid(&v["result"]["uuid"]));
    }
    old.sort();
    let mut b = Board {
        engine,
        net,
        old,
        _dir: dir,
    };
    let kept = LoopOptions {
        through_vias: false,
        ..LoopOptions::default()
    };
    let c = plan(&b, &straight(&b, Nm::from_um(250)), kept);
    assert!(c.removes.is_empty());
    assert_eq!(c.note, Some(Note::LoopThroughVias));

    let c = plan(&b, &straight(&b, Nm::from_um(250)), LoopOptions::default());
    assert_eq!(c.removes.objects, b.old);
    run(&mut b, &c);
    assert_eq!(pcb(&b.engine).vias().count(), 0);
}

#[test]
fn a_track_split_by_the_route_end_keeps_what_stays() {
    let mut b = board("loops-split");
    // The new route ends in the middle of the old top Track, at (5, 5).
    let steps = vec![Step::Leg(Leg {
        layer: layer(&b.engine, Side::Top),
        width: Nm::from_um(250),
        points: vec![mm(0.0, 0.0), mm(5.0, 0.0), mm(5.0, 5.0)],
    })];
    let c = plan(&b, &steps, LoopOptions::default());
    assert_eq!(c.removes.objects.len(), 2, "the up Track and the split one");
    assert_eq!(c.removes.keep.len(), 1);
    assert_eq!(
        (c.removes.keep[0].from, c.removes.keep[0].to),
        (mm(5.0, 5.0), mm(10.0, 5.0))
    );
    assert_eq!(run(&mut b, &c), 1);
    assert_eq!(
        lines(&b.engine),
        [
            (mm(0.0, 0.0), mm(5.0, 0.0)),
            (mm(5.0, 0.0), mm(5.0, 5.0)),
            (mm(5.0, 5.0), mm(10.0, 5.0)),
            (mm(10.0, 5.0), mm(10.0, 0.0)),
        ]
    );
}
