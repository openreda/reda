//! Closest boundary points for check markers, using the same primitives as distance.
use super::{Core, P, Shape, edges, poly_contains, segments_cross};
use crate::{Nm, Point};

type F = [f64; 2];
fn fp(p: P) -> F {
    [p[0] as f64, p[1] as f64]
}
fn distance(a: F, b: F) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}
fn projection(p: P, a: P, b: P) -> F {
    let (p, a, b) = (fp(p), fp(a), fp(b));
    let d = [b[0] - a[0], b[1] - a[1]];
    let len = d[0] * d[0] + d[1] * d[1];
    let t = if len == 0.0 {
        0.0
    } else {
        ((p[0] - a[0]) * d[0] + (p[1] - a[1]) * d[1]) / len
    }
    .clamp(0.0, 1.0);
    [a[0] + t * d[0], a[1] + t * d[1]]
}
fn segments(a: P, b: P, c: P, d: P) -> (F, F) {
    if segments_cross(a, b, c, d) {
        let (af, bf, cf, df) = (fp(a), fp(b), fp(c), fp(d));
        let (u, v) = (
            [bf[0] - af[0], bf[1] - af[1]],
            [df[0] - cf[0], df[1] - cf[1]],
        );
        let cross = u[0] * v[1] - u[1] * v[0];
        if cross != 0.0 {
            let t = ((cf[0] - af[0]) * v[1] - (cf[1] - af[1]) * v[0]) / cross;
            let p = [af[0] + t * u[0], af[1] + t * u[1]];
            return (p, p);
        }
    }
    [
        (fp(a), projection(a, c, d)),
        (fp(b), projection(b, c, d)),
        (projection(c, a, b), fp(c)),
        (projection(d, a, b), fp(d)),
    ]
    .into_iter()
    .min_by(|(a, b), (c, d)| distance(*a, *b).total_cmp(&distance(*c, *d)))
    .unwrap()
}
fn core_segments(core: &Core) -> Vec<(P, P)> {
    match core {
        Core::Seg(a, b) => vec![(*a, *b)],
        Core::Poly(p) => edges(p).collect(),
    }
}
fn cores(a: &Core, b: &Core) -> (F, F) {
    if let Core::Poly(poly) = a {
        for p in b.points() {
            if poly_contains(poly, p) {
                return (fp(p), fp(p));
            }
        }
    }
    if let Core::Poly(poly) = b {
        for p in a.points() {
            if poly_contains(poly, p) {
                return (fp(p), fp(p));
            }
        }
    }
    core_segments(a)
        .into_iter()
        .flat_map(|(a0, a1)| {
            core_segments(b)
                .into_iter()
                .map(move |(b0, b1)| segments(a0, a1, b0, b1))
        })
        .min_by(|(a, b), (c, d)| distance(*a, *b).total_cmp(&distance(*c, *d)))
        .unwrap()
}

impl Shape {
    /// The nearest points on the two shapes. Overlap returns one shared point.
    pub fn closest_points(&self, other: &Self) -> [Point; 2] {
        let mut best = (f64::INFINITY, [0.0, 0.0], [0.0, 0.0]);
        for a in &self.prims {
            for b in &other.prims {
                let (p, q) = cores(&a.core, &b.core);
                let d = distance(p, q);
                let gap = d - a.radius as f64 - b.radius as f64;
                if gap >= best.0 {
                    continue;
                }
                let (p, q) = if d == 0.0 {
                    (p, p)
                } else {
                    let unit = [(q[0] - p[0]) / d, (q[1] - p[1]) / d];
                    let pa = [
                        p[0] + unit[0] * a.radius as f64,
                        p[1] + unit[1] * a.radius as f64,
                    ];
                    let qb = [
                        q[0] - unit[0] * b.radius as f64,
                        q[1] - unit[1] * b.radius as f64,
                    ];
                    if gap <= 0.0 {
                        let t = ((d - b.radius as f64).max(0.0) + (a.radius as f64).min(d)) / 2.0;
                        let at = [p[0] + unit[0] * t, p[1] + unit[1] * t];
                        (at, at)
                    } else {
                        (pa, qb)
                    }
                };
                best = (gap, p, q);
            }
        }
        let point = |p: F| Point::new(Nm(p[0].round() as i64), Nm(p[1].round() as i64));
        [point(best.1), point(best.2)]
    }
}
