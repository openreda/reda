//! The rigid move under every drag: a group of segments (and the Via or Free Pad they
//! hang on) moves by one offset, and each end where the group meets copper that stays
//! put is resolved by the slide rules.
//!
//! Every new point is an affine function of how far along the offset the drag gets, so
//! where a neighbour would shrink past nothing, or a moved segment would fold over, the
//! drag is cut back to exactly that fraction: the drag stops there.

use reda_model::geom::{Nm, Point};
use reda_model::{PcbDocument, Track, TrackGeometry, Uuid};

use super::{Options, Plan, move_object};
use crate::copper::{Copper, Item};

pub(super) type V = (f64, f64);

/// Below this a squeezed neighbour counts as gone and a merge as collinear, in nm: what
/// rounding to whole nanometres can leave.
pub(super) const SNAP: f64 = 2.0;

/// A moved segment keeps at least this length, in nm.
const MIN_LENGTH: f64 = 1_000.0;

pub(super) fn vec(p: Point) -> V {
    (p.x.0 as f64, p.y.0 as f64)
}

pub(super) fn point(v: V) -> Point {
    Point::new(Nm(v.0.round() as i64), Nm(v.1.round() as i64))
}

pub(super) fn sub(a: V, b: V) -> V {
    (a.0 - b.0, a.1 - b.1)
}

fn add(a: V, b: V) -> V {
    (a.0 + b.0, a.1 + b.1)
}

fn scale(a: V, k: f64) -> V {
    (a.0 * k, a.1 * k)
}

pub(super) fn dot(a: V, b: V) -> f64 {
    a.0 * b.0 + a.1 * b.1
}

pub(super) fn cross(a: V, b: V) -> f64 {
    a.0 * b.1 - a.1 * b.0
}

fn unit(a: V) -> V {
    let l = a.0.hypot(a.1);
    if l == 0.0 {
        (0.0, 0.0)
    } else {
        scale(a, 1.0 / l)
    }
}

/// The unit normal of the line from `a` to `b` (the direction turned a quarter
/// counter-clockwise).
pub(super) fn normal(a: Point, b: Point) -> V {
    let u = unit(sub(vec(b), vec(a)));
    (-u.1, u.0)
}

/// `geometry` with each end mapped by `f`; an arc keeps its sweep.
pub(super) fn map_ends(geometry: TrackGeometry, f: impl Fn(Point) -> Point) -> TrackGeometry {
    match geometry {
        TrackGeometry::Line { from, to } => TrackGeometry::Line {
            from: f(from),
            to: f(to),
        },
        TrackGeometry::Arc { from, to, sweep } => TrackGeometry::Arc {
            from: f(from),
            to: f(to),
            sweep,
        },
    }
}

/// What a group end meets.
#[derive(Clone, Copy)]
enum Meets<'a> {
    /// Nothing: the end moves with the group.
    Free,
    /// Copper that can't follow: a connector joins it.
    Fixed,
    /// A lone straight neighbour that keeps its line; `far` is its other end.
    Stretch { neighbour: &'a Track, far: Point },
    /// A lone neighbour whose end follows the group (no angle keeping).
    Follow { neighbour: &'a Track },
}

/// One end of a moved segment where the group meets copper that stays put.
struct Joint<'a> {
    member: usize,
    at: Point,
    meets: Meets<'a>,
}

