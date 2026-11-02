//! Picking objects in a Scene: by a point within a tolerance, or by a box.
//!
//! Distances are measured to each primitive's ink: a Wire's capsule, a filled or outlined
//! rect (inside an outline counts as on it, so a click inside a Unit's body picks it), a
//! ring as a disc, and text and triangles by their bounds. A linear scan is plenty for a
//! sheet; a spatial index can replace it behind the same functions when boards need one.

use reda_engine::model::Uuid;
use reda_engine::model::geom::Point;
use reda_engine::render::{Item, Primitive, Scene};

/// A Document point in nm, y up.
type P = (f64, f64);

/// How a box picks objects.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoxMode {
    /// Every item of the object lies inside the box.
    Enclose,
    /// Any item of the object touches the box.
    Touch,
}

impl BoxMode {
    /// A box dragged left to right encloses; right to left, it touches.
    pub fn of_drag(from: P, to: P) -> BoxMode {
        if to.0 >= from.0 {
            BoxMode::Enclose
        } else {
            BoxMode::Touch
        }
    }
}

/// The pickable object whose ink is nearest `at`, if any is within `tolerance` nm. On a
/// tie the one drawn on top (later in the Scene) wins.
pub fn hit_test(
    scene: &Scene,
    at: P,
    tolerance: f64,
    pickable: impl Fn(Uuid) -> bool,
) -> Option<Uuid> {
    let mut best: Option<(f64, Uuid)> = None;
    for item in &scene.items {
        let d = distance(&item.primitive, at);
        if d > tolerance || best.is_some_and(|(b, _)| d > b) || !pickable(item.object) {
            continue;
        }
        best = Some((d, item.object));
    }
    best.map(|(_, uuid)| uuid)
}

/// The pickable objects the box between corners `a` and `b` picks, in Scene order.
pub fn box_select(
    scene: &Scene,
    a: P,
    b: P,
    mode: BoxMode,
    pickable: impl Fn(Uuid) -> bool,
) -> Vec<Uuid> {
    let lo = (a.0.min(b.0), a.1.min(b.1));
    let hi = (a.0.max(b.0), a.1.max(b.1));
    let mut seen: Vec<Uuid> = Vec::new();
    let mut out: Vec<Uuid> = Vec::new();
    for item in &scene.items {
        if seen.contains(&item.object) {
            continue;
        }
        seen.push(item.object);
        if !pickable(item.object) {
            continue;
        }
        let items = || scene.items.iter().filter(|i| i.object == item.object);
        let picked = match mode {
            BoxMode::Enclose => items().all(|i| inside(i, lo, hi)),
            BoxMode::Touch => items().any(|i| touches(i, lo, hi)),
        };
        if picked {
            out.push(item.object);
        }
    }
    out
}

fn f(p: Point) -> P {
    (p.x.0 as f64, p.y.0 as f64)
}

fn bounds(item: &Item) -> Option<(P, P)> {
    item.primitive.bounds().map(|(lo, hi)| (f(lo), f(hi)))
}

fn inside(item: &Item, lo: P, hi: P) -> bool {
    bounds(item).is_none_or(|(a, b)| a.0 >= lo.0 && a.1 >= lo.1 && b.0 <= hi.0 && b.1 <= hi.1)
}

fn touches(item: &Item, lo: P, hi: P) -> bool {
    let Some((a, b)) = bounds(item) else {
        return false;
    };
    if a.0 > hi.0 || b.0 < lo.0 || a.1 > hi.1 || b.1 < lo.1 {
        return false;
    }
    match &item.primitive {
        Primitive::Capsule { a, b, width } => {
            let r = width.0 as f64 / 2.0;
            segment_touches_box(f(*a), f(*b), (lo.0 - r, lo.1 - r), (hi.0 + r, hi.1 + r))
        }
        _ => true,
    }
}

/// Whether the segment `a`–`b` meets the box `lo`..`hi` (Liang–Barsky clipping).
fn segment_touches_box(a: P, b: P, lo: P, hi: P) -> bool {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let (mut t0, mut t1) = (0.0f64, 1.0f64);
    for (p, q) in [
        (-dx, a.0 - lo.0),
        (dx, hi.0 - a.0),
        (-dy, a.1 - lo.1),
        (dy, hi.1 - a.1),
    ] {
        if p == 0.0 {
            if q < 0.0 {
                return false;
            }
        } else {
            let t = q / p;
            if p < 0.0 {
                t0 = t0.max(t);
            } else {
                t1 = t1.min(t);
            }
            if t0 > t1 {
                return false;
            }
        }
    }
    true
}

/// The distance in nm from `at` to the primitive's ink; 0 on or inside it.
fn distance(primitive: &Primitive, at: P) -> f64 {
    match primitive {
        Primitive::Capsule { a, b, width } => {
            (segment_distance(at, f(*a), f(*b)) - width.0 as f64 / 2.0).max(0.0)
        }
        Primitive::Rect {
            center,
            size,
            rotation,
            stroke,
            ..
        } => {
            let c = f(*center);
            let angle = (rotation.0 as f64 / 1000.0).to_radians();
            let (s, co) = angle.sin_cos();
            let (x, y) = (at.0 - c.0, at.1 - c.1);
            let local = (x * co + y * s, -x * s + y * co);
            let pad = stroke.map_or(0.0, |w| w.0 as f64 / 2.0);
            let half = (size.0.0 as f64 / 2.0 + pad, size.1.0 as f64 / 2.0 + pad);
            let dx = (local.0.abs() - half.0).max(0.0);
            let dy = (local.1.abs() - half.1).max(0.0);
            dx.hypot(dy)
        }
        Primitive::Ring { center, outer, .. } => {
            let c = f(*center);
            ((at.0 - c.0).hypot(at.1 - c.1) - outer.0 as f64).max(0.0)
        }
        other => match other.bounds() {
            Some((lo, hi)) => {
                let (lo, hi) = (f(lo), f(hi));
                let dx = (lo.0 - at.0).max(at.0 - hi.0).max(0.0);
                let dy = (lo.1 - at.1).max(at.1 - hi.1).max(0.0);
                dx.hypot(dy)
            }
            None => f64::INFINITY,
        },
    }
}

/// The distance from `p` to the segment `a`–`b`.
pub(super) fn segment_distance(p: P, a: P, b: P) -> f64 {
    let c = closest_on_segment(p, a, b);
    (p.0 - c.0).hypot(p.1 - c.1)
}

/// The point of the segment `a`–`b` nearest `p`.
pub(super) fn closest_on_segment(p: P, a: P, b: P) -> P {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len2 = dx * dx + dy * dy;
    let t = if len2 > 0.0 {
        (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / len2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (a.0 + dx * t, a.1 + dy * t)
}
