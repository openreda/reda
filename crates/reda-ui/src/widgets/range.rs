//! Progress and sliders (docs/spec/v0.1/design-language.md#progress-and-sliders).

use std::ops::RangeInclusive;

use egui::accesskit::{Action, ActionData};
use egui::{
    Align2, Color32, EventFilter, Key, Rect, Response, Role, Sense, Stroke, Ui, Widget, WidgetInfo,
    pos2, vec2,
};

use super::paint::{self, State};
use crate::theme::TokensExt as _;

/// The height of a progress bar's rail.
pub const PROGRESS_RAIL: f32 = 6.0;
/// The height of a slider's rail.
pub const SLIDER_RAIL: f32 = 4.0;
/// The width of a slider's rail when it is given none.
pub const SLIDER_WIDTH: f32 = 100.0;
/// The diameter of a slider's knob.
pub const SLIDER_KNOB: f32 = 16.0;
/// How far outside the knob its focus ring sits.
const KNOB_RING_GAP: f32 = 1.0;
/// The share of the rail one arrow key moves a slider that has no step of its own.
const KEY_STEP: f64 = 0.01;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Outcome {
    Running,
    Success,
    Error,
}

/// A progress bar: a rounded rail in `fill.control`, filled with the accent solid up to
/// `fraction` (0 to 1). Its final state turns to the success or the error solid.
pub fn progress(fraction: f32) -> Progress {
    Progress {
        fraction,
        width: None,
        outcome: Outcome::Running,
        text: String::new(),
    }
}

#[must_use = "add it with ui.add"]
pub struct Progress {
    fraction: f32,
    width: Option<f32>,
    outcome: Outcome,
    text: String,
}

impl Progress {
    /// The rail's width; all there is when not given.
    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

    /// Finished well.
    pub fn success(mut self) -> Self {
        self.outcome = Outcome::Success;
        self
    }

    /// Finished badly, or stopped.
    pub fn error(mut self) -> Self {
        self.outcome = Outcome::Error;
        self
    }

    /// What is progressing, for a screen reader. Nothing is drawn.
    pub fn named(mut self, text: impl Into<String>) -> Self {
        self.text = text.into();
        self
    }
}

impl Widget for Progress {
    fn ui(self, ui: &mut Ui) -> Response {
        let t = ui.tokens();
        let width = self.width.unwrap_or_else(|| ui.available_width());
        let (rect, response) = ui.allocate_exact_size(vec2(width, PROGRESS_RAIL), Sense::hover());
        let fraction = self.fraction.clamp(0.0, 1.0);
        response.widget_info(|| {
            let mut info = WidgetInfo::labeled(Role::ProgressIndicator, true, &self.text);
            info.value = Some(f64::from(fraction));
            info
        });
        if ui.is_rect_visible(rect) {
            let round = PROGRESS_RAIL / 2.0;
            let solid = match self.outcome {
                Outcome::Running => t.color.accent.solid,
                Outcome::Success => t.color.success.solid,
                Outcome::Error => t.color.error.solid,
            };
            ui.painter().rect_filled(rect, round, t.color.fill.control);
            if fraction > 0.0 {
                // Never narrower than its own round ends.
                let done = (width * fraction).max(PROGRESS_RAIL);
                let done = rect.with_max_x(rect.left() + done);
                ui.painter().rect_filled(done, round, solid);
            }
        }
        response
    }
}

/// A slider over `range`: a rail filled with the accent solid up to a white knob, then
/// the value in the numeric style and, when given, a label.
pub fn slider(value: &mut f64, range: RangeInclusive<f64>) -> Slider<'_> {
    Slider {
        value,
        range,
        logarithmic: false,
        step: None,
        suffix: String::new(),
        formatter: None,
        text: String::new(),
        width: None,
        show_value: true,
        state: State::Live,
    }
}

type Formatter<'a> = Box<dyn Fn(f64) -> String + 'a>;

#[must_use = "add it with ui.add"]
pub struct Slider<'a> {
    value: &'a mut f64,
    range: RangeInclusive<f64>,
    logarithmic: bool,
    step: Option<f64>,
    suffix: String,
    formatter: Option<Formatter<'a>>,
    text: String,
    width: Option<f32>,
    show_value: bool,
    state: State,
}

