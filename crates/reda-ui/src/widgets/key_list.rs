//! A list of keys and what they do: the Keys in Tool card's rows
//! (docs/spec/v0.1/app.md#keys-in-tool).
//!
//! Two columns: each key as a key chip, then its action in body text, the actions lined
//! up in one column.
//!
//! ```ignore
//! widgets::key_list(ui, id.with("rows"), &[("W".into(), "Wire".into())]);
//! ```

use egui::{Grid, Ui};

use super::tooltip::key_chip;
use crate::theme::TokensExt as _;

/// Draws `rows` of `(key, action)`, the actions in one column.
pub fn key_list(ui: &mut Ui, id_salt: impl egui::AsIdSalt, rows: &[(String, String)]) {
    let t = ui.tokens();
    Grid::new(egui::IdSalt::new(id_salt))
        .num_columns(2)
        .spacing([t.space.section, t.space.row_y])
        .show(ui, |ui| {
            for (key, action) in rows {
                ui.add(key_chip(key.as_str()));
                let text = t.ty.body.rich(action.as_str()).color(t.color.text.primary);
                ui.label(text);
                ui.end_row();
            }
        });
}
