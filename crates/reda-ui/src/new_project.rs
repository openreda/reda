//! The New Project dialog: name, location, Fab Preset and the board. It builds the
//! params of one `project.new` Command.

use egui::{Id, Ui};
use serde_json::{Value, json};

use crate::settings::{self, layout};
use crate::theme::TokensExt as _;
use crate::tr;
use crate::widgets;

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
    pub layers: LayerCount,
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
            layers: LayerCount::default(),
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
        let mut params = json!({"path": location, "name": name, "fab_preset": self.preset, "layers": self.layers.value()?});
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
                    let t = ui.tokens();
                    widgets::form("new-project-fields").show(ui, |ui| {
                        labelled_field(ui, &tr!("new-project-name"), &mut self.name, true);
                        ui.end_row();
                        labelled_field(ui, &tr!("new-project-location"), &mut self.location, true);
                        ui.end_row();
                        ui.label("");
                        if ui.add(widgets::button(tr!("new-project-browse"))).clicked() {
                            action = FormAction::Browse;
                        }
                        ui.end_row();
                    });
                    ui.add_space(t.space.section);
                    settings::strong(ui, tr!("new-project-fab-preset"));
                    for preset in &self.presets {
                        radio(ui, &mut self.preset, preset.clone(), preset);
                    }
                    layout::muted(ui, tr!("new-project-fab-preset-note"));
                    ui.add_space(t.space.section);
                    settings::strong(ui, tr!("new-project-board"));
                    self.layers.show(ui);
                    radio(
                        ui,
                        &mut self.board,
                        Board::DrawLater,
                        tr!("new-project-draw-later"),
                    );
                    radio(
                        ui,
                        &mut self.board,
                        Board::Rectangle,
                        tr!("new-project-rectangle"),
                    );
                    let rectangle = self.board == Board::Rectangle;
                    widgets::form("new-project-board").show(ui, |ui| {
                        for (label, value) in [
                            (tr!("new-project-width"), &mut self.width),
                            (tr!("new-project-height"), &mut self.height),
                            (tr!("new-project-corner-radius"), &mut self.corner_radius),
                        ] {
                            labelled_field(ui, &label, value, rectangle);
                            ui.end_row();
                        }
                    });
                    crate::modal::buttons(ui, |ui| {
                        let ready = self.params().is_some();
                        if ui
                            .add_enabled(
                                ready,
                                widgets::button(tr!("new-project-create")).primary(),
                            )
                            .clicked()
                        {
                            action = FormAction::Create;
                        }
                        if ui.add(widgets::button(tr!("dialog-cancel"))).clicked() {
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
    let label = ui.add(widgets::form_name(label));
    ui.add_enabled(enabled, widgets::text_field(value).width(FIELD_WIDTH))
        .labelled_by(label.id);
}

/// The width of the dialog's text fields.
const FIELD_WIDTH: f32 = 240.0;

/// A radio button for `choice`: clicking it sets `value` to `choice`.
fn radio<T: PartialEq>(ui: &mut Ui, value: &mut T, choice: T, text: impl Into<String>) {
    if ui.add(widgets::radio(*value == choice, text)).clicked() {
        *value = choice;
    }
}

/// The copper Layer count entered when creating a board.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LayerCount(String);

impl Default for LayerCount {
    fn default() -> Self {
        Self("2".into())
    }
}

impl LayerCount {
    pub fn value(&self) -> Option<u8> {
        self.0.trim().parse().ok().filter(|n| (2..=32).contains(n))
    }

    pub fn show(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            let label = ui.add(widgets::form_name(tr!("stackup-copper-layers")));
            let width = ui.tokens().size.of(60.0);
            ui.add(widgets::text_field(&mut self.0).width(width))
                .labelled_by(label.id);
            layout::muted(ui, tr!("stackup-layer-range"));
        });
    }
}
