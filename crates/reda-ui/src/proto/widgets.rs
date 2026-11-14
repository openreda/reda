//! PROTOTYPE (issue #276): the recipes. Each widget reads only `Tokens`, and takes a
//! forced state so the gallery can show rest, hover, press, focus and disabled side by
//! side. `St::Live` follows the pointer and keyboard.

use std::sync::Arc;

use egui::{
    Align2, Color32, Galley, Pos2, Rect, Response, Sense, Stroke, StrokeKind, Ui, Vec2, pos2, vec2,
};

use super::{Text, Tokens, Tone, over};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum St {
    Live,
    Rest,
    Hover,
    Press,
    Focus,
    Disabled,
}

/// What a widget shows: hover as 0..1 (animated when live), press, keyboard focus.
pub struct Seen {
    pub hover: f32,
    pub press: bool,
    pub focus: bool,
}

pub fn seen(ui: &Ui, t: &Tokens, response: &Response, st: St) -> Seen {
    match st {
        St::Live => Seen {
            hover: ui.ctx().animate_bool_with_time(
                response.id.with("hover"),
                response.hovered(),
                t.motion.fast,
            ),
            press: response.is_pointer_button_down_on(),
            focus: response.has_focus(),
        },
        St::Hover => Seen { hover: 1.0, press: false, focus: false },
        St::Press => Seen { hover: 1.0, press: true, focus: false },
        St::Focus => Seen { hover: 0.0, press: false, focus: true },
        St::Rest | St::Disabled => Seen { hover: 0.0, press: false, focus: false },
    }
}

/// Runs `add` at the disabled opacity when `st` is disabled: once per widget.
pub fn stated<R>(ui: &mut Ui, t: &Tokens, st: St, add: impl FnOnce(&mut Ui) -> R) -> R {
    if st == St::Disabled {
        ui.scope(|ui| {
            ui.multiply_opacity(t.opacity_disabled);
            add(ui)
        })
        .inner
    } else {
        add(ui)
    }
}

fn fade(c: Color32, k: f32) -> Color32 {
    c.gamma_multiply(k)
}

/// A neutral state overlay on `under`: hover fades in, press replaces it.
fn state_fill(t: &Tokens, under: Color32, s: &Seen) -> Color32 {
    let top = if s.press {
        t.color.fill.press
    } else {
        fade(t.color.fill.hover, s.hover)
    };
    if under == Color32::TRANSPARENT {
        top
    } else {
        over(top, under)
    }
}

pub fn galley(ui: &Ui, style: &Text, text: &str, color: Color32) -> Arc<Galley> {
    ui.painter().layout_job(style.job(text, color))
}

pub fn width(ui: &Ui, style: &Text, text: &str) -> f32 {
    galley(ui, style, text, Color32::WHITE).size().x
}

/// Paints `text` in `rect`, vertically centred on its line height.
pub fn text_in(
    ui: &Ui,
    rect: Rect,
    align: Align2,
    style: &Text,
    text: &str,
    color: Color32,
) -> Rect {
    let g = galley(ui, style, text, color);
    let at = align.align_size_within_rect(g.size(), rect).min;
    let at = pos2(at.x.round(), at.y.round());
    let drawn = Rect::from_min_size(at, g.size());
    ui.painter().galley(at, g, color);
    drawn
}

/// Text cut with an ellipsis to fit `rect`.
pub fn text_clipped(ui: &Ui, rect: Rect, style: &Text, text: &str, color: Color32) {
    let mut job = style.job(text, color);
    job.wrap.max_width = rect.width();
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    let g = ui.painter().layout_job(job);
    let at = pos2(rect.left(), (rect.center().y - g.size().y / 2.0).round());
    ui.painter().galley(at, g, color);
}

pub fn label(ui: &mut Ui, style: &Text, color: Color32, text: &str) -> Response {
    let g = galley(ui, style, text, color);
    let (rect, response) = ui.allocate_exact_size(g.size(), Sense::hover());
    ui.painter().galley(rect.min, g, color);
    response
}

/// A wrapped paragraph.
pub fn paragraph(ui: &mut Ui, style: &Text, color: Color32, text: &str, wrap: f32) {
    let mut job = style.job(text, color);
    job.wrap.max_width = wrap;
    let g = ui.painter().layout_job(job);
    let (rect, _) = ui.allocate_exact_size(g.size(), Sense::hover());
    ui.painter().galley(rect.min, g, color);
}

