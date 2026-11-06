//! Copper shapes and the distance between them.
//!
//! Every piece of copper is a union of primitives, each a core (a segment, possibly of zero
//! length, or a convex polygon) grown by a radius: a Track is a segment grown by half its
//! width, a round Pad a point grown by its radius, a rectangular Pad a polygon grown by
//! nothing, a rounded one a smaller polygon grown by its corner radius, and one with mixed corners
//! the polygon through its corners with a disc in each rounded one. The distance
//! between two primitives is the distance between their cores less both radii, so it is
//! negative where they overlap and 0 where they touch.
//!
//! Orientation and intersection tests are exact in `i128`; distances are `f64`.

use reda_model::geom::{CornerRect, Millideg, Nm, Point};
use reda_model::{FootprintInstance, FreePad, Pad, Side, TrackGeometry};
use rstar::AABB;

pub(crate) type P = [i64; 2];

/// The core of a primitive.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Core {
    /// A segment; a point when both ends are the same.
    Seg(P, P),
    /// A convex polygon of at least three corners, either winding.
    Poly(Vec<P>),
}

/// A core grown by `radius`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Prim {
    pub core: Core,
    pub radius: i64,
}

/// A piece of copper on one Layer.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Shape {
    pub prims: Vec<Prim>,
}

impl Shape {
    pub fn disc(at: Point, diameter: Nm) -> Shape {
        let p = [at.x.0, at.y.0];
        Shape {
            prims: vec![Prim {
                core: Core::Seg(p, p),
                radius: diameter.0 / 2,
            }],
        }
    }

    /// A Track's copper: its centreline grown by half the width; an arc as a chain of
    /// chords within 1 µm of it.
    pub fn track(geometry: &TrackGeometry, width: Nm) -> Shape {
        let radius = width.0 / 2;
        let points = match *geometry {
            TrackGeometry::Line { from, to } => vec![from, to],
            TrackGeometry::Arc { from, to, sweep } => arc_points(from, to, sweep),
        };
        let prims = points
            .windows(2)
            .map(|w| Prim {
                core: Core::Seg([w[0].x.0, w[0].y.0], [w[1].x.0, w[1].y.0]),
                radius,
            })
            .collect();
        Shape { prims }
    }

    /// A Pad's copper as placed on the board by `instance`.
    pub fn pad(instance: &FootprintInstance, pad: &Pad) -> Shape {
        outline_shape(&pad.outline(), |p| {
            let p = rotate(p, pad.rotation);
            board_point(instance, [p[0] + pad.at.x.0, p[1] + pad.at.y.0])
        })
    }

    /// A Free Pad's copper: it is drawn in board coordinates.
    pub fn free_pad(pad: &FreePad) -> Shape {
        outline_shape(&pad.outline(), |p| {
            let p = rotate(p, pad.rotation);
            [p[0] + pad.at.x.0, p[1] + pad.at.y.0]
        })
    }

    pub fn envelope(&self) -> AABB<P> {
        let mut lo = [i64::MAX, i64::MAX];
        let mut hi = [i64::MIN, i64::MIN];
        for prim in &self.prims {
            let r = prim.radius;
            for p in prim.core.points() {
                lo = [lo[0].min(p[0] - r), lo[1].min(p[1] - r)];
                hi = [hi[0].max(p[0] + r), hi[1].max(p[1] + r)];
            }
        }
        AABB::from_corners(lo, hi)
    }

    /// The gap between the two shapes' copper: negative where they overlap.
    pub fn distance(&self, other: &Shape) -> f64 {
        let mut best = f64::INFINITY;
        for a in &self.prims {
            for b in &other.prims {
                let d = core_distance(&a.core, &b.core) - a.radius as f64 - b.radius as f64;
                best = best.min(d);
            }
        }
        best
    }

    /// Whether the copper touches or overlaps `other`'s.
    pub fn touches(&self, other: &Shape) -> bool {
        self.distance(other) <= TOUCH
    }

    /// Whether `point` lies on this copper (its edge included).
    pub fn contains(&self, point: Point) -> bool {
        self.touches(&Shape::disc(point, Nm::ZERO))
    }
}

