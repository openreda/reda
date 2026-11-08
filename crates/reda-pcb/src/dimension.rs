//! Dimensions on the board: where their Hotspot references are, the value they measure,
//! and their layout as plain lines and text.
//!
//! Outputs, checks, the canvas and `pcb.dimension.explode` all take a Dimension's lines and
//! text from [`drawing`], so a Dimension plots as the lines and text of its Layer.

use reda_command::Fonts;
use reda_model::geom::{Millideg, Nm, Point, arc_centre};
use reda_model::{
    ArrowPosition, Dimension, DimensionFormat, DimensionTextPosition, DimensionUnits, GraphicShape,
    Hotspot, HotspotRef, PcbDocument, PcbText, TextHJustify, TextJustify, TextVJustify,
    TrackGeometry,
};

use crate::placed::{placed_free_pad, placed_pads};
use crate::text_geometry;

/// Where `reference` is on `pcb` now, or `None` when its object or Hotspot is gone.
pub fn hotspot_point(pcb: &PcbDocument, reference: &HotspotRef) -> Option<Point> {
    let uuid = reference.object;
    let vertex = |points: Vec<Point>, i: usize| points.get(i).copied();
    match reference.hotspot {
        Hotspot::Origin => pcb.footprint(uuid).map(|f| f.at),
        Hotspot::Pad(pad) => {
            let instance = pcb.footprint(uuid)?;
            placed_pads(pcb, instance)
                .into_iter()
                .find(|p| p.pad == pad)
                .map(|p| p.at)
        }
        Hotspot::Anchor => pcb.text(uuid).map(|t| t.position),
        Hotspot::Center => {
            if let Some(pad) = pcb.free_pad(uuid) {
                return Some(placed_free_pad(pad).at);
            }
            if let Some(via) = pcb.via(uuid) {
                return Some(via.at);
            }
            if let Some(track) = pcb.track(uuid) {
                return match track.geometry {
                    TrackGeometry::Arc { from, to, sweep } => Some(arc_centre(from, to, sweep)),
                    TrackGeometry::Line { .. } => None,
                };
            }
            match pcb.graphic(uuid)?.shape {
                GraphicShape::Arc { from, to, sweep } => Some(arc_centre(from, to, sweep)),
                _ => None,
            }
        }
        Hotspot::Start | Hotspot::End => {
            let (from, to) = if let Some(track) = pcb.track(uuid) {
                track.geometry.ends()
            } else {
                match pcb.graphic(uuid)?.shape {
                    GraphicShape::Line { from, to } | GraphicShape::Arc { from, to, .. } => {
                        (from, to)
                    }
                    GraphicShape::Rect { .. } => return None,
                }
            };
            Some(match reference.hotspot {
                Hotspot::Start => from,
                _ => to,
            })
        }
        Hotspot::Vertex(i) => {
            let contour = |v: &[reda_model::geom::Vertex]| {
                v.iter().map(|v| Point::new(v.x, v.y)).collect::<Vec<_>>()
            };
            if let Some(outline) = pcb.outline().filter(|o| o.uuid == uuid) {
                return vertex(contour(&outline.vertices), i);
            }
            if let Some(cutout) = pcb.cutout(uuid) {
                return vertex(contour(&cutout.vertices), i);
            }
            if let Some(zone) = pcb.zone(uuid) {
                return vertex(contour(&zone.vertices), i);
            }
            if let Some(keepout) = pcb.keepout(uuid) {
                return vertex(contour(&keepout.vertices), i);
            }
            match pcb.graphic(uuid)?.shape {
                GraphicShape::Rect { from, to, .. } => vertex(
                    vec![from, Point::new(to.x, from.y), to, Point::new(from.x, to.y)],
                    i,
                ),
                _ => None,
            }
        }
    }
}

/// The Dimensions of `pcb` whose ends no longer match their references, brought up to
/// date: an end moves onto its Hotspot, or drops its `ref` (keeping its point) when the
/// object or Hotspot is gone.
pub fn follow(pcb: &PcbDocument) -> Vec<Dimension> {
    let mut out = Vec::new();
    for dimension in pcb.dimensions() {
        let mut next = dimension.clone();
        for end in [&mut next.a, &mut next.b] {
            let Some(reference) = end.reference else {
                continue;
            };
            match hotspot_point(pcb, &reference) {
                Some(point) => end.point = point,
                None => end.reference = None,
            }
        }
        if next != *dimension {
            out.push(next);
        }
    }
    out
}

/// The unit vector along `angle`, exact at quarter turns.
fn direction(angle: Millideg) -> (f64, f64) {
    match angle.normalized().0 {
        0 => (1.0, 0.0),
        90_000 => (0.0, 1.0),
        180_000 => (-1.0, 0.0),
        270_000 => (0.0, -1.0),
        r => {
            let rad = r as f64 / Millideg::TURN.0 as f64 * std::f64::consts::TAU;
            (rad.cos(), rad.sin())
        }
    }
}

