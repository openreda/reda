//! The stepper field: a whole number typed or stepped, such as the text size
//! (docs/spec/v0.1/app.md#preferences).
//!
//! The recipe is a number field (the well, the numeric style, the unit in muted text)
//! with a 20 px column at its trailing end, set off by a hairline and split into an up
//! and a down half. Each half holds a chevron Glyph in muted text and takes `fill.hover`
//! and `fill.press` like anything flat; a half that cannot step further is drawn at
//! `opacity.disabled`. Typing commits on Enter or when the field loses the keyboard, and
//! Up and Down step while it has it.

use std::ops::RangeInclusive;

use egui::accesskit::Action;
use egui::{
    Align2, Color32, CornerRadius, Id, Key, Rect, Response, Sense, Shape, Stroke, StrokeKind, Ui,
    Widget, pos2, vec2,
};

use super::field::{self, PART_GAP};
use super::glyph::Glyph;
use super::paint::{self, Seen, State};
use crate::theme::TokensExt as _;

/// The width of the column that holds the two step halves.
pub const STEPPER_WIDTH: f32 = 20.0;

/// A stepper field over `range`, showing and setting `value`.
pub fn stepper_field(value: &mut i64, range: RangeInclusive<i64>) -> StepperField<'_> {
    StepperField {
        value,
        range,
        step: 1,
        suffix: String::new(),
        width: None,
        id: None,
        state: State::Live,
    }
}

#[must_use = "add it with ui.add"]
pub struct StepperField<'a> {
    value: &'a mut i64,
    range: RangeInclusive<i64>,
    step: i64,
    suffix: String,
    width: Option<f32>,
    id: Option<Id>,
    state: State,
}

impl StepperField<'_> {
    /// How far one step goes.
    pub fn step(mut self, step: i64) -> Self {
        self.step = step;
        self
    }

    /// The unit, in muted text after the number, such as "px".
    pub fn suffix(mut self, suffix: impl Into<String>) -> Self {
        self.suffix = suffix.into();
        self
    }

    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

    pub fn id(mut self, id: Id) -> Self {
        self.id = Some(id);
        self
    }

    /// Forces a state for the gallery: hover and press show on the up half.
    pub fn forced(mut self, state: State) -> Self {
        self.state = state;
        self
    }
}

