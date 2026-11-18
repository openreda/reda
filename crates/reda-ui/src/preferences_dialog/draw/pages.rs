//! The Preferences pages with content of their own beside their settings: Libraries (the
//! user Library folders and the Library check's severities), Agents (the bridge, who is
//! connected and who is allowed in) and Fab Presets (the user's presets).

use egui::{Role, Ui, WidgetInfo};
use reda_engine::fab_presets;
use serde_json::json;

use super::{Cx, Outcome, label_width};
use crate::preferences_dialog::{
    LIBRARY_PATHS, LIBRARY_SEVERITIES, LIBRARY_STARTER_ACTIVE, PageId, PreferencesDialog, Setting,
};
use crate::settings::layout;
use crate::settings::{Problem, field};
use crate::theme::TokensExt as _;
use crate::tr;
use crate::widgets::{self, Tooltip as _};

/// The width of the field a new Library folder is typed into.
const PATH_WIDTH: f32 = 300.0;
/// The width of a Fab Preset's name field.
const NAME_WIDTH: f32 = 220.0;

impl PreferencesDialog {
    /// The user Library folders: each with Remove, then a field and Add, and Add Folder….
    pub(super) fn libraries(&mut self, ui: &mut Ui, cx: &mut Cx<'_>) {
        let t = ui.tokens();
        layout::text(ui, tr!("pref-libraries-intro"));
        let paths = cx.shell.preferences().libraries.paths.clone();
        if paths.is_empty() {
            layout::muted(ui, tr!("pref-libraries-none"));
        }
        let libraries = cx.shell.preferences().libraries.clone();
        let mut remove = None;
        let mut toggle = None;
        for (i, path) in paths.iter().enumerate() {
            ui.horizontal(|ui| {
                let active = !libraries.inactive.contains(path);
                if active_checkbox(ui, active, path) {
                    toggle = Some(path.clone());
                }
                ui.label(t.ty.mono.rich(path.as_str()).color(t.color.text.primary));
                let button = ui.add(widgets::button(tr!("pref-remove")).flat());
                let name = tr!("pref-remove-named", item = path.as_str());
                button.widget_info(|| WidgetInfo::labeled(Role::Button, true, name.clone()));
                if button.clicked() {
                    remove = Some(i);
                }
            });
        }
        if let Some(i) = remove {
            let _ = self.remove_library_path(cx.shell, i);
        }
        if let Some(path) = toggle {
            let _ = self.toggle_library_active(cx.shell, &path);
        }
        ui.horizontal(|ui| {
            let starter = tr!("pref-libraries-starter");
            if active_checkbox(ui, libraries.starter_active, &starter) {
                let value = json!(!libraries.starter_active);
                let _ = self.set(cx.shell, LIBRARY_STARTER_ACTIVE, value);
            }
            layout::text(ui, starter);
        });
        ui.horizontal(|ui| {
            let mut text = std::mem::take(&mut self.new_library_path);
            let problem = self
                .problem(LIBRARY_PATHS)
                .map(|p| Problem::new(LIBRARY_PATHS, p));
            field(
                ui,
                &tr!("pref-library-path"),
                &mut text,
                PATH_WIDTH,
                problem.as_ref(),
            );
            self.new_library_path = text;
            if ui.add(widgets::button(tr!("pref-add"))).clicked()
                && self
                    .add_library_path(cx.shell, &self.new_library_path.clone())
                    .is_ok()
            {
                self.new_library_path.clear();
            }
            if ui.add(widgets::button(tr!("pref-add-folder"))).clicked() {
                cx.outcome = Outcome::PickFolder;
            }
        });
        self.library_severities(ui, cx);
    }

    /// The Library check's severities: a row per check with its severity and the reset
    /// dot, in the order of the check's table, with Reset severities.
    fn library_severities(&mut self, ui: &mut Ui, cx: &mut Cx<'_>) {
        layout::group(ui, &tr!("pref-libraries-severities"));
        layout::text(ui, tr!("pref-libraries-severities-intro"));
        let settings: Vec<Setting> = PageId::LIBRARIES
            .settings()
            .into_iter()
            .filter(|s| s.key.starts_with(LIBRARY_SEVERITIES))
            .collect();
        let width = label_width(ui, settings.iter().map(|s| s.label.as_str()));
        for setting in &settings {
            self.setting_row(ui, cx, setting, width);
        }
        let changed = settings.iter().any(|s| self.can_reset(cx.shell, &s.key));
        let reset = widgets::button(tr!("pref-libraries-severities-reset"));
        let reset = ui.add_enabled(changed, reset);
        if reset.clicked() {
            let keys: Vec<String> = settings.iter().map(|s| s.key.clone()).collect();
            self.reset_with(cx, |d, shell| d.reset_keys(shell, &keys));
        }
    }

