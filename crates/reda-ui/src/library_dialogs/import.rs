//! Import part: a symbol file and a footprint file, as vendor download services provide
//! them, picked (or typed) and imported into a chosen Library as a Symbol, a Footprint and
//! a Part with `library.import_part`.

use std::any::Any;

use egui::{Grid, Id, Modal, Role, TextEdit, Ui, WidgetInfo};
use reda_engine::command::CommandError;
use reda_engine::model::Uuid;
use serde_json::{Value, json};

use crate::command_dialog::{Outcome, ParamsDialog};
use crate::dialogs::{FileDialogs, Pick};
use crate::shell::Shell;
use crate::tr;

/// The Import part dialog's state.
pub struct ImportDialog {
    pub symbol_file: String,
    pub footprint_file: String,
    pub library: Option<Uuid>,
    /// The Part's name; empty takes the symbol's.
    pub name: String,
    /// Which symbol of a file holding several; empty takes the only one.
    pub symbol: String,
    error: Option<String>,
    files: FileDialogs,
    /// The Library the call gave, until the first frame picks the starting one.
    given_library: Option<Value>,
}

impl ImportDialog {
    /// The dialog, filled in with what the call gave.
    pub fn new(given: &Value) -> ImportDialog {
        let text = |key: &str| given[key].as_str().unwrap_or_default().to_owned();
        ImportDialog {
            symbol_file: text("symbol_file"),
            footprint_file: text("footprint_file"),
            library: None,
            name: text("name"),
            symbol: text("symbol"),
            error: None,
            files: FileDialogs::default(),
            given_library: Some(given["library"].clone()),
        }
    }

    /// The params to import with, or `None` until both files and a Library are given.
    pub fn params(&self) -> Option<Value> {
        let symbol_file = self.symbol_file.trim();
        let footprint_file = self.footprint_file.trim();
        if symbol_file.is_empty() || footprint_file.is_empty() {
            return None;
        }
        let mut params = json!({
            "library": self.library?,
            "symbol_file": symbol_file,
            "footprint_file": footprint_file,
        });
        for (key, value) in [("name", &self.name), ("symbol", &self.symbol)] {
            if !value.trim().is_empty() {
                params[key] = json!(value.trim());
            }
        }
        Some(params)
    }

    fn file_row(&mut self, ui: &mut Ui, label: String, pick: Pick) {
        ui.label(&label);
        ui.horizontal(|ui| {
            let text = match pick {
                Pick::ImportSymbol => &mut self.symbol_file,
                _ => &mut self.footprint_file,
            };
            let response = ui.add(TextEdit::singleline(text).desired_width(260.0));
            let name = label.clone();
            response.widget_info(|| WidgetInfo::labeled(Role::TextInput, true, name.clone()));
            let browse =
                ui.add_enabled(!self.files.busy(), egui::Button::new(tr!("import-browse")));
            let what = tr!("import-browse-for", what = label.as_str());
            browse.widget_info(|| WidgetInfo::labeled(Role::Button, true, what.clone()));
            if browse.clicked() {
                self.files.start(ui.ctx(), pick);
            }
        });
        ui.end_row();
    }

    fn text_row(ui: &mut Ui, label: String, hint: String, text: &mut String) {
        ui.label(&label);
        let response = ui.add(
            TextEdit::singleline(text)
                .hint_text(hint)
                .desired_width(260.0),
        );
        response.widget_info(|| WidgetInfo::labeled(Role::TextInput, true, label.clone()));
        ui.end_row();
    }
}

impl ParamsDialog for ImportDialog {
    fn command(&self) -> &str {
        "library.import_part"
    }

    fn show(&mut self, ctx: &egui::Context, shell: &mut Shell) -> Outcome {
        if let Some(given) = self.given_library.take() {
            self.library = super::initial_library(shell, &given);
        }
        match self.files.poll() {
            Some((Pick::ImportSymbol, path)) => self.symbol_file = path.display().to_string(),
            Some((Pick::ImportFootprint, path)) => {
                self.footprint_file = path.display().to_string();
            }
            _ => {}
        }
        let mut outcome = Outcome::Open;
        let modal = Modal::new(Id::NULL.with("import-part")).show(ctx, |ui| {
            ui.set_width(480.0);
            ui.heading(tr!("import-title"));
            ui.weak(tr!("import-intro"));
            ui.add_space(6.0);
            Grid::new("import-part-fields")
                .num_columns(2)
                .spacing([8.0, 6.0])
                .show(ui, |ui| {
                    self.file_row(ui, tr!("import-symbol-file"), Pick::ImportSymbol);
                    self.file_row(ui, tr!("import-footprint-file"), Pick::ImportFootprint);
                    super::library_picker(
                        ui,
                        shell,
                        Id::NULL.with("import-library"),
                        &mut self.library,
                    );
                    ui.end_row();
                    Self::text_row(
                        ui,
                        tr!("import-name"),
                        tr!("import-name-hint"),
                        &mut self.name,
                    );
                    Self::text_row(
                        ui,
                        tr!("import-symbol"),
                        tr!("import-symbol-hint"),
                        &mut self.symbol,
                    );
                });
            if super::writable_libraries(shell).is_empty() {
                ui.colored_label(ui.visuals().warn_fg_color, tr!("library-target-missing"));
            }
            if let Some(error) = &self.error {
                ui.colored_label(ui.visuals().error_fg_color, error);
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let params = self.params();
                let run = ui.add_enabled(params.is_some(), egui::Button::new(tr!("import-run")));
                if let Some(params) = params {
                    if run.clicked() {
                        outcome = Outcome::Run(params);
                    }
                } else {
                    run.on_disabled_hover_text(tr!("import-needs"));
                }
                if ui.button(tr!("dialog-cancel")).clicked() {
                    outcome = Outcome::Cancel;
                }
            });
        });
        if modal.should_close() && outcome == Outcome::Open {
            outcome = Outcome::Cancel;
        }
        outcome
    }

    fn failed(&mut self, error: &CommandError) {
        self.error = Some(error.message.clone());
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}
