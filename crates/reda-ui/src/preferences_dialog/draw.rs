//! Drawing the Preferences window into its tab, in the layout every settings window
//! shares ([`crate::settings::layout`]): the search box with Modified only and Reset all
//! across the top, the page tree, the shown page (or the search results) and Close.
//! Every change runs at once; there is no OK or Cancel.
//!
//! A page is a list of setting rows ([`PreferencesDialog::setting_row`]) under group
//! headings, or content of its own: Libraries, Fab Presets and Agents in `draw/pages.rs`,
//! Keyboard Shortcuts in `draw/shortcuts_page.rs`.

use std::path::PathBuf;

use egui::{Color32, Role, Ui, WidgetInfo};
use reda_engine::command::CommandError;
use serde_json::json;

use super::{Choice, Kind, PageId, PreferencesDialog, Setting, schema};
use crate::bridge::Bridge;
use crate::settings::layout::{self, Content, TreeRow};
use crate::settings::{Problem, field};
use crate::shell::Shell;
use crate::theme::{self, TokensExt as _};
use crate::tr;
use crate::widgets::{self, Column, Tooltip as _};

mod pages;
mod shortcuts_page;

/// The least width of the label column. It grows to the widest label shown.
const LABEL_WIDTH: f32 = 230.0;
/// The width of the reset dot's column.
const DOT_WIDTH: f32 = 14.0;
/// The radius of the reset dot, at rest and hovered.
const DOT_RADIUS: [f32; 2] = [3.5, 4.5];
/// The width of a setting's field or dropdown.
const EDITOR_WIDTH: f32 = 220.0;
/// The width of the search box.
const SEARCH_WIDTH: f32 = 260.0;

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

/// One frame of the window in the shared layout.
struct Shown<'a, 'b> {
    dialog: &'a mut PreferencesDialog,
    cx: Cx<'b>,
}

impl Content for Shown<'_, '_> {
    fn has_top_bar(&self) -> bool {
        true
    }

    /// The search box, Modified only and Reset all.
    fn top_bar(&mut self, ui: &mut Ui) {
        let (dialog, cx) = (&mut *self.dialog, &mut self.cx);
        let search = ui.add(
            widgets::text_field(&mut dialog.query)
                .search()
                .hint(tr!("pref-search-hint"))
                .width(SEARCH_WIDTH),
        );
        search.widget_info(|| WidgetInfo::labeled(Role::SearchInput, true, tr!("pref-search")));
        ui.add(widgets::checkbox(
            &mut dialog.modified_only,
            tr!("pref-modified-only"),
        ));
        let any = dialog.modified_count(cx.shell, PageId::ALL_SETTINGS) > 0;
        if ui
            .add_enabled(any, widgets::button(tr!("pref-reset-all")))
            .clicked()
        {
            dialog.reset_with(cx, |d, shell| d.reset_all(shell));
        }
    }

    /// The page tree: each page indented under its branch, with the count of its changed
    /// settings. While searching, pages without a match are dimmed.
    fn tree(&mut self) -> Vec<TreeRow> {
        let (dialog, shell) = (&*self.dialog, &*self.cx.shell);
        let searching = dialog.searching();
        let found = if searching {
            dialog.found(shell)
        } else {
            Vec::new()
        };
        dialog
            .pages(shell)
            .into_iter()
            .map(|page| {
                let title = page.title();
                let count = dialog.modified_count(shell, page);
                TreeRow {
                    name: (count > 0)
                        .then(|| tr!("pref-page-modified", page = title.as_str(), count = count)),
                    title,
                    heading: None,
                    depth: page.depth(),
                    selected: page == dialog.page && !searching,
                    muted: searching && !found.iter().any(|(p, _)| *p == page),
                    invalid: false,
                    trailing: (count > 0).then(|| count.to_string()),
                }
            })
            .collect()
    }

    fn choose(&mut self, index: usize) {
        let dialog = &mut *self.dialog;
        let Some(page) = dialog.pages(self.cx.shell).get(index).copied() else {
            return;
        };
        dialog.page = page;
        if page != PageId::ALL_SETTINGS {
            dialog.query.clear();
        }
    }

    fn page(&mut self, ui: &mut Ui) {
        let (dialog, cx) = (&mut *self.dialog, &mut self.cx);
        // All settings has the same search over its own table.
        if dialog.searching() {
            let found = dialog.found(cx.shell);
            dialog.results(ui, cx, found);
        } else {
            dialog.page_header(ui, cx);
            dialog.page_content(ui, cx);
        }
    }

    fn footer(&mut self, ui: &mut Ui) {
        layout::muted(ui, tr!("pref-live"));
    }

    fn buttons(&mut self, ui: &mut Ui) {
        if ui.add(widgets::button(tr!("pref-close"))).clicked() {
            self.cx.outcome = Outcome::Closed;
        }
    }
}

