//! The rule actions that act on the board at once rather than on Apply: Save rules as
//! Fab Preset…, Export rules… and Import rules… (after a preview). Each works on the
//! rules as applied, so the dialog offers them only while nothing is pending.

use std::path::{Path, PathBuf};

use reda_engine::command::CommandError;
use reda_engine::model::Uuid;
use serde_json::{Value, json};

use super::{BoardSetupDraft, kind_name};
use crate::settings::FileRequest;
use crate::shell::Shell;
use crate::tr;

/// What importing a rules file would change, shown before it does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportPreview {
    pub path: PathBuf,
    /// One line per record overwritten, class created and severity changed.
    pub rows: Vec<String>,
}

/// The lines of a `pcb.rules.import` preview.
fn preview_rows(result: &Value) -> Vec<String> {
    let mut rows = Vec::new();
    for o in result["overwrites"].as_array().into_iter().flatten() {
        let kind = serde_json::from_value(o["kind"].clone())
            .map(kind_name)
            .unwrap_or_default();
        let kind = capitalised(&kind);
        rows.push(match o["net_class"].as_str() {
            Some(class) => tr!(
                "board-setup-import-class-record",
                rule = kind,
                class = class
            ),
            None => tr!("board-setup-import-board-record", rule = kind),
        });
    }
    for class in result["new_classes"].as_array().into_iter().flatten() {
        let name = class.as_str().unwrap_or_default();
        rows.push(tr!("board-setup-import-new-class", class = name));
    }
    for s in result["severities"].as_array().into_iter().flatten() {
        rows.push(tr!(
            "board-setup-import-severity",
            check = s["check"].as_str().unwrap_or_default(),
            before = s["before"].as_str().unwrap_or_default(),
            after = s["after"].as_str().unwrap_or_default()
        ));
    }
    rows
}

fn capitalised(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

impl BoardSetupDraft {
    /// What the last action reported, such as where it exported the rules.
    pub fn notice(&self) -> Option<&str> {
        self.notice.as_deref()
    }

    /// The import waiting for its preview to be confirmed.
    pub fn import_preview(&self) -> Option<&ImportPreview> {
        self.import.as_ref()
    }

    pub fn cancel_import(&mut self) {
        self.import = None;
    }

    /// A file picked for `request`: exports the rules there, or previews importing them.
    pub fn picked_file(&mut self, shell: &mut Shell, request: FileRequest, path: &Path) {
        match request {
            FileRequest::ExportRules => {
                self.notice = Some(
                    match shell.run("pcb.rules.export", json!({ "path": path })) {
                        Ok(out) => tr!(
                            "board-setup-exported",
                            count = out.get("result").unwrap_or(&out)["rules"]
                                .as_u64()
                                .unwrap_or(0),
                            path = path.display().to_string()
                        ),
                        Err(e) => tr!("error-command", error = e.message),
                    },
                );
            }
            FileRequest::ImportRules => {
                let params = json!({ "path": path, "preview": true });
                match shell.run("pcb.rules.import", params) {
                    Ok(out) => {
                        self.notice = None;
                        self.import = Some(ImportPreview {
                            path: path.to_owned(),
                            rows: preview_rows(out.get("result").unwrap_or(&out)),
                        });
                    }
                    Err(e) => self.notice = Some(tr!("error-command", error = e.message)),
                }
            }
        }
    }

    /// Imports the previewed file into the board, as one undo step.
    pub fn confirm_import(&mut self, shell: &mut Shell) {
        let Some(import) = self.import.take() else {
            return;
        };
        self.notice = Some(
            match shell.run("pcb.rules.import", json!({ "path": import.path })) {
                Ok(_) => tr!(
                    "board-setup-imported",
                    path = import.path.display().to_string()
                ),
                Err(e) => tr!("error-command", error = e.message),
            },
        );
    }

    /// Adds a Fab Preset copy to the Project holding the board's rules, called `name`.
    pub fn save_as_preset(&mut self, shell: &mut Shell, name: &str) -> Result<Uuid, CommandError> {
        let out = shell.run("pcb.rules.save_as_preset", json!({ "name": name }));
        self.notice = Some(match &out {
            Ok(_) => tr!("board-setup-saved-preset", preset = name.trim()),
            Err(e) => tr!("error-command", error = e.message.clone()),
        });
        let out = out?;
        let uuid = out["result"]["uuid"].as_str().unwrap_or_default();
        uuid.parse()
            .map_err(|_| CommandError::internal("no preset UUID in the result"))
    }
}
