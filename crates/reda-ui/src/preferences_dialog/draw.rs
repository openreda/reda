//! Drawing the Preferences window into its tab: the search box with Modified only and
//! Reset all, the page tree, the shown page (or the search results) and Close. Every change runs at once; there is no OK or
//! Cancel.

use std::path::PathBuf;

use egui::{Button, ComboBox, Key, RichText, Role, TextEdit, Ui, WidgetInfo};
use reda_engine::command::CommandError;
use reda_engine::fab_presets;
use serde_json::json;

use super::shortcuts::Row;
use super::{
    Choice, Kind, LIBRARY_PATHS, LIBRARY_SEVERITIES, LIBRARY_STARTER_ACTIVE, PageId,
    PreferencesDialog, Setting, schema,
};
use crate::bars::DocumentType;
use crate::bridge::Bridge;
use crate::keymap::{Chord, Context};
use crate::settings::{Problem, field};
use crate::shell::Shell;
use crate::tr;

/// The width of the page list.
const LIST_WIDTH: f32 = 160.0;
/// The width of the label column.
const LABEL_WIDTH: f32 = 230.0;
/// How far each level of the page tree is indented.
const INDENT: f32 = 14.0;
/// The width of the reset dot's column.
const DOT_WIDTH: f32 = 14.0;

/// The bridge setting, which turns the control port on and off as well as keeping the
/// choice.
const BRIDGE: &str = "agents.bridge";

/// What a frame of the dialog asks of the window.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// It stays open.
    Open,
    /// It closed (Close).
    Closed,
    /// The Libraries page wants a folder picked; the answer goes to
    /// [`PreferencesDialog::picked_folder`].
    PickFolder,
    /// Show this folder in the file manager ("Open folder").
    ShowFolder(PathBuf),
}

/// What the page drawing may use.
struct Cx<'a> {
    shell: &'a mut Shell,
    bridge: &'a mut Bridge,
    outcome: Outcome,
}

