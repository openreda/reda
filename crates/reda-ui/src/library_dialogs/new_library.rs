//! New Library: a name, a folder and who it is for. A Library for the open Project is
//! created with `library.create` and then registered with `project.library.add` (undoable
//! on the Project stack); a Library for the user is registered in Preferences by
//! `library.create` itself (`register: user`).

use std::any::Any;

use egui::{Grid, Id, Role, TextEdit, Ui, WidgetInfo};
use reda_engine::command::CommandError;
use serde_json::{Value, json};

use crate::command_dialog::{Outcome, ParamsDialog};
use crate::dialogs::{FileDialogs, Pick};
use crate::shell::Shell;
use crate::tr;

/// Who a new Library is registered for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum For {
    /// The open Project, by a path relative to its folder.
    Project,
    /// The user, in Preferences.
    User,
}

/// The New Library dialog's state.
pub struct NewLibraryDialog {
    pub name: String,
    /// The folder; for the Project it may be relative to the Project folder, and empty
    /// means a folder named after the Library in the Project folder.
    pub folder: String,
    pub description: String,
    pub owner: For,
    /// Whether a Project is open (only then can the Library be the Project's).
    has_project: bool,
    error: Option<String>,
    files: FileDialogs,
}

impl NewLibraryDialog {
    /// The dialog, filled in with what the call gave; it is for the Project when one is
    /// open, unless the call asked for `register: user`.
    pub fn new(shell: &Shell, given: &Value) -> NewLibraryDialog {
        let text = |key: &str| given[key].as_str().unwrap_or_default().to_owned();
        let has_project = shell.engine().project().is_some();
        let owner = if has_project && given["register"] != "user" {
            For::Project
        } else {
            For::User
        };
        NewLibraryDialog {
            name: text("name"),
            folder: text("path"),
            description: text("description"),
            owner,
            has_project,
            error: None,
            files: FileDialogs::default(),
        }
    }

    /// The `library.create` params, or `None` until a name (and, for the user, a folder)
    /// is given.
    pub fn params(&self) -> Option<Value> {
        let name = self.name.trim();
        let folder = self.folder.trim();
        if name.is_empty() {
            return None;
        }
        let path = match (self.owner, folder.is_empty()) {
            (For::User, true) => return None,
            (For::Project, true) => name.to_owned(),
            (_, false) => folder.to_owned(),
        };
        let register = match self.owner {
            For::Project => "none",
            For::User => "user",
        };
        let mut params = json!({"path": path, "name": name, "register": register});
        if !self.description.trim().is_empty() {
            params["description"] = json!(self.description.trim());
        }
        Some(params)
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

impl ParamsDialog for NewLibraryDialog {
    fn command(&self) -> &str {
        "library.create"
    }

    fn show(&mut self, ctx: &egui::Context, shell: &mut Shell) -> Outcome {
        if let Some((Pick::LibraryFolder, path)) = self.files.poll() {
            // A Project Library is stored relative to the Project folder.
            let path = match (self.owner, shell.engine().location()) {
                (For::Project, Some(base)) => crate::os::relative_path(base, &path),
                _ => path,
            };
            self.folder = path.display().to_string().replace('\\', "/");
        }
        let mut outcome = Outcome::Open;
        let modal =
            crate::modal::Dialog::new(Id::NULL.with("new-library"), tr!("new-library-title"))
                .width(480.0)
                .show(ctx, |ui| {
                    ui.add_space(6.0);
                    Grid::new("new-library-fields")
                        .num_columns(2)
                        .spacing([8.0, 6.0])
                        .show(ui, |ui| {
                            Self::text_row(
                                ui,
                                tr!("new-library-name"),
                                String::new(),
                                &mut self.name,
                            );
                            ui.label(tr!("new-library-for"));
                            ui.horizontal(|ui| {
                                ui.add_enabled_ui(self.has_project, |ui| {
                                    ui.radio_value(
                                        &mut self.owner,
                                        For::Project,
                                        tr!("new-library-for-project"),
                                    );
                                });
                                ui.radio_value(
                                    &mut self.owner,
                                    For::User,
                                    tr!("new-library-for-user"),
                                );
                            });
                            ui.end_row();
                            let label = tr!("new-library-folder");
                            ui.label(&label);
                            ui.horizontal(|ui| {
                                let hint = match self.owner {
                                    For::Project => tr!("new-library-folder-project-hint"),
                                    For::User => String::new(),
                                };
                                let response = ui.add(
                                    TextEdit::singleline(&mut self.folder)
                                        .hint_text(hint)
                                        .desired_width(200.0),
                                );
                                let name = label.clone();
                                response.widget_info(|| {
                                    WidgetInfo::labeled(Role::TextInput, true, name.clone())
                                });
                                let browse = ui.add_enabled(
                                    !self.files.busy(),
                                    egui::Button::new(tr!("import-browse")),
                                );
                                if browse.clicked() {
                                    self.files.start(ui.ctx(), Pick::LibraryFolder);
                                }
                            });
                            ui.end_row();
                            Self::text_row(
                                ui,
                                tr!("new-library-description"),
                                String::new(),
                                &mut self.description,
                            );
                        });
                    if let Some(error) = &self.error {
                        ui.colored_label(ui.visuals().error_fg_color, error);
                    }
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        let params = self.params();
                        let run = ui.add_enabled(
                            params.is_some(),
                            egui::Button::new(tr!("new-library-create")),
                        );
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

    fn then(&mut self, result: &Value) -> Option<(String, Value)> {
        let path = result["project_path"].as_str()?;
        (self.owner == For::Project)
            .then(|| ("project.library.add".to_owned(), json!({ "path": path })))
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}
