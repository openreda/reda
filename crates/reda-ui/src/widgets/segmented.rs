//! The segmented control: a short set of exclusive choices that apply when picked, such
//! as System · Light · Dark (docs/spec/v0.1/app.md#preferences).
//!
//! The recipe is built from its neighbours. The track is a filled control (`fill.control`
//! at the control radius and height), each segment is as wide as the widest label plus
//! the control padding, and the chosen segment takes the accent subtle fill, as a toggle
//! that is on does. Segments sit 2 px inside the track with a radius 2 px smaller, so
//! the corners nest the way a row's do inside a popup.

use egui::{Align2, Color32, Rect, Response, Role, Sense, Ui, Widget, WidgetInfo, pos2, vec2};

use super::paint::{self, State};
use crate::theme::TokensExt as _;

/// How far inside the track its segments sit.
const INSET: f32 = 2.0;

/// A segmented control over `options` (value and label), showing and setting `value`.
pub fn segmented<'a, T: PartialEq + Clone, S: AsRef<str>>(
    value: &'a mut T,
    options: &'a [(T, S)],
) -> Segmented<'a, T, S> {
    Segmented {
        value,
        options,
        state: State::Live,
    }
}

#[must_use = "add it with ui.add"]
pub struct Segmented<'a, T, S> {
    value: &'a mut T,
    options: &'a [(T, S)],
    state: State,
}

impl<T, S> Segmented<'_, T, S> {
    /// Forces a state for the gallery: hover and press show on the segment after the
    /// chosen one, focus on the chosen one.
    pub fn forced(mut self, state: State) -> Self {
        self.state = state;
        self
    }
}

impl<T: PartialEq + Clone, S: AsRef<str>> Widget for Segmented<'_, T, S> {
    fn ui(self, ui: &mut Ui) -> Response {
        let t = ui.tokens();
        paint::stated(ui, self.state, |ui| {
            let c = &t.color;
            let count = self.options.len();
            let widest = self
                .options
                .iter()
                .map(|(_, text)| paint::width(ui, &t.ty.body, text.as_ref()))
                .fold(0.0, f32::max);
            let segment = (widest + 2.0 * t.space.control_x).ceil();
            let size = vec2(segment * count as f32 + 2.0 * INSET, t.control_height());
            let (rect, mut response) = ui.allocate_exact_size(size, Sense::hover());
            let chosen = self.options.iter().position(|(v, _)| v == self.value);
            let enabled = ui.is_enabled();
            let visible = ui.is_rect_visible(rect);
            if visible {
                ui.painter()
                    .rect_filled(rect, t.radius.control, c.fill.control);
            }
            let radius = t.radius.control - INSET;
            let mut picked = None;
            for (i, (value, text)) in self.options.iter().enumerate() {
                let text = text.as_ref();
                let left = rect.left() + INSET + segment * i as f32;
                let at = Rect::from_min_max(
                    pos2(left, rect.top() + INSET),
                    pos2(left + segment, rect.bottom() - INSET),
                );
                let on = chosen == Some(i);
                let part = ui.interact(at, response.id.with(i), Sense::click());
                part.widget_info(|| WidgetInfo::selected(Role::RadioButton, enabled, on, text));
                if part.clicked() && !on {
                    picked = Some(value.clone());
                }
                if !visible {
                    continue;
                }
                let forced = match self.state {
                    State::Live => State::Live,
                    State::Hover | State::Press
                        if Some(i) == chosen.map(|chosen| (chosen + 1) % count) =>
                    {
                        self.state
                    }
                    State::Focus if on => State::Focus,
                    _ => State::Rest,
                };
                let s = paint::seen(&part, forced);
                let under = if on {
                    c.accent.subtle
                } else {
                    Color32::TRANSPARENT
                };
                let fill = paint::state_fill(&t, under, s);
                ui.painter().rect_filled(at, radius, fill);
                let ink = c.text.primary;
                paint::text_in(ui, at, Align2::CENTER_CENTER, &t.ty.body, text, ink);
                if s.focus {
                    // Drawn on the segment's edge: the track leaves no room outside it.
                    ui.painter().rect_stroke(
                        at,
                        radius,
                        egui::Stroke::new(t.stroke.focus_ring, c.stroke.focus_ring),
                        egui::StrokeKind::Outside,
                    );
                }
            }
            if let Some(value) = picked {
                *self.value = value;
                response.mark_changed();
            }
            response
        })
    }
}
