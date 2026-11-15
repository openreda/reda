//! What gallery pages are laid out with: the page ground, a titled section card, and a
//! grid with one column per forced state.

use egui::{Frame, Grid, Stroke, Ui};
use reda_ui::theme::TokensExt as _;
use reda_ui::widgets::State;

/// The states every control is shown in, with their column headers.
pub const STATES: [(State, &str); 5] = [
    (State::Rest, "Rest"),
    (State::Hover, "Hover"),
    (State::Press, "Press"),
    (State::Focus, "Focus"),
    (State::Disabled, "Disabled"),
];

/// The page ground: the base surface with the dialog padding.
pub fn page(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    let t = ui.tokens();
    Frame::new()
        .fill(t.color.surface.base)
        .inner_margin(t.space.dialog)
        .show(ui, |ui| {
            ui.set_min_size(ui.available_size());
            add(ui);
        });
}

/// A card with a title: one family of widgets.
pub fn section(ui: &mut Ui, title: &str, add: impl FnOnce(&mut Ui)) {
    let t = ui.tokens();
    Frame::new()
        .fill(t.color.surface.card)
        .stroke(Stroke::new(t.stroke.hairline, t.color.stroke.hairline))
        .corner_radius(t.radius.card)
        .inner_margin(t.space.panel)
        .show(ui, |ui| {
            ui.label(t.ty.title.rich(title).color(t.color.text.primary));
            ui.add_space(t.space.stack);
            add(ui);
        });
}

/// Small muted text: a row's name, a column header, a note.
pub fn caption(ui: &mut Ui, text: &str) {
    let t = ui.tokens();
    ui.label(t.ty.small.rich(text).color(t.color.text.muted));
}

/// A grid of rows, each drawn once per state. `rows` gives a row's name and how to draw
/// it in a state.
pub fn states(
    ui: &mut Ui,
    id: &str,
    column: f32,
    rows: &mut [(&str, &mut dyn FnMut(&mut Ui, State))],
) {
    let t = ui.tokens();
    Grid::new(id)
        .num_columns(STATES.len() + 1)
        .min_col_width(column)
        .spacing([t.space.gap * 2.0, t.space.stack])
        .show(ui, |ui| {
            ui.label("");
            for (_, name) in STATES {
                caption(ui, name);
            }
            ui.end_row();
            for (name, draw) in rows.iter_mut() {
                caption(ui, name);
                for (state, _) in STATES {
                    ui.horizontal(|ui| draw(ui, state));
                }
                ui.end_row();
            }
        });
}

/// A row with a name and anything after it: for what has no states.
pub fn row(ui: &mut Ui, name: &str, column: f32, add: impl FnOnce(&mut Ui)) {
    let t = ui.tokens();
    ui.horizontal(|ui| {
        ui.allocate_ui_with_layout(
            egui::vec2(column, t.control_height()),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.set_min_width(column);
                caption(ui, name);
            },
        );
        ui.add_space(t.space.gap * 2.0);
        add(ui);
    });
}