impl PreferencesDialog {
    /// Draws the window's content into `ui` (its tab) for one frame.
    pub fn show(&mut self, ui: &mut Ui, shell: &mut Shell, bridge: &mut Bridge) -> Outcome {
        let mut cx = Cx {
            shell,
            bridge,
            outcome: Outcome::Open,
        };
        ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
            ui.horizontal(|ui| {
                ui.weak(tr!("pref-live"));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(tr!("pref-close")).clicked() {
                        cx.outcome = Outcome::Closed;
                    }
                });
            });
            ui.separator();
            ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                self.top_bar(ui, &mut cx);
                ui.separator();
                ui.horizontal_top(|ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("preferences-tree")
                        .max_width(LIST_WIDTH)
                        .auto_shrink([false, false])
                        .show(ui, |ui| self.page_list(ui, cx.shell));
                    ui.separator();
                    ui.vertical(|ui| {
                        egui::ScrollArea::both()
                            .id_salt("preferences-page")
                            .auto_shrink(false)
                            .show(ui, |ui| {
                                // All settings has the same search over its own table.
                                if self.searching() {
                                    let found = self.found(cx.shell);
                                    self.results(ui, &mut cx, found);
                                } else {
                                    self.page_header(ui, &mut cx);
                                    ui.add_space(6.0);
                                    self.page_content(ui, &mut cx);
                                }
                            });
                    });
                });
            });
        });
        cx.outcome
    }

    /// Whether the search replaces the page with its results.
    fn searching(&self) -> bool {
        !self.query.trim().is_empty() && self.page != PageId::ALL_SETTINGS
    }

    /// The search box, Modified only and Reset all.
    fn top_bar(&mut self, ui: &mut Ui, cx: &mut Cx<'_>) {
        ui.horizontal(|ui| {
            let search = ui.add(
                TextEdit::singleline(&mut self.query)
                    .hint_text(tr!("pref-search-hint"))
                    .desired_width(240.0),
            );
            search.widget_info(|| WidgetInfo::labeled(Role::SearchInput, true, tr!("pref-search")));
            ui.checkbox(&mut self.modified_only, tr!("pref-modified-only"));
            let any = self.modified_count(cx.shell, PageId::ALL_SETTINGS) > 0;
            if ui
                .add_enabled(any, Button::new(tr!("pref-reset-all")))
                .clicked()
            {
                self.reset_with(cx, |d, shell| d.reset_all(shell));
            }
        });
    }

    /// The page tree: each page indented under its branch, with the count of its changed
    /// settings. While searching, pages without a match are dimmed.
    fn page_list(&mut self, ui: &mut Ui, shell: &Shell) {
        let searching = self.searching();
        let found = if searching {
            self.found(shell)
        } else {
            Vec::new()
        };
        ui.vertical(|ui| {
            ui.set_width(LIST_WIDTH);
            for page in self.pages(shell) {
                let title = page.title();
                let mut text = RichText::new(title.clone());
                if searching && !found.iter().any(|(p, _)| *p == page) {
                    text = text.weak();
                }
                let count = self.modified_count(shell, page);
                let row = ui
                    .horizontal(|ui| {
                        ui.add_space(INDENT * page.depth() as f32);
                        let height = ui.spacing().interact_size.y;
                        let count = if count > 0 {
                            count.to_string()
                        } else {
                            String::new()
                        };
                        ui.add(
                            Button::selectable(page == self.page && !searching, text)
                                .right_text(RichText::new(count).weak())
                                .min_size(egui::vec2(ui.available_width(), height)),
                        )
                    })
                    .inner;
                let label = match count {
                    0 => title.clone(),
                    n => tr!("pref-page-modified", page = title.as_str(), count = n),
                };
                row.widget_info(|| WidgetInfo::labeled(Role::TreeItem, true, label.clone()));
                if row.clicked() {
                    self.page = page;
                    if page != PageId::ALL_SETTINGS {
                        self.query.clear();
                    }
                }
            }
        });
    }

    /// The page's title, with Reset page and, on a branch, Reset branch.
    fn page_header(&mut self, ui: &mut Ui, cx: &mut Cx<'_>) {
        let page = self.page;
        ui.horizontal(|ui| {
            ui.strong(page.title());
            if page == PageId::ALL_SETTINGS {
                return;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if page.has_children() {
                    let changed = self
                        .pages_in(page)
                        .any(|p| self.modified_count(cx.shell, p) > 0);
                    if ui
                        .add_enabled(changed, Button::new(tr!("pref-reset-branch")).small())
                        .clicked()
                    {
                        self.reset_with(cx, |d, shell| d.reset_page(shell, page, true));
                    }
                }
                let changed = self.modified_count(cx.shell, page) > 0;
                if ui
                    .add_enabled(changed, Button::new(tr!("pref-reset-page")).small())
                    .clicked()
                {
                    self.reset_with(cx, |d, shell| d.reset_page(shell, page, false));
                }
            });
        });
    }

    fn pages_in(&self, branch: PageId) -> impl Iterator<Item = PageId> {
        PageId::tree().into_iter().filter(move |p| p.is_in(branch))
    }

    /// Runs a reset, then turns the bridge back on when its switch was reset.
    fn reset_with(
        &mut self,
        cx: &mut Cx<'_>,
        reset: impl FnOnce(&mut Self, &mut Shell) -> Result<Vec<String>, CommandError>,
    ) {
        if let Ok(keys) = reset(self, cx.shell)
            && keys.iter().any(|k| k == BRIDGE)
        {
            cx.bridge.set_enabled(cx.shell, true);
        }
    }

    /// The settings the search found, under their pages' titles.
    fn results(&mut self, ui: &mut Ui, cx: &mut Cx<'_>, found: Vec<(PageId, Setting)>) {
        if found.is_empty() {
            ui.weak(tr!("pref-search-none"));
            return;
        }
        let mut page = None;
        for (on, setting) in found {
            if page != Some(on) {
                page = Some(on);
                ui.add_space(4.0);
                ui.strong(on.title());
            }
            self.setting_row(ui, cx, &setting);
        }
    }

    fn page_content(&mut self, ui: &mut Ui, cx: &mut Cx<'_>) {
        match self.page {
            PageId::LIBRARIES => self.libraries(ui, cx),
            PageId::FAB_PRESETS => self.fab_presets(ui, cx),
            PageId::AGENTS => self.agents(ui, cx),
            PageId::KEYBOARD_SHORTCUTS => self.keyboard_shortcuts(ui, cx),
            PageId::ALL_SETTINGS => self.all_settings_table(ui, cx),
            page => {
                let mut rows = self.rows(cx.shell, page);
                if page == PageId::CHECKS {
                    rows.retain(|s| !s.key.starts_with(super::check_grid::CHECK_DISPLAY));
                }
                if rows.is_empty() {
                    ui.weak(tr!("pref-none-modified"));
                }
                let mut heading = None;
                for setting in rows {
                    if setting.heading != heading {
                        heading = setting.heading.clone();
                        if let Some(h) = &heading {
                            ui.add_space(6.0);
                            ui.strong(h);
                        }
                    }
                    self.setting_row(ui, cx, &setting);
                }
                if page == PageId::CHECKS && !self.searching() {
                    super::check_grid::show(self, ui, cx.shell);
                }
            }
        }
    }

    /// All settings: key, value (editable), default, the reset dot and description, for
    /// every key the search and Modified only leave.
    fn all_settings_table(&mut self, ui: &mut Ui, cx: &mut Cx<'_>) {
        let rows = self.all_settings(cx.shell);
        if rows.is_empty() {
            ui.weak(tr!("pref-search-none"));
            return;
        }
        egui::Grid::new("preferences-all-settings")
            .striped(true)
            .num_columns(5)
            .show(ui, |ui| {
                ui.strong(tr!("pref-all-key"));
                ui.strong(tr!("pref-all-value"));
                ui.strong(tr!("pref-all-default"));
                ui.label("");
                ui.strong(tr!("pref-all-description"));
                ui.end_row();
                for row in rows {
                    ui.monospace(row.setting.key.as_str());
                    ui.horizontal(|ui| self.editor(ui, cx, &row.setting));
                    ui.weak(row.default.as_str());
                    self.reset_dot(ui, cx, &row.setting);
                    ui.add(egui::Label::new(row.setting.description.as_str()).wrap());
                    ui.end_row();
                }
            });
    }

    /// The dot beside a changed setting, which puts it back to its default; an unchanged
    /// setting keeps the space empty.
    fn reset_dot(&mut self, ui: &mut Ui, cx: &mut Cx<'_>, setting: &Setting) {
        let key = setting.key.as_str();
        let size = egui::vec2(DOT_WIDTH, ui.spacing().interact_size.y);
        if !self.can_reset(cx.shell, key) {
            ui.allocate_space(size);
            return;
        }
        let (rect, dot) = ui.allocate_exact_size(size, egui::Sense::click());
        let radius = if dot.hovered() { 4.5 } else { 3.5 };
        let color = ui.visuals().selection.bg_fill;
        ui.painter().circle_filled(rect.center(), radius, color);
        let name = tr!("pref-reset-named", setting = setting.label.as_str());
        dot.widget_info(|| WidgetInfo::labeled(Role::Button, true, name.clone()));
        let dot = dot.on_hover_text(tr!("pref-reset-dot"));
        if dot.clicked() {
            let keys = [setting.key.clone()];
            self.reset_with(cx, |d, shell| d.reset_keys(shell, &keys));
        }
    }

    /// One setting: the reset dot when it is changed, its label (described on hover) and
    /// its editor.
    fn setting_row(&mut self, ui: &mut Ui, cx: &mut Cx<'_>, setting: &Setting) {
        let key = setting.key.as_str();
        ui.horizontal(|ui| {
            self.reset_dot(ui, cx, setting);
            let label = fixed_label(ui, LABEL_WIDTH, RichText::new(setting.label.as_str()));
            if !setting.description.is_empty() {
                label.on_hover_text(setting.description.as_str());
            }
            self.editor(ui, cx, setting);
        });
        if let Some(problem) = self.problem(key).filter(|_| setting.kind == Kind::List) {
            crate::settings::problem_label(ui, &Problem::new(key, problem));
        }
    }

    fn editor(&mut self, ui: &mut Ui, cx: &mut Cx<'_>, setting: &Setting) {
        let key = setting.key.as_str();
        let value = self.value(cx.shell, key);
        match &setting.kind {
            Kind::Toggle => {
                let mut on = value.as_bool().unwrap_or_default();
                if key == BRIDGE {
                    on = cx.bridge.is_on();
                }
                let response = ui.checkbox(&mut on, "");
                let label = setting.label.clone();
                response
                    .widget_info(|| WidgetInfo::selected(Role::CheckBox, true, on, label.clone()));
                if response.changed() {
                    if key == BRIDGE {
                        cx.bridge.set_enabled(cx.shell, on);
                    } else {
                        let _ = self.set(cx.shell, key, json!(on));
                    }
                }
            }
            Kind::Choice(choices) => {
                let current = &value;
                let same = |c: &Choice| match (c.value.as_f64(), current.as_f64()) {
                    (Some(a), Some(b)) => (a - b).abs() < 1e-9,
                    _ => c.value == *current,
                };
                let shown = choices.iter().find(|c| same(c)).map_or_else(
                    || current.as_str().map_or(current.to_string(), str::to_owned),
                    |c| c.label.clone(),
                );
                let mut picked = None;
                let response = ComboBox::from_id_salt(("pref", key))
                    .selected_text(shown)
                    .width(180.0)
                    .show_ui(ui, |ui| {
                        for choice in choices {
                            if ui
                                .selectable_label(same(choice), choice.label.as_str())
                                .clicked()
                            {
                                picked = Some(choice.value.clone());
                            }
                        }
                    })
                    .response;
                let label = setting.label.clone();
                response.widget_info(|| WidgetInfo::labeled(Role::ComboBox, true, label.clone()));
                if let Some(picked) = picked.filter(|p| p != current) {
                    let _ = self.set(cx.shell, key, picked);
                }
            }
            Kind::Number | Kind::Count | Kind::Text => {
                let mut text = self.text(cx.shell, key);
                let problem = self.problem(key).map(|p| Problem::new(key, p));
                let response = ui
                    .scope(|ui| {
                        if setting.kind == Kind::Count && text.is_empty() {
                            ui.weak(schema::unlimited());
                        }
                        field(ui, &setting.label, &mut text, 180.0, problem.as_ref())
                    })
                    .inner;
                if response.changed() {
                    self.edit_text(key, text.clone());
                }
                if response.lost_focus() {
                    self.commit_text(cx.shell, key, &text);
                }
            }
            Kind::List => {
                let count = value.as_array().map_or(0, Vec::len);
                ui.label(tr!("pref-list-count", count = count));
            }
        }
    }

    /// The user Library folders: each with Remove, then a field and Add, and Add Folder….
    fn libraries(&mut self, ui: &mut Ui, cx: &mut Cx<'_>) {
        ui.label(tr!("pref-libraries-intro"));
        ui.add_space(4.0);
        let paths = cx.shell.preferences().libraries.paths.clone();
        if paths.is_empty() {
            ui.weak(tr!("pref-libraries-none"));
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
                ui.label(path.as_str());
                let button = ui.small_button(tr!("pref-remove"));
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
            ui.label(starter);
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let mut text = std::mem::take(&mut self.new_library_path);
            let problem = self
                .problem(LIBRARY_PATHS)
                .map(|p| Problem::new(LIBRARY_PATHS, p));
            field(
                ui,
                &tr!("pref-library-path"),
                &mut text,
                300.0,
                problem.as_ref(),
            );
            self.new_library_path = text;
            if ui.button(tr!("pref-add")).clicked()
                && self
                    .add_library_path(cx.shell, &self.new_library_path.clone())
                    .is_ok()
            {
                self.new_library_path.clear();
            }
            if ui.button(tr!("pref-add-folder")).clicked() {
                cx.outcome = Outcome::PickFolder;
            }
        });
        self.library_severities(ui, cx);
    }

    /// The Library check's severities: a grid of check, severity and the reset dot, in
    /// the order of the check's table, with Reset severities.
    fn library_severities(&mut self, ui: &mut Ui, cx: &mut Cx<'_>) {
        ui.add_space(12.0);
        ui.strong(tr!("pref-libraries-severities"));
        ui.label(tr!("pref-libraries-severities-intro"));
        ui.add_space(4.0);
        let settings: Vec<Setting> = PageId::LIBRARIES
            .settings()
            .into_iter()
            .filter(|s| s.key.starts_with(LIBRARY_SEVERITIES))
            .collect();
        egui::Grid::new("preferences-library-severities")
            .striped(true)
            .num_columns(3)
            .show(ui, |ui| {
                for setting in &settings {
                    let label = ui.label(setting.label.as_str());
                    if !setting.description.is_empty() {
                        label.on_hover_text(setting.description.as_str());
                    }
                    ui.horizontal(|ui| self.editor(ui, cx, setting));
                    self.reset_dot(ui, cx, setting);
                    ui.end_row();
                }
            });
        ui.add_space(4.0);
        let changed = settings.iter().any(|s| self.can_reset(cx.shell, &s.key));
        let reset = ui.add_enabled(changed, Button::new(tr!("pref-libraries-severities-reset")));
        if reset.clicked() {
            let keys: Vec<String> = settings.iter().map(|s| s.key.clone()).collect();
            self.reset_with(cx, |d, shell| d.reset_keys(shell, &keys));
        }
    }

    /// The bridge switch, the agents connected now and the agents allowed in, each with
    /// Forget.
    fn agents(&mut self, ui: &mut Ui, cx: &mut Cx<'_>) {
        for setting in PageId::AGENTS.settings() {
            self.setting_row(ui, cx, &setting);
        }
        if let Some(error) = cx.bridge.error() {
            ui.colored_label(
                ui.visuals().error_fg_color,
                tr!("bridge-error", error = error),
            );
        }
        ui.add_space(8.0);
        ui.strong(tr!("bridge-connected"));
        let agents = cx.bridge.agents();
        if agents.is_empty() {
            ui.weak(tr!("pref-agents-none-connected"));
        }
        for agent in &agents {
            ui.label(format!("{} ({})", agent.name, agent.source.label()));
        }
        ui.add_space(8.0);
        ui.strong(tr!("bridge-allowed"));
        let allowed = cx.bridge.allowed(cx.shell);
        if allowed.is_empty() {
            ui.weak(tr!("bridge-none-allowed"));
        }
        for name in allowed {
            ui.horizontal(|ui| {
                ui.label(name.as_str());
                let button = ui.small_button(tr!("bridge-forget"));
                let label = tr!("pref-forget-named", agent = name.as_str());
                button.widget_info(|| WidgetInfo::labeled(Role::Button, true, label.clone()));
                if button.clicked() {
                    cx.bridge.forget_agent(cx.shell, &name);
                }
            });
        }
    }

    /// Every Command and Preset with its keys: search, recording, conflicts, Remove,
    /// Reset and Open folder.
    fn keyboard_shortcuts(&mut self, ui: &mut Ui, cx: &mut Cx<'_>) {
        ui.horizontal(|ui| {
            let search = ui.add(
                TextEdit::singleline(&mut self.shortcuts.query)
                    .hint_text(tr!("shortcuts-search-hint"))
                    .desired_width(260.0),
            );
            search.widget_info(|| {
                WidgetInfo::labeled(Role::SearchInput, true, tr!("shortcuts-search"))
            });
            if let Some(folder) = super::Shortcuts::folder(cx.shell)
                && ui.link(tr!("shortcuts-open-folder")).clicked()
            {
                cx.outcome = Outcome::ShowFolder(folder);
            }
        });
        for problem in super::Shortcuts::problems(cx.shell) {
            ui.colored_label(ui.visuals().error_fg_color, problem);
        }
        if let Some(error) = self.shortcuts.error() {
            ui.colored_label(ui.visuals().error_fg_color, error.to_owned());
        }
        self.recording(ui, cx);
        ui.add_space(4.0);
        let query = self.shortcuts.query.clone();
        let rows = self.shortcuts.matching(cx.shell, &query);
        if rows.is_empty() {
            ui.weak(tr!("pref-search-none"));
        }
        for row in rows {
            self.shortcut_row(ui, cx, &row);
        }
    }

    fn shortcut_row(&mut self, ui: &mut Ui, cx: &mut Cx<'_>, row: &Row) {
        let mac = cfg!(target_os = "macos");
        ui.horizontal_top(|ui| {
            let changed = super::Shortcuts::is_changed(cx.shell, &row.binding);
            let mut title = RichText::new(row.title.as_str());
            if changed {
                title = title.strong();
            }
            fixed_label(ui, LABEL_WIDTH, title).on_hover_text(row.id.as_str());
            // A Command bound in many contexts wraps its keys rather than widening the
            // dialog.
            let width = ui.available_width();
            ui.allocate_ui(egui::vec2(width, 0.0), |ui| {
                ui.horizontal_wrapped(|ui| {
                    let mut remove = None;
                    for (context, chord) in super::Shortcuts::keys(cx.shell, &row.binding) {
                        let text = match &context {
                            Context::Global => chord.label(mac),
                            other => format!("{} ({other})", chord.label(mac)),
                        };
                        let chip = ui.add(Button::new(format!("{text}  ×")).small());
                        let name = tr!(
                            "shortcuts-remove-named",
                            key = text.as_str(),
                            command = row.title.as_str()
                        );
                        chip.widget_info(|| WidgetInfo::labeled(Role::Button, true, name.clone()));
                        if chip.clicked() {
                            remove = Some((context, chord));
                        }
                    }
                    if let Some((context, chord)) = remove {
                        self.shortcuts.remove_key(cx.shell, &context, &chord);
                    }
                    let add = ui.small_button(tr!("shortcuts-add"));
                    let name = tr!("shortcuts-add-named", command = row.title.as_str());
                    add.widget_info(|| WidgetInfo::labeled(Role::Button, true, name.clone()));
                    if add.clicked() {
                        self.shortcuts
                            .start_recording_in(cx.shell, row.binding.clone());
                    }
                    let reset = ui.add_enabled(changed, Button::new(tr!("pref-reset")).small());
                    let name = tr!("pref-reset-named", setting = row.title.as_str());
                    reset.widget_info(|| WidgetInfo::labeled(Role::Button, true, name.clone()));
                    if reset.clicked() {
                        self.shortcuts.reset_binding(cx.shell, &row.binding);
                    }
                });
            });
        });
    }

    /// The key being recorded: waits for a key (Escape cancels), then shows it with its
    /// context, any conflict, and Assign or Cancel.
    fn recording(&mut self, ui: &mut Ui, cx: &mut Cx<'_>) {
        let Some(recording) = self.shortcuts.recording.clone() else {
            return;
        };
        let mac = cfg!(target_os = "macos");
        if recording.chord.is_none() {
            let pressed = ui.ctx().input_mut(|i| {
                let mut pressed = None;
                i.events.retain(|event| {
                    if pressed.is_some() {
                        return true;
                    }
                    if let egui::Event::Key {
                        key: Key::Escape,
                        pressed: true,
                        modifiers,
                        ..
                    } = event
                        && modifiers.is_none()
                    {
                        pressed = Some(None);
                        return false;
                    }
                    match Chord::from_event(event).first() {
                        Some(chord) => {
                            pressed = Some(Some(*chord));
                            false
                        }
                        None => true,
                    }
                });
                pressed
            });
            match pressed {
                Some(None) => {
                    self.shortcuts.cancel();
                    return;
                }
                Some(Some(chord)) => self.shortcuts.record(chord),
                None => {}
            }
        }
        let title = super::shortcuts::title(cx.shell, &recording.binding);
        egui::Frame::group(ui.style()).show(ui, |ui| {
            match &self.shortcuts.recording.as_ref().and_then(|r| r.chord) {
                None => ui.label(tr!("shortcuts-press-key", command = title.as_str())),
                Some(chord) => ui.label(tr!(
                    "shortcuts-recorded",
                    key = chord.label(mac),
                    command = title.as_str()
                )),
            };
            ui.horizontal(|ui| {
                ui.label(tr!("shortcuts-context"));
                let current = recording.context.clone();
                let mut picked = None;
                let combo = ComboBox::from_id_salt("shortcuts-context")
                    .selected_text(context_label(&current))
                    .show_ui(ui, |ui| {
                        for context in contexts(&current) {
                            let label = context_label(&context);
                            if ui.selectable_label(context == current, label).clicked() {
                                picked = Some(context);
                            }
                        }
                    })
                    .response;
                combo.widget_info(|| {
                    WidgetInfo::labeled(Role::ComboBox, true, tr!("shortcuts-context"))
                });
                if let Some(context) = picked {
                    self.shortcuts.set_context(context);
                }
            });
            if let Some(conflict) = self.shortcuts.conflict(cx.shell) {
                ui.colored_label(ui.visuals().warn_fg_color, conflict);
            }
            ui.horizontal(|ui| {
                let has_key = self
                    .shortcuts
                    .recording
                    .as_ref()
                    .is_some_and(|r| r.chord.is_some());
                if ui
                    .add_enabled(has_key, Button::new(tr!("shortcuts-assign")))
                    .clicked()
                {
                    let _ = self.shortcuts.confirm(cx.shell);
                }
                if ui.button(tr!("dialog-cancel")).clicked() {
                    self.shortcuts.cancel();
                }
            });
        });
    }
}

