//! The colour button: the swatch of a data colour that opens a colour picker
//! (docs/spec/v0.1/design-language.md#other-colour-sources).
//!
//! The button is the one swatch shape every data colour shows in. The picker is in a
//! menu's popup and stays open while it is used (a click outside, or Escape, closes it):
//! a field of saturation and brightness, a strip of hues, and a row with the colour's
//! swatch, its `#rrggbb` to type or read, and a button that copies it.

use egui::ecolor::Hsva;
use egui::{
    Color32, Id, Mesh, Pos2, Rect, Response, Role, Sense, Shape, Stroke, StrokeKind, Ui, Widget,
    WidgetInfo, lerp, pos2, remap_clamp, vec2,
};

use super::glyph::Glyph;
use super::mark::swatch;
use super::paint;
use crate::theme::{Tokens, TokensExt as _};
use crate::tr;

/// The width of the colour picker.
pub const PICKER_WIDTH: f32 = 240.0;
/// The height of the picker's field of saturation and brightness.
pub const PICKER_FIELD: f32 = 150.0;
/// The height of the picker's strip of hues.
pub const PICKER_HUE: f32 = 14.0;
/// The radius of the ring that marks the colour in the field.
const RING: f32 = 6.0;
/// How many steps a gradient is drawn in along each side.
const STEPS: usize = 24;
/// The width of the `#rrggbb` field.
const HEX_WIDTH: f32 = 104.0;

/// A data colour (a Layer's, a Net's) from its red, green and blue, for the colour
/// button to edit. A data colour is the user's, not the theme's, so this is the one way
/// to build one outside `theme`.
pub const fn data_color(r: u8, g: u8, b: u8) -> Color32 {
    Color32::from_rgb(r, g, b)
}

/// A swatch of `color` that opens a picker for it when clicked. `name` is the button's
/// accessible name. The response is marked changed while the picker changes the colour.
pub fn color_button(color: &mut Color32, name: impl Into<String>) -> ColorButton<'_> {
    ColorButton {
        color,
        name: name.into(),
    }
}

#[must_use = "add it with ui.add"]
pub struct ColorButton<'a> {
    color: &'a mut Color32,
    name: String,
}

impl Widget for ColorButton<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let t = ui.tokens();
        let mut response = ui.add(swatch(*self.color).button(self.name));
        let color = self.color;
        let id = response.id.with("picker");
        let picked = super::menu(&response)
            .stay_open()
            .show(|ui| picker(ui, &t, id, color));
        match picked {
            Some(picked) if picked.inner => response.mark_changed(),
            Some(_) => {}
            // A closed picker forgets its hue and what was typed.
            None => ui.data_mut(|d| {
                d.remove::<Picking>(id);
                d.remove::<String>(id);
            }),
        }
        response
    }
}

/// What the picker keeps while it is open: the colour as hue, saturation and brightness
/// (so the hue is not lost on a grey), and the colour that stands for.
#[derive(Clone, Copy)]
struct Picking {
    hsv: Hsva,
    color: Color32,
}

fn rgb_of(hsv: Hsva) -> Color32 {
    let [r, g, b] = hsv.to_srgb();
    Color32::from_rgb(r, g, b)
}

fn hex_of(color: Color32) -> String {
    format!("#{:02x}{:02x}{:02x}", color.r(), color.g(), color.b())
}

