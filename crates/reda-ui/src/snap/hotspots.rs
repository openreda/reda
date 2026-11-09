//! The Hotspots of each Document type, gathered from a Document as [`Target`]s.
//!
//! - Board: Pad centres, Via centres, Track ends, arc ends and centres, outline vertices
//!   (Board Outline, Board Cutouts, drawn lines and rectangles), Footprint origins and
//!   text anchors.
//! - Sheet: Pin ends, Wire ends and vertices, anywhere along a Wire, Port and Power Port
//!   anchors and Net Label anchors.
//! - Footprint: its Pads, drawing vertices, arc ends and centres, text anchors and its
//!   origin.
//! - Symbol: its Pin ends.

use reda_engine::model::geom::{Millideg, Point, Vertex, arc_centre};
use reda_engine::model::{
    Footprint, GraphicShape, LayerKind, PcbDocument, SchematicSheet, Shape as Drawn, Side, Symbol,
    TrackGeometry, Uuid,
};
use reda_engine::pcb::placed::{PlacedPad, placed_free_pad, placed_pads};
use reda_engine::schematic::connectivity::placed_pins;

use super::{Hotspot, OnLayers, Shape, Target};

/// Every Hotspot of the board `pcb`.
pub fn board(pcb: &PcbDocument) -> Vec<Target> {
    let mut out = Vec::new();
    let copper = |side: Side| {
        pcb.layers()
            .find(LayerKind::Copper, Some(side))
            .map_or(OnLayers::Copper, |l| OnLayers::One(l.uuid))
    };
    let pad_layers = |pad: &PlacedPad| match pad.side {
        None => OnLayers::Copper,
        Some(side) => copper(side),
    };
    for instance in pcb.footprints() {
        for pad in placed_pads(pcb, instance) {
            let layers = pad_layers(&pad);
            out.push(Target::point(
                pad.at,
                Hotspot::PadCenter,
                instance.uuid,
                layers,
            ));
        }
        out.push(Target::point(
            instance.at,
            Hotspot::FootprintOrigin,
            instance.uuid,
            OnLayers::Every,
        ));
    }
    for pad in pcb.free_pads() {
        let placed = placed_free_pad(pad);
        let layers = pad_layers(&placed);
        out.push(Target::point(
            placed.at,
            Hotspot::PadCenter,
            pad.uuid,
            layers,
        ));
    }
    for track in pcb.tracks() {
        let on = OnLayers::One(track.layer);
        match track.geometry {
            TrackGeometry::Line { from, to } => {
                for at in [from, to] {
                    out.push(Target::point(at, Hotspot::TrackEnd, track.uuid, on));
                }
            }
            TrackGeometry::Arc { from, to, sweep } => {
                arc(&mut out, from, to, sweep, track.uuid, on);
            }
        }
    }
    for via in pcb.vias() {
        out.push(Target::point(
            via.at,
            Hotspot::ViaCenter,
            via.uuid,
            OnLayers::Copper,
        ));
    }
    for graphic in pcb.graphics() {
        let on = OnLayers::One(graphic.layer);
        match graphic.shape {
            GraphicShape::Line { from, to } => {
                for at in [from, to] {
                    out.push(Target::point(at, Hotspot::OutlineVertex, graphic.uuid, on));
                }
            }
            GraphicShape::Arc { from, to, sweep } => {
                arc(&mut out, from, to, sweep, graphic.uuid, on);
            }
            GraphicShape::Rect { from, to, .. } => {
                for at in corners(from, to) {
                    out.push(Target::point(at, Hotspot::OutlineVertex, graphic.uuid, on));
                }
            }
        }
    }
    if let Some(outline) = pcb.outline() {
        contour(&mut out, &outline.vertices, outline.uuid);
    }
    for cutout in pcb.cutouts() {
        contour(&mut out, &cutout.vertices, cutout.uuid);
    }
    for text in pcb.texts() {
        out.push(Target::point(
            text.position,
            Hotspot::TextAnchor,
            text.uuid,
            OnLayers::One(text.layer),
        ));
    }
    out
}

