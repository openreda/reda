//! The Appearance page (docs/spec/v0.1/app.md#preferences): theme, Density, text size,
//! UI scale and the two fonts, in four headed groups, each setting with the control the
//! spec gives it. The keys stay `general.*`; the page is a place in the tree.
//!
//! A setting shown here is drawn by [`PreferencesDialog::setting_row`] like any other:
//! the row asks [`PreferencesDialog::appearance_control`] for the control, so the search
//! results show the same segmented control or font list as the page does.

use egui::{Role, Ui, WidgetInfo};
use serde_json::{Value, json};

use super::{Cx, EDITOR_WIDTH, label_width};
use crate::preferences_dialog::{Kind, PageId, PreferencesDialog, Setting};
use crate::settings::layout;
use crate::theme::fonts::{self, Outcome};
use crate::theme::motion;
use crate::tr;
use crate::widgets;

const THEME: &str = "general.theme";
const DENSITY: &str = "general.density";
const TEXT_SIZE: &str = "general.text_size";
const UI_FONT: &str = "general.ui_font";
const MONO_FONT: &str = "general.mono_font";
const ANIMATE: &str = "general.animate";

/// The page's groups, in order: the catalogue id of each heading and the keys under it.
/// A key the Preferences do not have yet is skipped, and a group with nothing to show
/// draws no heading, so a row appears as soon as its Preference exists.
const GROUPS: &[(&str, &[&str])] = &[
    ("pref-appearance-theme", &[THEME]),
    (
        "pref-appearance-size",
        &[DENSITY, TEXT_SIZE, "general.ui_scale"],
    ),
    ("pref-appearance-fonts", &[UI_FONT, MONO_FONT]),
    (
        "pref-appearance-window",
        &[ANIMATE, "general.system_title_bar"],
    ),
];

/// The order of the theme's segments: System · Light · Dark.
const THEMES: [&str; 3] = ["follow_os", "light", "dark"];

/// The installed families each font dropdown offers, in alphabetical order.
pub struct FontLists {
    /// Every family with a regular face that covers basic Latin.
    ui: Vec<String>,
    /// Those of them that are fixed-pitch.
    mono: Vec<String>,
}

impl FontLists {
    fn of(source: &dyn fonts::Source) -> FontLists {
        let families = source.families();
        let names = |keep: &dyn Fn(&fonts::Family) -> bool| {
            let mut names: Vec<String> = families
                .iter()
                .filter(|f| f.has_latin_regular() && keep(f))
                .map(|f| f.name.clone())
                .collect();
            // A family is indexed under each of its names; one row each is enough.
            names.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
            names
        };
        FontLists {
            ui: names(&|_| true),
            mono: names(&|f| f.is_fixed_pitch()),
        }
    }
}

impl PreferencesDialog {
    /// The page: each group's heading over its settings, the labels in one column.
    pub(super) fn appearance(&mut self, ui: &mut Ui, cx: &mut Cx<'_>) {
        let rows = self.rows(cx.shell, PageId::APPEARANCE);
        if rows.is_empty() {
            layout::muted(ui, tr!("pref-none-modified"));
            return;
        }
        let width = label_width(ui, rows.iter().map(|s| s.label.as_str()));
        for (heading, keys) in GROUPS {
            let mut shown = keys
                .iter()
                .filter_map(|key| rows.iter().find(|s| s.key == *key))
                .peekable();
            if shown.peek().is_some() {
                layout::group(ui, &crate::i18n::translate(heading, &[]));
            }
            for setting in shown {
                self.setting_row(ui, cx, setting, width);
            }
        }
    }

    /// Draws the control of an Appearance setting that has one of its own, and says
    /// whether it did. The others (the UI scale's dropdown, a check box) are the
    /// schema's.
    pub(super) fn appearance_control(
        &mut self,
        ui: &mut Ui,
        cx: &mut Cx<'_>,
        setting: &Setting,
    ) -> bool {
        match setting.key.as_str() {
            THEME | DENSITY => self.segments(ui, cx, setting),
            TEXT_SIZE => self.text_size(ui, cx, setting),
            UI_FONT | MONO_FONT => self.font(ui, cx, setting),
            ANIMATE => self.animate(ui, cx, setting),
            _ => return false,
        }
        true
    }

    /// The warning under a font whose family REDA could not use.
    pub(super) fn appearance_warning(
        &self,
        ui: &Ui,
        cx: &Cx<'_>,
        setting: &Setting,
    ) -> Option<String> {
        let key = setting.key.as_str();
        if !matches!(key, UI_FONT | MONO_FONT) {
            return None;
        }
        let family = self.value(cx.shell, key).as_str()?.to_owned();
        fell_back(ui, key).then(|| tr!("pref-font-not-installed", family = family.as_str()))
    }

