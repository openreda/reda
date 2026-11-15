//! The status bar's items (docs/spec/v0.1/design-language.md#status-bar).
//!
//! The bar is on the base with no rule above it. Its items are flat buttons in small
//! muted text, with the numeric style for coordinates and deltas. A note may take a
//! semantic colour's text strength.

use egui::{
    Align2, Color32, InnerResponse, Rect, Response, Role, Sense, TextStyle, Ui, Vec2, Widget,
    WidgetInfo, pos2, vec2,
};

use super::button::ICON_GAP;
use super::mark::{SWATCH, Status, paint_swatch};
use super::paint::{self, State};
use crate::theme::TokensExt as _;

/// The padding beside an item's text.
pub const STATUS_ITEM_PAD: f32 = 8.0;
/// The least width of an item, so that a blank one can still be clicked.
pub const STATUS_ITEM_MIN: f32 = 12.0;
/// How far an item's hover fill stands in from the bar's top and bottom.
const FILL_INSET: f32 = 2.0;

/// Lays out the status bar's row in `ui`, as tall as the bar: the items `add` shows touch, each bringing its
/// own padding. A stock control that is still put on the bar takes the bar's height and
/// the small text style.
pub fn status_bar<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> InnerResponse<R> {
    let t = ui.tokens();
    let size = vec2(ui.available_width(), t.status_bar_height());
    let row = egui::Layout::left_to_right(egui::Align::Center);
    ui.allocate_ui_with_layout(size, row, |ui| {
        let spacing = ui.spacing_mut();
        spacing.item_spacing.x = 0.0;
        spacing.interact_size.y = t.status_bar_height();
        spacing.button_padding = vec2(STATUS_ITEM_PAD, 0.0);
        ui.style_mut().override_text_style = Some(TextStyle::Small);
        add(ui)
    })
}

/// A flat button on the status bar, as tall as the bar.
pub fn status_item(text: impl Into<String>) -> StatusItem {
    StatusItem {
        text: text.into(),
        numeric: false,
        swatch: None,
        tone: None,
        state: State::Live,
    }
}

/// The colour of text on the status bar: muted, or the text strength of `tone`.
pub fn status_ink(ui: &Ui, tone: Option<Status>) -> Color32 {
    let t = ui.tokens();
    match tone {
        Some(tone) => tone.color(&t),
        None => t.color.text.muted,
    }
}

#[must_use = "add it with ui.add"]
pub struct StatusItem {
    text: String,
    numeric: bool,
    swatch: Option<Color32>,
    tone: Option<Status>,
    state: State,
}

impl StatusItem {
    /// Tabular digits: coordinates, deltas and the grid.
    pub fn numeric(mut self) -> Self {
        self.numeric = true;
        self
    }

    /// The swatch of a data colour before the text: the active Layer's.
    pub fn swatch_rgb(mut self, r: u8, g: u8, b: u8) -> Self {
        self.swatch = Some(Color32::from_rgb(r, g, b));
        self
    }

    /// The text strength of a semantic colour, for a note that warns.
    pub fn tone(mut self, tone: Status) -> Self {
        self.tone = Some(tone);
        self
    }

    pub fn forced(mut self, state: State) -> Self {
        self.state = state;
        self
    }
}

impl Widget for StatusItem {
    fn ui(self, ui: &mut Ui) -> Response {
        let t = ui.tokens();
        paint::stated(ui, self.state, |ui| {
            let style = if self.numeric {
                &t.ty.numeric_small
            } else {
                &t.ty.small
            };
            let lead = if self.swatch.is_some() {
                SWATCH + ICON_GAP
            } else {
                0.0
            };
            let width = lead + paint::width(ui, style, &self.text) + 2.0 * STATUS_ITEM_PAD;
            let size = vec2(width.max(STATUS_ITEM_MIN), t.status_bar_height());
            let (rect, response) = ui.allocate_exact_size(size, Sense::click());
            let enabled = ui.is_enabled();
            response.widget_info(|| WidgetInfo::labeled(Role::Button, enabled, &self.text));
            if !ui.is_rect_visible(rect) {
                return response;
            }
            let s = paint::seen(&response, self.state);
            let ground = rect.shrink2(vec2(0.0, FILL_INSET));
            let fill = paint::state_fill(&t, Color32::TRANSPARENT, s);
            ui.painter().rect_filled(ground, t.radius.row, fill);
            let ink = status_ink(ui, self.tone);
            let mut label = rect.shrink2(vec2(STATUS_ITEM_PAD, 0.0));
            if let Some(color) = self.swatch {
                let at = pos2(label.left() + SWATCH / 2.0, rect.center().y);
                let mark = Rect::from_center_size(at, Vec2::splat(SWATCH));
                paint_swatch(ui.painter(), &t, mark, color);
                label = label.with_min_x(label.left() + lead);
            }
            paint::text_in(ui, label, Align2::CENTER_CENTER, style, &self.text, ink);
            if s.focus {
                // Inside the item: the bar leaves no room outside it.
                paint::focus_ring(
                    ui,
                    &t,
                    ground.shrink(paint::RING_GAP + t.stroke.focus_ring),
                    t.radius.row - paint::RING_GAP - t.stroke.focus_ring,
                );
            }
            response
        })
    }
}
