//! The text view: a long text that is read, not edited, in a field's well, selectable so
//! that it can be copied (the About box's third-party notices).
//!
//! It is drawn as a multi-line field is (`surface.well`, the `stroke.strong` outline, the
//! focus ring while it has the keyboard), but it grows to the whole text instead of
//! holding a number of rows: put it in a scroll area to cap its height. Assistive tech
//! reads it as a multi-line text input whose value is the text.

use egui::{Response, Sense, StrokeKind, Ui, Widget, vec2};

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
            // The text keeps clear of the focus ring, which is drawn inside the well.
            let inset = vec2(t.space.control_x - t.stroke.focus_ring, t.space.control_y);
            let mut prepared = egui::Frame::new()
                .fill(c.surface.well)
                .corner_radius(t.radius.control)
                .inner_margin(egui::Margin::symmetric(inset.x as i8, inset.y as i8))
                .begin(ui);
            let inside = &mut prepared.content_ui;
            let width = inside.available_width();
            let ink = c.text.primary;
            let edited = if self.state == State::Live {
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
                    .desired_width(width)
                    .layouter(&mut layouter);
                // egui grows a text edit to `interact_size.y`; the line height is the token's.
                inside.spacing_mut().interact_size.y = style.line;
                inside.add(edit)
            } else {
                // A forced state draws the text without an editor to hold it.
                let mut job = style.job(self.text, ink);
                job.wrap.max_width = width;
                let galley = inside.painter().layout_job(job);
                let size = vec2(width, galley.size().y);
                let (rect, response) = inside.allocate_exact_size(size, Sense::hover());
                inside.painter().galley(rect.min, galley, ink);
                response
            };
            let s = if self.state == State::Live {
                let focus = edited.has_focus();
                if focus {
                    paint::draws_focus(ui.ctx(), edited.id);
                }
                paint::Seen::still(edited.hovered(), false, focus)
            } else {
                paint::seen(&edited, self.state)
            };
            let response = prepared.end(ui);
            // The outline goes inside the well, so focus does not change its size.
            let line = field::outline(&t, false, s);
            ui.painter()
                .rect_stroke(response.rect, t.radius.control, line, StrokeKind::Inside);
            edited.union(response)
        })
    }
}