    /// **Animate**: a switch that applies when flipped. While the Preference is unset it
    /// shows what the OS reduced-motion setting gave at launch; a flip stores the user's
    /// value, and resetting it follows the OS again from the next launch's read.
    fn animate(&mut self, ui: &mut Ui, cx: &mut Cx<'_>, setting: &Setting) {
        let stored = self.value(cx.shell, ANIMATE).as_bool();
        let mut on = stored.unwrap_or_else(|| motion::system::remembered(ui.ctx()));
        let label = setting.label.clone();
        let response = ui.add(widgets::switch(&mut on, ""));
        response.widget_info(|| WidgetInfo::selected(Role::Switch, true, on, label.clone()));
        if response.changed() {
            let _ = self.set(cx.shell, ANIMATE, json!(on));
        }
    }

    /// A segmented control over the setting's choices: it applies when clicked.
    fn segments(&mut self, ui: &mut Ui, cx: &mut Cx<'_>, setting: &Setting) {
        let Kind::Choice(choices) = &setting.kind else {
            return;
        };
        let key = setting.key.as_str();
        let mut options: Vec<(Value, &str)> = choices
            .iter()
            .map(|c| (c.value.clone(), c.label.as_str()))
            .collect();
        if key == THEME {
            let place = |value: &Value| THEMES.iter().position(|t| value == t);
            options.sort_by_key(|(value, _)| place(value));
        }
        let mut value = self.value(cx.shell, key);
        if ui.add(widgets::segmented(&mut value, &options)).changed() {
            let _ = self.set(cx.shell, key, value);
        }
    }

    /// The text size: typed, or stepped a pixel at a time. It applies when a number is
    /// entered or a step is taken, never while typing.
    fn text_size(&mut self, ui: &mut Ui, cx: &mut Cx<'_>, setting: &Setting) {
        let key = setting.key.as_str();
        let sizes = crate::theme::Settings::TEXT_SIZES;
        let range = i64::from(*sizes.start())..=i64::from(*sizes.end());
        let before = self.value(cx.shell, key).as_i64().unwrap_or_default();
        let mut size = before;
        let field = widgets::stepper_field(&mut size, range).suffix(tr!("pref-unit-px"));
        let response = ui.add(field);
        let label = setting.label.clone();
        response.widget_info(|| WidgetInfo::labeled(Role::TextInput, true, label.clone()));
        if size != before {
            let _ = self.set(cx.shell, key, json!(size));
        }
    }

    /// A font: the bundled face first, then a rule, then the installed families in plain
    /// text, narrowed by typing. A stored family REDA could not use stays in the field
    /// and in the list, marked.
    fn font(&mut self, ui: &mut Ui, cx: &mut Cx<'_>, setting: &Setting) {
        let key = setting.key.as_str();
        let stored = self.value(cx.shell, key);
        let stored = stored.as_str();
        let lists = self
            .font_lists
            .get_or_insert_with(|| FontLists::of(&*cx.shell.font_source()));
        let (bundled, families) = if key == MONO_FONT {
            (fonts::BUNDLED_MONO, &lists.mono)
        } else {
            (fonts::BUNDLED_UI, &lists.ui)
        };
        let default = tr!("pref-font-default", family = bundled);
        let missing = fell_back(ui, key);
        let note = tr!("pref-font-not-installed-note");
        let listed = stored.and_then(|s| families.iter().position(|f| f.eq_ignore_ascii_case(s)));

        // The default, the stored family if the list does not hold it, then the list.
        let mut choices = vec![widgets::Choice::new(&default).rule_below()];
        let extra = stored.filter(|_| missing || listed.is_none());
        if let Some(family) = extra {
            let choice = widgets::Choice::new(family);
            choices.push(if missing { choice.note(&note) } else { choice });
        }
        let first = choices.len();
        choices.extend(
            families
                .iter()
                .enumerate()
                .filter(|(i, _)| !(missing && listed == Some(*i)))
                .map(|(_, f)| widgets::Choice::new(f)),
        );
        let selected = match (stored, extra, listed) {
            (None, ..) => 0,
            (Some(_), Some(_), _) => 1,
            (Some(_), None, Some(at)) => first + at,
            (Some(_), None, None) => 0,
        };
        let picked = widgets::filter_dropdown(("pref", key), choices[selected].text)
            .width(EDITOR_WIDTH)
            .hint(tr!("pref-font-filter"))
            .none_text(tr!("pref-font-none"))
            .show(ui, &choices, Some(selected));
        let label = setting.label.clone();
        picked
            .response
            .widget_info(|| WidgetInfo::labeled(Role::ComboBox, true, label.clone()));
        if let Some(index) = picked.index.filter(|i| *i != selected) {
            let value = match index {
                0 => Value::Null,
                i => json!(choices[i].text),
            };
            let _ = self.set(cx.shell, key, value);
        }
    }
}

/// Whether the bundled face is standing in for the family `key` names: it is not
/// installed, or its file failed validation.
fn fell_back(ui: &Ui, key: &str) -> bool {
    let outcome = fonts::report(ui.ctx()).map_or(Outcome::Bundled, |report| {
        if key == MONO_FONT {
            report.mono
        } else {
            report.ui
        }
    });
    outcome.fell_back()
}
