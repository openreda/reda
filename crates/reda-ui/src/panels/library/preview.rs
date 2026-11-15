//! The Library panel's previews: a Symbol Unit and a Footprint drawn with egui's painter,
//! fitted into a box. They are small line drawings to recognise a Part by, not the
//! canvas's rendering.

use egui::{Color32, Pos2, Rect, Role, Sense, Shape as Paint, Stroke, Ui, Vec2, WidgetInfo, pos2};
use reda_engine::model::geom::{Millideg, Nm, Point};
use reda_engine::model::{Footprint, FootprintLayer, PadShape, Shape, Symbol, Uuid};

use crate::theme::{Text, TokensExt as _};
use crate::tr;
use crate::widgets;

const HEIGHT: f32 = 120.0;
const MARGIN: f32 = 8.0;
/// The copper of a Footprint preview's Pads: a Layer's colour, the same in both themes.
const COPPER: Color32 = widgets::data_color(0xc8, 0x8a, 0x2e);
/// The least side a Pad is given when it is looked for under the pointer.
const MIN_PAD: f32 = 4.0;
/// The radius of the ring on a probed Pin's end.
const PIN_RING: f32 = 4.0;
/// The sizes a preview's text is kept between, however the drawing is scaled.
const TEXT_SIZES: std::ops::RangeInclusive<f32> = 6.0..=14.0;

/// Fits Document points (nm, y up) into a screen rect.
pub(crate) struct Fit {
    rect: Rect,
    center: (f64, f64),
    scale: f64,
}

impl Fit {
    fn new(rect: Rect, lo: (f64, f64), hi: (f64, f64)) -> Fit {
        Fit::with_margin(rect, lo, hi, MARGIN)
    }

    fn with_margin(rect: Rect, lo: (f64, f64), hi: (f64, f64), margin: f32) -> Fit {
        let inner = rect.shrink(margin);
        let w = (hi.0 - lo.0).max(1.0);
        let h = (hi.1 - lo.1).max(1.0);
        let scale = (f64::from(inner.width()) / w).min(f64::from(inner.height()) / h);
        Fit {
            rect,
            center: ((lo.0 + hi.0) / 2.0, (lo.1 + hi.1) / 2.0),
            scale,
        }
    }

    pub(crate) fn pos(&self, p: Point) -> Pos2 {
        let c = self.rect.center();
        pos2(
            c.x + ((p.x.0 as f64 - self.center.0) * self.scale) as f32,
            c.y - ((p.y.0 as f64 - self.center.1) * self.scale) as f32,
        )
    }

    pub(crate) fn len(&self, n: Nm) -> f32 {
        (n.0 as f64 * self.scale) as f32
    }
}

#[derive(Default)]
struct Bounds(Option<((f64, f64), (f64, f64))>);

impl Bounds {
    fn add(&mut self, p: Point, pad: f64) {
        let (x, y) = (p.x.0 as f64, p.y.0 as f64);
        let (lo, hi) = self.0.get_or_insert(((x, y), (x, y)));
        lo.0 = lo.0.min(x - pad);
        lo.1 = lo.1.min(y - pad);
        hi.0 = hi.0.max(x + pad);
        hi.1 = hi.1.max(y + pad);
    }

    fn shape(&mut self, shape: &Shape) {
        match shape {
            Shape::Line { start, end } | Shape::Rect { start, end } => {
                self.add(*start, 0.0);
                self.add(*end, 0.0);
            }
            Shape::Arc { center, start, .. } => {
                let r = distance(*center, *start);
                self.add(*center, r);
            }
            Shape::Circle { center, radius } => self.add(*center, radius.0 as f64),
            Shape::Polygon { points } => points.iter().for_each(|p| self.add(*p, 0.0)),
            Shape::Text { at, .. } => self.add(*at, 0.0),
        }
    }
}

fn distance(a: Point, b: Point) -> f64 {
    let (dx, dy) = ((b.x.0 - a.x.0) as f64, (b.y.0 - a.y.0) as f64);
    dx.hypot(dy)
}

/// `p` turned by `angle` about the origin.
fn turned(p: (f64, f64), angle: Millideg) -> (f64, f64) {
    let a = (angle.0 as f64 / 1000.0).to_radians();
    let (s, c) = a.sin_cos();
    (p.0 * c - p.1 * s, p.0 * s + p.1 * c)
}

fn point(p: (f64, f64)) -> Point {
    Point::new(Nm(p.0.round() as i64), Nm(p.1.round() as i64))
}

/// A box for a preview, announced by `label`.
fn canvas(ui: &mut Ui, label: String) -> (Rect, egui::Painter) {
    canvas_sized(ui, label, HEIGHT)
}

