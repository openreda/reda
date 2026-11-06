//! The Fab Presets page: the Project's copies, each with "Update from Source" (after a
//! preview, committed on Apply with `fab_preset.update_from_source`) and "Save as User
//! Preset" (`fab_preset.save_user`, which writes a per-user file at once and leaves the
//! Project alone, so it is not part of Apply).

use std::collections::{BTreeMap, BTreeSet};

use egui::{Button, Ui};
use reda_engine::model::{Project, Uuid};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::settings::{Page, PageCx, Problem, Step, field};
use crate::shell::Shell;
use crate::tr;

/// One Project copy of a Fab Preset.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PresetCopy {
    pub uuid: Uuid,
    pub name: String,
}

/// One field where a copy differs from its source.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct FieldChange {
    /// Such as `rules.clearance` or `bom`.
    pub field: String,
    pub project: Value,
    pub source: Value,
}

/// What "Update from Source" would change in one copy, or why it cannot tell.
#[derive(Clone, Debug, PartialEq)]
pub struct Preview {
    pub preset: Uuid,
    pub changes: Result<Vec<FieldChange>, String>,
}

/// The outcome of the last "Save as User Preset".
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Status {
    pub ok: bool,
    pub message: String,
}

#[derive(Clone, Debug, Default)]
pub struct FabPresetsPage {
    copies: Vec<PresetCopy>,
    /// Copies to update from their source on Apply.
    updating: BTreeSet<Uuid>,
    preview: Option<Preview>,
    /// The name field of an open "Save as User Preset", per copy.
    saving: BTreeMap<Uuid, String>,
    status: Option<Status>,
}

impl FabPresetsPage {
    pub fn copies(&self) -> &[PresetCopy] {
        &self.copies
    }

    pub fn preview(&self) -> Option<&Preview> {
        self.preview.as_ref()
    }

    /// Whether `preset` updates from its source on Apply.
    pub fn updating(&self, preset: Uuid) -> bool {
        self.updating.contains(&preset)
    }

    pub fn status(&self) -> Option<&Status> {
        self.status.as_ref()
    }

    /// Previews what updating `preset` from its source would change.
    pub fn preview_update(&mut self, shell: &mut Shell, preset: Uuid) {
        let changes = shell
            .run("fab_preset.source_diff", json!({ "preset": preset }))
            .map_err(|e| e.message)
            .and_then(|out| {
                serde_json::from_value::<Vec<FieldChange>>(out["changes"].clone())
                    .map_err(|e| e.to_string())
            });
        self.preview = Some(Preview { preset, changes });
    }

    /// Marks the previewed copy for updating on Apply, if the preview found changes.
    pub fn accept_preview(&mut self) {
        if let Some(preview) = self.preview.take()
            && preview.changes.is_ok_and(|c| !c.is_empty())
        {
            self.updating.insert(preview.preset);
        }
    }

    pub fn cancel_preview(&mut self) {
        self.preview = None;
    }

    /// Drops a pending update of `preset`.
    pub fn keep(&mut self, preset: Uuid) {
        self.updating.remove(&preset);
    }

    /// Saves the copy `preset` (as the Project has it) as the user preset `name`, at once.
    pub fn save_as_user(&mut self, shell: &mut Shell, preset: Uuid, name: &str) {
        let name = name.trim();
        let params = json!({ "preset": preset, "name": name });
        self.status = Some(match shell.run("fab_preset.save_user", params) {
            Ok(_) => {
                self.saving.remove(&preset);
                Status {
                    ok: true,
                    message: tr!("settings-preset-saved", name = name),
                }
            }
            Err(e) => Status {
                ok: false,
                message: tr!("settings-preset-not-saved", name = name, error = e.message),
            },
        });
    }

    fn copy(&mut self, ui: &mut Ui, cx: &mut PageCx<'_>, copy: &PresetCopy) {
        ui.horizontal(|ui| {
            ui.strong(&copy.name);
            if self.updating(copy.uuid) {
                ui.weak(tr!("settings-preset-will-update"));
                if ui.button(tr!("settings-preset-keep")).clicked() {
                    self.keep(copy.uuid);
                }
            } else if ui.button(tr!("settings-preset-update")).clicked() {
                self.preview_update(cx.shell, copy.uuid);
            }
            if ui.button(tr!("settings-preset-save-user")).clicked() {
                self.saving.insert(copy.uuid, copy.name.clone());
            }
        });
        if let Some(name) = self.saving.get_mut(&copy.uuid) {
            let mut save = false;
            let mut cancel = false;
            ui.horizontal(|ui| {
                let label = tr!("settings-preset-user-name");
                ui.label(&label);
                field(ui, &label, name, 200.0, None);
                let ready = !name.trim().is_empty();
                save = ui
                    .add_enabled(ready, Button::new(tr!("settings-preset-save")))
                    .clicked();
                cancel = ui.button(tr!("dialog-cancel")).clicked();
            });
            if save {
                let name = name.clone();
                self.save_as_user(cx.shell, copy.uuid, &name);
            } else if cancel {
                self.saving.remove(&copy.uuid);
            }
        }
        if self.preview.as_ref().is_some_and(|p| p.preset == copy.uuid) {
            self.preview_box(ui);
        }
    }