/// `#rrggbb` as a colour.
fn parse_hex(text: &str) -> Option<Color32> {
    let hex = text.trim().strip_prefix('#')?;
    if hex.len() != 6 || !hex.is_ascii() {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
    Some(Color32::from_rgb(byte(0)?, byte(2)?, byte(4)?))
}

/// The colour picker's contents, as the colour button's popup holds them: for a picker
/// drawn in place. `id` keeps its hue and what is typed between frames. Returns whether
/// it changed `color` this frame.
pub fn color_picker(ui: &mut Ui, id: Id, color: &mut Color32) -> bool {
    let t = ui.tokens();
    ui.scope(|ui| picker(ui, &t, id, color)).inner
}

fn picker(ui: &mut Ui, t: &Tokens, id: Id, color: &mut Color32) -> bool {
    ui.spacing_mut().item_spacing = vec2(t.space.gap, t.space.stack);
    let kept: Option<Picking> = ui.data(|d| d.get_temp(id));
    let [r, g, b, _] = color.to_array();
    let mut hsv = match kept {
        Some(kept) if kept.color == *color => kept.hsv,
        _ => Hsva::from_srgb([r, g, b]),
    };
    let before = hsv;
    shade_field(ui, t, &mut hsv);
    hue_strip(ui, t, &mut hsv);
    let mut changed = false;
    if hsv != before {
        *color = rgb_of(hsv);
        changed = true;
    }
    ui.horizontal(|ui| {
        ui.add(swatch(*color));
        // The field shows the colour, and takes one: what is typed is kept until it
        // reads as a colour.
        let shown = hex_of(*color);
        let mut text: String = ui.data(|d| d.get_temp(id)).unwrap_or_else(|| shown.clone());
        let unread = parse_hex(&text).is_none();
        let field = super::text_field(&mut text).mono().width(HEX_WIDTH);
        let field = ui.add(field.invalid(unread));
        let name = tr!("colour-hex");
        field.widget_info(|| WidgetInfo::labeled(Role::TextInput, true, &name));
        if field.changed()
            && let Some(typed) = parse_hex(&text)
            && typed != *color
        {
            *color = typed;
            let [r, g, b, _] = typed.to_array();
            hsv = Hsva::from_srgb([r, g, b]);
            changed = true;
        }
        if field.has_focus() {
            ui.data_mut(|d| d.insert_temp(id, text));
        } else {
            ui.data_mut(|d| d.remove::<String>(id));
        }
        let copy = ui.add(super::glyph_button(Glyph::Copy));
        let name = tr!("colour-copy");
        copy.widget_info(|| WidgetInfo::labeled(Role::Button, true, &name));
        if copy.clicked() {
            ui.ctx().copy_text(hex_of(*color));
        }
    });
    let picking = Picking { hsv, color: *color };
    ui.data_mut(|d| d.insert_temp(id, picking));
    changed
}

/// A gradient over `rect`, `color_at` giving the colour at each point of the unit
/// square (x to the right, y up).
fn gradient(rect: Rect, steps: (usize, usize), color_at: impl Fn(f32, f32) -> Color32) -> Shape {
    let (nx, ny) = steps;
    let mut mesh = Mesh::default();
    for yi in 0..=ny {
        for xi in 0..=nx {
            let (x, y) = (xi as f32 / nx as f32, yi as f32 / ny as f32);
            let at = pos2(
                lerp(rect.left()..=rect.right(), x),
                lerp(rect.bottom()..=rect.top(), y),
            );
            mesh.colored_vertex(at, color_at(x, y));
        }
    }
    let row = (nx + 1) as u32;
    for yi in 0..ny as u32 {
        for xi in 0..nx as u32 {
            let i = yi * row + xi;
            mesh.add_triangle(i, i + 1, i + row);
            mesh.add_triangle(i + 1, i + row + 1, i + row);
        }
    }
    Shape::mesh(mesh)
}

/// The rim of a gradient, and its focus ring.
fn rim(ui: &Ui, t: &Tokens, rect: Rect, response: &Response) {
    let line = Stroke::new(t.stroke.hairline, t.color.stroke.strong);
    ui.painter()
        .rect_stroke(rect, 0.0, line, StrokeKind::Inside);
    if paint::seen(response, paint::State::Live).focus {
        paint::focus_ring(ui, t, rect, 0.0);
    }
}

/// The white ring, with a dark line round it, that marks a place on a gradient.
fn mark(p: &egui::Painter, t: &Tokens, at: Pos2) {
    p.circle_stroke(at, RING, Stroke::new(t.stroke.accent_line, paint::KNOB));
    let outer = RING + t.stroke.accent_line / 2.0;
    let dark = Stroke::new(t.stroke.hairline, t.color.surface.scrim.to_opaque());
    p.circle_stroke(at, outer, dark);
}

/// The field of saturation (to the right) and brightness (up) at the picker's hue.
fn shade_field(ui: &mut Ui, t: &Tokens, hsv: &mut Hsva) {
    let size = vec2(PICKER_WIDTH, PICKER_FIELD);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click_and_drag());
    let name = tr!("colour-shade");
    response.widget_info(|| WidgetInfo::labeled(Role::Slider, true, &name));
    if let Some(at) = response.interact_pointer_pos() {
        hsv.s = remap_clamp(at.x, rect.left()..=rect.right(), 0.0..=1.0);
        hsv.v = remap_clamp(at.y, rect.bottom()..=rect.top(), 0.0..=1.0);
    }
    if !ui.is_rect_visible(rect) {
        return;
    }
    let hue = hsv.h;
    let shades = gradient(rect, (STEPS, STEPS), |s, v| {
        rgb_of(Hsva::new(hue, s, v, 1.0))
    });
    ui.painter().add(shades);
    rim(ui, t, rect, &response);
    let at = pos2(
        lerp(rect.left()..=rect.right(), hsv.s),
        lerp(rect.bottom()..=rect.top(), hsv.v),
    );
    mark(&ui.painter_at(rect), t, at);
}

/// The strip of hues.
fn hue_strip(ui: &mut Ui, t: &Tokens, hsv: &mut Hsva) {
    let size = vec2(PICKER_WIDTH, PICKER_HUE);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click_and_drag());
    let name = tr!("colour-hue");
    response.widget_info(|| WidgetInfo::labeled(Role::Slider, true, &name));
    if let Some(at) = response.interact_pointer_pos() {
        hsv.h = remap_clamp(at.x, rect.left()..=rect.right(), 0.0..=1.0);
    }
    if !ui.is_rect_visible(rect) {
        return;
    }
    let hues = gradient(rect, (STEPS, 1), |h, _| rgb_of(Hsva::new(h, 1.0, 1.0, 1.0)));
    ui.painter().add(hues);
    rim(ui, t, rect, &response);
    let x = lerp(rect.left()..=rect.right(), hsv.h);
    mark(
        &ui.painter_at(rect.expand(RING)),
        t,
        pos2(x, rect.center().y),
    );
}
