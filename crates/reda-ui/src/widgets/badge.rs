//! Badges and counts: a pill of small text on a tone's subtle tint
//! (docs/spec/v0.1/design-language.md#lists-trees-and-tables).
//!
//! A badge names a state ("Up to date", "Locally edited") or a tally ("3 Errors"), with
//! an optional status mark before its text. A count is a number alone on the neutral
//! fill, in tabular digits. Neither is a control: a badge is a label to assistive
//! technology. A badge that filters what a list shows ([`Badge::filter`]) is a toggle:
//! on, it is the badge; off, it goes neutral and muted.

use egui::{Align2, Color32, Response, Role, Sense, Ui, Widget, WidgetInfo, pos2, vec2};

use super::paint::State;

use super::glyph;
use super::mark::Status;
use super::paint;
use crate::theme::{Text, Tokens, TokensExt as _};

/// Inside a badge, sideways.
pub const BADGE_PAD_X: f32 = 8.0;
/// Above and below a badge's line of small text.
pub const BADGE_PAD_Y: f32 = 2.0;
/// Between a badge's status mark and its text.
const MARK_GAP: f32 = 2.0;

/// The tint a badge sits on.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum BadgeTone {
    /// No tone: muted text on the neutral control fill. A count, a state that says
    /// nothing is wrong or right.
    #[default]
    Neutral,
    Accent,
    Error,
    Warning,
    Success,
}

impl BadgeTone {
    /// Every tone with its name, in the order the gallery shows them.
    pub const ALL: [(BadgeTone, &'static str); 5] = [
        (BadgeTone::Neutral, "Neutral"),
        (BadgeTone::Accent, "Accent"),
        (BadgeTone::Error, "Error"),
        (BadgeTone::Warning, "Warning"),
        (BadgeTone::Success, "Success"),
    ];

    /// The pill's fill and the colour of its text.
    fn colors(self, t: &Tokens) -> (Color32, Color32) {
        let c = &t.color;
        match self {
            BadgeTone::Neutral => (c.fill.control, c.text.muted),
            BadgeTone::Accent => (c.accent.subtle, c.text.primary),
            BadgeTone::Error => (c.error.subtle, c.text.primary),
            BadgeTone::Warning => (c.warning.subtle, c.text.primary),
            BadgeTone::Success => (c.success.subtle, c.text.primary),
        }
    }
}

/// A badge: `text` in a pill. Neutral until given a tone.
pub fn badge(text: impl Into<String>) -> Badge {
    Badge {
        text: text.into(),
        tone: BadgeTone::Neutral,
        status: None,
        numeric: false,
        filter: None,
        state: State::Live,
    }
}

/// A count pill: a number on the neutral fill, in tabular digits.
pub fn count_pill(count: impl ToString) -> Badge {
    Badge {
        text: count.to_string(),
        tone: BadgeTone::Neutral,
        status: None,
        numeric: true,
        filter: None,
        state: State::Live,
    }
}

#[must_use = "add it with ui.add"]
pub struct Badge {
    text: String,
    tone: BadgeTone,
    status: Option<Status>,
    numeric: bool,
    filter: Option<bool>,
    state: State,
}

impl Badge {
    /// The tint the pill sits on.
    pub fn tone(mut self, tone: BadgeTone) -> Self {
        self.tone = tone;
        self
    }

    /// A status mark before the text, on that status's tint: a cross on the error tint,
    /// a triangle on the warning tint, a tick on the success tint, and a muted minus on
    /// the neutral fill.
    pub fn status(mut self, status: Status) -> Self {
        self.status = Some(status);
        self.tone = match status {
            Status::Error => BadgeTone::Error,
            Status::Warning => BadgeTone::Warning,
            Status::Success => BadgeTone::Success,
            Status::Off => BadgeTone::Neutral,
        };
        self
    }

    /// Makes the badge a toggle that filters a list by what it counts: a button named by
    /// its text, selected while `on`. Off, it is neutral and muted. Flip the filter when
    /// it is clicked.
    pub fn filter(mut self, on: bool) -> Self {
        self.filter = Some(on);
        self
    }

    /// Draws one state whatever the pointer does: for the gallery.
    pub fn forced(mut self, state: State) -> Self {
        self.state = state;
        self
    }

    fn style<'t>(&self, t: &'t Tokens) -> &'t Text {
        if self.numeric {
            &t.ty.numeric_small
        } else {
            &t.ty.small
        }
    }
}

impl Widget for Badge {
    fn ui(self, ui: &mut Ui) -> Response {
        let t = ui.tokens();
        paint::stated(ui, self.state, |ui| self.show(ui, &t))
    }
}

impl Badge {
    fn show(self, ui: &mut Ui, t: &Tokens) -> Response {
        let style = self.style(t);
        let mark = if self.status.is_some() {
            glyph::BOX + MARK_GAP
        } else {
            0.0
        };
        let text_width = paint::width(ui, style, &self.text);
        // A pill is never narrower than it is tall.
        let height = style.line + 2.0 * BADGE_PAD_Y;
        let width = (text_width + mark + 2.0 * BADGE_PAD_X).max(height);
        let sense = if self.filter.is_some() {
            Sense::click()
        } else {
            Sense::hover()
        };
        let (rect, response) = ui.allocate_exact_size(vec2(width, height), sense);
        let enabled = ui.is_enabled();
        response.widget_info(|| match self.filter {
            Some(on) => WidgetInfo::selected(Role::Button, enabled, on, &self.text),
            None => WidgetInfo::labeled(Role::Label, enabled, &self.text),
        });
        if !ui.is_rect_visible(rect) {
            return response;
        }
        let off = self.filter == Some(false);
        let tone = if off { BadgeTone::Neutral } else { self.tone };
        let (mut fill, ink) = tone.colors(t);
        let radius = height / 2.0;
        if self.filter.is_some() {
            let s = paint::seen(&response, self.state);
            fill = paint::state_fill(t, fill, s);
            if s.focus {
                paint::focus_ring(ui, t, rect, radius);
            }
        }
        ui.painter().rect_filled(rect, radius, fill);
        let mut text = rect;
        if let Some(status) = self.status {
            // The mark's ink starts a few pixels into its box, so the box starts nearer
            // the pill's end than the text would.
            let left = rect.left() + BADGE_PAD_X - MARK_GAP;
            let at = pos2(left + glyph::BOX / 2.0, rect.center().y);
            if off {
                status.glyph().paint(ui.painter(), at, ink);
            } else {
                status.paint(ui.painter(), at, t);
            }
            text.min.x = left + glyph::BOX + MARK_GAP;
            text.max.x = rect.right() - BADGE_PAD_X;
        }
        paint::text_in(ui, text, Align2::CENTER_CENTER, style, &self.text, ink);
        response
    }
}
