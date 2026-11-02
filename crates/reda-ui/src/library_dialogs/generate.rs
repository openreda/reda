//! A Footprint generator's dialog: the form generated from the generator Command's params
//! schema, with the target Library as a picker instead of a UUID field, and a preview of
//! the land pattern the params give, computed as they are typed
//! ([`reda_engine::library::preview_footprint`]).

use std::any::Any;

use egui::{Id, Modal, Ui};
use reda_engine::command::CommandError;
use reda_engine::model::{Footprint, Uuid};
use serde_json::Value;

use crate::command_dialog::{Outcome, ParamForm, ParamsDialog, form_row};
use crate::panels::library::preview;
use crate::shell::Shell;
use crate::tr;

/// A generator's dialog.
pub struct GenerateDialog {
    form: ParamForm,
    library: Option<Uuid>,
    /// The Library the call gave, until the first frame picks the starting one.
    given_library: Option<Value>,
    error: Option<String>,
}

/// What the preview shows for the params so far.
#[derive(Clone, Debug, PartialEq)]
pub enum Preview {
    /// The form is not complete yet.
    Incomplete,
    /// The land pattern the generator would write.
    Footprint(Box<Footprint>),
    /// Why the params give no land pattern.
    Invalid(String),
}

impl GenerateDialog {
    /// The dialog for generator `id`, filled in with what the call gave.
    pub fn new(shell: &Shell, id: &str, given: &Value) -> GenerateDialog {
        GenerateDialog {
            form: ParamForm::new(shell.engine().registry(), id, given),
            library: None,
            given_library: Some(given["library"].clone()),
            error: None,
        }
    }

    /// The generated form, for tests that fill it in.
    pub fn form_mut(&mut self) -> &mut ParamForm {
        &mut self.form
    }

    pub fn library(&self) -> Option<Uuid> {
        self.library
    }

    /// What the preview shows for the params so far.
    pub fn preview(&self, shell: &Shell) -> Preview {
        // Without a Library yet, the preview still shows the land pattern.
        let mut form = self.form.clone();
        if self.library.is_none() {
            form.set_text("library", &Uuid::NIL.to_string());
        }
        let Some(params) = form.params(shell.engine().registry()) else {
            return Preview::Incomplete;
        };
        match reda_engine::library::preview_footprint(self.form.command(), &params) {
            Some(Ok(footprint)) => Preview::Footprint(Box::new(footprint)),
            Some(Err(e)) => Preview::Invalid(e.message),
            None => Preview::Incomplete,
        }
    }

    fn sync_library(&mut self) {
        let text = self.library.map(|l| l.to_string()).unwrap_or_default();
        self.form.set_text("library", &text);
    }

    fn draw_form(&mut self, ui: &mut Ui, shell: &mut Shell) {
        egui::Grid::new("generate-fields")
            .num_columns(2)
            .spacing([8.0, 4.0])
            .show(ui, |ui| {
                super::library_picker(
                    ui,
                    shell,
                    Id::NULL.with("generate-library"),
                    &mut self.library,
                );
                ui.end_row();
                self.sync_library();
                let problems = self.form.problems(shell.engine().registry());
                for field in self.form.fields_mut() {
                    if field.name == "library" {
                        continue;
                    }
                    let problem = problems
                        .iter()
                        .find(|(name, _)| *name == field.name)
                        .map(|(_, m)| m.as_str());
                    form_row(ui, field, problem);
                }
            });
    }
}

fn draw_preview(ui: &mut Ui, preview: &Preview) {
    ui.strong(tr!("generate-preview"));
    match preview {
        Preview::Incomplete => {
            ui.weak(tr!("generate-preview-incomplete"));
        }
        Preview::Invalid(why) => {
            ui.colored_label(ui.visuals().error_fg_color, why);
        }
        Preview::Footprint(footprint) => {
            ui.label(&footprint.name);
            ui.weak(tr!(
                "generate-preview-pads",
                count = footprint.pads.len().to_string()
            ));
            preview::footprint(ui, footprint);
        }
    }
}

impl ParamsDialog for GenerateDialog {
    fn command(&self) -> &str {
        self.form.command()
    }

    fn show(&mut self, ctx: &egui::Context, shell: &mut Shell) -> Outcome {
        if let Some(given) = self.given_library.take() {
            self.library = super::initial_library(shell, &given);
        }
        self.sync_library();
        let mut outcome = Outcome::Open;
        let modal = Modal::new(Id::NULL.with("generate-footprint")).show(ctx, |ui| {
            ui.set_width(480.0);
            ui.heading(self.form.title().to_owned());
            ui.weak(self.form.description().to_owned());
            ui.add_space(6.0);
            self.draw_form(ui, shell);
            ui.separator();
            draw_preview(ui, &self.preview(shell));
            if super::writable_libraries(shell).is_empty() {
                ui.colored_label(ui.visuals().warn_fg_color, tr!("library-target-missing"));
            }
            if let Some(error) = &self.error {
                ui.colored_label(ui.visuals().error_fg_color, error);
            }
            ui.add_space(8.0);
            let ready = self.form.params(shell.engine().registry());
            ui.horizontal(|ui| {
                let run = ui.add_enabled(ready.is_some(), egui::Button::new(tr!("generate-run")));
                match ready {
                    Some(params) if run.clicked() => outcome = Outcome::Run(params),
                    Some(_) => {}
                    None => {
                        run.on_disabled_hover_text(tr!("form-fix-fields"));
                    }
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