impl<'a> Slider<'a> {
    /// Gives the small values as much of the rail as the large ones. The range must be
    /// above zero.
    pub fn logarithmic(mut self, logarithmic: bool) -> Self {
        self.logarithmic = logarithmic;
        self
    }

    /// The smallest change the value takes.
    pub fn step(mut self, step: f64) -> Self {
        self.step = Some(step);
        self
    }

    /// Written after the value, such as `" s"`.
    pub fn suffix(mut self, suffix: impl Into<String>) -> Self {
        self.suffix = suffix.into();
        self
    }

    /// How the value is written, instead of as a number.
    pub fn formatter(mut self, formatter: impl Fn(f64) -> String + 'a) -> Self {
        self.formatter = Some(Box::new(formatter));
        self
    }

    /// The label after the value, which also names the slider to a screen reader.
    pub fn text(mut self, text: impl Into<String>) -> Self {
        self.text = text.into();
        self
    }

    /// The rail's width.
    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

    /// Leaves the value out: the rail and the label only.
    pub fn show_value(mut self, show: bool) -> Self {
        self.show_value = show;
        self
    }

    pub fn forced(mut self, state: State) -> Self {
        self.state = state;
        self
    }

    fn ends(&self) -> (f64, f64) {
        (*self.range.start(), *self.range.end())
    }

    /// Where `value` sits along the rail, 0 to 1.
    fn share(&self, value: f64) -> f64 {
        let (lo, hi) = self.ends();
        if hi <= lo {
            return 0.0;
        }
        let share = if self.logarithmic && lo > 0.0 {
            (value.max(lo) / lo).ln() / (hi / lo).ln()
        } else {
            (value - lo) / (hi - lo)
        };
        share.clamp(0.0, 1.0)
    }

    /// The value at `share` of the rail, before rounding.
    fn at(&self, share: f64) -> f64 {
        let (lo, hi) = self.ends();
        let share = share.clamp(0.0, 1.0);
        if self.logarithmic && lo > 0.0 {
            lo * (hi / lo).powf(share)
        } else {
            lo + (hi - lo) * share
        }
    }

    /// How many decimals a value is kept and shown with when there is no step: three
    /// significant digits.
    fn decimals(&self, value: f64) -> usize {
        if let Some(step) = self.step {
            return if step.fract() == 0.0 { 0 } else { 2 };
        }
        match value.abs() {
            v if v >= 100.0 => 0,
            v if v >= 10.0 => 1,
            _ => 2,
        }
    }

    /// `value` as it is stored: in range, on the step grid, and no finer than it is shown.
    fn rounded(&self, value: f64) -> f64 {
        let (lo, hi) = self.ends();
        let value = value.clamp(lo, hi);
        let value = match self.step {
            Some(step) if step > 0.0 => lo + ((value - lo) / step).round() * step,
            _ => {
                let grid = 10f64.powi(self.decimals(value) as i32);
                (value * grid).round() / grid
            }
        };
        value.clamp(lo, hi)
    }

    fn written(&self, value: f64) -> String {
        match &self.formatter {
            Some(formatter) => formatter(value),
            None => format!("{value:.*}{}", self.decimals(value), self.suffix),
        }
    }
}