/// A label left-aligned in a column `width` wide, cut short with … when longer.
fn fixed_label(ui: &mut Ui, width: f32, text: RichText) -> egui::Response {
    ui.allocate_ui_with_layout(
        egui::vec2(width, 20.0),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.set_width(width);
            ui.add(egui::Label::new(text).truncate())
        },
    )
    .inner
}

impl PreferencesDialog {
    /// The user's Fab Presets, each renamed in its name field (Enter), deleted, or opened
    /// on its Fab Preset pages, with the folder they live in.
    fn fab_presets(&mut self, ui: &mut Ui, cx: &mut Cx<'_>) {
        ui.label(tr!("pref-fab-presets-intro"));
        ui.add_space(4.0);
        let Some(dirs) = cx.shell.engine().user_dirs().cloned() else {
            ui.weak(tr!("shortcuts-no-folder"));
            return;
        };
        let (presets, warnings) = fab_presets::user_presets(&dirs);
        if presets.is_empty() {
            ui.weak(tr!("pref-fab-presets-none"));
        }
        for preset in presets {
            let name = preset.preset.name;
            let key = format!("fab_presets/{name}");
            ui.horizontal(|ui| {
                let mut text = self.texts.get(&key).cloned().unwrap_or(name.clone());
                let label = tr!("pref-fab-preset-name", name = name.as_str());
                let problem = self.problem(&key).map(|p| Problem::new(key.as_str(), p));
                let response = field(ui, &label, &mut text, 200.0, problem.as_ref());
                if response.changed() {
                    self.texts.insert(key.clone(), text.clone());
                }
                if response.lost_focus() && text.trim() != name {
                    self.rename_fab_preset(cx.shell, &name, &text);
                }
                let button = ui.small_button(tr!("pref-delete"));
                let delete = tr!("pref-delete-named", item = name.as_str());
                button.widget_info(|| WidgetInfo::labeled(Role::Button, true, delete.clone()));
                if button.clicked() {
                    self.delete_fab_preset(cx.shell, &name);
                }
                let open = self.preset_pages().is_some_and(|p| p.name == name);
                let edit = ui.add_enabled(
                    !open,
                    egui::Button::new(tr!("pref-fab-preset-edit")).small(),
                );
                let label = tr!("pref-fab-preset-edit-named", name = name.as_str());
                edit.widget_info(|| WidgetInfo::labeled(Role::Button, true, label.clone()));
                if edit.clicked() {
                    self.edit_fab_preset(cx.shell, &name);
                }
            });
        }
        if self.preset_pages().is_some() {
            ui.add_space(6.0);
            self.draw_preset_pages(ui, cx.shell);
        }
        for warning in warnings {
            ui.colored_label(ui.visuals().error_fg_color, warning);
        }
        ui.add_space(4.0);
        if ui.link(tr!("shortcuts-open-folder")).clicked() {
            cx.outcome = Outcome::ShowFolder(fab_presets::dir(&dirs));
        }
    }
}

/// The contexts a key can be recorded in: global, each Document type, and a Tool context
/// the binding already uses.
fn contexts(current: &Context) -> Vec<Context> {
    let mut out = vec![Context::Global];
    out.extend(DocumentType::ALL.iter().map(|&t| Context::Document(t)));
    out.push(Context::Part);
    if !out.contains(current) {
        out.push(current.clone());
    }
    out
}

fn context_label(context: &Context) -> String {
    match context {
        Context::Global => tr!("shortcuts-context-global"),
        other => other.to_string(),
    }
}

/// A Library's Active checkbox, named after the Library; true when it was clicked.
fn active_checkbox(ui: &mut Ui, active: bool, library: &str) -> bool {
    let mut checked = active;
    let response = ui.checkbox(&mut checked, "");
    let name = tr!("pref-library-active-named", library = library);
    response.widget_info(|| WidgetInfo::selected(Role::CheckBox, true, active, name.clone()));
    let tip = tr!("pref-library-active-tip");
    response.on_hover_text(tip).changed()
}
