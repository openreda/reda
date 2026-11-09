//! Cutting a silkscreen centre line (a straight line or a circular arc) where it enters a
//! forbidden area or leaves the board.
//!
//! The pieces kept are the stretches whose middles are outside every forbidden shape and,
//! when a board area is given, inside it. A stretch ends wherever the line crosses an edge
//! of either, so the cuts are exact up to rounding to the nanometre.

use std::f64::consts::TAU;

use reda_model::geom::Point;

use crate::zone::poly::{Area, Contour, bounds, inside};

/// Forbidden shapes, each with its box for quick rejection.
pub(super) struct Cuts {
    shapes: Vec<(Box2, Area)>,
}

impl Cuts {
    /// The shapes of `area`, which must not overlap (as a union returns them), so the
    /// even-odd test of each holds.
    pub fn new(area: Area) -> Cuts {
        Cuts {
            shapes: area
                .into_iter()
                .filter_map(|shape| {
                    let one = vec![shape];
                    let (lo, hi) = bounds(&one)?;
                    Some((Box2::new(lo, hi), one))
                })
                .collect(),
        }
    }

    /// The shapes whose boxes meet `b`.
    fn near(&self, b: Box2) -> impl Iterator<Item = &Area> {
        self.shapes
            .iter()
            .filter(move |(sb, _)| sb.meets(b))
            .map(|(_, a)| a)
    }

    /// Whether the area overlaps `area` with some area.
    pub fn overlaps(&self, area: &Area) -> bool {
        let Some((lo, hi)) = bounds(area) else {
            return false;
        };
        let b = Box2::new(lo, hi);
        self.near(b)
            .any(|shape| crate::zone::poly::overlaps(shape, area))
    }
}

#[derive(Clone, Copy, Debug)]
struct Box2 {
    lo: [f64; 2],
    hi: [f64; 2],
}

impl Box2 {
    fn new(lo: [i64; 2], hi: [i64; 2]) -> Box2 {
        Box2 {
            lo: [lo[0] as f64, lo[1] as f64],
            hi: [hi[0] as f64, hi[1] as f64],
        }
    }

    fn meets(self, o: Box2) -> bool {
        self.lo[0] <= o.hi[0]
            && o.lo[0] <= self.hi[0]
            && self.lo[1] <= o.hi[1]
            && o.lo[1] <= self.hi[1]
    }
}

/// A centre line with a parameter `t` from 0 (its start) to 1 (its end).
#[derive(Clone, Copy, Debug)]
pub(super) enum Curve {
    Line {
        a: [f64; 2],
        b: [f64; 2],
    },
    /// About `center`, `radius` from it, from angle `start` sweeping `sweep` (radians,
    /// counter-clockwise when positive).
    Arc {
        center: [f64; 2],
        radius: f64,
        start: f64,
        sweep: f64,
    },
}

impl Curve {
    pub fn line(a: Point, b: Point) -> Curve {
        Curve::Line { a: f(a), b: f(b) }
    }

    /// The arc from `start` about `center` sweeping `sweep` radians.
    pub fn arc(center: Point, start: Point, sweep: f64) -> Curve {
        let (c, s) = (f(center), f(start));
        Curve::Arc {
            center: c,
            radius: (s[0] - c[0]).hypot(s[1] - c[1]),
            start: (s[1] - c[1]).atan2(s[0] - c[0]),
            sweep,
        }
    }

    pub fn at(&self, t: f64) -> [f64; 2] {
        match *self {
            Curve::Line { a, b } => [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t],
            Curve::Arc {
                center,
                radius,
                start,
                sweep,
            } => {
                let angle = start + sweep * t;
                [
                    center[0] + radius * angle.cos(),
                    center[1] + radius * angle.sin(),
                ]
            }
        }
    }

    fn bounds(&self) -> Box2 {
        match *self {
            Curve::Line { a, b } => Box2 {
                lo: [a[0].min(b[0]), a[1].min(b[1])],
                hi: [a[0].max(b[0]), a[1].max(b[1])],
            },
            Curve::Arc { center, radius, .. } => Box2 {
                lo: [center[0] - radius, center[1] - radius],
                hi: [center[0] + radius, center[1] + radius],
            },
        }
    }

