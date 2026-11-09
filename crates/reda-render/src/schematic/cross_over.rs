//! Display Cross-Overs: where two Wire segments cross without connecting, the flatter
//! one hops over the other in a small half circle. It is drawn on the canvas only.

use std::collections::BTreeMap;

use reda_model::geom::{Nm, Point};
use reda_model::{SchematicSheet, Uuid};

/// A bridge's radius: a quarter of the usual 100 mil grid.
const RADIUS: f64 = 635_000.0;
/// Each half circle is drawn as this many strokes.
const STEPS: usize = 8;

/// One Wire segment: its Wire, its index in the Wire, and its ends.
struct Segment {
    wire: Uuid,
    index: usize,
    a: Point,
    b: Point,
}

impl Segment {
    fn delta(&self) -> (i128, i128) {
        (
            i128::from((self.b.x - self.a.x).0),
            i128::from((self.b.y - self.a.y).0),
        )
    }

    /// Whether this segment hops over `other` at their crossing: the flatter one does, and
    /// of two as steep, the one earlier in UUID and segment order.
    fn hops_over(&self, other: &Segment) -> bool {
        let ((ax, ay), (bx, by)) = (self.delta(), other.delta());
        let (mine, theirs) = (ay.abs() * bx.abs(), by.abs() * ax.abs());
        match mine.cmp(&theirs) {
            std::cmp::Ordering::Less => true,
            std::cmp::Ordering::Greater => false,
            std::cmp::Ordering::Equal => (self.wire, self.index) < (other.wire, other.index),
        }
    }
}

fn cross(o: Point, p: Point, q: Point) -> i128 {
    let (px, py) = (i128::from((p.x - o.x).0), i128::from((p.y - o.y).0));
    let (qx, qy) = (i128::from((q.x - o.x).0), i128::from((q.y - o.y).0));
    px * qy - py * qx
}

/// Where `s` and `t` cross strictly inside both, as `s`'s parameter from 0 to 1: segments
/// that only touch (an end on the other, a T) or run along each other connect or overlap
/// instead.
fn crossing(s: &Segment, t: &Segment) -> Option<f64> {
    let (d1, d2) = (cross(s.a, s.b, t.a), cross(s.a, s.b, t.b));
    let (d3, d4) = (cross(t.a, t.b, s.a), cross(t.a, t.b, s.b));
    let opposite = |p: i128, q: i128| (p > 0 && q < 0) || (p < 0 && q > 0);
    if !(opposite(d1, d2) && opposite(d3, d4)) {
        return None;
    }
    // d3 and d4 are the signed areas of s's ends against t, so the crossing sits at
    // d3 / (d3 - d4) along s.
    Some(d3 as f64 / (d3 - d4) as f64)
}

/// Where each segment that hops is crossed, keyed by (Wire, segment index), as parameters
/// along it. Crossings on a Junction connect, so they get no bridge.
pub(super) fn hops(
    sheet: &SchematicSheet,
    junctions: &[Point],
) -> BTreeMap<(Uuid, usize), Vec<f64>> {
    let segments: Vec<Segment> = sheet
        .wires()
        .flat_map(|w| {
            w.segments().enumerate().map(|(index, (a, b))| Segment {
                wire: w.uuid,
                index,
                a,
                b,
            })
        })
        .collect();
    let mut out: BTreeMap<(Uuid, usize), Vec<f64>> = BTreeMap::new();
    for s in &segments {
        for t in &segments {
            if !s.hops_over(t) {
                continue;
            }
            let Some(at) = crossing(s, t) else {
                continue;
            };
            if junctions.contains(&along(s.a, s.b, at)) {
                continue;
            }
            out.entry((s.wire, s.index)).or_default().push(at);
        }
    }
    for at in out.values_mut() {
        at.sort_by(f64::total_cmp);
    }
    out
}

fn along(a: Point, b: Point, t: f64) -> Point {
    let lerp = |p: Nm, q: Nm| Nm((p.0 as f64 + (q.0 - p.0) as f64 * t).round() as i64);
    Point::new(lerp(a.x, b.x), lerp(a.y, b.y))
}

/// The path of segment `a`–`b` hopping over its crossings at parameters `at` (sorted): a
/// half circle over each, on the side that faces up (or left on an upright segment). A
/// crossing too near an end or the last bridge to fit one is crossed straight.
pub(super) fn bridged(a: Point, b: Point, at: &[f64]) -> Vec<Point> {
    let (dx, dy) = ((b.x - a.x).0 as f64, (b.y - a.y).0 as f64);
    let len = dx.hypot(dy);
    let (ux, uy) = (dx / len, dy / len);
    // The normal pointing up, or left when the segment is upright.
    let (nx, ny) = match (-uy, ux) {
        (nx, ny) if ny > 0.0 || (ny == 0.0 && nx < 0.0) => (nx, ny),
        (nx, ny) => (-nx, -ny),
    };
    let point = |d: f64, h: f64| {
        Point::new(
            Nm((a.x.0 as f64 + ux * d + nx * h).round() as i64),
            Nm((a.y.0 as f64 + uy * d + ny * h).round() as i64),
        )
    };
    let mut path = vec![a];
    let mut done = 0.0;
    for t in at {
        let mid = t * len;
        if mid - RADIUS < done || mid + RADIUS > len {
            continue;
        }
        for i in 0..=STEPS {
            let angle = std::f64::consts::PI * (1.0 - i as f64 / STEPS as f64);
            path.push(point(mid + RADIUS * angle.cos(), RADIUS * angle.sin()));
        }
        done = mid + RADIUS;
    }
    path.push(b);
    path
}