/// Every Hotspot of `sheet`.
pub fn sheet(sheet: &SchematicSheet) -> Vec<Target> {
    let mut out = Vec::new();
    for (unit, _, at) in placed_pins(sheet) {
        out.push(Target::point(
            at,
            Hotspot::PinEnd,
            unit.uuid,
            OnLayers::Every,
        ));
    }
    for wire in sheet.wires() {
        let last = wire.vertices.len().saturating_sub(1);
        for (i, at) in wire.vertices.iter().enumerate() {
            let kind = match i == 0 || i == last {
                true => Hotspot::WireEnd,
                false => Hotspot::WireVertex,
            };
            out.push(Target::point(*at, kind, wire.uuid, OnLayers::Every));
        }
        for (a, b) in wire.segments() {
            out.push(Target {
                shape: Shape::Segment(a, b),
                kind: Hotspot::Wire,
                object: wire.uuid,
                layers: OnLayers::Every,
            });
        }
    }
    for port in sheet.ports() {
        out.push(Target::point(
            port.at,
            Hotspot::PortAnchor,
            port.uuid,
            OnLayers::Every,
        ));
    }
    for port in sheet.power_ports() {
        out.push(Target::point(
            port.at,
            Hotspot::PortAnchor,
            port.uuid,
            OnLayers::Every,
        ));
    }
    for label in sheet.net_labels() {
        out.push(Target::point(
            label.at,
            Hotspot::LabelAnchor,
            label.uuid,
            OnLayers::Every,
        ));
    }
    out
}

/// Every Hotspot of the Footprint `footprint` in its editor, where there is no active
/// Layer: everything snaps in either Layer mode.
pub fn footprint(footprint: &Footprint) -> Vec<Target> {
    let every = OnLayers::Every;
    let mut out = vec![Target::point(
        Point::default(),
        Hotspot::FootprintOrigin,
        footprint.uuid,
        every,
    )];
    for pad in &footprint.pads {
        out.push(Target::point(pad.at, Hotspot::PadCenter, pad.uuid, every));
    }
    for graphic in &footprint.graphics {
        drawn(&mut out, &graphic.shape, graphic.uuid);
    }
    for text in &footprint.texts {
        out.push(Target::point(
            text.at,
            Hotspot::TextAnchor,
            text.uuid,
            every,
        ));
    }
    out
}

/// Every Hotspot of `symbol` in its editor: its Pin ends.
pub fn symbol(symbol: &Symbol) -> Vec<Target> {
    (symbol.pins.iter())
        .map(|pin| Target::point(pin.at, Hotspot::PinEnd, pin.uuid, OnLayers::Every))
        .collect()
}

/// Leaves out the Targets of `objects` (what a move carries).
pub fn without(targets: Vec<Target>, objects: &[Uuid]) -> Vec<Target> {
    targets
        .into_iter()
        .filter(|t| !objects.contains(&t.object))
        .collect()
}

fn arc(out: &mut Vec<Target>, from: Point, to: Point, sweep: Millideg, object: Uuid, on: OnLayers) {
    for at in [from, to] {
        out.push(Target::point(at, Hotspot::ArcEnd, object, on));
    }
    if sweep != Millideg::ZERO {
        out.push(Target::point(
            arc_centre(from, to, sweep),
            Hotspot::ArcCenter,
            object,
            on,
        ));
    }
}

fn corners(from: Point, to: Point) -> [Point; 4] {
    [from, Point::new(to.x, from.y), to, Point::new(from.x, to.y)]
}

fn contour(out: &mut Vec<Target>, vertices: &[Vertex], object: Uuid) {
    let at = |v: &Vertex| Point::new(v.x, v.y);
    for (i, v) in vertices.iter().enumerate() {
        out.push(Target::point(
            at(v),
            Hotspot::OutlineVertex,
            object,
            OnLayers::Every,
        ));
        if v.sweep != Millideg::ZERO {
            let next = &vertices[(i + 1) % vertices.len()];
            let centre = arc_centre(at(v), at(next), v.sweep);
            out.push(Target::point(
                centre,
                Hotspot::ArcCenter,
                object,
                OnLayers::Every,
            ));
        }
    }
}

fn drawn(out: &mut Vec<Target>, shape: &Drawn, object: Uuid) {
    let every = OnLayers::Every;
    let mut vertex = |at: Point| out.push(Target::point(at, Hotspot::OutlineVertex, object, every));
    match shape {
        Drawn::Line { start, end } => {
            vertex(*start);
            vertex(*end);
        }
        Drawn::Rect { start, end } => corners(*start, *end).into_iter().for_each(vertex),
        Drawn::Polygon { points } => points.iter().copied().for_each(vertex),
        Drawn::Arc {
            center,
            start,
            sweep,
        } => {
            let end = start.rotated_about(*center, *sweep);
            out.push(Target::point(*start, Hotspot::ArcEnd, object, every));
            out.push(Target::point(end, Hotspot::ArcEnd, object, every));
            out.push(Target::point(*center, Hotspot::ArcCenter, object, every));
        }
        Drawn::Circle { center, .. } => {
            out.push(Target::point(*center, Hotspot::ArcCenter, object, every));
        }
        _ => {}
    }
}
