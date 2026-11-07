//! Property tests for Zone fill: whatever the copper, a fill stays inside its outline,
//! keeps its clearance from other Nets, keeps only connected islands, is deterministic
//! and survives the cache bytes.

use proptest::prelude::*;
use reda_model::geom::Point;
use reda_model::{FillShape, PcbDocument, Uuid};
use reda_testkit::{TempDir, TestEngine, probe_engine};
use serde_json::{Value, json};

const MM: i64 = 1_000_000;
const CLEARANCE: i64 = 300_000;

fn pcb(engine: &TestEngine) -> &PcbDocument {
    engine.project().unwrap().pcb().unwrap()
}

fn uuid(value: &Value) -> Uuid {
    value.as_str().unwrap().parse().unwrap()
}

/// A SIG Track: from, to, width.
type TrackSeed = ((i64, i64), (i64, i64), i64);

/// A board: SIG Tracks (from, to, width) and GND Pads (centre, size) under a GND Zone over
/// (0, 0)–(20 mm, 20 mm).
#[derive(Debug, Clone)]
struct Board {
    tracks: Vec<TrackSeed>,
    pads: Vec<((i64, i64), i64)>,
    thermal: bool,
    min_width: i64,
}

fn board() -> impl Strategy<Value = Board> {
    let coord = -2 * MM..22 * MM;
    let track = (
        (coord.clone(), coord.clone()),
        (coord.clone(), coord.clone()),
        100_000i64..800_000,
    )
        .prop_filter("a Track has length", |(a, b, _)| a != b);
    let pad = ((MM..19 * MM, MM..19 * MM), 300_000i64..2 * MM);
    (
        prop::collection::vec(track, 0..5),
        prop::collection::vec(pad, 0..4),
        any::<bool>(),
        100_000i64..600_000,
    )
        .prop_map(|(tracks, pads, thermal, min_width)| Board {
            tracks,
            pads,
            thermal,
            min_width,
        })
}

fn build(b: &Board, dir: &TempDir) -> (TestEngine, Uuid) {
    let mut engine = probe_engine();
    engine.run_ok("project.new", json!({"path": dir.path(), "name": "Demo"}));
    let gnd = uuid(&engine.run_ok("test.net.add", json!({"name": "GND"}))["result"]["uuid"]);
    let sig = uuid(&engine.run_ok("test.net.add", json!({"name": "SIG"}))["result"]["uuid"]);
    for (from, to, width) in &b.tracks {
        engine.run_ok(
            "pcb.track.add",
            json!({"layer": "copper_top", "net": sig.to_string(), "width": width,
                   "points": [{"x": from.0, "y": from.1}, {"x": to.0, "y": to.1}]}),
        );
    }
    for (at, size) in &b.pads {
        engine.run_ok(
            "test.footprint.add",
            json!({"at": {"x": at.0, "y": at.1},
                   "pads": [{"at": {"x": 0, "y": 0}, "width": size, "height": size,
                             "net": gnd.to_string()}]}),
        );
    }
    if !b.thermal {
        engine.run_ok(
            "pcb.rules.set",
            json!({"kind": "zone_connection", "values": {"smd": {"style": "solid"}}}),
        );
    }
    let corner = |x: i64, y: i64| json!({"x": x, "y": y});
    let zone = engine.run_ok(
        "pcb.zone.add",
        json!({"layer": "copper_top", "net": gnd.to_string(),
               "vertices": [corner(0, 0), corner(20 * MM, 0), corner(20 * MM, 20 * MM),
                            corner(0, 20 * MM)],
               "clearance": CLEARANCE, "min_width": b.min_width}),
    );
    (engine, uuid(&zone["result"]["uuid"]))
}

fn rings(shape: &FillShape) -> impl Iterator<Item = &Vec<Point>> {
    std::iter::once(&shape.outer).chain(&shape.holes)
}

fn edges(ring: &[Point]) -> impl Iterator<Item = (Point, Point)> + '_ {
    (0..ring.len()).map(move |i| (ring[i], ring[(i + 1) % ring.len()]))
}

fn xy(p: Point) -> (f64, f64) {
    (p.x.0 as f64, p.y.0 as f64)
}

