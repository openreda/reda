//! The Layer chip and the strip that holds a row of them along the canvas bottom
//! (docs/spec/v0.1/pcb.md#layer-control).
//!
//! A chip is a flat button in body text with a data colour's swatch before its name: the
//! row radius, the neutral hover and press fills, and the accent subtle tint while it is
//! the chosen one (the active Layer). It is shorter than a control so that a row of them
//! fits the 24 px Layer strip.

use egui::{
    Align2, Color32, CornerRadius, Rect, Response, Role, Sense, Stroke, Ui, Vec2, Widget,
    WidgetInfo, pos2, vec2,
};

use super::button::ICON_GAP;
use super::glyph::{self, Glyph};
use super::mark::{SWATCH, paint_swatch};
use super::paint::{self, State};
use crate::theme::TokensExt as _;

/// The padding above and below a chip's text.
pub const CHIP_PAD_Y: f32 = 1.0;
/// Between two chips in a row.
pub const CHIP_GAP: f32 = 2.0;

/// A chip named `text`: a Layer's, or a button that sits among them.
pub fn chip(text: impl Into<String>) -> Chip {
    Chip {
        text: text.into(),
        name: None,
        swatch: None,
        selected: false,
        chevron: false,
        state: State::Live,
    }
}

#[must_use = "add it with ui.add"]
pub struct Chip {
    text: String,
    name: Option<String>,
    swatch: Option<Color32>,
    selected: bool,
    chevron: bool,
    state: State,
}

impl Chip {
    /// The swatch of a data colour before the text: the Layer's.
    pub fn swatch_rgb(mut self, r: u8, g: u8, b: u8) -> Self {
        self.swatch = Some(Color32::from_rgb(r, g, b));
        self
    }

    /// The chosen chip (the active Layer): the accent subtle tint.
    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    /// What assistive tech reads, when it differs from the text (a chip showing only its
    /// swatch still needs a name).
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// A chevron after the text: the chip opens a menu.
    pub fn chevron(mut self) -> Self {
        self.chevron = true;
        self
    }

    pub fn forced(mut self, state: State) -> Self {
        self.state = state;
        self
    }

    /// The chip's height in `ui`: the body line and its padding.
    pub fn height(ui: &Ui) -> f32 {
        ui.tokens().ty.body.line + 2.0 * CHIP_PAD_Y
    }
}

impl Widget for Chip {
    fn ui(self, ui: &mut Ui) -> Response {
        let t = ui.tokens();
        paint::stated(ui, self.state, |ui| {
            let style = &t.ty.body;
            let text_width = if self.text.is_empty() {
                0.0
            } else {
                paint::width(ui, style, &self.text).ceil()
            };
            let mut width = text_width;
            if self.swatch.is_some() {
                width += SWATCH;
                if text_width > 0.0 {
                    width += ICON_GAP;
                }
            }
            if self.chevron {
                width += glyph::BOX;
            }
            let size = vec2(width + 2.0 * t.space.row_x, Chip::height(ui));
            let (rect, response) = ui.allocate_exact_size(size, Sense::click());
            let enabled = ui.is_enabled();
            let name = self.name.as_deref().unwrap_or(&self.text);
            response.widget_info(|| {
                WidgetInfo::selected(Role::Button, enabled, self.selected, name)
            });
            if !ui.is_rect_visible(rect) {
                return response;
            }
            let s = paint::seen(&response, self.state);
            let c = &t.color;
            let under = if self.selected {
                c.accent.subtle
            } else {
                Color32::TRANSPARENT
            };
            let fill = paint::state_fill(&t, under, s);
            if fill != Color32::TRANSPARENT {
                ui.painter().rect_filled(rect, t.radius.row, fill);
            }
            let mut left = rect.left() + t.space.row_x;
            if let Some(color) = self.swatch {
                let at = pos2(left + SWATCH / 2.0, rect.center().y);
                paint_swatch(
                    ui.painter(),
                    &t,
                    Rect::from_center_size(at, Vec2::splat(SWATCH)),
                    color,
                );
                left += SWATCH + ICON_GAP;
            }
            if text_width > 0.0 {
                let label = Rect::from_min_max(pos2(left, rect.top()), rect.right_bottom());
                let drawn = paint::text_in(
                    ui,
                    label,
                    Align2::LEFT_CENTER,
                    style,
                    &self.text,
                    c.text.primary,
                );
                left = drawn.right();
            }
            if self.chevron {
                let at = pos2(left + glyph::BOX / 2.0, rect.center().y);
                Glyph::ChevronDown.paint(ui.painter(), at, c.text.muted);
            }
            if s.focus {
                // Inside the chip: chips sit close together.
                let inset = paint::RING_GAP + t.stroke.focus_ring;
                paint::focus_ring(ui, &t, rect.shrink(inset), t.radius.row - inset);
            }
            response
        })
    }
}

/// The strip in `rect` along a canvas's bottom edge: the card's surface with a hairline
/// on top and the card's corners at the bottom, so it sits inside the Document's card.
/// `add` lays out its chips left to right, [`CHIP_GAP`] apart, vertically centred, in a
/// child of `ui` placed at `rect`; `ui`'s own layout does not move.
pub fn chip_strip<R>(ui: &mut Ui, rect: Rect, add: impl FnOnce(&mut Ui) -> R) -> R {
    let t = ui.tokens();
    let corners = CornerRadius {
        nw: 0,
        ne: 0,
        sw: t.radius.card as u8,
        se: t.radius.card as u8,
    };
    ui.painter().rect_filled(rect, corners, t.color.surface.card);
    ui.painter().hline(
        rect.x_range(),
        rect.top(),
        Stroke::new(t.stroke.hairline, t.color.stroke.hairline),
    );
    let layout = egui::Layout::left_to_right(egui::Align::Center);
    let mut row = ui.new_child(egui::UiBuilder::new().max_rect(rect).layout(layout));
    row.set_clip_rect(rect.intersect(ui.clip_rect()));
    row.spacing_mut().item_spacing.x = CHIP_GAP;
    // Clear of the card's rounded corner.
    row.add_space(t.radius.card - CHIP_GAP);
    add(&mut row)
}

/// A thin upright rule between groups of chips in a strip.
pub fn chip_rule(ui: &mut Ui) {
    let t = ui.tokens();
    let height = Chip::height(ui) - 2.0 * t.space.row_y;
    let (rect, _) = ui.allocate_exact_size(vec2(t.space.gap, height), Sense::hover());
    ui.painter().vline(
        rect.center().x,
        rect.y_range(),
        Stroke::new(t.stroke.hairline, t.color.stroke.hairline),
    );
}
