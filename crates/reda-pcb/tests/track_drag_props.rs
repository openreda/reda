//! Property tests on the slide rules (docs/spec/v0.1/pcb.md#dragging-tracks): random
//! three-segment Tracks on the 45° grid, the middle one slid by a random offset through
//! `pcb.track.drag`.

use proptest::prelude::*;
use reda_command::ErrorCode;
use reda_model::geom::{Nm, Point};
use reda_model::{PcbDocument, TrackGeometry, Uuid};
use reda_testkit::{TempDir, TestEngine, probe_engine};
use serde_json::{Value, json};

/// The eight 45° directions, as unit steps.
const DIRS: [(i64, i64); 8] = [
    (1, 0),
    (1, 1),
    (0, 1),
    (-1, 1),
    (-1, 0),
    (-1, -1),
    (0, -1),
    (1, -1),
];

#[derive(Clone, Debug)]
struct Case {
    /// Directions of the three segments, as indexes into `DIRS`.
    dirs: [usize; 3],
    /// Their lengths in whole 0.1 mm steps along each axis.
    steps: [i64; 3],
    /// The drag offset in µm.
    by: (i64, i64),
    miter: bool,
    /// A Via at the joint of the first two segments, so that end is fixed.
    via: bool,
}

fn case() -> impl Strategy<Value = Case> {
    (
        0usize..8,
        prop::sample::select(vec![1usize, 2, 3, 5, 6, 7]),
        prop::sample::select(vec![1usize, 2, 3, 5, 6, 7]),
        [20i64..150, 20i64..150, 20i64..150],
        (-20_000i64..20_000, -20_000i64..20_000),
        any::<bool>(),
        any::<bool>(),
    )
        .prop_map(|(d0, turn1, turn2, steps, by, miter, via)| {
            let d1 = (d0 + turn1) % 8;
            let d2 = (d1 + turn2) % 8;
            Case {
                dirs: [d0, d1, d2],
                steps,
                by,
                miter,
                via,
            }
        })
        .prop_filter("the drag must move across the middle segment", |c| {
            let (dx, dy) = DIRS[c.dirs[1]];
            // The normal component, scaled: zero means a drag along the segment.
            (c.by.1 * dx - c.by.0 * dy).abs() > 1_000
        })
        .prop_filter("the Track must not come back to its own points", |c| {
            let pts = points(c);
            (0..pts.len()).all(|i| !pts[i + 1..].contains(&pts[i]))
        })
}

fn points(c: &Case) -> Vec<Point> {
    let mut p = Point::new(Nm(0), Nm(0));
    let mut out = vec![p];
    for i in 0..3 {
        let (dx, dy) = DIRS[c.dirs[i]];
        let step = c.steps[i] * 100_000;
        p = Point::new(Nm(p.x.0 + dx * step), Nm(p.y.0 + dy * step));
        out.push(p);
    }
    out
}

fn xy(p: Point) -> (f64, f64) {
    (p.x.0 as f64, p.y.0 as f64)
}

/// The angle of a segment in degrees, in [0, 180).
fn angle(a: Point, b: Point) -> f64 {
    let (ax, ay) = xy(a);
    let (bx, by) = xy(b);
    (by - ay).atan2(bx - ax).to_degrees().rem_euclid(180.0)
}

fn same_angle(a: f64, b: f64) -> bool {
    let d = (a - b).rem_euclid(180.0);
    !(1e-3..=180.0 - 1e-3).contains(&d)
}

fn ends(pcb: &PcbDocument, track: Uuid) -> (Point, Point) {
    match pcb.track(track).unwrap().geometry {
        TrackGeometry::Line { from, to } => (from, to),
        TrackGeometry::Arc { .. } => panic!("a slide makes no arcs"),
    }
}

fn uuids(value: &Value) -> Vec<Uuid> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().parse().unwrap())
        .collect()
}

/// Whether two segments are in line and share more than a point.
fn overlap((a, b): (Point, Point), (p, q): (Point, Point)) -> bool {
    let (ax, ay) = xy(a);
    let (ux, uy) = (xy(b).0 - ax, xy(b).1 - ay);
    let len = ux.hypot(uy);
    let off = |p: Point| ((xy(p).1 - ay) * ux - (xy(p).0 - ax) * uy) / len;
    if off(p).abs() > 2.0 || off(q).abs() > 2.0 {
        return false;
    }
    let along = |p: Point| ((xy(p).0 - ax) * ux + (xy(p).1 - ay) * uy) / len;
    let (s, t) = (along(p), along(q));
    s.max(t).min(len) - s.min(t).max(0.0) > 2.0
}

