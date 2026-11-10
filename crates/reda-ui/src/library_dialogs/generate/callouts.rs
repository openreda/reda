//! The dimension callouts on a generator form's preview: the copper's overall span each
//! way, Pad 1's size, and the pitch from Pad 1 to Pad 2 when they sit in a row.

use egui::{Color32, Painter, Stroke, pos2, vec2};
use reda_engine::model::Footprint;
use reda_engine::model::geom::{Nm, Point};

use crate::panels::library::preview::Fit;

/// The direction a callout measures in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    X,
    Y,
}

/// Which side of what it measures a callout is drawn on: below or left (`Low`), above or
/// right (`High`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Low,
    High,
}

/// One dimension: `from`–`to` along `axis`, drawn beside the edge at `edge` on `side`.
#[derive(Clone, Debug, PartialEq)]
pub struct Callout {
    pub axis: Axis,
    pub from: Nm,
    pub to: Nm,
    /// The coordinate, on the other axis, of the edge the callout stands off from.
    pub edge: Nm,
    pub side: Side,
    /// The length in millimetres, such as `2.45`.
    pub text: String,
}

/// The callouts for `footprint`'s Pads; none without Pads.
pub fn callouts(footprint: &Footprint) -> Vec<Callout> {
    let boxes: Vec<_> = footprint.pads.iter().map(|p| p.bounds()).collect();
    let Some(first) = boxes.first() else {
        return Vec::new();
    };
    let (mut lo, mut hi) = (first.min, first.max);
    for b in &boxes {
        lo = Point::new(lo.x.min(b.min.x), lo.y.min(b.min.y));
        hi = Point::new(hi.x.max(b.max.x), hi.y.max(b.max.y));
    }
    let callout = |axis, from: Nm, to: Nm, edge, side| Callout {
        axis,
        from,
        to,
        edge,
        side,
        text: length(Nm(to.0 - from.0)),
    };
    let mut out = vec![
        callout(Axis::X, lo.x, hi.x, lo.y, Side::Low),
        callout(Axis::Y, lo.y, hi.y, lo.x, Side::Low),
    ];
    let one = footprint.pads.iter().position(|p| p.number == "1");
    if let Some(i) = one {
        let b = boxes[i];
        out.push(callout(Axis::X, b.min.x, b.max.x, hi.y, Side::High));
        out.push(callout(Axis::Y, b.min.y, b.max.y, hi.x, Side::High));
        let a = footprint.pads[i].at;
        let two = footprint
            .pads
            .iter()
            .find(|p| p.number == "2")
            .map(|p| p.at);
        if footprint.pads.len() > 2
            && let Some(b2) = two
        {
            if a.y == b2.y && a.x != b2.x {
                out.push(callout(
                    Axis::X,
                    a.x.min(b2.x),
                    a.x.max(b2.x),
                    hi.y,
                    Side::High,
                ));
            } else if a.x == b2.x && a.y != b2.y {
                out.push(callout(
                    Axis::Y,
                    a.y.min(b2.y),
                    a.y.max(b2.y),
                    lo.x,
                    Side::Low,
                ));
            }
        }
    }
    out
}

/// `nm` in millimetres with two decimals at least, without the unit.
fn length(nm: Nm) -> String {
    super::mm(nm.0).trim_end_matches("mm").to_owned()
}

/// The space the callouts need around the drawing, in points.
pub const ROOM: f32 = 44.0;
const STEP: f32 = 14.0;

/// Paints `callouts` over a preview drawn with `fit`; each one further out than the one
/// before on its side.
pub fn paint(painter: &Painter, fit: &Fit, callouts: &[Callout], colour: Color32) {
    let stroke = Stroke::new(1.0, colour);
    let font = egui::FontId::proportional(10.0);
    let mut placed: Vec<(Axis, Side)> = Vec::new();
    for c in callouts {
        let level = placed.iter().filter(|p| **p == (c.axis, c.side)).count() as f32;
        placed.push((c.axis, c.side));
        let gap = 8.0 + STEP * level;
        let (a, b, out) = match c.axis {
            Axis::X => {
                let a = fit.pos(Point::new(c.from, c.edge));
                let b = fit.pos(Point::new(c.to, c.edge));
                let dir = if c.side == Side::Low { 1.0 } else { -1.0 };
                (a, b, vec2(0.0, dir * gap))
            }
            Axis::Y => {
                let a = fit.pos(Point::new(c.edge, c.from));
                let b = fit.pos(Point::new(c.edge, c.to));
                let dir = if c.side == Side::Low { -1.0 } else { 1.0 };
                (a, b, vec2(dir * gap, 0.0))
            }
        };
        let (da, db) = (a + out, b + out);
        painter.line_segment([a, da], stroke);
        painter.line_segment([b, db], stroke);
        painter.line_segment([da, db], stroke);
        let tick = out.normalized() * 3.0;
        for end in [da, db] {
            painter.line_segment([end - tick, end + tick], stroke);
        }
        let middle = pos2((da.x + db.x) / 2.0, (da.y + db.y) / 2.0) + out.normalized() * 6.0;
        painter.text(
            middle,
            egui::Align2::CENTER_CENTER,
            &c.text,
            font.clone(),
            colour,
        );
    }
}