    /// The bridge switch, the agents connected now and the agents allowed in, each with
    /// Forget.
    pub(super) fn agents(&mut self, ui: &mut Ui, cx: &mut Cx<'_>) {
        self.setting_rows(ui, cx, &PageId::AGENTS.settings());
        if let Some(error) = cx.bridge.error() {
            layout::error(ui, tr!("bridge-error", error = error));
        }
        layout::group(ui, &tr!("bridge-connected"));
        let agents = cx.bridge.agents();
        if agents.is_empty() {
            layout::muted(ui, tr!("pref-agents-none-connected"));
        }
        for agent in &agents {
            layout::text(ui, format!("{} ({})", agent.name, agent.source.label()));
        }
        layout::group(ui, &tr!("bridge-allowed"));
        let allowed = cx.bridge.allowed(cx.shell);
        if allowed.is_empty() {
            layout::muted(ui, tr!("bridge-none-allowed"));
        }
        for name in allowed {
            ui.horizontal(|ui| {
                layout::text(ui, name.as_str());
                let button = ui.add(widgets::button(tr!("bridge-forget")).flat());
                let label = tr!("pref-forget-named", agent = name.as_str());
                button.widget_info(|| WidgetInfo::labeled(Role::Button, true, label.clone()));
                if button.clicked() {
                    cx.bridge.forget_agent(cx.shell, &name);
                }
            });
        }
    }

    /// The user's Fab Presets, each renamed in its name field (Enter), deleted, or opened
    /// on its Fab Preset pages, with the folder they live in.
    pub(super) fn fab_presets(&mut self, ui: &mut Ui, cx: &mut Cx<'_>) {
        layout::text(ui, tr!("pref-fab-presets-intro"));
        let Some(dirs) = cx.shell.engine().user_dirs().cloned() else {
            layout::muted(ui, tr!("shortcuts-no-folder"));
            return;
        };
        let (presets, warnings) = fab_presets::user_presets(&dirs);
        if presets.is_empty() {
            layout::muted(ui, tr!("pref-fab-presets-none"));
        }
        for preset in presets {
            let name = preset.preset.name;
            let key = format!("fab_presets/{name}");
            ui.horizontal(|ui| {
                let mut text = self.texts.get(&key).cloned().unwrap_or(name.clone());
                let label = tr!("pref-fab-preset-name", name = name.as_str());
                let problem = self.problem(&key).map(|p| Problem::new(key.as_str(), p));
                let response = field(ui, &label, &mut text, NAME_WIDTH, problem.as_ref());
                if response.changed() {
                    self.texts.insert(key.clone(), text.clone());
                }
                if response.lost_focus() && text.trim() != name {
                    self.rename_fab_preset(cx.shell, &name, &text);
                }
                let button = ui.add(widgets::button(tr!("pref-delete")).flat());
                let delete = tr!("pref-delete-named", item = name.as_str());
                button.widget_info(|| WidgetInfo::labeled(Role::Button, true, delete.clone()));
                if button.clicked() {
                    self.delete_fab_preset(cx.shell, &name);
                }
                let open = self.preset_pages().is_some_and(|p| p.name == name);
                let edit = widgets::button(tr!("pref-fab-preset-edit")).flat();
                let edit = ui.add_enabled(!open, edit);
                let label = tr!("pref-fab-preset-edit-named", name = name.as_str());
                edit.widget_info(|| WidgetInfo::labeled(Role::Button, true, label.clone()));
                if edit.clicked() {
                    self.edit_fab_preset(cx.shell, &name);
                }
            });
        }
        if self.preset_pages().is_some() {
            self.draw_preset_pages(ui, cx.shell);
        }
        for warning in warnings {
            layout::error(ui, warning);
        }
        if ui
            .add(widgets::link(tr!("shortcuts-open-folder")))
            .clicked()
        {
            cx.outcome = Outcome::ShowFolder(fab_presets::dir(&dirs));
        }
    }
}

/// A Library's Active checkbox, named after the Library; true when it was clicked.
fn active_checkbox(ui: &mut Ui, active: bool, library: &str) -> bool {
    let mut checked = active;
    let response = ui.add(widgets::checkbox(&mut checked, ""));
    let name = tr!("pref-library-active-named", library = library);
    response.widget_info(|| WidgetInfo::selected(Role::CheckBox, true, active, name.clone()));
    let tip = tr!("pref-library-active-tip");
    response.tooltip(tip).changed()
}