pub fn icon(ui: &Ui, key: &str, center: Pos2, size: f32, enabled: bool) {
    let rect = Rect::from_center_size(center, Vec2::splat(size));
    if let Some(image) = crate::icons::image(key, size, ui.ctx().theme(), enabled) {
        image.paint_at(ui, rect);
    } else {
        ui.painter().rect_stroke(
            rect.shrink(2.0),
            2.0,
            Stroke::new(1.0, Color32::GRAY),
            StrokeKind::Inside,
        );
    }
}

pub fn focus_ring(ui: &Ui, t: &Tokens, rect: Rect, radius: f32) {
    ui.painter().rect_stroke(
        rect.expand(1.0),
        radius + 1.0,
        Stroke::new(t.focus_ring, t.color.stroke.focus_ring),
        StrokeKind::Outside,
    );
}

// --- Glyphs REDA paints by hand today (the icon ticket decides whether they become art).

#[derive(Clone, Copy)]
pub enum Glyph {
    ChevronDown,
    ChevronRight,
    Tick,
    Cross,
    Minus,
    Maximise,
    Restore,
    Search,
    Dot,
    Triangle,
}

pub fn glyph(ui: &Ui, glyph: Glyph, c: Pos2, size: f32, color: Color32) {
    let p = ui.painter();
    let s = size / 2.0;
    let pen = Stroke::new(1.5, color);
    let line = |a: (f32, f32), b: (f32, f32)| {
        p.line_segment(
            [c + vec2(a.0 * s, a.1 * s), c + vec2(b.0 * s, b.1 * s)],
            pen,
        );
    };
    match glyph {
        Glyph::ChevronDown => {
            line((-0.6, -0.3), (0.0, 0.3));
            line((0.0, 0.3), (0.6, -0.3));
        }
        Glyph::ChevronRight => {
            line((-0.3, -0.6), (0.3, 0.0));
            line((0.3, 0.0), (-0.3, 0.6));
        }
        Glyph::Tick => {
            line((-0.7, 0.05), (-0.2, 0.55));
            line((-0.2, 0.55), (0.7, -0.5));
        }
        Glyph::Cross => {
            line((-0.6, -0.6), (0.6, 0.6));
            line((-0.6, 0.6), (0.6, -0.6));
        }
        Glyph::Minus => line((-0.6, 0.0), (0.6, 0.0)),
        Glyph::Maximise => {
            p.rect_stroke(
                Rect::from_center_size(c, Vec2::splat(size * 0.62)),
                1.5,
                Stroke::new(1.2, color),
                StrokeKind::Middle,
            );
        }
        Glyph::Restore => {
            let r = Rect::from_center_size(c + vec2(-1.0, 1.0), Vec2::splat(size * 0.5));
            line((-0.25, -0.65), (0.65, -0.65));
            line((0.65, -0.65), (0.65, 0.25));
            p.rect_stroke(r, 1.5, Stroke::new(1.2, color), StrokeKind::Middle);
        }
        Glyph::Search => {
            p.circle_stroke(c + vec2(-0.12 * s, -0.12 * s), 0.55 * s, pen);
            line((0.3, 0.3), (0.8, 0.8));
        }
        Glyph::Dot => {
            p.circle_filled(c, size * 0.22, color);
        }
        Glyph::Triangle => {
            let pts = vec![
                c + vec2(0.0, -0.75 * s),
                c + vec2(0.8 * s, 0.65 * s),
                c + vec2(-0.8 * s, 0.65 * s),
            ];
            p.add(egui::Shape::convex_polygon(pts, color, Stroke::NONE));
        }
    }
}

// --- Surfaces -------------------------------------------------------------------------

/// A card: tab groups and Docks. 1px rim, 1px soft shadow, on the base.
pub fn card(ui: &Ui, t: &Tokens, rect: Rect) {
    let p = ui.painter();
    p.add(t.elevation.card.as_shape(rect, t.radius.card));
    p.rect_filled(rect, t.radius.card, t.color.surface.card);
    p.rect_stroke(
        rect,
        t.radius.card,
        Stroke::new(t.hairline, t.color.stroke.hairline),
        StrokeKind::Inside,
    );
}

/// A popup: menus, combo lists, tooltips, floating bars.
pub fn popup(ui: &Ui, t: &Tokens, rect: Rect) {
    floating(ui, t, rect, t.radius.popup, false);
}

/// A window: dialogs and the Command Palette.
pub fn window(ui: &Ui, t: &Tokens, rect: Rect) {
    floating(ui, t, rect, t.radius.window, true);
}

fn floating(ui: &Ui, t: &Tokens, rect: Rect, radius: f32, window: bool) {
    let p = ui.painter();
    let shadow = if window {
        t.elevation.window
    } else {
        t.elevation.popup
    };
    p.add(shadow.as_shape(rect, radius));
    p.rect_filled(rect, radius, t.color.surface.floating);
    p.rect_stroke(
        rect,
        radius,
        Stroke::new(t.hairline, t.color.stroke.hairline),
        StrokeKind::Inside,
    );
}

