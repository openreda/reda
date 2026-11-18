//! PROTOTYPE (issue #330): what the three variants share. Throwaway.
//!
//! Only the parts that are the same question in every variant: what colour a material
//! is, how a Via barrel is drawn, a number field that reads itself back, the sample
//! picker. Each variant lays its own screen out.

use egui::{Color32, Painter, Pos2, Rect, Response, Sense, Stroke, StrokeKind, Ui, pos2, vec2};

use super::model::{Entry, Kind, Make, PRESETS, Stack};
use crate::theme::{Tokens, TokensExt as _};
use crate::widgets::{self, BadgeTone};

// The board's own colours: data, like a Layer's, the same in light and dark.
pub const COPPER: Color32 = widgets::data_color(0xc8, 0x75, 0x33);
pub const CORE: Color32 = widgets::data_color(0xb5, 0xa6, 0x5a);
pub const PREPREG: Color32 = widgets::data_color(0xd3, 0xc7, 0x8a);

pub fn mask_color(name: &str) -> Color32 {
    match name.trim().to_lowercase().as_str() {
        n if n.contains("red") => widgets::data_color(0xb7, 0x1c, 0x1c),
        n if n.contains("blue") => widgets::data_color(0x15, 0x65, 0xc0),
        n if n.contains("black") => widgets::data_color(0x21, 0x21, 0x21),
        n if n.contains("white") => widgets::data_color(0xf5, 0xf5, 0xf5),
        n if n.contains("yellow") => widgets::data_color(0xf9, 0xa8, 0x25),
        n if n.contains("purple") => widgets::data_color(0x6a, 0x1b, 0x9a),
        _ => widgets::data_color(0x2e, 0x7d, 0x32),
    }
}

pub fn entry_color(e: &Entry) -> Color32 {
    match e.kind {
        Kind::Copper { .. } => COPPER,
        Kind::Dielectric { make: Make::Core } => CORE,
        Kind::Dielectric {
            make: Make::Prepreg,
        } => PREPREG,
    }
}

/// Text in a token style.
pub fn text(ui: &mut Ui, style: &crate::theme::Text, text: impl Into<String>, color: Color32) {
    ui.label(style.rich(text).color(color));
}

pub fn muted(ui: &mut Ui, s: impl Into<String>) {
    let t = ui.tokens();
    text(ui, &t.ty.small, s, t.color.text.muted);
}

pub fn strong(ui: &mut Ui, s: impl Into<String>) {
    let t = ui.tokens();
    text(ui, &t.ty.body_strong, s, t.color.text.primary);
}

/// Paints `text` at `at`, in a token style.
pub fn paint_text(
    painter: &Painter,
    at: Pos2,
    align: egui::Align2,
    style: &crate::theme::Text,
    text: impl ToString,
    color: Color32,
) -> Rect {
    painter.text(at, align, text, style.font_id(), color)
}

/// A number field `width` wide with a unit; true when what is typed changed.
pub fn number(ui: &mut Ui, value: &mut String, width: f32, unit: &str) -> Response {
    let t = ui.tokens();
    let bad = value.trim().parse::<f64>().is_err();
    let mut field = widgets::number_field(value)
        .width(t.size.of(width))
        .invalid(bad);
    if !unit.is_empty() {
        field = field.unit(unit);
    }
    ui.add(field)
}

/// An entry's thickness field: typing a positive number sets the thickness.
pub fn thickness(ui: &mut Ui, e: &mut Entry, width: f32) -> Response {
    let r = number(ui, &mut e.text, width, "mm");
    if r.changed() {
        e.commit_text();
    }
    r
}

/// The sample picker: replaces the Stackup with a preset, as a Fab Preset's template
/// would.
pub fn presets(ui: &mut Ui, s: &mut Stack) {
    let t = ui.tokens();
    let mut picked = s.preset;
    widgets::dropdown("proto-stackup-preset", picked)
        .width(t.size.of(220.0))
        .show(ui, |ui| {
            for p in PRESETS {
                widgets::choice_value(ui, &mut picked, p, p);
            }
        });
    if picked != s.preset {
        *s = Stack::preset(picked);
    }
}

/// The symmetry state as a badge, with what differs on hover.
pub fn symmetry(ui: &mut Ui, s: &Stack) {
    let off = s.asymmetries();
    if off.is_empty() {
        ui.add(widgets::badge("Symmetric").status(widgets::Status::Success));
    } else {
        ui.add(widgets::badge("Not symmetric").status(widgets::Status::Warning))
            .on_hover_text(format!("These differ: {}.", off.join("; ")));
    }
}

/// The total against the thickness ordered, as a badge.
pub fn total(ui: &mut Ui, s: &Stack) {
    let (line, ok) = s.total_line();
    let tone = if ok {
        BadgeTone::Neutral
    } else {
        BadgeTone::Warning
    };
    ui.add(widgets::badge(line).tone(tone));
}

