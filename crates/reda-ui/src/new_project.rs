//! The New Project dialog: name, location, Fab Preset and the board. It builds the
//! params of one `project.new` Command.

use egui::{Id, Ui};
use serde_json::{Value, json};

use crate::tr;

/// The board a new Project starts with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Board {
    /// No Board Outline yet: it is drawn later.
    DrawLater,
    /// A rectangular Board Outline of the dialog's size.
    Rectangle,
}

/// The New Project dialog's fields.
#[derive(Clone, Debug, PartialEq)]
pub struct NewProjectForm {
    pub name: String,
    pub location: String,
    /// The Fab Presets to pick from: the shipped ones, then the user's.
    pub presets: Vec<String>,
    pub preset: String,
    pub board: Board,
    /// The rectangle's size and corner radius in mm, as typed. The radius may be blank.
    pub width: String,
    pub height: String,
    pub corner_radius: String,
}

/// What the user did with the dialog this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FormAction {
    None,
    Browse,
    Create,
    Cancel,
}

impl NewProjectForm {
    /// A dialog with a suggested name in the user's home folder, offering `presets` with
    /// `default` picked.
    pub fn new(presets: Vec<String>, default: String) -> NewProjectForm {
        NewProjectForm {
            name: tr!("new-project-default-name"),
            location: std::env::home_dir()
                .map(|p| p.display().to_string())
                .unwrap_or_default(),
            presets,
            preset: default,
            board: Board::DrawLater,
            width: "100".into(),
            height: "80".into(),
            corner_radius: String::new(),
        }
    }

    /// The `project.new` params, or `None` while a field is missing or invalid.
    pub fn params(&self) -> Option<Value> {
        let (name, location) = (self.name.trim(), self.location.trim());
        if name.is_empty() || location.is_empty() {
            return None;
        }
        let mut params = json!({"path": location, "name": name, "fab_preset": self.preset});
        if self.board == Board::Rectangle {
            let width = positive_mm(&self.width)?;
            let height = positive_mm(&self.height)?;
            let mut board = json!({"width": width, "height": height});
            if !self.corner_radius.trim().is_empty() {
                board["corner_radius"] = json!(positive_mm(&self.corner_radius)?);
            }
            params["board"] = board;
        }
        Some(params)
    }

    /// Shows the dialog as a modal.
    pub fn show(&mut self, ctx: &egui::Context) -> FormAction {
        let mut action = FormAction::None;
        let modal =
            crate::modal::Dialog::new(Id::NULL.with("new-project"), tr!("new-project-title"))
                .width(380.0)
                .show(ctx, |ui| {
                    ui.add_space(6.0);
                    egui::Grid::new("new-project-fields")
                        .num_columns(2)
                        .spacing([8.0, 6.0])
                        .show(ui, |ui| {
                            labelled_field(ui, &tr!("new-project-name"), &mut self.name, true);
                            ui.end_row();
                            labelled_field(
                                ui,
                                &tr!("new-project-location"),
                                &mut self.location,
                                true,
                            );
                            ui.end_row();
                            ui.label("");
                            if ui.button(tr!("new-project-browse")).clicked() {
                                action = FormAction::Browse;
                            }
                            ui.end_row();
                        });
                    ui.add_space(8.0);
                    ui.strong(tr!("new-project-fab-preset"));
                    for preset in &self.presets {
                        ui.radio_value(&mut self.preset, preset.clone(), preset);
                    }
                    ui.weak(tr!("new-project-fab-preset-note"));
                    ui.add_space(8.0);
                    ui.strong(tr!("new-project-board"));
                    ui.radio_value(
                        &mut self.board,
                        Board::DrawLater,
                        tr!("new-project-draw-later"),
                    );
                    ui.radio_value(
                        &mut self.board,
                        Board::Rectangle,
                        tr!("new-project-rectangle"),
                    );
                    let rectangle = self.board == Board::Rectangle;
                    egui::Grid::new("new-project-board")
                        .num_columns(2)
                        .spacing([8.0, 6.0])
                        .show(ui, |ui| {
                            for (label, value) in [
                                (tr!("new-project-width"), &mut self.width),
                                (tr!("new-project-height"), &mut self.height),
                                (tr!("new-project-corner-radius"), &mut self.corner_radius),
                            ] {
                                labelled_field(ui, &label, value, rectangle);
                                ui.end_row();
                            }
                        });
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        let ready = self.params().is_some();
                        if ui
                            .add_enabled(ready, egui::Button::new(tr!("new-project-create")))
                            .clicked()
                        {
                            action = FormAction::Create;
                        }
                        if ui.button(tr!("dialog-cancel")).clicked() {
                            action = FormAction::Cancel;
                        }
                    });
                });
        if modal.should_close() && action == FormAction::None {
            action = FormAction::Cancel;
        }
        action
    }
}

/// `text` as a length in mm for a Command (`"12.5mm"`), if it is a positive number.
fn positive_mm(text: &str) -> Option<String> {
    let value: f64 = text.trim().parse().ok()?;
    (value.is_finite() && value > 0.0).then(|| format!("{value}mm"))
}

fn labelled_field(ui: &mut Ui, label: &str, value: &mut String, enabled: bool) {
    let label = ui.label(label);
    ui.add_enabled(
        enabled,
        egui::TextEdit::singleline(value).desired_width(240.0),
    )
    .labelled_by(label.id);
}
