//! Merge Collinear: after a drag, a changed straight segment that lines up with its lone
//! neighbour merges into one segment with it.

use reda_model::geom::{Nm, Point};
use reda_model::{PcbDocument, Track, TrackGeometry, Uuid};

use super::translate::{SNAP, cross, dot, sub, vec};
use super::{Options, Plan};
use crate::copper::{Copper, Item};

/// Merges collinear joints the plan made, while `options.merge_collinear`. `moved` is the
/// Via or Free Pad the plan carries, which counts at its new place.
pub(super) fn run(
    pcb: &PcbDocument,
    copper: &Copper,
    plan: &mut Plan,
    options: Options,
    moved: Option<Item>,
) {
    if !options.merge_collinear {
        return;
    }
    while let Some((keep, gone)) = next(pcb, copper, plan, moved) {
        if let Some(t) = plan.modified.iter_mut().find(|t| t.uuid == keep.uuid) {
            *t = keep;
        } else if let Some(t) = plan.created.iter_mut().find(|t| t.uuid == keep.uuid) {
            *t = keep;
        }
        if plan.created.iter().any(|t| t.uuid == gone) {
            plan.created.retain(|t| t.uuid != gone);
        } else {
            plan.modified.retain(|t| t.uuid != gone);
            plan.deleted.push(gone);
        }
    }
}

/// The board's Tracks as the plan leaves them.
fn after(pcb: &PcbDocument, plan: &Plan) -> Vec<Track> {
    pcb.tracks()
        .filter(|t| !plan.deleted.contains(&t.uuid))
        .map(|t| {
            plan.modified
                .iter()
                .find(|m| m.uuid == t.uuid)
                .unwrap_or(t)
                .clone()
        })
        .chain(plan.created.iter().cloned())
        .collect()
}

/// The next merge: the changed segment, grown over its neighbour, and the neighbour.
fn next(
    pcb: &PcbDocument,
    copper: &Copper,
    plan: &Plan,
    moved: Option<Item>,
) -> Option<(Track, Uuid)> {
    let tracks = after(pcb, plan);
    let changed = plan.modified.iter().chain(&plan.created);
    for t in changed {
        let TrackGeometry::Line { from, to } = t.geometry else {
            continue;
        };
        if t.locked {
            continue;
        }
        for (at, start) in [(from, to), (to, from)] {
            let mut here = tracks.iter().filter(|o| {
                let (p, q) = o.geometry.ends();
                o.layer == t.layer && o.uuid != t.uuid && (p == at || q == at)
            });
            let (Some(o), None) = (here.next(), here.next()) else {
                continue;
            };
            let TrackGeometry::Line { from: p, to: q } = o.geometry else {
                continue;
            };
            if o.locked || o.width != t.width || o.net != t.net {
                continue;
            }
            let beyond = if p == at { q } else { p };
            let along = sub(vec(at), vec(start));
            let on = sub(vec(beyond), vec(at));
            let len = along.0.hypot(along.1);
            if len == 0.0 || (cross(along, on) / len).abs() > SNAP || dot(along, on) <= 0.0 {
                continue;
            }
            if solid(copper, plan, moved, t.layer, at) {
                continue;
            }
            let mut keep = t.clone();
            keep.geometry = if at == to {
                TrackGeometry::Line { from, to: beyond }
            } else {
                TrackGeometry::Line { from: beyond, to }
            };
            return Some((keep, o.uuid));
        }
    }
    None
}

/// Whether a Pad, Free Pad or Via sits at `at` on `layer` once the plan is done.
fn solid(copper: &Copper, plan: &Plan, moved: Option<Item>, layer: Uuid, at: Point) -> bool {
    let fixed = copper.items_at(layer, at).into_iter().any(|i| {
        Some(i) != moved
            && matches!(
                i,
                Item::Pad { .. } | Item::FreePad { .. } | Item::Via { .. }
            )
    });
    let carried = moved.is_some_and(|item| {
        copper
            .items_at(
                layer,
                Point::new(Nm(at.x.0 - plan.by.x.0), Nm(at.y.0 - plan.by.y.0)),
            )
            .contains(&item)
    });
    fixed || carried
}
