//! The marks a row leads with: the status mark and the swatch of a data colour
//! (docs/spec/v0.1/design-language.md#lists-trees-and-tables).

use egui::{Color32, Painter, Pos2, Rect, Response, Role, Sense, Stroke, StrokeKind, Ui, Vec2};
use egui::{Widget, WidgetInfo};

use super::glyph::{self, Glyph};
use super::paint::{self, State};
use crate::theme::{Tokens, TokensExt as _};

/// The side of a data colour's swatch.
pub const SWATCH: f32 = 14.0;

/// What a status mark says. Each is a shape and a colour, so it reads without the
/// colour: a cross for an Error, a triangle for a Warning, a tick for what is clean, and
/// a muted minus for what is off or waived.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Status {
    Error,
    Warning,
    Success,
    /// Off, waived or stale: muted, with no severity colour.
    Off,
}

impl Status {
    /// Every status with its name, in the order the gallery shows them.
    pub const ALL: [(Status, &'static str); 4] = [
        (Status::Error, "Error"),
        (Status::Warning, "Warning"),
        (Status::Success, "Clean"),
        (Status::Off, "Waived"),
    ];

    /// The shape of this status.
    pub fn glyph(self) -> Glyph {
        match self {
            Status::Error => Glyph::Cross,
            Status::Warning => Glyph::Warning,
            Status::Success => Glyph::Tick,
            Status::Off => Glyph::Minus,
        }
    }

    /// The colour of this status: a semantic tone's text strength, or muted.
    pub fn color(self, t: &Tokens) -> Color32 {
        match self {
            Status::Error => t.color.error.text,
            Status::Warning => t.color.warning.text,
            Status::Success => t.color.success.text,
            Status::Off => t.color.text.muted,
        }
    }

    /// Paints the mark with its Glyph box centred on `center`.
    pub fn paint(self, painter: &Painter, center: Pos2, t: &Tokens) {
        self.glyph().paint(painter, center, self.color(t));
    }
}

/// A status mark: the shape and colour of a severity or a state, in a Glyph's box.
pub fn status_mark(status: Status) -> StatusMark {
    StatusMark { status }
}

#[must_use = "add it with ui.add"]
pub struct StatusMark {
    status: Status,
}

impl Widget for StatusMark {
    fn ui(self, ui: &mut Ui) -> Response {
        let (rect, response) = ui.allocate_exact_size(Vec2::splat(glyph::BOX), Sense::hover());
        if ui.is_rect_visible(rect) {
            self.status.paint(ui.painter(), rect.center(), &ui.tokens());
        }
        response
    }
}

/// Paints a swatch of the data colour `color` in `rect`: the mark radius and a neutral
/// rim, so a colour close to the surface still shows its edge.
pub(super) fn paint_swatch(painter: &Painter, t: &Tokens, rect: Rect, color: Color32) {
    painter.rect_filled(rect, t.radius.mark, color);
    painter.rect_stroke(
        rect,
        t.radius.mark,
        Stroke::new(t.stroke.hairline, t.color.stroke.strong),
        StrokeKind::Inside,
    );
}

/// The swatch of a data colour (a Layer's, a Net's, a Net Class's, a stackup material's):
/// one 14 px shape everywhere. The colour is the user's data, not the theme's, so it is
/// drawn as given in light and dark.
pub fn swatch(color: Color32) -> Swatch {
    Swatch {
        color,
        name: None,
        state: State::Live,
    }
}

/// A swatch from the red, green and blue of a data colour.
pub fn swatch_rgb(r: u8, g: u8, b: u8) -> Swatch {
    swatch(Color32::from_rgb(r, g, b))
}

#[must_use = "add it with ui.add"]
pub struct Swatch {
    color: Color32,
    name: Option<String>,
    state: State,
}

impl Swatch {
    /// Makes the swatch a button that opens the colour's editor, with this accessible
    /// name. A swatch without one only shows the colour.
    pub fn button(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Draws one state whatever the pointer does: for the gallery.
    pub fn forced(mut self, state: State) -> Self {
        self.state = state;
        self
    }
}

impl Widget for Swatch {
    fn ui(self, ui: &mut Ui) -> Response {
        let t = ui.tokens();
        paint::stated(ui, self.state, |ui| {
            let sense = if self.name.is_some() {
                Sense::click()
            } else {
                Sense::hover()
            };
            let (rect, response) = ui.allocate_exact_size(Vec2::splat(SWATCH), sense);
            if let Some(name) = &self.name {
                let enabled = ui.is_enabled();
                response.widget_info(|| WidgetInfo::labeled(Role::Button, enabled, name));
            }
            if !ui.is_rect_visible(rect) {
                return response;
            }
            paint_swatch(ui.painter(), &t, rect, self.color);
            if self.name.is_some() {
                let s = paint::seen(&response, self.state);
                let fill = paint::state_fill(&t, Color32::TRANSPARENT, s);
                if fill != Color32::TRANSPARENT {
                    ui.painter().rect_filled(rect, t.radius.mark, fill);
                }
                if s.focus {
                    paint::focus_ring(ui, &t, rect, t.radius.mark);
                }
            }
            response
        })
    }
}