/// A Pad outline (in the Pad's frame) with each core point moved onto the board by
/// `place`. Uniform corners make one rectangle grown by their radius; otherwise the
/// outline is the polygon through its corners with a disc in each rounded one.
fn outline_shape(outline: &CornerRect, place: impl Fn(P) -> P) -> Shape {
    let (cx, cy) = (outline.center.x.0, outline.center.y.0);
    let moved = |core: Core| match core {
        Core::Seg(a, b) => Core::Seg(place([a[0] + cx, a[1] + cy]), place([b[0] + cx, b[1] + cy])),
        Core::Poly(points) => Core::Poly(
            points
                .into_iter()
                .map(|p| place([p[0] + cx, p[1] + cy]))
                .collect(),
        ),
    };
    let (hw, hh) = (outline.width.0 / 2, outline.height.0 / 2);
    if let Some(r) = outline.uniform_radius() {
        let r = r.0.clamp(0, hw.min(hh));
        return Shape {
            prims: vec![Prim {
                core: moved(rect_core(hw - r, hh - r)),
                radius: r,
            }],
        };
    }
    let corners: Vec<P> = outline
        .contour()
        .iter()
        .map(|v| place([v.x.0, v.y.0]))
        .collect();
    let mut prims = vec![Prim {
        core: Core::Poly(corners),
        radius: 0,
    }];
    prims.extend(outline.round_corners().into_iter().map(|(c, r)| {
        let c = place([c.x.0, c.y.0]);
        Prim {
            core: Core::Seg(c, c),
            radius: r.0,
        }
    }));
    Shape { prims }
}

/// Copper closer than half a nanometre touches: coordinates are whole nanometres.
const TOUCH: f64 = 0.5;

impl Core {
    fn points(&self) -> Vec<P> {
        match self {
            Core::Seg(a, b) => vec![*a, *b],
            Core::Poly(points) => points.clone(),
        }
    }
}

/// The core of a rectangle with half-sizes `hw` × `hh` about the origin, as a segment or
/// a point when it has no area.
fn rect_core(hw: i64, hh: i64) -> Core {
    match (hw, hh) {
        (0, _) => Core::Seg([0, -hh], [0, hh]),
        (_, 0) => Core::Seg([-hw, 0], [hw, 0]),
        _ => Core::Poly(vec![[-hw, -hh], [hw, -hh], [hw, hh], [-hw, hh]]),
    }
}

/// A point of a Footprint (relative to its origin) on the board: flipped for the bottom
/// side, then turned, then moved to the instance's position.
pub(crate) fn board_point(instance: &FootprintInstance, p: P) -> P {
    let p = match instance.side {
        Side::Top => p,
        Side::Bottom => [-p[0], p[1]],
    };
    let p = rotate(p, instance.rotation);
    [p[0] + instance.at.x.0, p[1] + instance.at.y.0]
}

/// `p` turned counter-clockwise about the origin; exact for quarter turns.
pub(crate) fn rotate(p: P, angle: Millideg) -> P {
    let (x, y) = reda_model::geom::rotate(p[0], p[1], angle);
    [x, y]
}

/// The corners of a chain of chords along the arc from `from` to `to` sweeping `sweep`,
/// none further than 1 µm from the arc.
pub(crate) fn arc_points(from: Point, to: Point, sweep: Millideg) -> Vec<Point> {
    let theta = (sweep.0 as f64 / 1000.0).to_radians();
    let (fx, fy) = (from.x.0 as f64, from.y.0 as f64);
    let (tx, ty) = (to.x.0 as f64, to.y.0 as f64);
    let (cx, cy) = (tx - fx, ty - fy);
    let chord = cx.hypot(cy);
    if theta == 0.0 || chord == 0.0 {
        return vec![from, to];
    }
    let half = chord / 2.0;
    let offset = half / (theta / 2.0).tan();
    let (mx, my) = ((fx + tx) / 2.0, (fy + ty) / 2.0);
    let (ox, oy) = (mx - cy / chord * offset, my + cx / chord * offset);
    let radius = (fx - ox).hypot(fy - oy);
    let step = if radius > 1000.0 {
        2.0 * (1.0 - 1000.0 / radius).acos()
    } else {
        theta.abs()
    };
    let n = ((theta.abs() / step).ceil() as usize).clamp(1, 720);
    let start = (fy - oy).atan2(fx - ox);
    let mut points = vec![from];
    for i in 1..n {
        let a = start + theta * i as f64 / n as f64;
        points.push(Point::new(
            Nm((ox + radius * a.cos()).round() as i64),
            Nm((oy + radius * a.sin()).round() as i64),
        ));
    }
    points.push(to);
    points
}

fn orient(a: P, b: P, c: P) -> i128 {
    let (ax, ay) = (a[0] as i128, a[1] as i128);
    (b[0] as i128 - ax) * (c[1] as i128 - ay) - (b[1] as i128 - ay) * (c[0] as i128 - ax)
}

fn within(a: P, b: P, p: P) -> bool {
    p[0] >= a[0].min(b[0])
        && p[0] <= a[0].max(b[0])
        && p[1] >= a[1].min(b[1])
        && p[1] <= a[1].max(b[1])
}

