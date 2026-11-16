//! The text view: a long text that is read, not edited, in a field's well, selectable so
//! that it can be copied (the About box's third-party notices).
//!
//! It is drawn as a multi-line field is (`surface.well`, the `stroke.strong` outline, the
//! focus ring while it has the keyboard), but it grows to the whole text instead of
//! holding a number of rows: put it in a scroll area to cap its height. Assistive tech
//! reads it as a multi-line text input whose value is the text.

use egui::{Rect, Response, Sense, Shape, StrokeKind, Ui, UiBuilder, Widget, vec2};

use super::field;
use super::paint::{self, State};
use crate::theme::TokensExt as _;

/// A read-only, selectable view of `text`, as wide as the `Ui` gives it.
pub fn text_view(text: &str) -> TextView<'_> {
    TextView {
        text,
        mono: false,
        state: State::Live,
    }
}

#[must_use = "add it with ui.add"]
pub struct TextView<'a> {
    text: &'a str,
    mono: bool,
    state: State,
}

impl TextView<'_> {
    /// The monospace style: notices, logs, file contents.
    pub fn mono(mut self) -> Self {
        self.mono = true;
        self
    }

    pub fn forced(mut self, state: State) -> Self {
        self.state = state;
        self
    }
}

impl Widget for TextView<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let t = ui.tokens();
        paint::stated(ui, self.state, |ui| {
            let c = &t.color;
            let style = if self.mono {
                t.ty.mono.clone()
            } else {
                t.ty.body.clone()
            };
            let width = ui.available_width();
            // The text keeps clear of the focus ring, which is drawn inside the well.
            let inset = vec2(t.space.control_x - t.stroke.focus_ring, t.space.control_y);
            let well = ui.painter().add(Shape::Noop);
            let outline = ui.painter().add(Shape::Noop);
            let top = ui.cursor().min;
            let inner =
                Rect::from_min_size(top + inset, vec2(width - 2.0 * inset.x, f32::INFINITY));
            let mut child = ui.new_child(UiBuilder::new().max_rect(inner));
            let ink = c.text.primary;
            let lay = style.clone();
            let mut layouter = move |ui: &Ui, buffer: &dyn egui::TextBuffer, wrap: f32| {
                let mut job = lay.job(buffer.as_str(), ink);
                job.wrap.max_width = wrap;
                ui.painter().layout_job(job)
            };
            // A `&str` buffer: egui lets the text be selected and copied, never changed.
            let mut text = self.text;
            let edit = egui::TextEdit::multiline(&mut text)
                .frame(egui::Frame::NONE)
                .margin(egui::Margin::ZERO)
                .desired_rows(1)
                .desired_width(inner.width())
                .layouter(&mut layouter);
            child.spacing_mut().interact_size.y = style.line;
            let edited = if self.state == State::Live {
                child.add(edit)
            } else {
                // A forced state draws the text without an editor to hold it.
                let galley = paint::galley(&child, &style, self.text, ink);
                let (rect, response) = child.allocate_exact_size(galley.size(), Sense::hover());
                child.painter().galley(rect.min, galley, ink);
                response
            };
            let rect = Rect::from_min_max(top, edited.rect.max + inset).with_max_x(top.x + width);
            let response = ui.allocate_rect(rect, Sense::hover());
            let mut s = paint::seen(&response, self.state);
            if self.state == State::Live {
                s.focus = edited.has_focus();
                if s.focus {
                    paint::draws_focus(ui.ctx(), edited.id);
                }
                s.hover = edited.hovered() || response.hovered();
            }
            let line = field::outline(&t, false, s);
            ui.painter().set(
                well,
                egui::epaint::RectShape::filled(rect, t.radius.control, c.surface.well),
            );
            ui.painter().set(
                outline,
                egui::epaint::RectShape::stroke(rect, t.radius.control, line, StrokeKind::Inside),
            );
            edited
        })
    }
}
