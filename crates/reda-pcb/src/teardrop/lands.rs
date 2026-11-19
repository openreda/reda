//! A Track taper starts where its centreline exits the actual posed land on its Layer.
use super::{Junction, path, percent};
use crate::{Copper, Item, placed::placed_pads, rules::teardrop};
use reda_model::geom::{
    Nm, Point,
    grown::{Core, Shape},
};
use reda_model::{PadKind, PcbDocument, TeardropKind, TeardropLocation, Track};

pub(super) fn junction<'a>(
    pcb: &PcbDocument,
    copper: &Copper,
    track: &'a Track,
    at: Point,
    target: Item,
) -> Option<Junction<'a>> {
    let kind = match target {
        Item::Via { uuid } if pcb.via(uuid)?.teardrop => TeardropKind::Via,
        Item::FreePad { uuid } => pad_kind(pcb.free_pad(uuid)?.kind)?,
        Item::Pad { footprint, pad } => {
            let instance = pcb.footprint(footprint)?;
            pad_kind(
                placed_pads(pcb, instance)
                    .into_iter()
                    .find(|p| p.pad == pad)?
                    .kind,
            )?
        }
        _ => return None,
    };
    let rule = teardrop(pcb, track.net, track.layer, kind);
    if !rule.enabled {
        return None;
    }
    let land = copper
        .item_pieces(target)
        .into_iter()
        .find(|(layer, _)| *layer == track.layer)?
        .1;
    let (a, b) = track.geometry.ends();
    let end = if at == a { b } else { a };
    if land.contains(end) {
        return None;
    }
    let dx = (end.x.0 - at.x.0) as f64;
    let dy = (end.y.0 - at.y.0) as f64;
    let span = dx.hypot(dy);
    if span == 0. {
        return None;
    }
    let size = transverse_extent(land, at, [-dy / span, dx / span]);
    if i128::from(track.width.0) * 100 >= i128::from(size.0) * i128::from(rule.skip_ratio) {
        return None;
    }
    let (anchor, inside) = exit(land, at, end, span);
    let requested = percent(size, rule.length, rule.max_length);
    let (mut path, path_tracks) = path::follow(
        pcb,
        copper,
        track,
        target,
        at,
        Nm(requested.0.saturating_add(inside.0)),
    );
    if path.len() < 2 || land.contains(path[1]) {
        return None;
    }
    path[0] = anchor;
    path.dedup();
    (path.len() >= 2).then_some(Junction {
        track,
        target,
        location: TeardropLocation {
            track: track.uuid,
            target: target.object(),
            pad: if let Item::Pad { pad, .. } = target {
                Some(pad)
            } else {
                None
            },
            layer: track.layer,
            at: anchor,
        },
        rule,
        size,
        path,
        path_tracks,
    })
}

fn pad_kind(kind: PadKind) -> Option<TeardropKind> {
    match kind {
        PadKind::Smd => Some(TeardropKind::SmdPad),
        PadKind::PlatedThroughHole => Some(TeardropKind::ThroughHolePad),
        PadKind::NonPlated => None,
    }
}

/// The projected width of a grown shape is the width of its core plus its diameter.
/// Coordinates relative to the Track end avoid subtracting large absolute projections.
fn transverse_extent(shape: &Shape, at: Point, normal: [f64; 2]) -> Nm {
    let mut low = f64::INFINITY;
    let mut high = f64::NEG_INFINITY;
    for p in &shape.prims {
        let mut add = |point: [i64; 2]| {
            let projection =
                (point[0] - at.x.0) as f64 * normal[0] + (point[1] - at.y.0) as f64 * normal[1];
            low = low.min(projection - p.radius as f64);
            high = high.max(projection + p.radius as f64);
        };
        match &p.core {
            Core::Seg(a, b) => {
                add(*a);
                add(*b);
            }
            Core::Poly(points) => points.iter().copied().for_each(add),
        }
    }
    Nm((high - low).round() as i64)
}

/// Plated and SMD lands are convex. The segment starts inside and ends outside,
/// so bisect the transition to the nearest design nanometre.
fn exit(land: &Shape, a: Point, b: Point, span: f64) -> (Point, Nm) {
    let mut lo = 0.;
    let mut hi = 1.;
    while (hi - lo) * span > 0.25 {
        let mid = (lo + hi) * 0.5;
        if land.contains(path::along(a, b, mid)) {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    (path::along(a, b, lo), Nm((lo * span).ceil() as i64))
}
