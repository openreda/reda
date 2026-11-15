//! Form-field rows: a property's name in muted text and its value's control, with every
//! value lined up in one column (docs/spec/v0.1/design-language.md#panels).

use egui::{Align2, Grid, InnerResponse, Response, Role, Sense, Ui, Widget, WidgetInfo, vec2};

use super::paint;
use crate::theme::TokensExt as _;

/// A form: rows of a name and a value, `stack` apart, the values in one column as wide
/// as the widest name allows. Add rows inside with [`form_row`].
pub fn form(id_salt: impl egui::AsIdSalt) -> Form {
    Form {
        id_salt: egui::IdSalt::new(id_salt),
    }
}

#[must_use = "call show"]
pub struct Form {
    id_salt: egui::IdSalt,
}

impl Form {
    pub fn show<R>(self, ui: &mut Ui, rows: impl FnOnce(&mut Ui) -> R) -> InnerResponse<R> {
        let t = ui.tokens();
        Grid::new(self.id_salt)
            .num_columns(2)
            .spacing([t.space.gap, t.space.stack])
            .min_row_height(t.control_height())
            .show(ui, rows)
    }
}

/// One row of a [`form`]: `name` in muted text, then whatever `value` adds (a field, a
/// dropdown, plain text for a value that is read-only). Returns what `value` returned.
pub fn form_row<R>(ui: &mut Ui, name: impl Into<String>, value: impl FnOnce(&mut Ui) -> R) -> R {
    ui.add(form_name(name));
    let out = value(ui);
    ui.end_row();
    out
}

/// The name of a form-field row on its own, for a row whose value cell is built in
/// steps: add the name, the value, then end the row.
pub fn form_name(text: impl Into<String>) -> FormName {
    FormName { text: text.into() }
}

#[must_use = "add it with ui.add"]
pub struct FormName {
    text: String,
}

impl Widget for FormName {
    fn ui(self, ui: &mut Ui) -> Response {
        let t = ui.tokens();
        let galley = paint::galley(ui, &t.ty.body, &self.text, t.color.text.muted);
        let size = vec2(galley.size().x, t.control_height());
        let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
        let enabled = ui.is_enabled();
        response.widget_info(|| WidgetInfo::labeled(Role::Label, enabled, &self.text));
        if ui.is_rect_visible(rect) {
            let at = Align2::LEFT_CENTER
                .align_size_within_rect(galley.size(), rect)
                .min
                .round();
            ui.painter().galley(at, galley, t.color.text.muted);
        }
        response
    }
}