impl Widget for Slider<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let t = ui.tokens();
        let state = self.state;
        paint::stated(ui, state, |ui| {
            let c = &t.color;
            let rail_width = self.width.unwrap_or(SLIDER_WIDTH);
            let (lo, hi) = self.ends();
            // The value's room is that of its widest end, so the label does not move.
            let value_width = if self.show_value {
                let widest = [lo, hi]
                    .map(|end| paint::width(ui, &t.ty.numeric, &self.written(end)))
                    .into_iter()
                    .fold(0.0, f32::max);
                t.space.gap + widest
            } else {
                0.0
            };
            let label_width = if self.text.is_empty() {
                0.0
            } else {
                t.space.gap + paint::width(ui, &t.ty.body, &self.text)
            };
            let size = vec2(rail_width + value_width + label_width, t.control_height());
            let (whole, allocated) = ui.allocate_exact_size(size, Sense::hover());
            let rect = whole.with_max_x(whole.left() + rail_width);
            let id = allocated.id.with("rail");
            let mut response = ui.interact(rect, id, Sense::click_and_drag());

            let old = *self.value;
            let knob = SLIDER_KNOB / 2.0;
            let travel = (rect.left() + knob)..=(rect.right() - knob);
            if let Some(pointer) = response.interact_pointer_pos() {
                let share = egui::remap_clamp(pointer.x, travel.clone(), 0.0..=1.0);
                *self.value = self.rounded(self.at(f64::from(share)));
            }
            let mut steps = ui.input(|i| {
                i.num_accesskit_action_requests(id, Action::Increment) as f64
                    - i.num_accesskit_action_requests(id, Action::Decrement) as f64
            });
            if response.has_focus() {
                // The arrows along the rail move the knob, not the focus.
                ui.memory_mut(|m| {
                    m.set_focus_lock_filter(
                        id,
                        EventFilter {
                            horizontal_arrows: true,
                            ..Default::default()
                        },
                    );
                });
                steps += ui.input(|i| {
                    i.num_presses(Key::ArrowRight) as f64 - i.num_presses(Key::ArrowLeft) as f64
                });
            }
            if steps != 0.0 {
                *self.value = match self.step {
                    Some(step) => self.rounded(*self.value + steps * step),
                    None => self.rounded(self.at(self.share(*self.value) + steps * KEY_STEP)),
                };
            }
            let asked = ui.input(|i| {
                i.accesskit_action_requests(id, Action::SetValue)
                    .filter_map(|request| match request.data {
                        Some(ActionData::NumericValue(value)) => Some(value),
                        _ => None,
                    })
                    .last()
            });
            if let Some(asked) = asked {
                *self.value = self.rounded(asked);
            }
            let value = *self.value;
            if value != old {
                response.mark_changed();
            }
            let enabled = ui.is_enabled();
            response.widget_info(|| WidgetInfo::slider(enabled, value, &self.text));
            ui.ctx().accesskit_node_builder(id, |node| {
                node.set_min_numeric_value(lo);
                node.set_max_numeric_value(hi);
                if let Some(step) = self.step {
                    node.set_numeric_value_step(step);
                }
                node.add_action(Action::SetValue);
                if value < hi {
                    node.add_action(Action::Increment);
                }
                if value > lo {
                    node.add_action(Action::Decrement);
                }
            });

            if !ui.is_rect_visible(whole) {
                return response;
            }
            let s = paint::seen(&response, state);
            let rail = Rect::from_center_size(rect.center(), vec2(rail_width, SLIDER_RAIL));
            let round = SLIDER_RAIL / 2.0;
            let share = self.share(value) as f32;
            let x = egui::lerp(travel, share);
            let at = pos2(x, rect.center().y);
            let p = ui.painter();
            p.rect_filled(rail, round, c.fill.control);
            p.rect_filled(rail.with_max_x(x), round, c.accent.solid);
            p.circle_filled(at, knob, paint::KNOB);
            let rim = if s.hover || s.press {
                c.text.muted
            } else {
                c.stroke.strong
            };
            let rim = Stroke::new(t.stroke.hairline, rim);
            p.circle_stroke(at, knob - rim.width / 2.0, rim);
            if s.focus {
                let ring = Stroke::new(t.stroke.focus_ring, c.stroke.focus_ring);
                p.circle_stroke(at, knob + KNOB_RING_GAP + ring.width / 2.0, ring);
            }
            let mut left = rect.right();
            if self.show_value {
                let room = Rect::from_min_max(
                    pos2(left, whole.top()),
                    pos2(left + value_width, whole.bottom()),
                );
                let written = self.written(value);
                let ink = c.text.primary;
                paint::text_in(ui, room, Align2::RIGHT_CENTER, &t.ty.numeric, &written, ink);
                left += value_width;
            }
            if !self.text.is_empty() {
                let room = Rect::from_min_max(pos2(left + t.space.gap, whole.top()), whole.max);
                let ink: Color32 = c.text.primary;
                paint::text_in(ui, room, Align2::LEFT_CENTER, &t.ty.body, &self.text, ink);
            }
            response
        })
    }
}