pub fn hairline(ui: &mut Ui, t: &Tokens, width: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(width, 1.0), Sense::hover());
    ui.painter().rect_filled(rect, 0.0, t.color.stroke.hairline);
}

// --- Buttons --------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// The one primary button of a dialog: accent solid.
    Primary,
    /// The default button: a neutral fill, no border.
    Secondary,
    /// No fill until hovered: toolbars, row actions.
    Flat,
    /// The confirm button of a destructive dialog: error solid.
    Danger,
    /// A toggle that is on: accent subtle.
    On,
}

pub fn button(
    ui: &mut Ui,
    t: &Tokens,
    st: St,
    kind: Kind,
    icon_key: Option<&str>,
    text: &str,
) -> Response {
    stated(ui, t, st, |ui| {
        let style = if matches!(kind, Kind::Primary | Kind::Danger) {
            &t.ty.body_strong
        } else {
            &t.ty.body
        };
        let text_w = width(ui, style, text);
        let icon_w = if icon_key.is_some() { t.icon + 6.0 } else { 0.0 };
        let size = vec2(
            text_w + icon_w + 2.0 * t.space.control_x,
            t.control_height(),
        );
        let (rect, response) = ui.allocate_exact_size(size, Sense::click());
        let s = seen(ui, t, &response, st);
        let c = &t.color;
        let (fill, ink) = match kind {
            Kind::Primary => (state_fill(t, c.accent.solid, &s), c.accent.on_solid),
            Kind::Danger => (state_fill(t, c.error.solid, &s), c.error.on_solid),
            Kind::Secondary => {
                ui.painter()
                    .rect_filled(rect, t.radius.control, c.fill.control);
                (state_fill(t, Color32::TRANSPARENT, &s), c.text.primary)
            }
            Kind::Flat => (state_fill(t, Color32::TRANSPARENT, &s), c.text.primary),
            Kind::On => {
                ui.painter()
                    .rect_filled(rect, t.radius.control, c.accent.subtle);
                (state_fill(t, Color32::TRANSPARENT, &s), c.text.primary)
            }
        };
        ui.painter().rect_filled(rect, t.radius.control, fill);
        let mut x = rect.left() + t.space.control_x;
        if let Some(key) = icon_key {
            let at = pos2(x + t.icon / 2.0, rect.center().y);
            icon(ui, key, at, t.icon, st != St::Disabled);
            x += icon_w;
        }
        let at = Rect::from_min_max(pos2(x, rect.top()), rect.max);
        text_in(ui, at, Align2::LEFT_CENTER, style, text, ink);
        if s.focus {
            focus_ring(ui, t, rect, t.radius.control);
        }
        response
    })
}

/// A square button holding one icon; flat at rest, accent subtle when on.
pub fn icon_button(ui: &mut Ui, t: &Tokens, st: St, key: &str, on: bool, size: f32) -> Response {
    stated(ui, t, st, |ui| {
        let side = t.control_height();
        let (rect, response) = ui.allocate_exact_size(Vec2::splat(side), Sense::click());
        let s = seen(ui, t, &response, st);
        if on {
            ui.painter()
                .rect_filled(rect, t.radius.control, t.color.accent.subtle);
        }
        ui.painter().rect_filled(
            rect,
            t.radius.control,
            state_fill(t, Color32::TRANSPARENT, &s),
        );
        icon(ui, key, rect.center(), size, st != St::Disabled);
        if s.focus {
            focus_ring(ui, t, rect, t.radius.control);
        }
        response
    })
}

pub fn link(ui: &mut Ui, t: &Tokens, st: St, text: &str) -> Response {
    stated(ui, t, st, |ui| {
        let g = galley(ui, &t.ty.body, text, t.color.accent.text);
        let (rect, response) = ui.allocate_exact_size(g.size(), Sense::click());
        let s = seen(ui, t, &response, st);
        ui.painter().galley(rect.min, g, t.color.accent.text);
        if s.hover > 0.5 || s.focus {
            let y = rect.bottom() - 3.0;
            ui.painter()
                .hline(rect.x_range(), y, Stroke::new(1.0, t.color.accent.text));
        }
        if s.focus {
            focus_ring(ui, t, rect.expand2(vec2(2.0, 0.0)), t.radius.mark);
        }
        response
    })
}

// --- Fields ---------------------------------------------------------------------------