fn canvas_sized(ui: &mut Ui, label: String, height: f32) -> (Rect, egui::Painter) {
    let size = Vec2::new(ui.available_width(), height);
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    response.widget_info(|| WidgetInfo::labeled(Role::Image, true, label.clone()));
    let painter = ui.painter_at(rect);
    let t = ui.tokens();
    painter.rect_filled(rect, t.radius.mark, t.color.surface.well);
    (rect, painter)
}

/// How a preview's shapes are drawn: the line, and the text style a text is scaled from.
struct Ink<'a> {
    stroke: Stroke,
    text: &'a Text,
}

fn draw_shape(painter: &egui::Painter, fit: &Fit, shape: &Shape, ink: &Ink<'_>, fill: bool) {
    let stroke = ink.stroke;
    match shape {
        Shape::Line { start, end } => {
            painter.line_segment([fit.pos(*start), fit.pos(*end)], stroke);
        }
        Shape::Rect { start, end } => {
            let r = Rect::from_two_pos(fit.pos(*start), fit.pos(*end));
            if fill {
                painter.rect_filled(r, 0.0, stroke.color);
            } else {
                painter.rect_stroke(r, 0.0, stroke, egui::StrokeKind::Middle);
            }
        }
        Shape::Circle { center, radius } => {
            let (c, r) = (fit.pos(*center), fit.len(*radius));
            if fill {
                painter.circle_filled(c, r, stroke.color);
            } else {
                painter.circle_stroke(c, r, stroke);
            }
        }
        Shape::Arc {
            center,
            start,
            sweep,
        } => {
            let from = (
                (start.x.0 - center.x.0) as f64,
                (start.y.0 - center.y.0) as f64,
            );
            let steps = 24;
            let points: Vec<Pos2> = (0..=steps)
                .map(|i| {
                    let angle = Millideg(sweep.0 * i / steps);
                    let (x, y) = turned(from, angle);
                    fit.pos(point((x + center.x.0 as f64, y + center.y.0 as f64)))
                })
                .collect();
            painter.add(Paint::line(points, stroke));
        }
        Shape::Polygon { points } => {
            let points: Vec<Pos2> = points.iter().map(|p| fit.pos(*p)).collect();
            if fill {
                painter.add(Paint::convex_polygon(points, stroke.color, Stroke::NONE));
            } else {
                painter.add(Paint::closed_line(points, stroke));
            }
        }
        Shape::Text { at, text, size, .. } => {
            // The text follows the drawing's scale, within what can be read.
            let mut font = ink.text.font_id();
            font.size = fit.len(*size).clamp(*TEXT_SIZES.start(), *TEXT_SIZES.end());
            painter.text(
                fit.pos(*at),
                egui::Align2::CENTER_CENTER,
                reda_engine::model::text::display(text),
                font,
                stroke.color,
            );
        }
    }
}

/// Draws one Unit of `symbol` (the first when `unit` is none): its graphics and Pins.
pub fn symbol(ui: &mut Ui, symbol: &Symbol, unit: Option<Uuid>) {
    symbol_drawn(ui, symbol, unit, &[]);
}

/// [`symbol`]'s first Unit with the Pins numbered in `lit` marked: the Part editor's
/// side of the Pins Panel cross-probe.
pub fn symbol_probed(ui: &mut Ui, symbol: &Symbol, lit: &[String]) {
    symbol_drawn(ui, symbol, None, lit);
}

fn symbol_drawn(ui: &mut Ui, symbol: &Symbol, unit: Option<Uuid>, lit: &[String]) {
    let Some(unit) = unit.or_else(|| symbol.units.first().map(|u| u.uuid)) else {
        return;
    };
    let graphics: Vec<_> = symbol.graphics.iter().filter(|g| g.unit == unit).collect();
    let pins: Vec<_> = symbol.unit_pins(unit).collect();
    let pin_end = |pin: &&reda_engine::model::Pin| {
        let (dx, dy) = turned((pin.length.0 as f64, 0.0), pin.rotation);
        point((pin.at.x.0 as f64 + dx, pin.at.y.0 as f64 + dy))
    };
    let mut bounds = Bounds::default();
    graphics.iter().for_each(|g| bounds.shape(&g.shape));
    for pin in &pins {
        bounds.add(pin.at, 0.0);
        bounds.add(pin_end(pin), 0.0);
    }
    let (rect, painter) = canvas(ui, tr!("library-symbol-preview"));
    let Some((lo, hi)) = bounds.0 else {
        return;
    };
    let fit = Fit::new(rect, lo, hi);
    let t = ui.tokens();
    let ink = Ink {
        stroke: Stroke::new(widgets::glyph::STROKE, t.color.text.primary),
        text: &t.ty.small,
    };
    for g in &graphics {
        draw_shape(&painter, &fit, &g.shape, &ink, g.fill);
    }
    let pin_colour = t.color.text.muted;
    let lit_colour = t.color.accent.text;
    for pin in &pins {
        let on = lit.contains(&pin.number);
        let (width, colour) = if on {
            (t.stroke.accent_line + t.stroke.hairline, lit_colour)
        } else {
            (t.stroke.hairline, pin_colour)
        };
        painter.line_segment(
            [fit.pos(pin.at), fit.pos(pin_end(pin))],
            Stroke::new(width, colour),
        );
        if on {
            let ring = Stroke::new(widgets::glyph::STROKE, lit_colour);
            painter.circle_stroke(fit.pos(pin.at), PIN_RING, ring);
        }
    }
}