/// How tall each entry's band is drawn in `height`: by thickness when `to_scale`, by kind
/// otherwise (and squeezed to fit).
pub fn band_heights(s: &Stack, height: f32, to_scale: bool) -> Vec<f32> {
    let raw: Vec<f32> = if to_scale {
        let total = s.totals().2 as f32;
        (s.entries.iter())
            .map(|e| (e.um as f32 / total * height).max(2.0))
            .collect()
    } else {
        (s.entries.iter())
            .map(|e| if e.is_copper() { 16.0 } else { 46.0 })
            .collect()
    };
    let sum: f32 = raw.iter().sum();
    let k = if sum > height || to_scale {
        height / sum
    } else {
        1.0
    };
    raw.into_iter().map(|h| (h * k).max(2.0)).collect()
}

/// Each entry's band in a board `width` wide whose top-left is `origin`.
pub fn band_rects(origin: Pos2, width: f32, heights: &[f32]) -> Vec<Rect> {
    let mut y = origin.y;
    heights
        .iter()
        .map(|h| {
            let r = Rect::from_min_size(pos2(origin.x, y), vec2(width, *h));
            y += h;
            r
        })
        .collect()
}

/// Paints one band with its rim.
pub fn paint_band(painter: &Painter, t: &Tokens, rect: Rect, color: Color32) {
    painter.rect_filled(rect, 0.0, color);
    painter.rect_stroke(
        rect,
        0.0,
        Stroke::new(t.stroke.hairline, t.color.stroke.strong),
        StrokeKind::Inside,
    );
}

/// How a Via type's barrel is drawn.
pub struct Barrel {
    pub x: f32,
    /// The copper bands it starts and ends on.
    pub from: Rect,
    pub to: Rect,
    /// The copper bands it passes or lands on, for the pads.
    pub lands: Vec<Rect>,
    /// Half the barrel's width.
    pub half: f32,
    pub micro: bool,
    pub selected: bool,
}

impl Barrel {
    /// The barrel's box, for hit-testing.
    pub fn rect(&self) -> Rect {
        let (top, bottom) = self.span();
        Rect::from_x_y_ranges(self.x - self.half - 4.0..=self.x + self.half + 4.0, top..=bottom)
    }

    fn span(&self) -> (f32, f32) {
        let top = self.from.top().min(self.to.top());
        let bottom = self.from.bottom().max(self.to.bottom());
        (top, bottom)
    }

    /// A drilled hole with plated walls, a pad on every copper Layer it meets; a micro Via
    /// is a cone, wide where the laser enters.
    pub fn paint(&self, painter: &Painter, t: &Tokens) {
        let (top, bottom) = self.span();
        let hole = t.color.surface.base;
        let wall = Stroke::new(2.0, COPPER);
        let rim = Stroke::new(t.stroke.hairline, t.color.stroke.strong);
        for land in &self.lands {
            let pad = Rect::from_x_y_ranges(
                self.x - self.half - 5.0..=self.x + self.half + 5.0,
                land.y_range(),
            );
            painter.rect_filled(pad, 0.0, COPPER);
            painter.rect_stroke(pad, 0.0, rim, StrokeKind::Inside);
        }
        if self.micro {
            // The laser enters from the outer Layer: the wide end is the one it starts on.
            let down = self.from.top() <= self.to.top();
            let (wide, narrow) = if down { (top, bottom) } else { (bottom, top) };
            let k = 0.55;
            let points = vec![
                pos2(self.x - self.half, wide),
                pos2(self.x + self.half, wide),
                pos2(self.x + self.half * k, narrow),
                pos2(self.x - self.half * k, narrow),
            ];
            painter.add(egui::Shape::convex_polygon(points.clone(), hole, Stroke::NONE));
            painter.line_segment([points[0], points[3]], wall);
            painter.line_segment([points[1], points[2]], wall);
            painter.line_segment([points[3], points[2]], wall);
        } else {
            let body = Rect::from_x_y_ranges(self.x - self.half..=self.x + self.half, top..=bottom);
            painter.rect_filled(body, 0.0, hole);
            painter.line_segment([body.left_top(), body.left_bottom()], wall);
            painter.line_segment([body.right_top(), body.right_bottom()], wall);
        }
        if self.selected {
            painter.rect_stroke(
                self.rect().expand(2.0),
                t.radius.mark,
                Stroke::new(t.stroke.accent_line, t.color.accent.solid),
                StrokeKind::Outside,
            );
        }
    }
}

/// The barrel of Via type `via` at `x` over the entry bands `rects`.
pub fn barrel(s: &Stack, via: usize, x: f32, rects: &[Rect], half: f32) -> Barrel {
    let v = &s.vias[via];
    let (a, b) = (v.from.min(v.to), v.from.max(v.to));
    let band = |n: usize| rects[s.copper_entry(n)];
    Barrel {
        x,
        from: band(v.from),
        to: band(v.to),
        lands: (a..=b).map(band).collect(),
        half,
        micro: v.class(s.coppers()) == "Micro",
        selected: s.sel == super::model::Sel::Via(via),
    }
}

/// A click-only hit area over `rect`.
pub fn hit(ui: &Ui, rect: Rect, id: impl egui::AsIdSalt, sense: Sense) -> Response {
    ui.interact(rect, ui.scope_id().with(id), sense)
}

/// A child `Ui` in `rect`, laid out left to right and centred.
pub fn cell<R>(ui: &mut Ui, rect: Rect, add: impl FnOnce(&mut Ui) -> R) -> R {
    let layout = egui::Layout::left_to_right(egui::Align::Center);
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect).layout(layout));
    add(&mut child)
}