#[derive(Default, Clone, Copy)]
pub struct Field<'a> {
    pub invalid: bool,
    /// A numeric recipe: tabular digits, muted unit at the right.
    pub numeric: bool,
    pub unit: Option<&'a str>,
    pub search: bool,
    pub hint: &'a str,
}

/// A field: a white (or near-black) well with a strong hairline; hover darkens the line,
/// focus swaps it for the 2px ring, invalid for the error line. With `St::Live` it is a
/// real text edit.
pub fn text_field(
    ui: &mut Ui,
    t: &Tokens,
    st: St,
    text: &mut String,
    w: f32,
    f: Field<'_>,
) -> Response {
    stated(ui, t, st, |ui| {
        let (rect, response) = ui.allocate_exact_size(vec2(w, t.control_height()), Sense::hover());
        let style = if f.numeric {
            t.ty.numeric.clone()
        } else {
            t.ty.body.clone()
        };
        let c = &t.color;
        let pad = t.space.control_x - 2.0;
        let unit_w = f.unit.map_or(0.0, |u| width(ui, &t.ty.body, u) + 6.0);
        let search_w = if f.search { t.icon + 4.0 } else { 0.0 };
        let inner = Rect::from_min_max(
            pos2(rect.left() + pad + search_w, rect.top()),
            pos2(rect.right() - pad - unit_w, rect.bottom()),
        );
        let mut focus = st == St::Focus;
        let mut hover = st == St::Hover;
        // The well goes behind the text: two shapes reserved now, set once focus is known.
        let fill_slot = ui.painter().add(egui::Shape::Noop);
        let line_slot = ui.painter().add(egui::Shape::Noop);
        if st == St::Live {
            let color = c.text.primary;
            let lay = style.clone();
            let mut layouter = move |ui: &Ui, buf: &dyn egui::TextBuffer, _wrap: f32| {
                ui.painter().layout_job(lay.job(buf.as_str(), color))
            };
            let edit = egui::TextEdit::singleline(text)
                .frame(egui::Frame::NONE)
                .margin(egui::Margin::ZERO)
                .desired_width(inner.width())
                .vertical_align(egui::Align::Center)
                .hint_text(style.rich(f.hint).color(c.text.muted))
                .layouter(&mut layouter);
            let edit_rect =
                Rect::from_center_size(inner.center(), vec2(inner.width(), style.line));
            // A child that takes no space of its own: the field's rect is already allocated.
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(edit_rect));
            // Stock egui grows a text edit to `interact_size.y`; the line height is ours.
            child.spacing_mut().interact_size.y = style.line;
            let r = child.add(edit);
            focus = r.has_focus();
            hover = r.hovered() || response.hovered();
        } else if text.is_empty() {
            text_in(ui, inner, Align2::LEFT_CENTER, &style, f.hint, c.text.muted);
        } else {
            let drawn = text_in(ui, inner, Align2::LEFT_CENTER, &style, text, c.text.primary);
            if focus {
                ui.painter().vline(
                    drawn.right() + 1.0,
                    drawn.y_range().shrink(2.0),
                    Stroke::new(1.5, c.accent.text),
                );
            }
        }
        let line = if f.invalid {
            Stroke::new(1.5, c.error.text)
        } else if focus {
            Stroke::new(t.focus_ring, c.stroke.focus_ring)
        } else if hover {
            Stroke::new(t.hairline, c.text.muted)
        } else {
            Stroke::new(t.hairline, c.stroke.strong)
        };
        ui.painter().set(
            fill_slot,
            egui::epaint::RectShape::filled(rect, t.radius.control, c.surface.well),
        );
        ui.painter().set(
            line_slot,
            egui::epaint::RectShape::stroke(rect, t.radius.control, line, StrokeKind::Inside),
        );
        if let Some(unit) = f.unit {
            let at = rect.shrink2(vec2(pad, 0.0));
            text_in(ui, at, Align2::RIGHT_CENTER, &t.ty.body, unit, c.text.muted);
        }
        if f.search {
            let at = pos2(rect.left() + pad + t.icon / 2.0, rect.center().y);
            glyph(ui, Glyph::Search, at, 13.0, c.text.muted);
        }
        response
    })
}

/// A combo box, closed: a filled control with a chevron.
pub fn combo(ui: &mut Ui, t: &Tokens, st: St, value: &str, w: f32) -> Response {
    stated(ui, t, st, |ui| {
        let (rect, response) = ui.allocate_exact_size(vec2(w, t.control_height()), Sense::click());
        let s = seen(ui, t, &response, st);
        let c = &t.color;
        ui.painter()
            .rect_filled(rect, t.radius.control, c.fill.control);
        ui.painter().rect_filled(
            rect,
            t.radius.control,
            state_fill(t, Color32::TRANSPARENT, &s),
        );
        let inner = rect.shrink2(vec2(t.space.control_x, 0.0));
        text_clipped(
            ui,
            inner.with_max_x(inner.right() - 16.0),
            &t.ty.body,
            value,
            c.text.primary,
        );
        glyph(
            ui,
            Glyph::ChevronDown,
            pos2(inner.right() - 5.0, rect.center().y),
            10.0,
            c.text.muted,
        );
        if s.focus {
            focus_ring(ui, t, rect, t.radius.control);
        }
        response
    })
}

