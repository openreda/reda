//! The Fab Preset pages of a user preset, on Preferences ▸ Fab Presets: the same pages as
//! a Project copy's. Like every Preference, a change applies at once: once no field is
//! being typed into, the changes are written with `fab_preset.set_user`. A user preset's
//! source, for Reset page to source, is the shipped Generic preset, REDA's own defaults.

use egui::{Button, Ui};
use reda_engine::fab_presets;
use reda_engine::model::FabPreset;

use super::PreferencesDialog;
use crate::settings::fab_preset::{self, PresetEditor, PresetPage};
use crate::settings::problem_label;
use crate::shell::Shell;
use crate::tr;

/// The user preset whose pages are open.
#[derive(Clone, Debug)]
pub struct UserPresetPages {
    /// The preset's name, as its file has it.
    pub name: String,
    /// The page shown.
    pub page: PresetPage,
    pub editor: PresetEditor,
    /// Why the last write was refused.
    pub error: Option<String>,
}

/// The source of a user preset's pages.
pub fn user_source() -> FabPreset {
    reda_engine::output::shipped_named(reda_engine::output::GENERIC_NAME)
        .expect("Generic is shipped")
}

impl PreferencesDialog {
    /// Opens the pages of the user preset `name` on Rules; false when there is none.
    pub fn edit_fab_preset(&mut self, shell: &Shell, name: &str) -> bool {
        let Some(preset) = read(shell, name) else {
            return false;
        };
        self.preset_pages = Some(UserPresetPages {
            name: preset.name.clone(),
            page: PresetPage::Rules,
            editor: PresetEditor::new(preset),
            error: None,
        });
        true
    }

    /// The open user preset's pages.
    pub fn preset_pages(&self) -> Option<&UserPresetPages> {
        self.preset_pages.as_ref()
    }

    pub fn preset_pages_mut(&mut self) -> Option<&mut UserPresetPages> {
        self.preset_pages.as_mut()
    }

    /// Reset page to source on the page shown.
    pub fn reset_preset_page(&mut self) {
        if let Some(open) = &mut self.preset_pages {
            open.editor.reset_page(open.page, &user_source());
        }
    }

    /// Writes the open preset's changes (`fab_preset.set_user`), unless a field holds
    /// text that is not a value.
    pub fn save_preset_pages(&mut self, shell: &mut Shell) {
        let Some(open) = &mut self.preset_pages else {
            return;
        };
        let invalid = PresetPage::ALL
            .iter()
            .any(|p| !open.editor.problems(*p).is_empty());
        let Some(values) = open.editor.all_changes() else {
            return;
        };
        if invalid {
            return;
        }
        let params = serde_json::json!({"name": open.name, "values": values});
        match shell.run("fab_preset.set_user", params) {
            Ok(_) => {
                open.editor.saved();
                open.error = None;
            }
            Err(e) => open.error = Some(e.message),
        }
    }

    /// Follows the preset's file; closes the pages when it is gone or renamed.
    fn reload_preset_pages(&mut self, shell: &Shell) {
        let Some(open) = &mut self.preset_pages else {
            return;
        };
        match read(shell, &open.name) {
            Some(preset) => open.editor.load(preset, false),
            None => self.preset_pages = None,
        }
    }

    /// Draws the open preset's pages: the page row, Reset page to source and the page.
    pub(super) fn draw_preset_pages(&mut self, ui: &mut Ui, shell: &mut Shell) {
        self.reload_preset_pages(shell);
        let Some(open) = &mut self.preset_pages else {
            return;
        };
        let mut close = false;
        let mut reset = false;
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.strong(tr!("pref-fab-preset-pages", name = open.name.as_str()));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    close = ui.button(tr!("settings-close")).clicked();
                    reset = ui
                        .add(Button::new(tr!("preset-reset-page")))
                        .on_hover_text(tr!("pref-fab-preset-source"))
                        .clicked();
                });
            });
            ui.horizontal_wrapped(|ui| {
                for page in PresetPage::ALL {
                    if ui
                        .selectable_label(open.page == page, page.title())
                        .clicked()
                    {
                        open.page = page;
                    }
                }
            });
            ui.separator();
            fab_preset::draw::page(ui, &mut open.editor, open.page);
            if let Some(error) = &open.error {
                let problem = crate::settings::Problem::new("", error.clone());
                problem_label(ui, &problem);
            }
        });
        if close {
            self.preset_pages = None;
            return;
        }
        if reset {
            self.reset_preset_page();
        }
        if !ui.ctx().text_edit_focused() {
            self.save_preset_pages(shell);
        }
    }
}

fn read(shell: &Shell, name: &str) -> Option<FabPreset> {
    let dirs = shell.engine().user_dirs()?;
    fab_presets::user_preset(dirs, name).ok().map(|(p, _)| p)
}
