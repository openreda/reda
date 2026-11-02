//! Polygon areas for Zone fill, on `i_overlay`'s i64 engine (coordinates in nm).
//!
//! An [`Area`] is a list of shapes, each an outer contour and its holes. Copper grown by
//! a clearance is turned into polygons that always *contain* the exact grown shape (round
//! ends become circumscribed polygons), so a fill cut from them never comes closer than
//! the clearance.

use i_overlay::core::fill_rule::FillRule;
use i_overlay::core::overlay::{Overlay, ShapeType};
use i_overlay::core::overlay_rule::OverlayRule;
use i_overlay::i_float::int::angle::Angle;
use i_overlay::i_float::int::point::IntPoint;
use i_overlay::mesh::int::arc::ArcOptions;
use i_overlay::mesh::int::outline::offset::IntOutlineOffset;
use i_overlay::mesh::int::style::{IntLineJoin, IntOutlineStyle};
use reda_model::geom::{Millideg, Point, Vertex};

use crate::copper::shape::{Core, P, Prim, Shape, arc_points};

pub(crate) type Contour = Vec<IntPoint<i64>>;
/// Shapes of outer contour + holes, as `i_overlay` returns them.
pub(crate) type Area = Vec<Vec<Contour>>;

/// How far a circumscribed polygon may stray outside its circle.
const ROUND_TOLERANCE: f64 = 1_000.0;

pub(crate) fn ip(p: P) -> IntPoint<i64> {
    IntPoint::new(p[0], p[1])
}

pub(crate) fn point(p: IntPoint<i64>) -> Point {
    Point::new(reda_model::geom::Nm(p.x), reda_model::geom::Nm(p.y))
}

/// A closed contour of vertices with straight or arc edges, arcs as chords.
pub(crate) fn contour(vertices: &[Vertex]) -> Contour {
    let n = vertices.len();
    let mut out = Vec::new();
    for (i, v) in vertices.iter().enumerate() {
        let next = vertices[(i + 1) % n].point();
        if v.sweep == Millideg::ZERO {
            out.push(IntPoint::new(v.x.0, v.y.0));
        } else {
            let points = arc_points(v.point(), next, v.sweep);
            out.extend(
                points[..points.len() - 1]
                    .iter()
                    .map(|p| IntPoint::new(p.x.0, p.y.0)),
            );
        }
    }
    out
}

/// The area inside one contour, either winding.
pub(crate) fn polygon(contour: Contour) -> Area {
    union(vec![vec![contour]])
}

fn run(subject: &[Vec<Contour>], clip: &[Vec<Contour>], rule: OverlayRule) -> Area {
    let mut overlay = Overlay::new(subject.len() + clip.len());
    overlay.add_source(subject, ShapeType::Subject);
    overlay.add_source(clip, ShapeType::Clip);
    overlay.overlay(rule, FillRule::NonZero)
}

/// Everything covered by any of the shapes.
pub(crate) fn union(shapes: Vec<Vec<Contour>>) -> Area {
    if shapes.is_empty() {
        return Vec::new();
    }
    run(&shapes, &[], OverlayRule::Subject)
}

pub(crate) fn difference(a: &Area, b: &Area) -> Area {
    if a.is_empty() || b.is_empty() {
        return a.clone();
    }
    run(a, b, OverlayRule::Difference)
}

pub(crate) fn intersection(a: &Area, b: &Area) -> Area {
    if a.is_empty() || b.is_empty() {
        return Vec::new();
    }
    run(a, b, OverlayRule::Intersect)
}

/// Whether two areas overlap with some area.
pub(crate) fn overlaps(a: &Area, b: &Area) -> bool {
    !intersection(a, b).is_empty()
}

/// The area shrunk then grown by `half` (a morphological opening): copper narrower than
/// `2 × half` disappears, the rest keeps its shape up to rounded inside corners. The
/// result never leaves `area`.
pub(crate) fn opened(area: &Area, half: i64) -> Area {
    if half <= 0 || area.is_empty() {
        return area.clone();
    }
    let join = IntLineJoin::Round(ArcOptions {
        max_step: Angle::from_bits(1 << 26),
        rotation_precision: 5,
    });
    let shrunk = area
        .outline(&IntOutlineStyle::new(-half).line_join(join))
        .unwrap_or_default();
    if shrunk.is_empty() {
        return Vec::new();
    }
    let grown = shrunk
        .outline(&IntOutlineStyle::new(half).line_join(join))
        .unwrap_or_default();
    intersection(&grown, area)
}