// --- Checks ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Check {
    Off,
    On,
    Mixed,
}

fn check_row(
    ui: &mut Ui,
    t: &Tokens,
    st: St,
    text: &str,
    mark_w: f32,
    paint: impl FnOnce(&Ui, Rect, &Seen),
) -> Response {
    stated(ui, t, st, |ui| {
        let text_w = if text.is_empty() {
            0.0
        } else {
            width(ui, &t.ty.body, text) + 8.0
        };
        // A control tall, so a column of checks lines up with a column of fields.
        let size = vec2(mark_w + text_w, t.control_height());
        let (rect, response) = ui.allocate_exact_size(size, Sense::click());
        let s = seen(ui, t, &response, st);
        let mark = Rect::from_center_size(
            pos2(rect.left() + mark_w / 2.0, rect.center().y),
            vec2(mark_w, 16.0),
        );
        paint(ui, mark, &s);
        let at = rect.with_min_x(rect.left() + mark_w + 8.0);
        let ink = t.color.text.primary;
        text_in(ui, at, Align2::LEFT_CENTER, &t.ty.body, text, ink);
        if s.focus {
            let radius = if mark_w > 16.0 { 8.0 } else { t.radius.mark };
            focus_ring(ui, t, mark, radius);
        }
        response
    })
}

pub fn checkbox(ui: &mut Ui, t: &Tokens, st: St, check: Check, text: &str) -> Response {
    check_row(ui, t, st, text, 16.0, |ui, mark, s| {
        let c = &t.color;
        if check == Check::Off {
            ui.painter()
                .rect_filled(mark, t.radius.mark, c.surface.well);
            let line = if s.hover > 0.5 {
                c.text.muted
            } else {
                c.stroke.strong
            };
            ui.painter().rect_stroke(
                mark,
                t.radius.mark,
                Stroke::new(t.hairline, line),
                StrokeKind::Inside,
            );
        } else {
            ui.painter()
                .rect_filled(mark, t.radius.mark, state_fill(t, c.accent.solid, s));
            let g = if check == Check::On {
                Glyph::Tick
            } else {
                Glyph::Minus
            };
            glyph(ui, g, mark.center(), 10.0, c.accent.on_solid);
        }
    })
}

pub fn radio(ui: &mut Ui, t: &Tokens, st: St, on: bool, text: &str) -> Response {
    check_row(ui, t, st, text, 16.0, |ui, mark, s| {
        let c = &t.color;
        let p = ui.painter();
        if on {
            p.circle_filled(mark.center(), 8.0, state_fill(t, c.accent.solid, s));
            p.circle_filled(mark.center(), 3.0, c.accent.on_solid);
        } else {
            p.circle_filled(mark.center(), 8.0, c.surface.well);
            let line = if s.hover > 0.5 {
                c.text.muted
            } else {
                c.stroke.strong
            };
            p.circle_stroke(mark.center(), 7.5, Stroke::new(t.hairline, line));
        }
    })
}

/// A switch: for a setting that applies the moment it is flipped.
pub fn switch(ui: &mut Ui, t: &Tokens, st: St, on: bool, text: &str) -> Response {
    let id = ui.next_auto_id();
    let k = if st == St::Live {
        ui.ctx()
            .animate_bool_with_time(id.with("switch"), on, t.motion.base)
    } else {
        f32::from(u8::from(on))
    };
    check_row(ui, t, st, text, 28.0, |ui, mark, s| {
        let c = &t.color;
        let p = ui.painter();
        let off = c.stroke.strong;
        let mix = |a: u8, b: u8| egui::lerp(f32::from(a)..=f32::from(b), k) as u8;
        let track = Color32::from_rgb(
            mix(off.r(), c.accent.solid.r()),
            mix(off.g(), c.accent.solid.g()),
            mix(off.b(), c.accent.solid.b()),
        );
        p.rect_filled(mark, 8.0, state_fill(t, track, s));
        let x = egui::lerp(mark.left() + 8.0..=mark.right() - 8.0, k);
        p.circle_filled(pos2(x, mark.center().y), 6.0, Color32::WHITE);
    })
}