fn near(a: Point, b: Point) -> bool {
    (a.x.0 - b.x.0).abs() <= 2 && (a.y.0 - b.y.0).abs() <= 2
}

fn setup(c: &Case, dir: &TempDir) -> (TestEngine, Vec<Uuid>) {
    let mut engine = probe_engine();
    engine.run_ok("project.new", json!({"path": dir.path(), "name": "Props"}));
    if c.via {
        let at = points(c)[1];
        engine.run_ok("pcb.via.add", json!({"at": {"x": at.x.0, "y": at.y.0}}));
    }
    let pts: Vec<Value> = points(c)
        .iter()
        .map(|p| json!({"x": p.x.0, "y": p.y.0}))
        .collect();
    let out = engine.run_ok(
        "pcb.track.add",
        json!({"layer": "copper_top", "points": pts}),
    );
    (engine, uuids(&out["result"]["tracks"]))
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 48, ..ProptestConfig::default() })]

    /// A slide moves the segment across itself only, keeps every surviving segment's
    /// angle, keeps each neighbour's far end, keeps the Track joined, and stops (never
    /// overshoots) where a neighbour runs out.
    #[test]
    fn a_slide_keeps_angles_far_ends_and_joints(c in case()) {
        let dir = TempDir::new("track-drag-props");
        let (mut engine, t) = setup(&c, &dir);
        let before = points(&c);
        let out = engine.run(
            "pcb.track.drag",
            json!({"track": t[1], "by": {"x": c.by.0 * 1000, "y": c.by.1 * 1000},
                   "miter": c.miter, "merge_collinear": false}),
        );
        let out = match out {
            Ok(out) => out,
            Err(e) => {
                // Only a drag that gets nowhere is refused: the middle can't shrink further.
                prop_assert_eq!(e.code, ErrorCode::Rejected, "{}", e);
                return Ok(());
            }
        };
        let out = &out["result"];
        let pcb = engine.project().unwrap().pcb().unwrap();
        let moved = Point::new(
            Nm(out["by"]["x"].as_i64().unwrap()),
            Nm(out["by"]["y"].as_i64().unwrap()),
        );

        // Across the segment only, and no further than asked.
        let (dx, dy) = DIRS[c.dirs[1]];
        let (ux, uy) = (dx as f64, dy as f64);
        let len = ux.hypot(uy);
        let (mx, my) = xy(moved);
        prop_assert!((mx * ux + my * uy).abs() / len <= 2.0, "moved along: {moved:?}");
        let asked = ((c.by.1 * dx - c.by.0 * dy) as f64 * 1000.0 / len).abs();
        let got = (my * ux - mx * uy).abs() / len;
        prop_assert!(got <= asked + 2.0, "overshot: {got} > {asked}");

        // The middle keeps its angle and lies on its line moved by `by`.
        let (a, b) = ends(pcb, t[1]);
        prop_assert!(same_angle(angle(a, b), angle(before[1], before[2])));
        let (px, py) = xy(before[1] + moved);
        let (ax, ay) = xy(a);
        prop_assert!(((ay - py) * ux - (ax - px) * uy).abs() / len <= 3.0);

        let deleted = uuids(&out["deleted"]);
        for (k, far, joint) in [(0usize, before[0], a), (2, before[3], b)] {
            if deleted.contains(&t[k]) {
                // Gone only by shrinking to nothing: the middle now ends at its far end.
                prop_assert!(near(joint, far), "{joint:?} vs {far:?}");
                continue;
            }
            let (p, q) = ends(pcb, t[k]);
            let (far_now, near_now) = if k == 0 { (p, q) } else { (q, p) };
            prop_assert_eq!(far_now, far, "the far end stays");
            let fixed = k == 0 && c.via;
            if fixed {
                // Untouched; a connector joins it to the middle.
                prop_assert_eq!(near_now, before[if k == 0 { 1 } else { 2 }]);
                let connector = pcb
                    .tracks()
                    .find(|o| {
                        !t.contains(&o.uuid) && {
                            let (p, q) = o.geometry.ends();
                            (p == near_now || q == near_now) && (near(p, joint) || near(q, joint))
                        }
                    });
                if near_now != joint {
                    let connector = connector.expect("a connector at a fixed end");
                    let (p, q) = connector.geometry.ends();
                    let turn = (angle(p, q) - angle(a, b)).rem_euclid(180.0);
                    let want = if c.miter { 45.0 } else { 90.0 };
                    prop_assert!(
                        (turn - want).abs() < 0.01 || (turn - (180.0 - want)).abs() < 0.01,
                        "connector at {turn}°"
                    );
                }
            } else {
                // A stretched neighbour keeps its angle and meets the middle.
                prop_assert!(same_angle(angle(p, q), angle(before[k], before[k + 1])));
                prop_assert!(near(near_now, joint), "{near_now:?} vs {joint:?}");
            }
        }

        // One undo puts the board back.
        let doc = pcb.uuid.to_string();
        engine.run_ok("edit.undo", json!({"document": doc}));
        let pcb = engine.project().unwrap().pcb().unwrap();
        prop_assert_eq!(pcb.tracks().count(), 3);
        prop_assert_eq!(ends(pcb, t[1]), (before[1], before[2]));
    }

    /// With Merge Collinear on, a slide leaves one chain from the Track's first point to
    /// its last (connectors included), and no two segments at a free joint are in line.
    #[test]
    fn a_slide_leaves_one_unbroken_chain_with_no_collinear_joints(c in case()) {
        one_unbroken_chain(&c)?;
    }
}

