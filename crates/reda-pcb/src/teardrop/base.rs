//! Where a taper meets its target. The base spans the target's copper across the
//! Track, and each base corner is pulled back along the Track onto the target's
//! outline, so the taper flares from the copper instead of a chord that only
//! touches it where the centreline crosses.
use super::lands::transverse_range;
use reda_model::geom::{
    Point,
    grown::{Core, Shape},
};

/// One side of the base: its distance from the centreline and how far along the
/// Track its corner sits from the anchor (zero or negative, back into the target).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Side {
    pub half: f64,
    pub shift: f64,
}

pub(super) struct Base {
    shape: Option<Shape>,
    origin: [f64; 2],
    along: [f64; 2],
    normal: [f64; 2],
    /// How far the target reaches from the centreline on the `+normal` and `-normal` sides.
    reach: [f64; 2],
}

impl Base {
    /// The base at the start of `path`, against `shape`, the target's copper on the
    /// Track's Layer. Without a shape the base is centred on the Track as before.
    pub fn new(shape: Option<Shape>, path: &[Point]) -> Base {
        let (a, b) = (path[0], path[1]);
        let (dx, dy) = ((b.x.0 - a.x.0) as f64, (b.y.0 - a.y.0) as f64);
        let d = dx.hypot(dy).max(1.);
        let along = [dx / d, dy / d];
        let normal = [-along[1], along[0]];
        let reach = shape.as_ref().map_or([f64::INFINITY; 2], |s| {
            let (low, high) = transverse_range(s, a, normal);
            [high.max(0.), (-low).max(0.)]
        });
        Base {
            shape,
            origin: [a.x.0 as f64, a.y.0 as f64],
            along,
            normal,
            reach,
        }
    }

    /// The two sides, `+normal` first, of a base `width` across. The base keeps its
    /// width where the target is wide enough, moving to the far side whatever would
    /// overhang the near side of the target, and is never narrower than the Track.
    pub fn sides(&self, width: f64, tip: f64) -> [Side; 2] {
        let mut half = self.reach.map(|reach| (width / 2.).min(reach).max(0.));
        for side in 0..2 {
            let spare = (width - half[0] - half[1]).max(0.);
            half[side] += spare.min(self.reach[side] - half[side]).max(0.);
        }
        [0, 1].map(|side| {
            let h = half[side].max(tip / 2.);
            let sign = if side == 0 { 1. } else { -1. };
            let start = [
                self.origin[0] + sign * h * self.normal[0],
                self.origin[1] + sign * h * self.normal[1],
            ];
            let shift = self
                .shape
                .as_ref()
                .map_or(0., |s| exit(s, start, self.along).min(0.));
            Side { half: h, shift }
        })
    }
}

/// The furthest point along `dir` from `start` that lies on `shape`, as a distance.
/// Each primitive is convex, so its distance along the line is convex: find its
/// closest approach, then bisect outwards to its edge. A line that only grazes a
/// primitive within half a nanometre meets it at the closest approach.
fn exit(shape: &Shape, start: [f64; 2], dir: [f64; 2]) -> f64 {
    let (lo, hi) = shape.bounds();
    let span = [lo, hi, [lo[0], hi[1]], [hi[0], lo[1]]]
        .iter()
        .map(|c| (c[0] as f64 - start[0]).hypot(c[1] as f64 - start[1]))
        .fold(0., f64::max)
        + 1.;
    let at = |t: f64| [start[0] + t * dir[0], start[1] + t * dir[1]];
    let mut best: Option<f64> = None;
    let mut nearest = (f64::INFINITY, 0.);
    for prim in &shape.prims {
        let gap = |t: f64| core_distance(&prim.core, at(t)) - prim.radius as f64;
        let (mut a, mut b) = (-span, span);
        for _ in 0..200 {
            let m1 = a + (b - a) / 3.;
            let m2 = b - (b - a) / 3.;
            if gap(m1) <= gap(m2) {
                b = m2;
            } else {
                a = m1;
            }
        }
        let closest = (a + b) / 2.;
        let least = gap(closest);
        if least < nearest.0 {
            nearest = (least, closest);
        }
        if least > 0.5 {
            continue;
        }
        let limit = least.max(0.) + 1e-9;
        let (mut inside, mut outside) = (closest, span);
        while outside - inside > 0.01 {
            let mid = (inside + outside) / 2.;
            if gap(mid) <= limit {
                inside = mid;
            } else {
                outside = mid;
            }
        }
        best = Some(best.map_or(inside, |b: f64| b.max(inside)));
    }
    best.unwrap_or(nearest.1)
}

fn core_distance(core: &Core, p: [f64; 2]) -> f64 {
    let f = |q: [i64; 2]| [q[0] as f64, q[1] as f64];
    match core {
        Core::Seg(a, b) => segment_distance(f(*a), f(*b), p),
        Core::Poly(points) => {
            let mut sign = 0.;
            let mut inside = true;
            let mut best = f64::INFINITY;
            for i in 0..points.len() {
                let a = f(points[i]);
                let b = f(points[(i + 1) % points.len()]);
                let cross = (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0]);
                if cross != 0. {
                    if sign == 0. {
                        sign = cross.signum();
                    } else if cross.signum() != sign {
                        inside = false;
                    }
                }
                best = best.min(segment_distance(a, b, p));
            }
            if inside { 0. } else { best }
        }
    }
}

fn segment_distance(a: [f64; 2], b: [f64; 2], p: [f64; 2]) -> f64 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let len = dx * dx + dy * dy;
    let t = if len == 0. {
        0.
    } else {
        (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / len).clamp(0., 1.)
    };
    (p[0] - a[0] - t * dx).hypot(p[1] - a[1] - t * dy)
}

#[cfg(test)]
mod tests {
    use super::*;
    use reda_model::geom::Nm;

    fn pt(x: i64, y: i64) -> Point {
        Point::new(Nm(x), Nm(y))
    }

    #[test]
    fn a_round_lands_full_width_base_meets_it_at_the_tangent_points() {
        let via = Shape::disc(pt(5_000_000, 5_000_000), Nm(1_000_000));
        let base = Base::new(
            Some(via),
            &[pt(5_500_000, 5_000_000), pt(6_000_000, 5_000_000)],
        );
        for side in base.sides(1_000_000., 200_000.) {
            assert!((side.half - 500_000.).abs() < 1e-6);
            assert!((side.shift + 500_000.).abs() < 2., "{side:?}");
        }
    }

    #[test]
    fn an_off_centre_base_moves_its_overhang_to_the_far_side() {
        let pad = Shape {
            prims: vec![reda_model::geom::grown::Prim {
                core: Core::Poly(vec![
                    [29_000_000, 41_500_000],
                    [31_000_000, 41_500_000],
                    [31_000_000, 42_500_000],
                    [29_000_000, 42_500_000],
                ]),
                radius: 0,
            }],
        };
        let base = Base::new(
            Some(pad),
            &[pt(31_000_000, 42_350_000), pt(32_000_000, 42_350_000)],
        );
        let [top, bottom] = base.sides(1_000_000., 200_000.);
        assert!((top.half - 150_000.).abs() < 1e-6);
        assert!((bottom.half - 850_000.).abs() < 1e-6);
        assert!(top.shift.abs() < 1. && bottom.shift.abs() < 1.);
    }
}