    /// The parameters at which the curve crosses the segment `p`–`q`.
    fn crossings(&self, p: [f64; 2], q: [f64; 2], out: &mut Vec<f64>) {
        let d = [q[0] - p[0], q[1] - p[1]];
        match *self {
            Curve::Line { a, b } => {
                let e = [b[0] - a[0], b[1] - a[1]];
                let den = e[0] * d[1] - e[1] * d[0];
                if den == 0.0 {
                    return;
                }
                let w = [p[0] - a[0], p[1] - a[1]];
                let t = (w[0] * d[1] - w[1] * d[0]) / den;
                let u = (w[0] * e[1] - w[1] * e[0]) / den;
                if (0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u) {
                    out.push(t);
                }
            }
            Curve::Arc {
                center,
                radius,
                start,
                sweep,
            } => {
                let w = [p[0] - center[0], p[1] - center[1]];
                let qa = d[0] * d[0] + d[1] * d[1];
                if qa == 0.0 {
                    return;
                }
                let qb = 2.0 * (w[0] * d[0] + w[1] * d[1]);
                let qc = w[0] * w[0] + w[1] * w[1] - radius * radius;
                let disc = qb * qb - 4.0 * qa * qc;
                if disc < 0.0 {
                    return;
                }
                let root = disc.sqrt();
                for u in [(-qb - root) / (2.0 * qa), (-qb + root) / (2.0 * qa)] {
                    if !(0.0..=1.0).contains(&u) {
                        continue;
                    }
                    let angle = (w[1] + u * d[1]).atan2(w[0] + u * d[0]);
                    let turned = if sweep >= 0.0 {
                        (angle - start).rem_euclid(TAU)
                    } else {
                        (start - angle).rem_euclid(TAU)
                    };
                    let t = turned / sweep.abs();
                    if t <= 1.0 {
                        out.push(t);
                    }
                }
            }
        }
    }
}

fn f(p: Point) -> [f64; 2] {
    [p.x.0 as f64, p.y.0 as f64]
}

fn point(p: [f64; 2]) -> Point {
    Point::new(
        reda_model::geom::Nm(p[0].round() as i64),
        reda_model::geom::Nm(p[1].round() as i64),
    )
}

pub(super) fn to_point(p: [f64; 2]) -> Point {
    point(p)
}

/// The stretches of `curve` to keep, as `(from, to)` parameters in order: outside every
/// shape of `forbidden`, and inside `board` when one is given.
pub(super) fn kept(curve: &Curve, forbidden: &Cuts, board: Option<&Area>) -> Vec<(f64, f64)> {
    let b = curve.bounds();
    let near: Vec<&Area> = forbidden.near(b).collect();
    let mut ts = vec![0.0, 1.0];
    let rings = near
        .iter()
        .flat_map(|a| a.iter().flatten())
        .chain(board.into_iter().flat_map(|a| a.iter().flatten()));
    for ring in rings {
        crossings_with_ring(curve, ring, &mut ts);
    }
    ts.sort_by(f64::total_cmp);
    ts.dedup_by(|x, y| (*x - *y).abs() < 1e-12);
    let mut out: Vec<(f64, f64)> = Vec::new();
    for w in ts.windows(2) {
        let (t0, t1) = (w[0], w[1]);
        if t1 - t0 < 1e-12 {
            continue;
        }
        let mid = point(curve.at((t0 + t1) / 2.0));
        let clear = !near.iter().any(|a| inside(a, mid)) && board.is_none_or(|a| inside(a, mid));
        if !clear {
            continue;
        }
        match out.last_mut() {
            Some(last) if last.1 == t0 => last.1 = t1,
            _ => out.push((t0, t1)),
        }
    }
    out
}

fn crossings_with_ring(curve: &Curve, ring: &Contour, out: &mut Vec<f64>) {
    let n = ring.len();
    for i in 0..n {
        let (p, q) = (ring[i], ring[(i + 1) % n]);
        curve.crossings([p.x as f64, p.y as f64], [q.x as f64, q.y as f64], out);
    }
}
