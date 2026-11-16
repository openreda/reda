//! egui's `Style`, generated from [`Tokens`]: what stock egui widgets, and third-party ones
//! such as the file dialog, draw with until a REDA widget replaces them. Nothing else sets
//! a style or visuals.

use egui::style::{HandleShape, ScrollStyle};
use egui::{CornerRadius, Margin, Stroke, TextStyle, Theme, Visuals};

use super::{Tokens, over};
use crate::widgets::{MARK, SLIDER_RAIL, TREE_INDENT, glyph};

/// The scrollbar's thumb, at rest and hovered. The scrollbar is egui's own, so its recipe
/// is the `ScrollStyle` below.
pub(super) const SCROLLBAR: f32 = 4.0;
pub(super) const SCROLLBAR_HOVER: f32 = 8.0;

/// egui takes radii and margins as whole points.
fn whole(points: f32) -> u8 {
    points.round() as u8
}

fn visuals(t: &Tokens) -> Visuals {
    let c = &t.color;
    let mut v = match t.theme {
        Theme::Dark => Visuals::dark(),
        Theme::Light => Visuals::light(),
    };
    let hairline = Stroke::new(t.stroke.hairline, c.stroke.hairline);
    let control = CornerRadius::same(whole(t.radius.control));
    v.override_text_color = None;
    v.weak_text_color = Some(c.text.muted);
    v.hyperlink_color = c.accent.text;
    v.faint_bg_color = c.fill.control;
    v.extreme_bg_color = c.surface.well;
    v.text_edit_bg_color = Some(c.surface.well);
    v.code_bg_color = c.fill.control;
    v.warn_fg_color = c.warning.text;
    v.error_fg_color = c.error.text;
    v.window_fill = c.surface.floating;
    v.window_stroke = hairline;
    v.window_corner_radius = CornerRadius::same(whole(t.radius.window));
    v.window_shadow = t.elevation.window;
    v.popup_shadow = t.elevation.popup;
    v.menu_corner_radius = CornerRadius::same(whole(t.radius.popup));
    v.panel_fill = c.surface.base;
    // A selection is the accent's subtle tint under ordinary text.
    v.selection.bg_fill = c.accent.subtle;
    v.selection.stroke = Stroke::new(t.stroke.hairline, c.text.primary);
    v.text_cursor.stroke = Stroke::new(glyph::STROKE, c.accent.text);
    v.striped = false;
    v.slider_trailing_fill = true;
    v.handle_shape = HandleShape::Circle;
    v.disabled_alpha = t.opacity.disabled;

    let w = &mut v.widgets;
    w.noninteractive.bg_fill = c.surface.base;
    w.noninteractive.weak_bg_fill = c.surface.base;
    w.noninteractive.bg_stroke = hairline;
    w.noninteractive.fg_stroke = Stroke::new(t.stroke.hairline, c.text.primary);
    w.noninteractive.corner_radius = control;
    // A filled control at rest, then the hover or press fill over it. egui shows keyboard
    // focus with the pressed visuals, so those carry the focus ring.
    let rest = c.fill.control;
    let ring = Stroke::new(t.stroke.focus_ring, c.stroke.focus_ring);
    for (state, fill, stroke) in [
        (&mut w.inactive, rest, Stroke::NONE),
        (&mut w.hovered, over(c.fill.hover, rest), Stroke::NONE),
        (&mut w.active, over(c.fill.press, rest), ring),
        (&mut w.open, over(c.fill.hover, rest), Stroke::NONE),
    ] {
        state.weak_bg_fill = fill;
        state.bg_fill = fill;
        state.fg_stroke = Stroke::new(t.stroke.hairline, c.text.primary);
        state.bg_stroke = stroke;
        state.corner_radius = control;
        state.expansion = 0.0;
    }
    // Check boxes, radios and slider rails at rest take a fill that stands off any surface.
    w.inactive.bg_fill = c.stroke.strong;
    v
}

/// The egui `Style` that goes with `t`.
pub fn style(t: &Tokens) -> egui::Style {
    let mut style = egui::Style {
        visuals: visuals(t),
        ..egui::Style::default()
    };
    style.text_styles = [
        (TextStyle::Small, t.ty.small.font_id()),
        (TextStyle::Body, t.ty.body.font_id()),
        (TextStyle::Button, t.ty.body.font_id()),
        (TextStyle::Heading, t.ty.heading.font_id()),
        (TextStyle::Monospace, t.ty.mono.font_id()),
    ]
    .into();
    let s = &mut style.spacing;
    s.item_spacing = egui::vec2(t.space.gap, t.space.row_y);
    s.button_padding = egui::vec2(t.space.control_x, t.space.control_y);
    s.interact_size.y = t.control_height();
    s.menu_margin = Margin::same(whole(t.space.menu) as i8);
    s.window_margin = Margin::same(whole(t.space.dialog) as i8);
    s.icon_width = MARK;
    s.indent = TREE_INDENT;
    s.slider_rail_height = SLIDER_RAIL;
    // A thin thumb with no track, which widens when hovered. The content keeps clear of
    // the widened thumb, so a control at the trailing edge is never under it.
    s.scroll = ScrollStyle {
        bar_width: SCROLLBAR_HOVER,
        floating_width: SCROLLBAR,
        floating_allocated_width: SCROLLBAR_HOVER,
        dormant_background_opacity: 0.0,
        active_background_opacity: 0.0,
        interact_background_opacity: 0.0,
        ..ScrollStyle::floating()
    };
    // egui's own transitions: a popup's fade-in (menus, combo lists, tooltips) and the
    // scrollbar widening. Zero with Animate off.
    style.animation_time = t.motion.menu.enter;
    style.interaction.tooltip_delay = t.motion.tooltip_wait;
    style.interaction.tooltip_grace_time = t.motion.tooltip_chain;
    style
}