/// [`footprint`] with the Pads numbered in `lit` ringed, giving back the number of the
/// Pad under the pointer: the Part editor's side of the Pins Panel cross-probe.
pub fn footprint_probed(ui: &mut Ui, footprint: &Footprint, lit: &[String]) -> Option<String> {
    let (painter, fit) = footprint_fitted(ui, footprint, HEIGHT, MARGIN)?;
    let pointer = ui
        .ctx()
        .pointer_hover_pos()
        .filter(|p| painter.clip_rect().contains(*p));
    let t = ui.tokens();
    let ring = Stroke::new(t.stroke.accent_line, t.color.accent.text);
    let mut hovered = None;
    for pad in &footprint.pads {
        let quarter = (pad.rotation.0 / 90_000).rem_euclid(2) == 1;
        let (w, h) = if quarter {
            (pad.height, pad.width)
        } else {
            (pad.width, pad.height)
        };
        let centre = pad.outline().center + pad.at;
        let c = fit.pos(centre.rotated_about(pad.at, pad.rotation));
        let size = Vec2::new(fit.len(w), fit.len(h)).max(Vec2::splat(MIN_PAD));
        let r = Rect::from_center_size(c, size);
        if lit.contains(&pad.number) {
            let around = r.expand(t.stroke.accent_line);
            painter.rect_stroke(around, t.radius.mark, ring, egui::StrokeKind::Outside);
        }
        if pointer.is_some_and(|p| r.contains(p)) && !pad.number.is_empty() {
            hovered = Some(pad.number.clone());
        }
    }
    hovered
}

fn layer_colour(ui: &Ui, layer: FootprintLayer) -> Color32 {
    let t = ui.tokens();
    match layer {
        FootprintLayer::SilkscreenTop | FootprintLayer::SilkscreenBottom => t.color.text.primary,
        _ => t.color.text.muted,
    }
}

/// Draws `footprint`: its Pads (copper colour) and graphics.
pub fn footprint(ui: &mut Ui, footprint: &Footprint) {
    footprint_fitted(ui, footprint, HEIGHT, MARGIN);
}

/// [`footprint`] in a box `height` high with `margin` round the drawing, giving back its
/// painter and fit for drawing over it.
pub(crate) fn footprint_fitted(
    ui: &mut Ui,
    footprint: &Footprint,
    height: f32,
    margin: f32,
) -> Option<(egui::Painter, Fit)> {
    let mut bounds = Bounds::default();
    footprint
        .graphics
        .iter()
        .for_each(|g| bounds.shape(&g.shape));
    for pad in &footprint.pads {
        let r = (pad.width.0.max(pad.height.0) as f64) / 2.0;
        bounds.add(pad.at, r);
    }
    let (rect, painter) = canvas_sized(ui, tr!("library-footprint-preview"), height);
    let (lo, hi) = bounds.0?;
    let fit = Fit::with_margin(rect, lo, hi, margin);
    let t = ui.tokens();
    let copper = COPPER;
    for pad in &footprint.pads {
        let quarter = (pad.rotation.0 / 90_000).rem_euclid(2) == 1;
        let (w, h) = if quarter {
            (pad.height, pad.width)
        } else {
            (pad.width, pad.height)
        };
        let centre = pad.outline().center + pad.at;
        let c = fit.pos(centre.rotated_about(pad.at, pad.rotation));
        let size = Vec2::new(fit.len(w), fit.len(h));
        match pad.shape {
            PadShape::Circle => {
                painter.circle_filled(c, size.x / 2.0, copper);
            }
            PadShape::Oval | PadShape::RoundedRect | PadShape::ChamferedRect | PadShape::Rect => {
                let radius = pad.outline().uniform_radius().map_or(0.0, |r| fit.len(r));
                painter.rect_filled(Rect::from_center_size(c, size), radius, copper);
            }
        }
        if let Some(hole) = pad.hole {
            painter.circle_filled(
                fit.pos(pad.at),
                fit.len(hole.width) / 2.0,
                t.color.surface.well,
            );
        }
    }
    for g in &footprint.graphics {
        let ink = Ink {
            stroke: Stroke::new(t.stroke.hairline, layer_colour(ui, g.layer)),
            text: &t.ty.small,
        };
        draw_shape(&painter, &fit, &g.shape, &ink, g.fill);
    }
    Some((painter, fit))
}
