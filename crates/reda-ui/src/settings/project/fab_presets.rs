//! The Fab Presets page: the Project's copies, each with "Update from Source" (after a
//! preview, committed on Apply with `fab_preset.update_from_source`) and "Save as User
//! Preset" (`fab_preset.save_user`, which writes a per-user file at once and leaves the
//! Project alone, so it is not part of Apply).

use std::collections::{BTreeMap, BTreeSet};

use egui::Ui;
use reda_engine::model::{Project, Uuid};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::settings::{Page, PageCx, Problem, Step, field, layout, mono, strong};
use crate::shell::Shell;
use crate::theme::TokensExt as _;
use crate::tr;
use crate::widgets::{self, Column};

/// The name field of Save as User Preset.
const NAME_WIDTH: f32 = 200.0;

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
            strong(ui, &copy.name);
            if self.updating(copy.uuid) {
                layout::muted(ui, tr!("settings-preset-will-update"));
                if ui
                    .add(widgets::button(tr!("settings-preset-keep")))
                    .clicked()
                {
                    self.keep(copy.uuid);
                }
            } else if ui
                .add(widgets::button(tr!("settings-preset-update")))
                .clicked()
            {
                self.preview_update(cx.shell, copy.uuid);
            }
            if ui
                .add(widgets::button(tr!("settings-preset-save-user")))
                .clicked()
            {
                self.saving.insert(copy.uuid, copy.name.clone());
            }
        });
        if let Some(name) = self.saving.get_mut(&copy.uuid) {
            let mut save = false;
            let mut cancel = false;
            ui.horizontal(|ui| {
                let label = tr!("settings-preset-user-name");
                layout::text(ui, &label);
                field(ui, &label, name, NAME_WIDTH, None);
                let ready = !name.trim().is_empty();
                let save_button = widgets::button(tr!("settings-preset-save")).primary();
                save = ui.add_enabled(ready, save_button).clicked();
                cancel = ui.add(widgets::button(tr!("dialog-cancel"))).clicked();
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
        let t = ui.tokens();
        widgets::card_frame(&t).show(ui, |ui| {
            match &preview.changes {
                Err(error) => {
                    let text = tr!("settings-preset-no-source", error = error.as_str());
                    layout::error(ui, text);
                }
                Ok(changes) if changes.is_empty() => {
                    layout::text(ui, tr!("settings-preset-up-to-date"));
                }
                Ok(changes) => {
                    layout::text(ui, tr!("settings-preset-preview"));
                    widgets::table("settings-preset-diff")
                        .columns([Column::auto(); 3])
                        .show(ui, |table| {
                            table.header(|h| {
                                h.text(tr!("settings-preset-field"));
                                h.text(tr!("settings-preset-project"));
                                h.text(tr!("settings-preset-source"));
                            });
                            for c in changes {
                                table.row(|row| {
                                    row.ui(|ui| mono(ui, &c.field));
                                    row.ui(|ui| mono(ui, short(&c.project)));
                                    row.ui(|ui| mono(ui, short(&c.source)));
                                });
                            }
                        });
                }
            }
            widgets::button_row(ui, |ui| {
                let changes = preview.changes.as_ref().is_ok_and(|c| !c.is_empty());
                let confirm = widgets::button(tr!("settings-preset-confirm")).primary();
                accept = ui.add_enabled(changes, confirm).clicked();
                cancel = ui.add(widgets::button(tr!("dialog-cancel"))).clicked();
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
        let t = ui.tokens();
        if self.copies.is_empty() {
            layout::muted(ui, tr!("settings-preset-none"));
        }
        for copy in self.copies.clone() {
            self.copy(ui, cx, &copy);
            ui.add_space(t.space.stack);
        }
        if let Some(status) = &self.status {
            if status.ok {
                layout::text(ui, &status.message);
            } else {
                layout::error(ui, &status.message);
            }
        }
        ui.add_space(t.space.section);
        layout::muted(ui, tr!("settings-preset-note"));
    }
}
