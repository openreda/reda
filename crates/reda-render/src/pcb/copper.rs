//! Tracks, Pads, Vias and drilled holes.

use reda_model::geom::{Millideg, Nm, Point};
use reda_model::{LayerKind, PadKind, Side, TrackGeometry};

use super::{BoardPad, Builder, capsule};
use crate::arc;
use crate::color::BoardColor;
use crate::polygon::triangulate;
use crate::scene::{Primitive, SceneLayer};

pub(super) fn tracks(b: &mut Builder) {
    let pcb = b.pcb;
    for track in pcb.tracks() {
        match track.geometry {
            TrackGeometry::Line { from, to } => {
                b.on_layer(track.uuid, track.layer, capsule(from, to, track.width));
            }
            TrackGeometry::Arc { from, to, sweep } => {
                let points = arc::through(from, to, sweep);
                b.polyline(track.uuid, track.layer, &points, track.width, false);
            }
        }
    }
}

pub(super) fn pads(b: &mut Builder, pads: &[BoardPad]) {
    for pad in pads {
        let shape = pad_shape(pad);
        let both = [Some(Side::Top), Some(Side::Bottom)];
        let (copper, mask, paste): (Vec<_>, Vec<_>, Vec<_>) = match (pad.kind, pad.side) {
            (PadKind::Smd, side) => (
                vec![b.find(LayerKind::Copper, side)],
                vec![b.find(LayerKind::SolderMask, side)],
                vec![b.find(LayerKind::Paste, side)],
            ),
            (PadKind::PlatedThroughHole, _) => (
                b.pcb.copper_layers().map(|l| Some(l.uuid)).collect(),
                both.map(|s| b.find(LayerKind::SolderMask, s)).to_vec(),
                Vec::new(),
            ),
            (PadKind::NonPlated, _) => (
                Vec::new(),
                both.map(|s| b.find(LayerKind::SolderMask, s)).to_vec(),
                Vec::new(),
            ),
        };
        for layer in copper.into_iter().chain(mask).chain(paste).flatten() {
            b.on_layer(pad.object, layer, shape.clone());
            if let Some(item) = b.scene.items.last_mut() {
                item.child = pad.child.clone();
            }
        }
    }
}

/// A Pad's copper as one primitive: a disc, a rounded rectangle, or triangles when its
/// corners differ.
fn pad_shape(pad: &BoardPad) -> Primitive {
    let o = &pad.outline;
    let center = pad.copper_center();
    if o.is_circle() {
        return Primitive::Ring {
            center,
            outer: Nm(o.width.0 / 2),
            inner: Nm(0),
        };
    }
    match o.uniform_radius() {
        Some(corner_radius) => Primitive::Rect {
            center,
            size: (o.width, o.height),
            rotation: pad.rotation.normalized(),
            corner_radius,
            stroke: None,
        },
        None => {
            let points: Vec<Point> = o
                .polygon(PAD_ARC_TOLERANCE)
                .into_iter()
                .map(|p| (p + pad.at).rotated_about(pad.at, pad.rotation))
                .collect();
            Primitive::Triangles(triangulate(&points))
        }
    }
}

/// How far a drawn Pad's chords may stray from its rounded corners.
const PAD_ARC_TOLERANCE: Nm = Nm(1_000);

pub(super) fn vias(b: &mut Builder) {
    let pcb = b.pcb;
    for via in pcb.vias() {
        for layer in pcb.span_layers(via.span) {
            let disc = Primitive::Ring {
                center: via.at,
                outer: Nm(via.diameter.0 / 2),
                inner: Nm(0),
            };
            b.on_layer(via.uuid, layer, disc);
        }
    }
}

/// Every drilled hole, over every Layer: Pads with holes, then Vias.
pub(super) fn holes(b: &mut Builder, pads: &[BoardPad]) {
    for pad in pads {
        let Some(hole) = pad.hole else { continue };
        let primitive = if hole.width == hole.height {
            Primitive::Ring {
                center: pad.at,
                outer: Nm(hole.width.0 / 2),
                inner: Nm(0),
            }
        } else {
            Primitive::Rect {
                center: pad.at,
                size: (hole.width, hole.height),
                rotation: (pad.rotation + hole.rotation).normalized(),
                corner_radius: Nm(hole.width.0.min(hole.height.0) / 2),
                stroke: None,
            }
        };
        b.push(pad.object, SceneLayer::Drill, BoardColor::Hole, primitive);
        if let Some(item) = b.scene.items.last_mut() {
            item.child = pad.child.clone();
        }
    }
    let pcb = b.pcb;
    for via in pcb.vias() {
        let disc = Primitive::Ring {
            center: via.at,
            outer: Nm(via.hole.0 / 2),
            inner: Nm(0),
        };
        b.push(via.uuid, SceneLayer::Drill, BoardColor::Hole, disc);
    }
}

/// Whether a `width` × `height` shape turned by `rotation` is taller than wide on the
/// board.
pub(super) fn upright(width: Nm, height: Nm, rotation: Millideg) -> bool {
    let quarter = (rotation.normalized().0 + 45_000) / 90_000 % 2 == 1;
    (height > width) != quarter
}
