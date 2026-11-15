//! The colour button: the swatch of a data colour that opens a colour picker
//! (docs/spec/v0.1/design-language.md#other-colour-sources).
//!
//! The button is the one swatch shape every data colour shows in. The picker is egui's,
//! in a menu's popup, and stays open while it is used: a click outside, or Escape,
//! closes it.

use egui::color_picker::{Alpha, color_picker_color32};
use egui::{Color32, Response, Ui, Widget};

use super::mark::swatch;
use crate::theme::TokensExt as _;

/// The width of the colour picker's sliders and of its two-dimensional field.
pub const PICKER_WIDTH: f32 = 275.0;

/// A data colour (a Layer's, a Net's) from its red, green and blue, for the colour
/// button to edit. A data colour is the user's, not the theme's, so this is the one way
/// to build one outside `theme`.
pub const fn data_color(r: u8, g: u8, b: u8) -> Color32 {
    Color32::from_rgb(r, g, b)
}

/// A swatch of `color` that opens a picker for it when clicked. `name` is the button's
/// accessible name. The response is marked changed while the picker changes the colour.
pub fn color_button(color: &mut Color32, name: impl Into<String>) -> ColorButton<'_> {
    ColorButton {
        color,
        name: name.into(),
    }
}

#[must_use = "add it with ui.add"]
pub struct ColorButton<'a> {
    color: &'a mut Color32,
    name: String,
}

impl Widget for ColorButton<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let t = ui.tokens();
        let mut response = ui.add(swatch(*self.color).button(self.name));
        let color = self.color;
        let picked = super::menu(&response).stay_open().show(|ui| {
            let spacing = ui.spacing_mut();
            spacing.slider_width = PICKER_WIDTH;
            spacing.item_spacing.y = t.space.stack;
            color_picker_color32(ui, color, Alpha::Opaque)
        });
        if picked.is_some_and(|picked| picked.inner) {
            response.mark_changed();
        }
        response
    }
}