fn segments_cross(a: P, b: P, c: P, d: P) -> bool {
    let (o1, o2) = (orient(a, b, c).signum(), orient(a, b, d).signum());
    let (o3, o4) = (orient(c, d, a).signum(), orient(c, d, b).signum());
    if o1 * o2 < 0 && o3 * o4 < 0 {
        return true;
    }
    (o1 == 0 && within(a, b, c))
        || (o2 == 0 && within(a, b, d))
        || (o3 == 0 && within(c, d, a))
        || (o4 == 0 && within(c, d, b))
}

fn point_segment(p: P, a: P, b: P) -> f64 {
    let (px, py) = (p[0] as f64, p[1] as f64);
    let (ax, ay) = (a[0] as f64, a[1] as f64);
    let (dx, dy) = (b[0] as f64 - ax, b[1] as f64 - ay);
    let len2 = dx * dx + dy * dy;
    let t = if len2 == 0.0 {
        0.0
    } else {
        (((px - ax) * dx + (py - ay) * dy) / len2).clamp(0.0, 1.0)
    };
    (px - ax - t * dx).hypot(py - ay - t * dy)
}

fn segment_segment(a: P, b: P, c: P, d: P) -> f64 {
    if segments_cross(a, b, c, d) {
        return 0.0;
    }
    point_segment(a, c, d)
        .min(point_segment(b, c, d))
        .min(point_segment(c, a, b))
        .min(point_segment(d, a, b))
}

fn poly_contains(poly: &[P], p: P) -> bool {
    let n = poly.len();
    let (mut pos, mut neg) = (false, false);
    for i in 0..n {
        let o = orient(poly[i], poly[(i + 1) % n], p);
        pos |= o > 0;
        neg |= o < 0;
    }
    !(pos && neg)
}

fn edges(poly: &[P]) -> impl Iterator<Item = (P, P)> + '_ {
    (0..poly.len()).map(move |i| (poly[i], poly[(i + 1) % poly.len()]))
}

fn poly_segment(poly: &[P], a: P, b: P) -> f64 {
    if poly_contains(poly, a) || poly_contains(poly, b) {
        return 0.0;
    }
    edges(poly)
        .map(|(c, d)| segment_segment(a, b, c, d))
        .fold(f64::INFINITY, f64::min)
}

fn core_distance(a: &Core, b: &Core) -> f64 {
    match (a, b) {
        (Core::Seg(a0, a1), Core::Seg(b0, b1)) => segment_segment(*a0, *a1, *b0, *b1),
        (Core::Poly(poly), Core::Seg(s0, s1)) | (Core::Seg(s0, s1), Core::Poly(poly)) => {
            poly_segment(poly, *s0, *s1)
        }
        (Core::Poly(pa), Core::Poly(pb)) => {
            if pa.iter().any(|p| poly_contains(pb, *p)) || pb.iter().any(|p| poly_contains(pa, *p))
            {
                return 0.0;
            }
            edges(pa)
                .map(|(a0, a1)| poly_segment(pb, a0, a1))
                .fold(f64::INFINITY, f64::min)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(a: P, b: P, radius: i64) -> Shape {
        Shape {
            prims: vec![Prim {
                core: Core::Seg(a, b),
                radius,
            }],
        }
    }

    #[test]
    fn segments_sharing_an_end_touch_and_parallel_ones_keep_their_gap() {
        let a = seg([0, 0], [100, 0], 10);
        assert!(a.touches(&seg([100, 0], [100, 100], 10)));
        assert_eq!(a.distance(&seg([0, 50], [100, 50], 10)), 30.0);
        assert_eq!(a.distance(&seg([50, -50], [50, 50], 0)), -10.0, "crossing");
    }

    #[test]
    fn a_square_holds_its_inside_and_measures_to_its_edge() {
        let square = Shape {
            prims: vec![Prim {
                core: Core::Poly(vec![[0, 0], [100, 0], [100, 100], [0, 100]]),
                radius: 0,
            }],
        };
        assert!(square.contains(Point::new(Nm(50), Nm(50))));
        assert!(square.contains(Point::new(Nm(100), Nm(30))));
        assert_eq!(square.distance(&seg([150, 50], [200, 50], 0)), 50.0);
        let corner = square.distance(&seg([130, 130], [130, 130], 0));
        assert!((corner - 42.426).abs() < 0.001, "{corner}");
    }

    #[test]
    fn a_quarter_arc_stays_on_its_circle() {
        let mm = |v: i64| Nm::from_mm(v);
        let points = arc_points(
            Point::new(mm(10), Nm(0)),
            Point::new(Nm(0), mm(10)),
            Millideg::from_deg(90),
        );
        assert!(points.len() > 2);
        for p in &points {
            let r = (p.x.0 as f64).hypot(p.y.0 as f64);
            assert!((r - 10_000_000.0).abs() < 2.0, "{p:?} is off the circle");
        }
    }
}