// --- Rows, menus ----------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Sel {
    None,
    /// Selected in the Panel that holds keyboard focus: accent subtle.
    Live,
    /// Selected, but focus is elsewhere or the window is in the background: neutral.
    Unfocused,
}

/// The background of one row of a list, tree, table or menu; the caller paints what is in
/// it. Rows are rounded and sit inside their container's padding.
pub fn row(ui: &mut Ui, t: &Tokens, st: St, sel: Sel, w: f32, h: f32) -> (Rect, Response) {
    let (rect, response) = ui.allocate_exact_size(vec2(w, h), Sense::click());
    let s = seen(ui, t, &response, st);
    let p = ui.painter();
    match sel {
        Sel::Live => {
            p.rect_filled(rect, t.radius.row, t.color.accent.subtle);
        }
        Sel::Unfocused => {
            p.rect_filled(rect, t.radius.row, t.color.fill.selected_unfocused);
        }
        Sel::None => {}
    }
    p.rect_filled(
        rect,
        t.radius.row,
        state_fill(t, Color32::TRANSPARENT, &s),
    );
    if s.focus {
        p.rect_stroke(
            rect,
            t.radius.row,
            Stroke::new(t.focus_ring, t.color.stroke.focus_ring),
            StrokeKind::Inside,
        );
    }
    (rect, response)
}

#[derive(Default, Clone, Copy)]
pub struct Item<'a> {
    pub icon: Option<&'a str>,
    pub text: &'a str,
    pub shortcut: &'a str,
    pub submenu: bool,
    pub checked: bool,
}

/// One menu item: icon column, label, shortcut in muted text, submenu chevron.
pub fn menu_item(ui: &mut Ui, t: &Tokens, st: St, w: f32, item: Item<'_>) -> Response {
    stated(ui, t, st, |ui| {
        let (rect, response) = row(ui, t, st, Sel::None, w, t.row_height());
        let c = &t.color;
        let x = rect.left() + t.space.row_x;
        let lead = pos2(x + t.icon / 2.0, rect.center().y);
        if let Some(key) = item.icon {
            icon(ui, key, lead, t.icon, st != St::Disabled);
        } else if item.checked {
            glyph(ui, Glyph::Tick, lead, 10.0, c.text.primary);
        }
        let inner = Rect::from_min_max(
            pos2(x + t.icon + 8.0, rect.top()),
            rect.max - vec2(t.space.row_x, 0.0),
        );
        text_in(ui, inner, Align2::LEFT_CENTER, &t.ty.body, item.text, c.text.primary);
        if item.submenu {
            let at = pos2(inner.right() - 4.0, rect.center().y);
            glyph(ui, Glyph::ChevronRight, at, 9.0, c.text.muted);
        } else if !item.shortcut.is_empty() {
            let ink = c.text.muted;
            text_in(ui, inner, Align2::RIGHT_CENTER, &t.ty.body, item.shortcut, ink);
        }
        response
    })
}

pub fn menu_separator(ui: &mut Ui, t: &Tokens, w: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(w, 9.0), Sense::hover());
    ui.painter().hline(
        rect.shrink2(vec2(t.space.row_x, 0.0)).x_range(),
        rect.center().y,
        Stroke::new(t.hairline, t.color.stroke.hairline),
    );
}

/// A shortcut shown as a key chip: tooltips and the Command Palette.
pub fn key_chip(ui: &mut Ui, t: &Tokens, keys: &str) -> Response {
    let w = width(ui, &t.ty.small, keys) + 10.0;
    let (rect, response) = ui.allocate_exact_size(vec2(w, t.ty.small.line + 2.0), Sense::hover());
    ui.painter()
        .rect_filled(rect, t.radius.mark, t.color.fill.control);
    let ink = t.color.text.muted;
    text_in(ui, rect, Align2::CENTER_CENTER, &t.ty.small, keys, ink);
    response
}

/// A count or a state on a tint: Messages' Errors / Warnings, an OK state.
pub fn badge(
    ui: &mut Ui,
    t: &Tokens,
    tone: Option<&Tone>,
    solid: bool,
    mark: Option<Glyph>,
    text: &str,
) -> Response {
    let mark_w = if mark.is_some() { 12.0 } else { 0.0 };
    let w = width(ui, &t.ty.small, text) + 16.0 + mark_w;
    let (rect, response) = ui.allocate_exact_size(vec2(w, t.ty.small.line + 4.0), Sense::hover());
    let (fill, ink, mark_ink) = match (tone, solid) {
        (Some(tone), true) => (tone.solid, tone.on_solid, tone.on_solid),
        (Some(tone), false) => (tone.subtle, t.color.text.primary, tone.text),
        (None, _) => (t.color.fill.control, t.color.text.muted, t.color.text.muted),
    };
    ui.painter().rect_filled(rect, rect.height() / 2.0, fill);
    if let Some(m) = mark {
        glyph(ui, m, pos2(rect.left() + 13.0, rect.center().y), 9.0, mark_ink);
    }
    let at = rect.with_min_x(rect.left() + mark_w);
    text_in(ui, at, Align2::CENTER_CENTER, &t.ty.small, text, ink);
    response
}