    fn preview_box(&mut self, ui: &mut Ui) {
        let Some(preview) = &self.preview else {
            return;
        };
        let mut accept = false;
        let mut cancel = false;
        egui::Frame::group(ui.style()).show(ui, |ui| {
            match &preview.changes {
                Err(error) => {
                    ui.colored_label(
                        ui.visuals().error_fg_color,
                        tr!("settings-preset-no-source", error = error.as_str()),
                    );
                }
                Ok(changes) if changes.is_empty() => {
                    ui.label(tr!("settings-preset-up-to-date"));
                }
                Ok(changes) => {
                    ui.label(tr!("settings-preset-preview"));
                    egui::Grid::new("settings-preset-diff")
                        .num_columns(3)
                        .striped(true)
                        .spacing([12.0, 4.0])
                        .show(ui, |ui| {
                            ui.strong(tr!("settings-preset-field"));
                            ui.strong(tr!("settings-preset-project"));
                            ui.strong(tr!("settings-preset-source"));
                            ui.end_row();
                            for c in changes {
                                ui.monospace(&c.field);
                                ui.monospace(short(&c.project));
                                ui.monospace(short(&c.source));
                                ui.end_row();
                            }
                        });
                }
            }
            ui.horizontal(|ui| {
                let changes = preview.changes.as_ref().is_ok_and(|c| !c.is_empty());
                accept = ui
                    .add_enabled(changes, Button::new(tr!("settings-preset-confirm")))
                    .clicked();
                cancel = ui.button(tr!("dialog-cancel")).clicked();
            });
        });
        if accept {
            self.accept_preview();
        } else if cancel {
            self.cancel_preview();
        }
    }
}

/// A value as compact JSON, cut short when long.
fn short(value: &Value) -> String {
    let text = value.to_string();
    match text.char_indices().nth(48) {
        Some((at, _)) => format!("{}…", &text[..at]),
        None => text,
    }
}

impl Page for FabPresetsPage {
    fn title(&self) -> String {
        tr!("settings-page-fab-presets")
    }

    fn load(&mut self, project: &Project, reset: bool) {
        self.copies = project
            .fab_presets()
            .map(|p| PresetCopy {
                uuid: p.uuid,
                name: p.name.clone(),
            })
            .collect();
        if reset {
            self.updating.clear();
        }
        let present: BTreeSet<Uuid> = self.copies.iter().map(|c| c.uuid).collect();
        self.updating.retain(|u| present.contains(u));
        self.saving.retain(|u, _| present.contains(u));
        if self
            .preview
            .as_ref()
            .is_some_and(|p| !present.contains(&p.preset))
        {
            self.preview = None;
        }
    }

    fn steps(&self) -> Vec<Step> {
        self.copies
            .iter()
            .filter(|c| self.updating.contains(&c.uuid))
            .map(|c| Step::new("fab_preset.update_from_source", json!({ "preset": c.uuid })))
            .collect()
    }

    fn problems(&self) -> Vec<Problem> {
        Vec::new()
    }

    fn has_defaults(&self) -> bool {
        true
    }

    /// A copy's defaults are its source's values: every copy that differs from its source
    /// updates from it on Apply.
    fn reset_to_defaults(&mut self, shell: &mut Shell) {
        for copy in &self.copies {
            let diff = shell.run("fab_preset.source_diff", json!({ "preset": copy.uuid }));
            let differs =
                diff.is_ok_and(|out| out["changes"].as_array().is_some_and(|c| !c.is_empty()));
            if differs {
                self.updating.insert(copy.uuid);
            }
        }
        self.preview = None;
    }

    fn show(&mut self, ui: &mut Ui, cx: &mut PageCx<'_>) {
        if self.copies.is_empty() {
            ui.weak(tr!("settings-preset-none"));
        }
        for copy in self.copies.clone() {
            self.copy(ui, cx, &copy);
            ui.add_space(6.0);
        }
        if let Some(status) = &self.status {
            if status.ok {
                ui.label(&status.message);
            } else {
                ui.colored_label(ui.visuals().error_fg_color, &status.message);
            }
        }
        ui.add_space(6.0);
        ui.weak(tr!("settings-preset-note"));
    }
}