fn point_segment(p: (f64, f64), a: (f64, f64), b: (f64, f64)) -> f64 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len2 = dx * dx + dy * dy;
    let t = if len2 == 0.0 {
        0.0
    } else {
        (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / len2).clamp(0.0, 1.0)
    };
    (p.0 - a.0 - t * dx).hypot(p.1 - a.1 - t * dy)
}

fn cross(o: (f64, f64), a: (f64, f64), b: (f64, f64)) -> f64 {
    (a.0 - o.0) * (b.1 - o.1) - (a.1 - o.1) * (b.0 - o.0)
}

fn segment_segment(a: (f64, f64), b: (f64, f64), c: (f64, f64), d: (f64, f64)) -> f64 {
    let crosses = cross(a, b, c).signum() * cross(a, b, d).signum() < 0.0
        && cross(c, d, a).signum() * cross(c, d, b).signum() < 0.0;
    if crosses {
        return 0.0;
    }
    point_segment(a, c, d)
        .min(point_segment(b, c, d))
        .min(point_segment(c, a, b))
        .min(point_segment(d, a, b))
}

fn ring_contains(ring: &[Point], p: (f64, f64)) -> bool {
    let mut inside = false;
    for (a, b) in edges(ring) {
        let ((ax, ay), (bx, by)) = (xy(a), xy(b));
        if (ay > p.1) != (by > p.1) && p.0 < ax + (p.1 - ay) / (by - ay) * (bx - ax) {
            inside = !inside;
        }
    }
    inside
}

fn shape_contains(shape: &FillShape, p: (f64, f64)) -> bool {
    ring_contains(&shape.outer, p) && !shape.holes.iter().any(|h| ring_contains(h, p))
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 24, ..ProptestConfig::default() })]

    #[test]
    fn fills_keep_their_clearance_outline_and_connections(b in board()) {
        let dir = TempDir::new("zone-props");
        let (engine, zone) = build(&b, &dir);
        let fills = reda_pcb::zone::fill(pcb(&engine), &mut |_, _| true).unwrap();
        let shapes = &fills.zones[&zone].shapes;

        for shape in shapes {
            for ring in rings(shape) {
                for p in ring {
                    // Inside the outline.
                    prop_assert!((0..=20 * MM).contains(&p.x.0) && (0..=20 * MM).contains(&p.y.0));
                }
                // Clear of every SIG Track by the clearance (1 nm of integer rounding).
                for (from, to, width) in &b.tracks {
                    let (c, d) = ((from.0 as f64, from.1 as f64), (to.0 as f64, to.1 as f64));
                    for (e0, e1) in edges(ring) {
                        let gap = segment_segment(xy(e0), xy(e1), c, d) - (*width / 2) as f64;
                        prop_assert!(gap >= (CLEARANCE - 2) as f64, "gap {gap} at {e0:?}");
                    }
                }
            }
            // No SIG Track runs inside the fill.
            for (from, _, _) in &b.tracks {
                prop_assert!(!shape_contains(shape, (from.0 as f64, from.1 as f64)));
            }
            // Island removal: every island reaches a GND Pad.
            let touches_pad = b.pads.iter().any(|(at, size)| {
                let half = (*size / 2) as f64;
                let centre = (at.0 as f64, at.1 as f64);
                shape_contains(shape, centre)
                    || rings(shape).flat_map(|r| edges(r)).any(|(e0, e1)| {
                        point_segment(centre, xy(e0), xy(e1)) <= half * std::f64::consts::SQRT_2
                    })
            });
            prop_assert!(touches_pad, "an island with no GND Pad survived");
        }

        // Deterministic, and the cache bytes give the same fills back.
        let again = reda_pcb::zone::fill(pcb(&engine), &mut |_, _| true).unwrap();
        prop_assert_eq!(&again, &fills);
        let bytes = reda_pcb::zone::encode(&fills);
        prop_assert_eq!(reda_pcb::zone::decode(&bytes), Some(fills.clone()));
        prop_assert_eq!(reda_pcb::zone::decode(&bytes[..bytes.len() - 1]), None);
    }
}