impl PreferencesDialog {
    /// Draws the window's content into `ui` (its tab) for one frame.
    pub fn show(&mut self, ui: &mut Ui, shell: &mut Shell, bridge: &mut Bridge) -> Outcome {
        let mut shown = Shown {
            dialog: self,
            cx: Cx {
                shell,
                bridge,
                outcome: Outcome::Open,
            },
        };
        layout::show(ui, &mut shown);
        shown.cx.outcome
    }

    /// Whether the search replaces the page with its results.
    fn searching(&self) -> bool {
        !self.query.trim().is_empty() && self.page != PageId::ALL_SETTINGS
    }

    /// The page's title, with Reset page and, on a branch, Reset branch.
    fn page_header(&mut self, ui: &mut Ui, cx: &mut Cx<'_>) {
        let page = self.page;
        layout::page_header(ui, &page.title(), |ui| {
            if page == PageId::ALL_SETTINGS {
                return;
            }
            if page.has_children() {
                let changed = self
                    .pages_in(page)
                    .any(|p| self.modified_count(cx.shell, p) > 0);
                if ui
                    .add_enabled(changed, widgets::button(tr!("pref-reset-branch")))
                    .clicked()
                {
                    self.reset_with(cx, |d, shell| d.reset_page(shell, page, true));
                }
            }
            let changed = self.modified_count(cx.shell, page) > 0;
            if ui
                .add_enabled(changed, widgets::button(tr!("pref-reset-page")))
                .clicked()
            {
                self.reset_with(cx, |d, shell| d.reset_page(shell, page, false));
            }
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
            layout::muted(ui, tr!("pref-search-none"));
            return;
        }
        let width = label_width(ui, found.iter().map(|(_, s)| s.label.as_str()));
        let mut page = None;
        for (on, setting) in found {
            if page != Some(on) {
                page = Some(on);
                layout::group(ui, &on.title());
            }
            self.setting_row(ui, cx, &setting, width);
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
                let grid_page = page == PageId::GRID;
                if grid_page {
                    rows.retain(|s| !super::grid_tables::is_table(&s.key));
                }
                if rows.is_empty() {
                    layout::muted(ui, tr!("pref-none-modified"));
                }
                self.setting_rows(ui, cx, &rows);
                if page == PageId::CHECKS && !self.searching() {
                    super::check_grid::show(self, ui, cx.shell);
                }
                if grid_page && !self.searching() {
                    super::grid_tables::show(self, ui, cx.shell);
                }
            }
        }
    }

    /// A list of settings, each group under its heading, the labels in one column.
    fn setting_rows(&mut self, ui: &mut Ui, cx: &mut Cx<'_>, rows: &[Setting]) {
        let width = label_width(ui, rows.iter().map(|s| s.label.as_str()));
        let mut heading = None;
        for setting in rows {
            if setting.heading != heading {
                heading.clone_from(&setting.heading);
                if let Some(h) = &heading {
                    layout::group(ui, h);
                }
            }
            self.setting_row(ui, cx, setting, width);
        }
    }

    /// All settings: key, value (editable), default, the reset dot and description, for
    /// every key the search and Modified only leave.
    fn all_settings_table(&mut self, ui: &mut Ui, cx: &mut Cx<'_>) {
        let rows = self.all_settings(cx.shell);
        if rows.is_empty() {
            layout::muted(ui, tr!("pref-search-none"));
            return;
        }
        let t = ui.tokens();
        widgets::table("preferences-all-settings")
            .columns([
                Column::auto(),
                Column::auto(),
                Column::auto(),
                Column::auto(),
                Column::fill(),
            ])
            .show(ui, |table| {
                table.header(|h| {
                    h.text(tr!("pref-all-key"));
                    h.text(tr!("pref-all-value"));
                    h.text(tr!("pref-all-default"));
                    h.empty();
                    h.text(tr!("pref-all-description"));
                });
                for row in rows {
                    table.row(|cells| {
                        cells.ui(|ui| {
                            let key = t.ty.mono.rich(row.setting.key.as_str());
                            ui.label(key.color(t.color.text.primary))
                        });
                        cells.ui(|ui| self.editor(ui, cx, &row.setting));
                        cells.muted(row.default.as_str());
                        cells.ui(|ui| self.reset_dot(ui, cx, &row.setting));
                        cells.ui(|ui| {
                            let text = t.ty.body.rich(row.setting.description.as_str());
                            ui.add(egui::Label::new(text.color(t.color.text.primary)).wrap())
                        });
                    });
                }
            });
    }

    /// The dot beside a changed setting, which puts it back to its default; an unchanged
    /// setting keeps the space empty.
    fn reset_dot(&mut self, ui: &mut Ui, cx: &mut Cx<'_>, setting: &Setting) {
        let t = ui.tokens();
        let key = setting.key.as_str();
        let size = egui::vec2(DOT_WIDTH, t.control_height());
        if !self.can_reset(cx.shell, key) {
            ui.allocate_space(size);
            return;
        }
        let (rect, dot) = ui.allocate_exact_size(size, egui::Sense::click());
        let radius = DOT_RADIUS[usize::from(dot.hovered())];
        ui.painter()
            .circle_filled(rect.center(), radius, t.color.accent.text);
        let name = tr!("pref-reset-named", setting = setting.label.as_str());
        dot.widget_info(|| WidgetInfo::labeled(Role::Button, true, name.clone()));
        let dot = dot.tooltip(tr!("pref-reset-dot"));
        if dot.clicked() {
            let keys = [setting.key.clone()];
            self.reset_with(cx, |d, shell| d.reset_keys(shell, &keys));
        }
    }