/// The distance a Dimension measures: from `a` to `b` along its `rotation`.
pub fn value(dimension: &Dimension) -> Nm {
    let (ux, uy) = direction(dimension.rotation);
    let d = dimension.b.point - dimension.a.point;
    Nm((d.x.0 as f64 * ux + d.y.0 as f64 * uy).abs().round() as i64)
}

/// The text a Dimension shows: prefix, value in its units and format, suffix.
pub fn label(dimension: &Dimension) -> String {
    let nm = value(dimension).0 as f64;
    let (per, unit) = match dimension.units {
        DimensionUnits::Mm | DimensionUnits::Automatic => (1e6, "mm"),
        DimensionUnits::Cm => (1e7, "cm"),
        DimensionUnits::Mil => (25_400.0, "mil"),
        DimensionUnits::In => (25_400_000.0, "in"),
    };
    let number = format!("{:.*}", usize::from(dimension.precision), nm / per);
    let shown = match dimension.format {
        DimensionFormat::None => String::new(),
        DimensionFormat::Value => number,
        DimensionFormat::ValueUnit => format!("{number}{unit}"),
        DimensionFormat::ValueBracketedUnit => format!("{number} ({unit})"),
    };
    format!("{}{shown}{}", dimension.prefix, dimension.suffix)
}

/// One straight pen stroke of a Dimension.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DimensionLine {
    pub from: Point,
    pub to: Point,
    pub width: Nm,
}

/// A Dimension laid out: the lines and text it plots as on its Layer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DimensionDrawing {
    /// The extension lines, the dimension line and the arrowheads.
    pub lines: Vec<DimensionLine>,
    /// The text, with the Dimension's UUID; `None` when it would be empty.
    pub text: Option<PcbText>,
}

/// The angle between an arrowhead side and the dimension line.
const ARROW_HALF_ANGLE_DEG: f64 = 20.0;

#[derive(Clone, Copy)]
struct V(f64, f64);

impl V {
    fn of(p: Point) -> V {
        V(p.x.0 as f64, p.y.0 as f64)
    }
    fn point(self) -> Point {
        Point::new(Nm(self.0.round() as i64), Nm(self.1.round() as i64))
    }
    fn add(self, o: V) -> V {
        V(self.0 + o.0, self.1 + o.1)
    }
    fn scale(self, k: f64) -> V {
        V(self.0 * k, self.1 * k)
    }
    fn dot(self, o: V) -> f64 {
        self.0 * o.0 + self.1 * o.1
    }
    fn sub(self, o: V) -> V {
        V(self.0 - o.0, self.1 - o.1)
    }
}

/// Lays `dimension` out as lines and text. `fonts` sizes the text for `automatic`.
pub fn drawing(dimension: &Dimension, fonts: &Fonts) -> DimensionDrawing {
    let d = dimension;
    let (ux, uy) = direction(d.rotation);
    let u = V(ux, uy);
    let n = V(-uy, ux);
    let a = V::of(d.a.point);
    let offset = d.offset.0 as f64;
    let gap = d.gap.0 as f64;
    let on_line = |s: f64| a.add(n.scale(offset)).add(u.scale(s));
    let mut lines = Vec::new();
    let mut line = |from: V, to: V, width: Nm| {
        let (from, to) = (from.point(), to.point());
        if from != to {
            lines.push(DimensionLine { from, to, width });
        }
    };
    let ends = [d.a.point, d.b.point].map(|p| {
        let p = V::of(p).sub(a);
        (p.dot(u), p.dot(n))
    });
    // Extension lines: from `gap` off each point to `gap` past the dimension line.
    for (s, t) in ends {
        let towards = if offset >= t { 1.0 } else { -1.0 };
        if (offset - t).abs() > gap {
            let start = a.add(u.scale(s)).add(n.scale(t + towards * gap));
            let end = on_line(s).add(n.scale(towards * gap));
            line(start, end, d.extension_width);
        }
    }
    let (s1, s2) = (ends[0].0.min(ends[1].0), ends[0].0.max(ends[1].0));
    let (e1, e2) = (on_line(s1), on_line(s2));
    let size = d.arrow_size.0 as f64;
    let (cos, sin) = (
        ARROW_HALF_ANGLE_DEG.to_radians().cos(),
        ARROW_HALF_ANGLE_DEG.to_radians().sin(),
    );
    let mut arrow = |tip: V, pointing: V| {
        let back = pointing.scale(-size * cos);
        let side = V(-pointing.1, pointing.0).scale(size * sin);
        line(tip, tip.add(back).add(side), d.width);
        line(tip, tip.add(back).sub(side), d.width);
    };
    let reach = match d.arrow_position {
        ArrowPosition::Inside => 0.0,
        ArrowPosition::Outside => d.arrow_length.0 as f64,
    };
    if s2 > s1 {
        match d.arrow_position {
            ArrowPosition::Inside => {
                arrow(e1, u.scale(-1.0));
                arrow(e2, u);
            }
            ArrowPosition::Outside => {
                arrow(e1, u);
                arrow(e2, u.scale(-1.0));
            }
        }
    }
    let (l1, l2) = (e1.add(u.scale(-reach)), e2.add(u.scale(reach)));
    line(l1, l2, d.width);
    let text = text(d, fonts, (u, n), (l1, l2), s2 - s1);
    DimensionDrawing { lines, text }
}

