//! The dialogs that write elements into a chosen Library: Copy to Library…
//! (`library.element.copy`), Make Project Library (`library.extract_from_project` with no
//! `components`) and Extract, which Edit in Library offers on Source Missing
//! (`library.extract_from_project` on one Component). Each asks only for the Library.

use std::any::Any;

use egui::Id;
use reda_engine::command::CommandError;
use reda_engine::model::Uuid;
use serde_json::{Value, json};

use crate::command_dialog::{Outcome, ParamsDialog};
use crate::shell::Shell;
use crate::tr;

pub const COPY: &str = "library.element.copy";
pub const EXTRACT: &str = "library.extract_from_project";

/// A Library picker over one of the Commands above, with the params the call gave.
pub struct ToLibraryDialog {
    command: &'static str,
    given: Value,
    pub library: Option<Uuid>,
    title: String,
    intro: String,
    run: String,
    error: Option<String>,
    /// Whether the first frame has picked the starting Library.
    started: bool,
}

impl ToLibraryDialog {
    /// The dialog for `command` with what the call gave.
    pub fn new(shell: &Shell, command: &'static str, given: &Value) -> ToLibraryDialog {
        let (title, intro, run) = if command == COPY {
            let element = given["element"].as_str().and_then(|s| s.parse().ok());
            let name = element
                .and_then(|e| shell.engine().libraries().element(e))
                .map(|e| e.name().to_owned())
                .unwrap_or_default();
            (
                tr!("to-library-copy-title"),
                tr!("to-library-copy-intro", name = name),
                tr!("to-library-copy-run"),
            )
        } else if let Some(components) = given["components"].as_array() {
            let designators: Vec<String> = components
                .iter()
                .filter_map(|c| c.as_str()?.parse::<Uuid>().ok())
                .filter_map(|c| designator(shell, c))
                .collect();
            (
                tr!("to-library-extract-title"),
                tr!(
                    "to-library-extract-intro",
                    designator = designators.join(", ")
                ),
                tr!("to-library-extract-run"),
            )
        } else {
            (
                tr!("to-library-make-title"),
                tr!("to-library-make-intro"),
                tr!("to-library-extract-run"),
            )
        };
        ToLibraryDialog {
            command,
            given: given.clone(),
            library: None,
            title,
            intro,
            run,
            error: None,
            started: false,
        }
    }

    /// The params to run with, once a Library is chosen.
    pub fn params(&self) -> Option<Value> {
        let mut params = self.given.clone();
        params["library"] = json!(self.library?);
        Some(params)
    }
}

/// The Designator of the placed Component `component`.
fn designator(shell: &Shell, component: Uuid) -> Option<String> {
    let project = shell.engine().project()?;
    project
        .documents()
        .filter_map(|d| project.schematic_sheet(d.uuid))
        .flat_map(|s| s.units())
        .find(|u| u.component.uuid == component)
        .map(|u| u.designator.clone())
}

impl ParamsDialog for ToLibraryDialog {
    fn command(&self) -> &str {
        self.command
    }

    fn show(&mut self, ctx: &egui::Context, shell: &mut Shell) -> Outcome {
        if !self.started {
            self.started = true;
            self.library = super::initial_library(shell, &self.given["library"]);
        }
        let mut outcome = Outcome::Open;
        let id = Id::NULL.with(("to-library", self.command));
        let modal = crate::modal::Dialog::new(id, self.title.clone())
            .width(420.0)
            .show(ctx, |ui| {
                ui.label(&self.intro);
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    super::library_picker(ui, shell, id.with("library"), &mut self.library);
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
                    let run = ui.add_enabled(params.is_some(), egui::Button::new(&self.run));
                    if let Some(params) = params
                        && run.clicked()
                    {
                        outcome = Outcome::Run(params);
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