/// A data colour (Layer, Net, Net Class): one shape, neutral rim, never themed.
pub fn swatch(ui: &mut Ui, t: &Tokens, color: Color32) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(14.0), Sense::click());
    ui.painter().rect_filled(rect, t.radius.mark, color);
    ui.painter().rect_stroke(
        rect,
        t.radius.mark,
        Stroke::new(t.hairline, t.color.stroke.strong),
        StrokeKind::Inside,
    );
    response
}

pub fn progress(ui: &mut Ui, t: &Tokens, tone: &Tone, fraction: f32, w: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(w, 6.0), Sense::hover());
    ui.painter().rect_filled(rect, 3.0, t.color.fill.control);
    let done = rect.with_max_x(rect.left() + w * fraction);
    ui.painter().rect_filled(done, 3.0, tone.solid);
}

pub fn slider(ui: &mut Ui, t: &Tokens, st: St, fraction: f32, w: f32) -> Response {
    stated(ui, t, st, |ui| {
        let (rect, response) = ui.allocate_exact_size(vec2(w, t.control_height()), Sense::drag());
        let s = seen(ui, t, &response, st);
        let rail = Rect::from_center_size(rect.center(), vec2(w, 4.0));
        ui.painter().rect_filled(rail, 2.0, t.color.fill.control);
        let x = rect.left() + 8.0 + (w - 16.0) * fraction;
        ui.painter()
            .rect_filled(rail.with_max_x(x), 2.0, t.color.accent.solid);
        let knob = pos2(x, rect.center().y);
        ui.painter().circle_filled(knob, 8.0, Color32::WHITE);
        ui.painter()
            .circle_stroke(knob, 8.0, Stroke::new(1.0, t.color.stroke.strong));
        if s.focus {
            ui.painter().circle_stroke(
                knob,
                10.0,
                Stroke::new(t.focus_ring, t.color.stroke.focus_ring),
            );
        }
        response
    })
}

pub fn scrollbar(ui: &mut Ui, t: &Tokens, hovered: bool, h: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(10.0, h), Sense::hover());
    let w = if hovered { 8.0 } else { 4.0 };
    let thumb = Rect::from_min_size(
        pos2(rect.right() - w - 1.0, rect.top() + h * 0.2),
        vec2(w, h * 0.4),
    );
    let ink = t
        .color
        .text
        .muted
        .gamma_multiply(if hovered { 0.7 } else { 0.45 });
    ui.painter().rect_filled(thumb, w / 2.0, ink);
}

// --- Tabs -----------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TabKind {
    /// The front tab of the tab group holding the active Document: a card joined to its
    /// panel, with the 2px accent line.
    Active,
    /// The front tab of any other tab group: a card, no line.
    Front,
    /// A tab behind the front one: on the base.
    Back,
}

#[derive(Clone, Copy)]
pub struct Tab<'a> {
    pub kind: TabKind,
    pub icon: &'a str,
    pub title: &'a str,
    pub modified: bool,
    pub out_of_sync: bool,
    /// REDA's window has focus; without it the accent line goes neutral.
    pub window_active: bool,
}