/// The Dimension's text: placed by `text_position` about the dimension line from `l1` to
/// `l2` (along `u`), `span` apart between the extension lines.
fn text(
    d: &Dimension,
    fonts: &Fonts,
    (u, n): (V, V),
    (l1, l2): (V, V),
    span: f64,
) -> Option<PcbText> {
    let label = label(d);
    if label.is_empty() {
        return None;
    }
    // The text reads left to right: turned by the line's angle, half a turn more when that
    // would put it upside down.
    let r = d.rotation.normalized().0;
    let flipped = r > 90_000 && r <= 270_000;
    let rotation = Millideg(if flipped { r - 180_000 } else { r }).normalized();
    let (reads, up) = if flipped {
        (u.scale(-1.0), n.scale(-1.0))
    } else {
        (u, n)
    };
    let (left, right) = if flipped { (l2, l1) } else { (l1, l2) };
    let middle = l1.add(l2).scale(0.5);
    let gap = d.text_gap.0 as f64;
    let above = gap + d.width.0 as f64 / 2.0;
    let mut style = d.style.clone();
    let justify = |h, v| TextJustify { h, v };
    use TextHJustify as H;
    use TextVJustify as Vj;
    let mut position = d.text_position;
    if position == DimensionTextPosition::Automatic {
        let probe = PcbText {
            uuid: d.uuid,
            layer: d.layer,
            text: label.clone(),
            position: Point::default(),
            style: style.clone(),
            frame: None,
            rotation: Millideg::ZERO,
            mirrored: false,
        };
        let width = text_width(&text_geometry(&probe, fonts));
        position = if width + 2.0 * gap <= span {
            DimensionTextPosition::Top
        } else {
            DimensionTextPosition::Right
        };
    }
    let (anchor, rotation, j) = match position {
        DimensionTextPosition::Automatic | DimensionTextPosition::Top => (
            middle.add(up.scale(above)),
            rotation,
            justify(H::Center, Vj::Bottom),
        ),
        DimensionTextPosition::Bottom => (
            middle.add(up.scale(-above)),
            rotation,
            justify(H::Center, Vj::Top),
        ),
        DimensionTextPosition::Center => (middle, rotation, justify(H::Center, Vj::Center)),
        DimensionTextPosition::Left => (
            left.add(reads.scale(-gap)),
            rotation,
            justify(H::Right, Vj::Center),
        ),
        DimensionTextPosition::Right => (
            right.add(reads.scale(gap)),
            rotation,
            justify(H::Left, Vj::Center),
        ),
        DimensionTextPosition::InsideLeft => (
            left.add(reads.scale(gap)).add(up.scale(above)),
            rotation,
            justify(H::Left, Vj::Bottom),
        ),
        DimensionTextPosition::InsideRight => (
            right.add(reads.scale(-gap)).add(up.scale(above)),
            rotation,
            justify(H::Right, Vj::Bottom),
        ),
        DimensionTextPosition::Unidirectional => {
            // Horizontal text, on the side of the line away from the measured points.
            let out = if d.offset.0 < 0 { n.scale(-1.0) } else { n };
            let j = if out.0.abs() > out.1.abs() {
                let h = if out.0 > 0.0 { H::Left } else { H::Right };
                justify(h, Vj::Center)
            } else {
                let v = if out.1 > 0.0 { Vj::Bottom } else { Vj::Top };
                justify(H::Center, v)
            };
            (middle.add(out.scale(above)), Millideg::ZERO, j)
        }
        DimensionTextPosition::Manual(at) => (V::of(at), rotation, d.style.justify),
    };
    style.justify = j;
    Some(PcbText {
        uuid: d.uuid,
        layer: d.layer,
        text: label,
        position: anchor.point(),
        style,
        frame: None,
        rotation,
        mirrored: false,
    })
}

/// The width of text laid out unturned.
fn text_width(text: &crate::PlacedText) -> f64 {
    let half = text.stroke_width.0 / 2;
    let xs = text
        .strokes
        .iter()
        .flat_map(|s| {
            [
                s.a.x.0 - half,
                s.a.x.0 + half,
                s.b.x.0 - half,
                s.b.x.0 + half,
            ]
        })
        .chain(text.fill.points.iter().map(|p| p.x.0));
    let (min, max) = xs.fold((i64::MAX, i64::MIN), |(lo, hi), x| (lo.min(x), hi.max(x)));
    if min > max { 0.0 } else { (max - min) as f64 }
}
