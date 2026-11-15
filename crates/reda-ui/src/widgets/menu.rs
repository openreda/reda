//! Menu items. The menu recipes (popup, item, shortcut, separator) belong to the menus
//! slice; until it lands an item is egui's own button, which a stock menu draws as a
//! flat full-width row. Call sites already name the REDA widget, so the recipe arrives
//! here without touching them.

use egui::{Response, Ui, Widget};

/// One item of a menu or a context menu.
pub fn menu_item(text: impl Into<String>) -> MenuItem {
    MenuItem { text: text.into() }
}

#[must_use = "add it with ui.add"]
pub struct MenuItem {
    text: String,
}

impl Widget for MenuItem {
    fn ui(self, ui: &mut Ui) -> Response {
        ui.add(egui::Button::new(self.text))
    }
}