pub fn tab(ui: &mut Ui, t: &Tokens, st: St, tab: Tab<'_>) -> Response {
    let c = &t.color;
    let front = tab.kind != TabKind::Back;
    let style = if tab.kind == TabKind::Active {
        &t.ty.body_strong
    } else {
        &t.ty.body
    };
    let badge_w = if tab.out_of_sync { 16.0 } else { 0.0 };
    let w = t.space.tab_x * 2.0 + t.icon + 6.0 + width(ui, style, tab.title) + badge_w + 22.0;
    let (rect, response) = ui.allocate_exact_size(vec2(w, t.tab_height()), Sense::click());
    let s = seen(ui, t, &response, st);
    let p = ui.painter();
    let top = egui::CornerRadius {
        nw: 10,
        ne: 10,
        sw: 0,
        se: 0,
    };
    if front {
        // Joined to the panel below: no bottom rim, and it covers the card's top rim.
        let body = rect.with_max_y(rect.bottom() + 1.0);
        p.rect_filled(body, top, c.surface.card);
        p.rect_stroke(
            body,
            top,
            Stroke::new(t.hairline, c.stroke.hairline),
            StrokeKind::Inside,
        );
        p.hline(
            (rect.left() + 1.0)..=(rect.right() - 1.0),
            rect.bottom() + 0.5,
            Stroke::new(1.0, c.surface.card),
        );
        if tab.kind == TabKind::Active {
            let line = if tab.window_active {
                c.accent.text
            } else {
                c.stroke.strong
            };
            let bar = Rect::from_min_max(
                pos2(rect.left() + 10.0, rect.top()),
                pos2(rect.right() - 10.0, rect.top() + t.accent_line),
            );
            let under = egui::CornerRadius {
                nw: 0,
                ne: 0,
                sw: 2,
                se: 2,
            };
            p.rect_filled(bar, under, line);
        }
    } else {
        let pill = rect.shrink2(vec2(2.0, 3.0));
        p.rect_filled(
            pill,
            t.radius.control,
            state_fill(t, Color32::TRANSPARENT, &s),
        );
    }
    let mut x = rect.left() + t.space.tab_x;
    let at = pos2(x + t.icon / 2.0, rect.center().y);
    icon(ui, tab.icon, at, t.icon, true);
    x += t.icon + 6.0;
    let ink = if front { c.text.primary } else { c.text.muted };
    let drawn = text_in(
        ui,
        Rect::from_min_max(pos2(x, rect.top()), rect.max),
        Align2::LEFT_CENTER,
        style,
        tab.title,
        ink,
    );
    x = drawn.right() + 6.0;
    if tab.out_of_sync {
        // Warning, solid, with a shape: colour is never the only signal.
        let at = pos2(x + 6.0, rect.center().y);
        glyph(ui, Glyph::Triangle, at, 13.0, c.warning.solid);
        ui.painter().vline(
            at.x,
            (at.y - 2.0)..=(at.y + 2.0),
            Stroke::new(1.2, c.warning.on_solid),
        );
        x += badge_w;
    }
    // Close ×, or the modified dot until the tab is hovered.
    let at = pos2(x + 8.0, rect.center().y);
    if tab.modified && s.hover < 0.5 {
        glyph(ui, Glyph::Dot, at, 16.0, ink);
    } else if front || s.hover > 0.5 {
        if s.hover > 0.5 {
            ui.painter().rect_filled(
                Rect::from_center_size(at, Vec2::splat(18.0)),
                t.radius.mark,
                c.fill.hover,
            );
        }
        glyph(ui, Glyph::Cross, at, 8.0, c.text.muted);
    }
    if s.focus {
        focus_ring(ui, t, rect.shrink(2.0), t.radius.control);
    }
    response
}

// --- Window controls ------------------------------------------------------------------

/// Minimise, maximise / restore, close: full bar height, flat, red hover on close.
/// `hot` forces one of them hovered. Returns the width used.
pub fn window_controls(
    ui: &Ui,
    t: &Tokens,
    bar: Rect,
    hot: Option<usize>,
    maximised: bool,
    active: bool,
    corner: f32,
) -> f32 {
    let w = 46.0;
    let c = &t.color;
    let glyphs = [
        Glyph::Minus,
        if maximised {
            Glyph::Restore
        } else {
            Glyph::Maximise
        },
        Glyph::Cross,
    ];
    for (i, g) in glyphs.into_iter().enumerate() {
        let rect = Rect::from_min_max(
            pos2(bar.right() - w * (3 - i) as f32, bar.top()),
            pos2(bar.right() - w * (2 - i) as f32, bar.bottom()),
        );
        let mut ink = if active { c.text.primary } else { c.text.muted };
        if hot == Some(i) {
            let radius = egui::CornerRadius {
                ne: if i == 2 { corner as u8 } else { 0 },
                ..Default::default()
            };
            if i == 2 {
                ui.painter().rect_filled(rect, radius, c.close_hover);
                ink = Color32::WHITE;
            } else {
                ui.painter().rect_filled(rect, radius, c.fill.hover);
            }
        }
        glyph(ui, g, rect.center(), 10.0, ink);
    }
    3.0 * w
}

/// macOS: the native traffic lights (drawn by AppKit; shown here for the inset only).
pub fn traffic_lights(ui: &Ui, bar: Rect, active: bool) -> f32 {
    for (i, hex) in [0xff5f57u32, 0xfebc2e, 0x28c840].into_iter().enumerate() {
        let c = if active {
            Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
        } else {
            Color32::from_gray(140)
        };
        ui.painter().circle_filled(
            pos2(bar.left() + 20.0 + 20.0 * i as f32, bar.center().y),
            6.0,
            c,
        );
    }
    78.0
}