impl Widget for StepperField<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let t = ui.tokens();
        paint::stated(ui, self.state, |ui| {
            let c = &t.color;
            let style = t.ty.numeric.clone();
            let (lo, hi) = (*self.range.start(), *self.range.end());
            let inset = t.space.control_x - t.stroke.focus_ring;
            let suffix_width = if self.suffix.is_empty() {
                0.0
            } else {
                paint::width(ui, &t.ty.body, &self.suffix) + PART_GAP
            };
            let width = self.width.unwrap_or_else(|| {
                let digits = [lo, hi]
                    .map(|end| paint::width(ui, &style, &end.to_string()))
                    .into_iter()
                    .fold(0.0, f32::max);
                (2.0 * inset + digits + 2.0 * PART_GAP + suffix_width + STEPPER_WIDTH).ceil()
            });
            let size = vec2(width, t.control_height());
            let (rect, outer) = ui.allocate_exact_size(size, Sense::hover());
            let id = self.id.unwrap_or(outer.id.with("edit"));
            let column = rect.with_min_x(rect.right() - STEPPER_WIDTH);
            let inner = Rect::from_min_max(
                pos2(rect.left() + inset, rect.top()),
                pos2(column.left() - suffix_width - PART_GAP, rect.bottom()),
            );
            let well = ui.painter().add(Shape::Noop);
            let line = ui.painter().add(Shape::Noop);

            let old = *self.value;
            let mut value = old.clamp(lo, hi);
            let draft_id = id.with("draft");
            let mut s = paint::seen(&outer, self.state);
            let mut response = outer;
            let live = self.state == State::Live;
            if live {
                let mut text = ui
                    .data(|d| d.get_temp::<String>(draft_id))
                    .unwrap_or_else(|| value.to_string());
                let hint = style.rich("");
                let edited = field::edit_line(ui, inner, &mut text, &style, &t, hint, Some(id));
                let clicked_beside = response.hovered()
                    && !edited.hovered()
                    && !column.contains(ui.input(|i| i.pointer.interact_pos()).unwrap_or_default())
                    && ui.input(|i| i.pointer.primary_clicked());
                if clicked_beside {
                    edited.request_focus();
                }
                let typed = |text: &str| text.trim().parse::<i64>().ok().map(|v| v.clamp(lo, hi));
                let mut steps = ui.input(|i| {
                    i.num_accesskit_action_requests(id, Action::Increment) as i64
                        - i.num_accesskit_action_requests(id, Action::Decrement) as i64
                });
                if edited.has_focus() {
                    steps += ui.input(|i| {
                        i.num_presses(Key::ArrowUp) as i64 - i.num_presses(Key::ArrowDown) as i64
                    });
                }
                let entered = edited.lost_focus()
                    || (edited.has_focus() && ui.input(|i| i.key_pressed(Key::Enter)));
                if entered || steps != 0 {
                    // What was typed counts before a step moves on from it; text that is
                    // not a number is dropped and the value stays.
                    value = typed(&text).unwrap_or(value);
                    ui.data_mut(|d| d.remove::<String>(draft_id));
                } else if edited.has_focus() {
                    ui.data_mut(|d| d.insert_temp(draft_id, text));
                }
                value = value
                    .saturating_add(steps.saturating_mul(self.step))
                    .clamp(lo, hi);
                s.focus = edited.has_focus();
                s.hover = edited.hovered() || response.hovered();
                response = edited;
            } else {
                let ink = c.text.primary;
                let written = value.to_string();
                let drawn = paint::text_in(ui, inner, Align2::LEFT_CENTER, &style, &written, ink);
                if s.focus {
                    let caret = Stroke::new(super::glyph::STROKE, c.accent.text);
                    let x = drawn.right() + caret.width / 2.0;
                    ui.painter().vline(x, drawn.y_range(), caret);
                }
            }

            // The two halves, inside the outline.
            let rim = t.stroke.hairline;
            let corner = (t.radius.control - rim).round() as u8;
            let halves = [
                (
                    Rect::from_min_max(
                        pos2(column.left(), rect.top() + rim),
                        pos2(rect.right() - rim, rect.center().y),
                    ),
                    CornerRadius {
                        ne: corner,
                        ..CornerRadius::ZERO
                    },
                    Glyph::ChevronUp,
                    1,
                    value < hi,
                ),
                (
                    Rect::from_min_max(
                        pos2(column.left(), rect.center().y),
                        pos2(rect.right() - rim, rect.bottom() - rim),
                    ),
                    CornerRadius {
                        se: corner,
                        ..CornerRadius::ZERO
                    },
                    Glyph::ChevronDown,
                    -1,
                    value > lo,
                ),
            ];
            let enabled = ui.is_enabled();
            for (i, (half, corners, glyph, direction, can)) in halves.into_iter().enumerate() {
                let part = ui.interact(half, id.with(("step", i)), Sense::hover());
                let pressed = live
                    && can
                    && enabled
                    && part.hovered()
                    && ui.input(|input| input.pointer.primary_down());
                if live
                    && can
                    && enabled
                    && part.hovered()
                    && ui.input(|input| input.pointer.primary_clicked())
                {
                    value = value.saturating_add(direction * self.step).clamp(lo, hi);
                    ui.data_mut(|d| d.remove::<String>(draft_id));
                }
                let seen = match self.state {
                    State::Live => Seen {
                        hover: can && enabled && part.hovered(),
                        press: pressed,
                        focus: false,
                    },
                    State::Hover | State::Press if i == 0 => paint::seen(&part, self.state),
                    _ => paint::seen(&part, State::Rest),
                };
                let mut painter = ui.painter().clone();
                if !can {
                    painter.multiply_opacity(t.opacity.disabled);
                }
                let fill = paint::state_fill(&t, Color32::TRANSPARENT, seen);
                painter.rect_filled(half, corners, fill);
                let ink = if seen.hover {
                    c.text.primary
                } else {
                    c.text.muted
                };
                // The chevrons sit a little towards the middle, where the halves meet.
                let nudge = vec2(0.0, -(direction as f32) * CHEVRON_NUDGE);
                glyph.paint(&painter, half.center() + nudge, ink);
            }

            ui.painter().set(
                well,
                egui::epaint::RectShape::filled(rect, t.radius.control, c.surface.well),
            );
            let outline = field::outline(&t, false, s);
            ui.painter().set(
                line,
                egui::epaint::RectShape::stroke(
                    rect,
                    t.radius.control,
                    outline,
                    StrokeKind::Inside,
                ),
            );
            ui.painter().vline(
                column.left(),
                rect.y_range().shrink(rim),
                Stroke::new(rim, c.stroke.hairline),
            );
            if !self.suffix.is_empty() {
                let at = rect.with_max_x(column.left() - PART_GAP);
                let ink = c.text.muted;
                paint::text_in(ui, at, Align2::RIGHT_CENTER, &t.ty.body, &self.suffix, ink);
            }

            if live {
                ui.ctx().accesskit_node_builder(id, |node| {
                    node.set_numeric_value(value as f64);
                    node.set_min_numeric_value(lo as f64);
                    node.set_max_numeric_value(hi as f64);
                    node.set_numeric_value_step(self.step as f64);
                    if value < hi {
                        node.add_action(Action::Increment);
                    }
                    if value > lo {
                        node.add_action(Action::Decrement);
                    }
                });
            }
            if value != old {
                *self.value = value;
                response.mark_changed();
            }
            response
        })
    }
}

/// How far each chevron is moved towards the middle of the field.
const CHEVRON_NUDGE: f32 = 1.0;
