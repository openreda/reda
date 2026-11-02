//! The Library panel's previews: a Symbol Unit and a Footprint drawn with egui's painter,
//! fitted into a box. They are small line drawings to recognise a Part by, not the
//! canvas's rendering.

use egui::{Color32, Pos2, Rect, Role, Sense, Shape as Paint, Stroke, Ui, Vec2, WidgetInfo, pos2};
use reda_engine::model::geom::{Millideg, Nm, Point};
use reda_engine::model::{Footprint, FootprintLayer, PadShape, Shape, Symbol, Uuid};

use crate::tr;

const HEIGHT: f32 = 120.0;
const MARGIN: f32 = 8.0;

/// Fits Document points (nm, y up) into a screen rect.
struct Fit {
    rect: Rect,
    center: (f64, f64),
    scale: f64,
}

impl Fit {
    fn new(rect: Rect, lo: (f64, f64), hi: (f64, f64)) -> Fit {
        let inner = rect.shrink(MARGIN);
        let w = (hi.0 - lo.0).max(1.0);
        let h = (hi.1 - lo.1).max(1.0);
        let scale = (f64::from(inner.width()) / w).min(f64::from(inner.height()) / h);
        Fit {
            rect,
            center: ((lo.0 + hi.0) / 2.0, (lo.1 + hi.1) / 2.0),
            scale,
        }
    }

    fn pos(&self, p: Point) -> Pos2 {
        let c = self.rect.center();
        pos2(
            c.x + ((p.x.0 as f64 - self.center.0) * self.scale) as f32,
            c.y - ((p.y.0 as f64 - self.center.1) * self.scale) as f32,
        )
    }

    fn len(&self, n: Nm) -> f32 {
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
    let size = Vec2::new(ui.available_width(), HEIGHT);
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    response.widget_info(|| WidgetInfo::labeled(Role::Image, true, label.clone()));
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, ui.visuals().extreme_bg_color);
    (rect, painter)
}

fn draw_shape(painter: &egui::Painter, fit: &Fit, shape: &Shape, stroke: Stroke, fill: bool) {
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
            let px = fit.len(*size).clamp(6.0, 14.0);
            painter.text(
                fit.pos(*at),
                egui::Align2::CENTER_CENTER,
                text,
                egui::FontId::proportional(px),
                stroke.color,
            );
        }
    }
}

/// Draws one Unit of `symbol` (the first when `unit` is none): its graphics and Pins.
pub fn symbol(ui: &mut Ui, symbol: &Symbol, unit: Option<Uuid>) {
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
    let ink = ui.visuals().text_color();
    let stroke = Stroke::new(1.5, ink);
    for g in &graphics {
        draw_shape(&painter, &fit, &g.shape, stroke, g.fill);
    }
    let pin_colour = ui.visuals().weak_text_color();
    for pin in &pins {
        painter.line_segment(
            [fit.pos(pin.at), fit.pos(pin_end(pin))],
            Stroke::new(1.0, pin_colour),
        );
    }
}

fn layer_colour(ui: &Ui, layer: FootprintLayer) -> Color32 {
    match layer {
        FootprintLayer::SilkscreenTop | FootprintLayer::SilkscreenBottom => {
            ui.visuals().text_color()
        }
        _ => ui.visuals().weak_text_color(),
    }
}

/// Draws `footprint`: its Pads (copper colour) and graphics.
pub fn footprint(ui: &mut Ui, footprint: &Footprint) {
    let mut bounds = Bounds::default();
    footprint
        .graphics
        .iter()
        .for_each(|g| bounds.shape(&g.shape));
    for pad in &footprint.pads {
        let r = (pad.width.0.max(pad.height.0) as f64) / 2.0;
        bounds.add(pad.at, r);
    }
    let (rect, painter) = canvas(ui, tr!("library-footprint-preview"));
    let Some((lo, hi)) = bounds.0 else {
        return;
    };
    let fit = Fit::new(rect, lo, hi);
    let copper = Color32::from_rgb(0xc8, 0x8a, 0x2e);
    for pad in &footprint.pads {
        let quarter = (pad.rotation.0 / 90_000).rem_euclid(2) == 1;
        let (w, h) = if quarter {
            (pad.height, pad.width)
        } else {
            (pad.width, pad.height)
        };
        let c = fit.pos(pad.at);
        let size = Vec2::new(fit.len(w), fit.len(h));
        match pad.shape {
            PadShape::Circle => {
                painter.circle_filled(c, size.x / 2.0, copper);
            }
            PadShape::Oval | PadShape::RoundedRect | PadShape::Rect => {
                let radius = match pad.shape {
                    PadShape::Oval => size.x.min(size.y) / 2.0,
                    PadShape::RoundedRect => size.x.min(size.y) / 4.0,
                    _ => 0.0,
                };
                painter.rect_filled(Rect::from_center_size(c, size), radius, copper);
            }
        }
        if let Some(hole) = pad.hole {
            painter.circle_filled(c, fit.len(hole.width) / 2.0, ui.visuals().extreme_bg_color);
        }
    }
    for g in &footprint.graphics {
        let stroke = Stroke::new(1.0, layer_colour(ui, g.layer));
        draw_shape(&painter, &fit, &g.shape, stroke, g.fill);
    }
}