/// Moves `members` (and `object`) by `m`, resolving their outer ends by the slide rules.
/// With `options.preserve_angle` the drag stops where a neighbour runs out; `plan.by`
/// says how far it got.
pub(super) fn run(
    pcb: &PcbDocument,
    copper: &Copper,
    members: &[Uuid],
    object: Option<Item>,
    m: Point,
    options: Options,
    mut new_id: impl FnMut() -> Uuid,
) -> Plan {
    let tracks: Vec<&Track> = members.iter().filter_map(|u| pcb.track(*u)).collect();
    let preserve = options.preserve_angle;
    let mut joints = Vec::new();
    for (i, t) in tracks.iter().enumerate() {
        let (a, b) = t.geometry.ends();
        for at in [a, b] {
            let shared = tracks.iter().enumerate().any(|(j, o)| {
                let (p, q) = o.geometry.ends();
                j != i && o.layer == t.layer && (p == at || q == at)
            });
            let items = copper.items_at(t.layer, at);
            if shared || object.is_some_and(|o| items.contains(&o)) {
                continue;
            }
            let fixed = items.iter().any(|it| {
                matches!(
                    it,
                    Item::Pad { .. } | Item::FreePad { .. } | Item::Via { .. }
                )
            });
            let others = super::joined(pcb, t.layer, at, members);
            let meets = match (&others[..], fixed) {
                (_, true) => Meets::Fixed,
                ([], false) => Meets::Free,
                ([n], false) => classify(t, at, n, preserve),
                _ => Meets::Fixed,
            };
            joints.push(Joint {
                member: i,
                at,
                meets,
            });
        }
    }

    let at_fraction = |k: f64| -> Vec<V> {
        let mv = scale(vec(m), k);
        joints
            .iter()
            .map(|j| resolve(tracks[j.member], j, mv, options))
            .collect()
    };
    let mut k = 1.0_f64;
    if preserve {
        let one = at_fraction(1.0);
        for (j, p1) in joints.iter().zip(&one) {
            if let Meets::Stretch { far, .. } = j.meets {
                let v = unit(sub(vec(j.at), vec(far)));
                let g0 = dot(sub(vec(j.at), vec(far)), v);
                let g1 = dot(sub(*p1, vec(far)), v);
                if g1 < 0.0 {
                    k = k.min(g0 / (g0 - g1));
                }
            }
        }
        for (i, t) in tracks.iter().enumerate() {
            if !matches!(t.geometry, TrackGeometry::Line { .. }) {
                continue;
            }
            let (a, b) = t.geometry.ends();
            let u = unit(sub(vec(b), vec(a)));
            let h0 = dot(sub(vec(b), vec(a)), u);
            let end_at = |e: Point| {
                joints
                    .iter()
                    .zip(&one)
                    .find(|(j, _)| j.member == i && j.at == e)
                    .map_or(add(vec(e), vec(m)), |(_, p)| *p)
            };
            let h1 = dot(sub(end_at(b), end_at(a)), u);
            let min = MIN_LENGTH.min(h0);
            if h1 < min {
                k = k.min((h0 - min) / (h0 - h1));
            }
        }
        k = k.clamp(0.0, 1.0);
    }
    let m = if k < 1.0 { point(scale(vec(m), k)) } else { m };
    let mv = vec(m);
    let ends: Vec<V> = joints
        .iter()
        .map(|j| resolve(tracks[j.member], j, mv, options))
        .collect();

    let mut plan = Plan {
        by: m,
        ..Plan::default()
    };
    // Each member's ends: a joint's resolved point, else moved with the group.
    for (i, t) in tracks.iter().enumerate() {
        let mut moved = (*t).clone();
        moved.geometry = map_ends(t.geometry, |e| {
            joints
                .iter()
                .zip(&ends)
                .find(|(j, _)| j.member == i && j.at == e)
                .map_or(e + m, |(j, p)| match j.meets {
                    Meets::Stretch { far, .. } if gone(j.at, far, *p) => far,
                    _ => point(*p),
                })
        });
        plan.modify(moved);
    }
    for (j, p) in joints.iter().zip(&ends) {
        let t = tracks[j.member];
        match j.meets {
            Meets::Free => {}
            Meets::Fixed => {
                let to = point(*p);
                if to != j.at {
                    plan.created.push(Track {
                        uuid: new_id(),
                        locked: false,
                        layer: t.layer,
                        net: t.net,
                        width: t.width,
                        geometry: TrackGeometry::Line { from: j.at, to },
                    });
                }
            }
            Meets::Stretch { neighbour, far } => {
                if gone(j.at, far, *p) {
                    plan.modified.retain(|o| o.uuid != neighbour.uuid);
                    plan.deleted.push(neighbour.uuid);
                } else {
                    let to = point(*p);
                    restretch(&mut plan, neighbour, j.at, to);
                }
            }
            Meets::Follow { neighbour } => restretch(&mut plan, neighbour, j.at, j.at + m),
        }
    }
    if let Some(item) = object {
        move_object(pcb, &mut plan, item, m);
    }
    plan
}

/// Moves `neighbour`'s end at `at` to `to`, on top of earlier changes to it; a neighbour
/// left with no length goes.
fn restretch(plan: &mut Plan, neighbour: &Track, at: Point, to: Point) {
    if plan.deleted.contains(&neighbour.uuid) {
        return;
    }
    let current = plan
        .modified
        .iter()
        .find(|o| o.uuid == neighbour.uuid)
        .cloned()
        .unwrap_or_else(|| neighbour.clone());
    let mut moved = current.clone();
    moved.geometry = map_ends(current.geometry, |e| if e == at { to } else { e });
    if super::zero(&moved) {
        plan.modified.retain(|o| o.uuid != neighbour.uuid);
        plan.deleted.push(neighbour.uuid);
    } else {
        plan.modify(moved);
    }
}

/// Whether a stretched neighbour from `far` to `at`, its end now at `p`, has shrunk to
/// nothing (or past it).
fn gone(at: Point, far: Point, p: V) -> bool {
    let v = unit(sub(vec(at), vec(far)));
    dot(sub(p, vec(far)), v) <= SNAP
}

fn classify<'a>(t: &Track, at: Point, n: &'a Track, preserve: bool) -> Meets<'a> {
    let TrackGeometry::Line { from, to } = n.geometry else {
        return Meets::Fixed;
    };
    if n.locked {
        return Meets::Fixed;
    }
    let line = matches!(t.geometry, TrackGeometry::Line { .. });
    if !preserve || !line {
        return Meets::Follow { neighbour: n };
    }
    let far = if from == at { to } else { from };
    let (a, b) = t.geometry.ends();
    let along = unit(sub(vec(b), vec(a)));
    let across = sub(vec(far), vec(at));
    if (cross(along, across)).abs() < SNAP {
        // Parallel: its line never meets the moved segment's.
        return Meets::Fixed;
    }
    Meets::Stretch { neighbour: n, far }
}

/// Where the joint `j` of `t` goes when the group moves by `mv`.
fn resolve(t: &Track, j: &Joint<'_>, mv: V, options: Options) -> V {
    let at = vec(j.at);
    let moved = add(at, mv);
    let (a, b) = t.geometry.ends();
    let line = matches!(t.geometry, TrackGeometry::Line { .. });
    let other = if a == j.at { b } else { a };
    // Inward along the segment, from this end.
    let u = unit(sub(vec(other), at));
    match j.meets {
        Meets::Free | Meets::Follow { .. } => moved,
        Meets::Fixed if options.preserve_angle && line => {
            let n = (-u.1, u.0);
            let d = dot(mv, n);
            let side = scale(n, d);
            if options.miter {
                add(add(at, side), scale(u, d.abs()))
            } else {
                add(at, side)
            }
        }
        Meets::Fixed => moved,
        Meets::Stretch { far, .. } => {
            // The moved segment's line through `moved` along `u`, met with the
            // neighbour's line through `far` and `at`.
            let w = sub(at, vec(far));
            let denom = cross(u, w);
            let s = cross(sub(vec(far), moved), w) / denom;
            add(moved, scale(u, s))
        }
    }
}