/// The number of sides of a polygon circumscribing a circle of `radius` that strays at
/// most [`ROUND_TOLERANCE`] outside it.
fn sides(radius: f64) -> usize {
    let n = std::f64::consts::PI / (radius / (radius + ROUND_TOLERANCE)).acos();
    (n.ceil() as usize).clamp(8, 256)
}

/// The corners of a polygon around the origin that contains the circle of `radius`.
fn circle_offsets(radius: i64) -> Vec<P> {
    if radius <= 0 {
        return vec![[0, 0]];
    }
    let n = sides(radius as f64);
    let step = std::f64::consts::TAU / n as f64;
    // The corner radius of the circumscribed polygon, rounded outwards.
    let r = (radius as f64 / (step / 2.0).cos()).ceil() + 1.0;
    (0..n)
        .map(|k| {
            let a = step * k as f64;
            [(r * a.cos()).round() as i64, (r * a.sin()).round() as i64]
        })
        .collect()
}

/// The convex hull of `points`, counter-clockwise; fewer than three points or no area
/// gives an empty contour.
fn hull(mut points: Vec<P>) -> Contour {
    points.sort();
    points.dedup();
    if points.len() < 3 {
        return Vec::new();
    }
    let cross = |o: P, a: P, b: P| -> i128 {
        (a[0] as i128 - o[0] as i128) * (b[1] as i128 - o[1] as i128)
            - (a[1] as i128 - o[1] as i128) * (b[0] as i128 - o[0] as i128)
    };
    let mut lower: Vec<P> = Vec::new();
    for &p in &points {
        while lower.len() >= 2 && cross(lower[lower.len() - 2], lower[lower.len() - 1], p) <= 0 {
            lower.pop();
        }
        lower.push(p);
    }
    let mut upper: Vec<P> = Vec::new();
    for &p in points.iter().rev() {
        while upper.len() >= 2 && cross(upper[upper.len() - 2], upper[upper.len() - 1], p) <= 0 {
            upper.pop();
        }
        upper.push(p);
    }
    lower.pop();
    upper.pop();
    lower.extend(upper);
    if lower.len() < 3 {
        return Vec::new();
    }
    lower.into_iter().map(ip).collect()
}

/// A convex polygon containing the primitive grown by `extra`.
fn grown_prim(prim: &Prim, extra: i64) -> Contour {
    let core: Vec<P> = match &prim.core {
        Core::Seg(a, b) => vec![*a, *b],
        Core::Poly(points) => points.clone(),
    };
    let offsets = circle_offsets(prim.radius + extra);
    let points = core
        .iter()
        .flat_map(|c| offsets.iter().map(move |o| [c[0] + o[0], c[1] + o[1]]))
        .collect();
    hull(points)
}

/// Polygons containing the copper of `shape` grown by `extra` (one convex polygon per
/// primitive; they may overlap).
pub(crate) fn grown(shape: &Shape, extra: i64) -> Vec<Vec<Contour>> {
    shape
        .prims
        .iter()
        .map(|p| grown_prim(p, extra))
        .filter(|c| !c.is_empty())
        .map(|c| vec![c])
        .collect()
}

/// Polygons containing every point within `distance` of the area's edges: a capsule
/// around each edge.
pub(crate) fn edge_band(area: &Area, distance: i64) -> Vec<Vec<Contour>> {
    let mut out = Vec::new();
    for shape in area {
        for ring in shape {
            for i in 0..ring.len() {
                let (a, b) = (ring[i], ring[(i + 1) % ring.len()]);
                let prim = Prim {
                    core: Core::Seg([a.x, a.y], [b.x, b.y]),
                    radius: 0,
                };
                let c = grown_prim(&prim, distance);
                if !c.is_empty() {
                    out.push(vec![c]);
                }
            }
        }
    }
    out
}

/// The area grown by `distance` (a containing approximation).
pub(crate) fn dilated(area: &Area, distance: i64) -> Vec<Vec<Contour>> {
    let mut out = area.clone();
    out.extend(edge_band(area, distance));
    out
}

/// The bounding box of an area.
pub(crate) fn bounds(area: &Area) -> Option<(P, P)> {
    let mut points = area.iter().flatten().flatten();
    let first = points.next()?;
    let mut lo = [first.x, first.y];
    let mut hi = lo;
    for p in points {
        lo = [lo[0].min(p.x), lo[1].min(p.y)];
        hi = [hi[0].max(p.x), hi[1].max(p.y)];
    }
    Some((lo, hi))
}
