//! View-only labels the renderer generates: Pad numbers, and Net names on Pads and Tracks.
//! Each is a [`GlyphRun`](crate::GlyphRun) marked `view_label`, so the canvas hides it below
//! a minimum on-screen size.

use reda_model::geom::{Millideg, Nm, Point};
use reda_model::{LayerKind, PadKind, TrackGeometry, Uuid};

use super::copper::upright;
use super::{BoardPad, Builder};
use crate::color::BoardColor;
use crate::scene::{Align, HAlign, Primitive, SceneLayer, VAlign};
use crate::text::{TextStyle, advance, glyph_run};
use crate::transform::rotate;

/// A label fills at most this share of the space it sits in.
const FILL: (i64, i64) = (4, 5);

/// Labels of SMD Pads, on their copper Layer.
pub(super) fn pads(b: &mut Builder, pads: &[BoardPad]) {
    for pad in pads.iter().filter(|p| p.kind == PadKind::Smd) {
        if let Some(layer) = b.find(LayerKind::Copper, pad.side) {
            pad_labels(b, pad, SceneLayer::Board(layer));
        }
    }
}

/// Labels of Pads with a hole, over the hole on the drill Layer.
pub(super) fn drilled_pads(b: &mut Builder, pads: &[BoardPad]) {
    for pad in pads.iter().filter(|p| p.kind != PadKind::Smd) {
        pad_labels(b, pad, SceneLayer::Drill);
    }
}

/// The Pad number in the middle of the Pad, along its longer side, with the Net name below
/// it when the Pad is on a Net.
fn pad_labels(b: &mut Builder, pad: &BoardPad, layer: SceneLayer) {
    let net = pad.net.and_then(|n| b.pcb.net(n)).map(|n| n.name.clone());
    let (width, height) = (pad.outline.width, pad.outline.height);
    let (long, short) = (width.max(height), width.min(height));
    let centre = pad.copper_center();
    let rotation = if upright(width, height, pad.rotation) {
        Millideg(90_000)
    } else {
        Millideg(0)
    };
    // Up the label's own y axis, on the board.
    let offset = |d: i64| {
        let (x, y) = rotate(0, d, rotation);
        Point::new(centre.x + Nm(x), centre.y + Nm(y))
    };
    let lines: Vec<(String, BoardColor, i64, i64)> = match (&net, pad.number.is_empty()) {
        (None, true) => Vec::new(),
        (None, false) => vec![(pad.number.clone(), BoardColor::PadNumber, 45, 0)],
        (Some(net), true) => vec![(net.clone(), BoardColor::NetName, 30, 0)],
        (Some(net), false) => vec![
            (pad.number.clone(), BoardColor::PadNumber, 32, 22),
            (net.clone(), BoardColor::NetName, 22, -24),
        ],
    };
    for (text, color, height, shift) in lines {
        let height = fit(&text, Nm(short.0 * height / 100), long);
        let at = offset(short.0 * shift / 100);
        let first = b.scene.items.len();
        label(b, pad.object, layer, color, &text, at, height, rotation);
        for item in &mut b.scene.items[first..] {
            item.child = pad.child.clone();
        }
    }
}

/// Net names along line Tracks, where they fit.
pub(super) fn tracks(b: &mut Builder) {
    let pcb = b.pcb;
    for track in pcb.tracks() {
        let (Some(net), TrackGeometry::Line { from, to }) = (track.net, track.geometry) else {
            continue;
        };
        let Some(net) = pcb.net(net) else { continue };
        let (dx, dy) = ((to.x - from.x).0 as f64, (to.y - from.y).0 as f64);
        let length = Nm(dx.hypot(dy) as i64);
        let height = Nm(track.width.0 / 2);
        if advance(&net.name, height).0 * FILL.1 > length.0 * FILL.0 {
            continue;
        }
        let rotation = Millideg((dy.atan2(dx).to_degrees() * 1000.0).round() as i64);
        let at = Point::new(Nm((from.x.0 + to.x.0) / 2), Nm((from.y.0 + to.y.0) / 2));
        let layer = SceneLayer::Board(track.layer);
        label(
            b,
            track.uuid,
            layer,
            BoardColor::NetName,
            &net.name,
            at,
            height,
            rotation,
        );
    }
}

/// `height`, shrunk so `text` spans at most its share of `room`.
fn fit(text: &str, height: Nm, room: Nm) -> Nm {
    let wide = advance(text, height).0;
    let max = room.0 * FILL.0 / FILL.1;
    if wide <= max || wide == 0 {
        height
    } else {
        Nm((height.0 as i128 * max as i128 / wide as i128) as i64)
    }
}

#[allow(clippy::too_many_arguments)]
fn label(
    b: &mut Builder,
    object: Uuid,
    layer: SceneLayer,
    color: BoardColor,
    text: &str,
    at: Point,
    height: Nm,
    rotation: Millideg,
) {
    if text.is_empty() || height.0 <= 0 {
        return;
    }
    let style = TextStyle {
        height,
        rotation,
        align: Align::new(HAlign::Center, VAlign::Middle),
        view_label: true,
    };
    b.push(
        object,
        layer,
        color,
        Primitive::Glyphs(glyph_run(text, at, style)),
    );
}