/// The body of `a_slide_leaves_one_unbroken_chain_with_no_collinear_joints`.
fn one_unbroken_chain(c: &Case) -> Result<(), TestCaseError> {
    let dir = TempDir::new("track-drag-chain");
    let (mut engine, t) = setup(c, &dir);
    let before = points(c);
    let out = engine.run(
        "pcb.track.drag",
        json!({"track": t[1], "by": {"x": c.by.0 * 1000, "y": c.by.1 * 1000},
               "miter": c.miter}),
    );
    if let Err(e) = out {
        prop_assert_eq!(e.code, ErrorCode::Rejected, "{}", e);
        return Ok(());
    }
    let pcb = engine.project().unwrap().pcb().unwrap();
    let mut segments: Vec<(Point, Point)> = pcb.tracks().map(|t| t.geometry.ends()).collect();
    // No segment lies over another: a mitred connector leans away from copper already at
    // its fixed end. A square one has no choice of direction (it runs along the normal,
    // as the spec asks), so it may lie over the Track beyond a Via.
    for (i, s) in segments.iter().enumerate().filter(|_| c.miter) {
        for o in &segments[i + 1..] {
            prop_assert!(!overlap(*s, *o), "{s:?} lies over {o:?}");
        }
    }
    // Walk from the first point, each joint leading to exactly one next segment.
    let mut at = before[0];
    let mut last: Option<(Point, Point)> = None;
    while let Some(i) = segments.iter().position(|(p, q)| *p == at || *q == at) {
        let (p, q) = segments.remove(i);
        let next = if p == at { q } else { p };
        prop_assert!(
            !segments.iter().any(|(p, q)| *p == at || *q == at),
            "a branch at {at:?}"
        );
        // A Via holds its joint: segments there stay apart even in line.
        let held = c.via && at == before[1];
        if let Some((a, b)) = last.filter(|_| !held) {
            prop_assert!(
                !same_angle(angle(a, b), angle(at, next)),
                "collinear joint at {at:?}"
            );
        }
        last = Some((at, next));
        at = next;
    }
    prop_assert!(segments.is_empty(), "left over: {segments:?}");
    prop_assert_eq!(at, before[3]);
    Ok(())
}

/// A mitred connector from a Via must not run back over the Track beyond the Via. Here
/// it used to retrace the first segment exactly, back to the Track's first point, and
/// leave a branch there (a shrunk proptest case).
#[test]
fn a_mitred_connector_does_not_retrace_the_track_beyond_a_via() {
    let c = Case {
        dirs: [2, 5, 7],
        steps: [88, 45, 44],
        by: (682, -8118),
        miter: true,
        via: true,
    };
    one_unbroken_chain(&c).unwrap();
}