    /// One setting: the reset dot when it is changed, its label (described on hover) in a
    /// column `label` wide, and its editor.
    fn setting_row(&mut self, ui: &mut Ui, cx: &mut Cx<'_>, setting: &Setting, label: f32) {
        self.row(ui, cx, setting, label, |dialog, ui, cx| {
            dialog.editor(ui, cx, setting);
        });
    }

    /// A setting's row with a control of the caller's: the reset dot, the label and
    /// whatever `control` adds.
    fn row(
        &mut self,
        ui: &mut Ui,
        cx: &mut Cx<'_>,
        setting: &Setting,
        label: f32,
        control: impl FnOnce(&mut Self, &mut Ui, &mut Cx<'_>),
    ) {
        let t = ui.tokens();
        let key = setting.key.as_str();
        // Rows are `stack` apart.
        ui.add_space((t.space.stack - t.space.row_y).max(0.0));
        ui.horizontal(|ui| {
            self.reset_dot(ui, cx, setting);
            let name = fixed_label(ui, label, &setting.label, false);
            if !setting.description.is_empty() {
                name.tooltip(setting.description.as_str());
            }
            control(self, ui, cx);
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
                let response = ui.add(widgets::checkbox(&mut on, ""));
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
                // As wide as its longest choice needs, so none is cut short.
                let t = ui.tokens();
                let widest = choices
                    .iter()
                    .map(|c| text_width(ui, &t.ty.body, &c.label))
                    .fold(0.0, f32::max);
                let fitted = widest + 2.0 * t.space.control_x + t.space.gap + widgets::glyph::BOX;
                let response = widgets::dropdown(("pref", key), shown)
                    .width(fitted.max(EDITOR_WIDTH))
                    .show(ui, |ui| {
                        for choice in choices {
                            let row = widgets::choice(same(choice), choice.label.as_str());
                            if ui.add(row).clicked() {
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
                let hint = (setting.kind == Kind::Count).then(schema::unlimited);
                let response = text_editor(ui, &setting.label, &mut text, hint, problem.as_ref());
                if response.changed() {
                    self.edit_text(key, text.clone());
                }
                if response.lost_focus() {
                    self.commit_text(cx.shell, key, &text);
                }
            }
            Kind::List => {
                let count = value.as_array().map_or(0, Vec::len);
                layout::text(ui, tr!("pref-list-count", count = count));
            }
        }
    }
}

/// A setting's text field. A blank count shows `hint` (Unlimited) in the field.
fn text_editor(
    ui: &mut Ui,
    label: &str,
    text: &mut String,
    hint: Option<String>,
    problem: Option<&Problem>,
) -> egui::Response {
    let Some(hint) = hint else {
        return field(ui, label, text, EDITOR_WIDTH, problem);
    };
    ui.vertical(|ui| {
        let field = widgets::text_field(text)
            .hint(hint)
            .width(EDITOR_WIDTH)
            .invalid(problem.is_some());
        let response = ui.add(field);
        let label = label.to_owned();
        response.widget_info(|| WidgetInfo::labeled(Role::TextInput, true, label.clone()));
        if let Some(problem) = problem {
            crate::settings::problem_label(ui, problem);
        }
        response
    })
    .inner
}

/// The width of `text` on one line in `style`.
fn text_width(ui: &Ui, style: &theme::Text, text: &str) -> f32 {
    let galley =
        ui.painter()
            .layout_no_wrap(text.to_owned(), style.font_id(), Color32::TRANSPARENT);
    galley.size().x.ceil()
}

/// A label on one line: a description standing in for a missing label may hold breaks.
fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The width of a label column that fits every one of `labels`, at least
/// [`LABEL_WIDTH`], so no label is cut short.
fn label_width<'a>(ui: &Ui, labels: impl Iterator<Item = &'a str>) -> f32 {
    let t = ui.tokens();
    // A changed row's label is drawn stronger, which is the wider of the two.
    labels
        .map(|label| text_width(ui, &t.ty.body_strong, &one_line(label)))
        .fold(LABEL_WIDTH, f32::max)
}

/// A label left-aligned in a column `width` wide and a control high; `strong` for a row
/// that was changed.
fn fixed_label(ui: &mut Ui, width: f32, text: &str, strong: bool) -> egui::Response {
    let t = ui.tokens();
    let style = if strong {
        &t.ty.body_strong
    } else {
        &t.ty.body
    };
    ui.allocate_ui_with_layout(
        egui::vec2(width, t.control_height()),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.set_width(width);
            let text = style.rich(one_line(text)).color(t.color.text.primary);
            ui.add(egui::Label::new(text).extend())
        },
    )
    .inner
}
