//! Hit-testing a Scene on the CPU: which object is under a point, and which objects lie
//! wholly inside a rectangle (the rubber band). It works on any Document's Scene, so the
//! schematic and PCB Tools share it; each Tool decides which objects it can pick.

use std::collections::BTreeMap;

use reda_engine::model::Uuid;
use reda_engine::model::geom::{Nm, Point};
use reda_engine::render::{Composite, Item, Primitive, Scene, SceneLayer};

/// The Layers that count, topmost first: the composited ones for a board (`layers`), or
/// every Layer of the Scene.
fn order(scene: &Scene, layers: Option<&[Composite]>) -> Vec<SceneLayer> {
    match layers {
        Some(layers) => layers.iter().rev().map(|c| c.layer).collect(),
        None => {
            let mut seen: Vec<SceneLayer> = Vec::new();
            for item in scene.items.iter().rev() {
                if !seen.contains(&item.layer) {
                    seen.push(item.layer);
                }
            }
            seen
        }
    }
}

/// The topmost object drawn within `tolerance` of `at` that `pick` accepts, on the
/// Layers drawn (`layers`, as composited; `None` = every Layer).
pub fn hit(
    scene: &Scene,
    layers: Option<&[Composite]>,
    at: Point,
    tolerance: Nm,
    pick: impl Fn(&Item) -> bool,
) -> Option<Uuid> {
    for layer in order(scene, layers) {
        let found = scene
            .items
            .iter()
            .rev()
            .filter(|i| i.layer == layer && pick(i))
            .find(|i| contains(&i.primitive, at, tolerance));
        if let Some(item) = found {
            return Some(item.object);
        }
    }
    None
}

/// The objects `pick` accepts whose drawn items on the Layers drawn all lie inside the
/// box `a`..`b` (either corner first), in Scene order.
pub fn inside(
    scene: &Scene,
    layers: Option<&[Composite]>,
    a: Point,
    b: Point,
    pick: impl Fn(&Item) -> bool,
) -> Vec<Uuid> {
    let (lo, hi) = (
        Point::new(a.x.min(b.x), a.y.min(b.y)),
        Point::new(a.x.max(b.x), a.y.max(b.y)),
    );
    let shown = order(scene, layers);
    // Per object: whether every item so far lies inside, in first-seen order.
    let mut all_in: BTreeMap<Uuid, bool> = BTreeMap::new();
    let mut first: Vec<Uuid> = Vec::new();
    for item in scene.items.iter() {
        if !shown.contains(&item.layer) || !pick(item) {
            continue;
        }
        let Some((ilo, ihi)) = item.primitive.bounds() else {
            continue;
        };
        let within = ilo.x >= lo.x && ilo.y >= lo.y && ihi.x <= hi.x && ihi.y <= hi.y;
        match all_in.get_mut(&item.object) {
            Some(ok) => *ok &= within,
            None => {
                all_in.insert(item.object, within);
                first.push(item.object);
            }
        }
    }
    first.into_iter().filter(|o| all_in[o]).collect()
}

/// Whether `at` is within `tol` of the primitive's ink.
pub fn contains(primitive: &Primitive, at: Point, tol: Nm) -> bool {
    let (px, py) = (at.x.0 as f64, at.y.0 as f64);
    let tol = tol.0 as f64;
    match primitive {
        Primitive::Capsule { a, b, width } => {
            segment_distance((px, py), xy(*a), xy(*b)) <= width.0 as f64 / 2.0 + tol
        }
        Primitive::Rect {
            center,
            size,
            rotation,
            stroke,
            ..
        } => {
            // Into the rectangle's own frame.
            let angle = -(rotation.0 as f64 / 1000.0).to_radians();
            let (dx, dy) = (px - center.x.0 as f64, py - center.y.0 as f64);
            let (lx, ly) = (
                dx * angle.cos() - dy * angle.sin(),
                dx * angle.sin() + dy * angle.cos(),
            );
            let pad = stroke.map_or(0.0, |w| w.0 as f64 / 2.0) + tol;
            let (hw, hh) = (size.0.0 as f64 / 2.0, size.1.0 as f64 / 2.0);
            let outside = lx.abs() > hw + pad || ly.abs() > hh + pad;
            if outside {
                return false;
            }
            match stroke {
                None => true,
                // An outline: near one of its edges.
                Some(_) => lx.abs() >= hw - pad || ly.abs() >= hh - pad,
            }
        }
        Primitive::Ring {
            center,
            outer,
            inner,
        } => {
            let d = ((px - center.x.0 as f64).powi(2) + (py - center.y.0 as f64).powi(2)).sqrt();
            d <= outer.0 as f64 + tol && d + tol >= inner.0 as f64
        }
        Primitive::Triangles(mesh) => mesh.indices.chunks_exact(3).any(|t| {
            let corner = |i: u32| xy(mesh.points[i as usize]);
            in_triangle((px, py), corner(t[0]), corner(t[1]), corner(t[2]))
        }),
        Primitive::Glyphs(_) => primitive_bounds_near(primitive, at, Nm(tol as i64)),
    }
}

fn primitive_bounds_near(primitive: &Primitive, at: Point, tol: Nm) -> bool {
    primitive.bounds().is_some_and(|(lo, hi)| {
        at.x >= lo.x - tol && at.x <= hi.x + tol && at.y >= lo.y - tol && at.y <= hi.y + tol
    })
}

fn xy(p: Point) -> (f64, f64) {
    (p.x.0 as f64, p.y.0 as f64)
}

fn segment_distance(p: (f64, f64), a: (f64, f64), b: (f64, f64)) -> f64 {
    let (vx, vy) = (b.0 - a.0, b.1 - a.1);
    let len2 = vx * vx + vy * vy;
    let t = if len2 == 0.0 {
        0.0
    } else {
        (((p.0 - a.0) * vx + (p.1 - a.1) * vy) / len2).clamp(0.0, 1.0)
    };
    let (cx, cy) = (a.0 + t * vx, a.1 + t * vy);
    ((p.0 - cx).powi(2) + (p.1 - cy).powi(2)).sqrt()
}

fn in_triangle(p: (f64, f64), a: (f64, f64), b: (f64, f64), c: (f64, f64)) -> bool {
    let cross = |o: (f64, f64), u: (f64, f64), v: (f64, f64)| {
        (u.0 - o.0) * (v.1 - o.1) - (u.1 - o.1) * (v.0 - o.0)
    };
    let (d1, d2, d3) = (cross(p, a, b), cross(p, b, c), cross(p, c, a));
    let neg = d1 < 0.0 || d2 < 0.0 || d3 < 0.0;
    let pos = d1 > 0.0 || d2 > 0.0 || d3 > 0.0;
    !(neg && pos)
}
