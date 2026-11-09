//! The Preferences window (docs/spec/v0.1/app.md#preferences): a page tree, a search box
//! over every setting, Modified only, a count of changed settings per page, a dot that
//! resets each changed setting, Reset page, Reset branch and Reset all, and a generated
//! All settings page. Unlike the Project settings windows ([`crate::settings`]) it has no
//! OK, Cancel or undo: Preferences are per-user, so every change runs
//! `app.preferences.set` (or `app.preferences.reset`) at once and applies live.
//!
//! The tree and which page each key is on come from `reda_engine::preferences::pages`,
//! the settings from the Preferences schema ([`schema`]); the Libraries, Fab Presets,
//! Agents and Keyboard Shortcuts pages have their own content ([`shortcuts`] for the
//! last). The model here has no drawing, so it is tested without a window; the modal is
//! drawn by [`PreferencesDialog::show`] (in `preferences_dialog/draw.rs`).

use std::collections::BTreeMap;

use reda_engine::command::CommandError;
use reda_engine::preferences::{self, Preferences, pages};
use serde_json::{Value, json};

use crate::shell::Shell;
use crate::tr;

mod check_grid;
mod draw;
mod fab_preset_pages;
mod schema;
pub mod shortcuts;
mod user_keymap;

pub use draw::Outcome;
pub use fab_preset_pages::{UserPresetPages, user_source};
pub use schema::{Choice, Kind, Setting, preference_keys, search, setting};
pub use shortcuts::Shortcuts;

/// A page of the window: a page of the Preferences tree
/// (`reda_engine::preferences::pages`), or the generated All settings page.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PageId(&'static str);

impl PageId {
    pub const GENERAL: PageId = PageId("general");
    pub const LIBRARIES: PageId = PageId("libraries");
    pub const FAB_PRESETS: PageId = PageId("fab_presets");
    pub const AGENTS: PageId = PageId("agents");
    pub const KEYBOARD_SHORTCUTS: PageId = PageId("keyboard_shortcuts");
    /// How check results show on the canvas, with the per-check display grid.
    pub const CHECKS: PageId = PageId("checks");
    /// The last page, generated from the schema: every key with its value, default and
    /// description.
    pub const ALL_SETTINGS: PageId = PageId("all");

    /// Pages drawn by hand rather than as a list of settings, shown even with no setting
    /// of their own.
    const OWN_CONTENT: [PageId; 5] = [
        PageId::LIBRARIES,
        PageId::FAB_PRESETS,
        PageId::AGENTS,
        PageId::KEYBOARD_SHORTCUTS,
        PageId::ALL_SETTINGS,
    ];

    /// Every page of the tree, then All settings.
    fn every() -> impl Iterator<Item = PageId> {
        pages::PAGES
            .iter()
            .map(|p| PageId(p.id))
            .chain([PageId::ALL_SETTINGS])
    }

    /// The pages the tree lists, in order: each page with settings or content of its own,
    /// or with such a page under it. Pages that later settings fill are left out until
    /// then.
    pub fn tree() -> Vec<PageId> {
        let filled: Vec<PageId> = PageId::every()
            .filter(|p| PageId::OWN_CONTENT.contains(p) || !p.settings().is_empty())
            .collect();
        PageId::every()
            .filter(|p| filled.iter().any(|f| f.is_in(*p)))
            .collect()
    }

    /// The page with the tree id `id`, such as `editing.snap`.
    pub fn from_id(id: &str) -> Option<PageId> {
        PageId::every().find(|p| p.0 == id)
    }

    /// The tree id, as `app.preferences.reset {page}` takes it.
    pub fn id(self) -> &'static str {
        self.0
    }

    /// The page this one is indented under.
    pub fn parent(self) -> Option<PageId> {
        pages::page(self.0).and_then(|p| p.parent).map(PageId)
    }

    /// How far the tree indents it: 0 for a top page.
    pub fn depth(self) -> usize {
        self.parent().map_or(0, |p| p.depth() + 1)
    }

    /// Whether the tree lists pages under this one, which gives it Reset branch.
    pub fn has_children(self) -> bool {
        PageId::tree().iter().any(|p| p.parent() == Some(self))
    }

    /// Whether this is `branch` or a page under it.
    pub fn is_in(self, branch: PageId) -> bool {
        self == branch || pages::is_in(self.0, branch.0)
    }

    /// The page's title, from the catalogue (`pref-page-<id>`, `.` and `_` as `-`).
    pub fn title(self) -> String {
        let id = format!("pref-page-{}", self.0.replace(['.', '_'], "-"));
        if crate::i18n::has_key(&id) {
            crate::i18n::translate(&id, &[])
        } else {
            self.0.to_owned()
        }
    }

    /// The Preferences this page shows from the schema, in order. Libraries, Fab Presets,
    /// Agents and Keyboard Shortcuts show other things as well; All settings shows every
    /// key.
    pub fn settings(self) -> Vec<Setting> {
        schema::settings_on(self)
    }

    /// Every key on the page, hidden ones too: what Reset page resets.
    fn keys(self, branch: bool) -> Vec<String> {
        if self == PageId::ALL_SETTINGS {
            preference_keys()
        } else {
            pages::keys_on(self.0, branch)
        }
    }
}

/// One row of All settings.
#[derive(Clone, Debug, PartialEq)]
pub struct AllSettingsRow {
    pub setting: Setting,
    /// The value now, as text (`follow_os`, `false`, `["a"]`, blank for unset).
    pub value: String,
    /// The default, as text.
    pub default: String,
}

/// The dialog's state: the page shown, the search, text being typed and the values that
/// were refused.
pub struct PreferencesDialog {
    page: PageId,
    /// The search box over every setting.
    pub query: String,
    /// Shows only pages and settings changed from their defaults.
    pub modified_only: bool,
    /// Text typed into a field and not yet committed, by key.
    texts: BTreeMap<String, String>,
    /// Why the last value committed to a key was refused, by key.
    problems: BTreeMap<String, String>,
    /// A Library folder being typed on the Libraries page.
    pub new_library_path: String,
    /// The Keyboard Shortcuts page.
    pub shortcuts: Shortcuts,
    /// The user Fab Preset whose pages are open on the Fab Presets page.
    preset_pages: Option<UserPresetPages>,
}

impl Default for PreferencesDialog {
    fn default() -> Self {
        PreferencesDialog::new()
    }
}

impl PreferencesDialog {
    /// The dialog on its General page.
    pub fn new() -> PreferencesDialog {
        PreferencesDialog {
            page: PageId::GENERAL,
            query: String::new(),
            modified_only: false,
            texts: BTreeMap::new(),
            problems: BTreeMap::new(),
            new_library_path: String::new(),
            shortcuts: Shortcuts::default(),
            preset_pages: None,
        }
    }

    /// The dialog on `page`.
    pub fn on(page: PageId) -> PreferencesDialog {
        PreferencesDialog {
            page,
            ..PreferencesDialog::new()
        }
    }

    /// Whether the Keyboard Shortcuts page is recording a key.
    pub fn is_recording(&self) -> bool {
        self.shortcuts.recording.is_some()
    }

    pub fn page(&self) -> PageId {
        self.page
    }

    pub fn select(&mut self, page: PageId) {
        self.page = page;
    }

    /// Sets one Preference through `app.preferences.set`, so it is journalled and applies
    /// at once. A refusal is kept as the key's problem.
    pub fn set(
        &mut self,
        shell: &mut Shell,
        key: &str,
        value: Value,
    ) -> Result<Value, CommandError> {
        let out = shell.run("app.preferences.set", json!({ "key": key, "value": value }));
        match &out {
            Ok(_) => {
                self.problems.remove(key);
            }
            Err(e) => {
                self.problems.insert(key.to_owned(), e.message.clone());
            }
        }
        out
    }

    /// Puts `key` back to its default.
    pub fn reset(&mut self, shell: &mut Shell, key: &str) -> Result<Value, CommandError> {
        self.texts.remove(key);
        self.set(shell, key, Value::Null)
    }

    /// Whether `key` differs from its default, which enables its Reset.
    pub fn can_reset(&self, shell: &Shell, key: &str) -> bool {
        value(shell.preferences(), key) != value(&Preferences::default(), key)
    }

    /// The value of `key` now.
    pub fn value(&self, shell: &Shell, key: &str) -> Value {
        value(shell.preferences(), key)
    }

    /// Why the last value given to `key` was refused, if it was.
    pub fn problem(&self, key: &str) -> Option<&str> {
        self.problems.get(key).map(String::as_str)
    }

    /// The text a typed field shows: what is being typed, else the value now.
    pub fn text(&self, shell: &Shell, key: &str) -> String {
        if let Some(text) = self.texts.get(key) {
            return text.clone();
        }
        as_text(&self.value(shell, key))
    }

    /// Keeps `text` as what is being typed into `key`'s field.
    pub fn edit_text(&mut self, key: &str, text: String) {
        self.texts.insert(key.to_owned(), text);
    }

    /// Commits the text typed into a Number, Count or Text field: blank clears an
    /// optional value. Text that is not a value of the right kind, or a value the
    /// Preferences refuse, is flagged and kept in the field.
    pub fn commit_text(&mut self, shell: &mut Shell, key: &str, text: &str) {
        let kind = schema::setting(key).kind;
        let text = text.trim();
        let parsed = match kind {
            Kind::Count if text.is_empty() => Ok(Value::Null),
            Kind::Count => match text.parse::<u32>() {
                Ok(n) if n > 0 => Ok(json!(n)),
                _ => Err(tr!("pref-not-a-count")),
            },
            Kind::Number => match text.parse::<f64>() {
                Ok(n) if n.is_finite() => Ok(json!(n)),
                _ => Err(tr!("pref-not-a-number")),
            },
            _ if text.is_empty() => Ok(Value::Null),
            _ => Ok(json!(text)),
        };
        match parsed {
            Ok(value) => {
                if self.set(shell, key, value).is_ok() {
                    self.texts.remove(key);
                } else {
                    self.texts.insert(key.to_owned(), text.to_owned());
                }
            }
            Err(problem) => {
                self.problems.insert(key.to_owned(), problem);
                self.texts.insert(key.to_owned(), text.to_owned());
            }
        }
    }

    /// How many settings on `page` (its own, not its sub-pages') differ from their
    /// defaults; on All settings, every changed setting.
    pub fn modified_count(&self, shell: &Shell, page: PageId) -> usize {
        page.keys(false)
            .iter()
            .filter(|k| self.can_reset(shell, k))
            .count()
    }

    /// The pages the tree lists now: with Modified only, just those with a changed
    /// setting on them or under them (and All settings when anything changed).
    pub fn pages(&self, shell: &Shell) -> Vec<PageId> {
        let tree = PageId::tree();
        if !self.modified_only {
            return tree;
        }
        let changed: Vec<PageId> = tree
            .iter()
            .copied()
            .filter(|&p| self.modified_count(shell, p) > 0)
            .collect();
        tree.into_iter()
            .filter(|&p| changed.iter().any(|c| c.is_in(p)))
            .collect()
    }

    /// The settings `page` shows as rows now: with Modified only, just the changed ones.
    pub fn rows(&self, shell: &Shell, page: PageId) -> Vec<Setting> {
        page.settings()
            .into_iter()
            .filter(|s| !self.modified_only || self.can_reset(shell, &s.key))
            .collect()
    }

    /// The settings the search finds now, with their pages.
    pub fn found(&self, shell: &Shell) -> Vec<(PageId, Setting)> {
        search(&self.query)
            .into_iter()
            .filter(|(_, s)| !self.modified_only || self.can_reset(shell, &s.key))
            .collect()
    }

    /// The All settings page's rows: every key, filtered by the search and Modified only.
    pub fn all_settings(&self, shell: &Shell) -> Vec<AllSettingsRow> {
        let defaults = Preferences::default();
        self.rows(shell, PageId::ALL_SETTINGS)
            .into_iter()
            .filter(|s| schema::matches(s, &self.query))
            .map(|setting| AllSettingsRow {
                value: as_text(&self.value(shell, &setting.key)),
                default: as_text(&value(&defaults, &setting.key)),
                setting,
            })
            .collect()
    }

    /// Puts `keys` back to their defaults (`app.preferences.reset {keys}`), as each
    /// setting's dot does. Returns the keys that changed.
    pub fn reset_keys(
        &mut self,
        shell: &mut Shell,
        keys: &[String],
    ) -> Result<Vec<String>, CommandError> {
        self.run_reset(shell, json!({ "keys": keys }))
    }

    /// Reset page, or with `branch` Reset branch (`app.preferences.reset {page}`).
    pub fn reset_page(
        &mut self,
        shell: &mut Shell,
        page: PageId,
        branch: bool,
    ) -> Result<Vec<String>, CommandError> {
        if page == PageId::ALL_SETTINGS {
            return self.reset_all(shell);
        }
        self.run_reset(shell, json!({ "page": page.id(), "branch": branch }))
    }

    /// Reset all (`app.preferences.reset {all}`).
    pub fn reset_all(&mut self, shell: &mut Shell) -> Result<Vec<String>, CommandError> {
        self.run_reset(shell, json!({ "all": true }))
    }

    fn run_reset(&mut self, shell: &mut Shell, params: Value) -> Result<Vec<String>, CommandError> {
        let out = shell.run("app.preferences.reset", params)?;
        let keys: Vec<String> = out["reset"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|k| k.as_str().map(str::to_owned))
            .collect();
        for key in &keys {
            self.texts.remove(key);
            self.problems.remove(key);
        }
        Ok(keys)
    }

    /// Adds a user Library folder.
    pub fn add_library_path(
        &mut self,
        shell: &mut Shell,
        path: &str,
    ) -> Result<Value, CommandError> {
        let mut paths = shell.preferences().libraries.paths.clone();
        let path = path.trim();
        if path.is_empty() {
            return Err(CommandError::invalid_params(
                "/value",
                tr!("pref-blank-path"),
            ));
        }
        if paths.iter().any(|p| p == path) {
            let error = CommandError::rejected(tr!("pref-duplicate-path", path = path));
            self.problems
                .insert(LIBRARY_PATHS.to_owned(), error.message.clone());
            return Err(error);
        }
        paths.push(path.to_owned());
        self.set(shell, LIBRARY_PATHS, json!(paths))
    }

    /// Renames the user Fab Preset `name` (`fab_preset.rename_user`); a refusal shows
    /// beside its name field.
    pub fn rename_fab_preset(&mut self, shell: &mut Shell, name: &str, new_name: &str) {
        let key = format!("fab_presets/{name}");
        let params = json!({"name": name, "new_name": new_name});
        match shell.run("fab_preset.rename_user", params) {
            Ok(_) => {
                self.texts.remove(&key);
                self.problems.remove(&key);
            }
            Err(e) => {
                self.problems.insert(key, e.message);
            }
        }
    }

    /// Moves the user Fab Preset `name` to the trash (`fab_preset.delete_user`).
    pub fn delete_fab_preset(&mut self, shell: &mut Shell, name: &str) {
        let key = format!("fab_presets/{name}");
        match shell.run("fab_preset.delete_user", json!({"name": name})) {
            Ok(_) => {
                self.texts.remove(&key);
                self.problems.remove(&key);
            }
            Err(e) => {
                self.problems.insert(key, e.message);
            }
        }
    }

    /// Removes the user Library folder at `index`.
    pub fn remove_library_path(
        &mut self,
        shell: &mut Shell,
        index: usize,
    ) -> Result<Value, CommandError> {
        let mut paths = shell.preferences().libraries.paths.clone();
        if index >= paths.len() {
            return Err(CommandError::not_found(format!(
                "no Library folder {index}"
            )));
        }
        let removed = paths.remove(index);
        let result = self.set(shell, LIBRARY_PATHS, json!(paths))?;
        let mut inactive = shell.preferences().libraries.inactive.clone();
        if inactive.contains(&removed) {
            inactive.retain(|p| *p != removed);
            self.set(shell, LIBRARY_INACTIVE, json!(inactive))?;
        }
        Ok(result)
    }

    /// Turns the Active checkbox of the user Library folder `path` on or off.
    pub fn toggle_library_active(
        &mut self,
        shell: &mut Shell,
        path: &str,
    ) -> Result<Value, CommandError> {
        let mut inactive = shell.preferences().libraries.inactive.clone();
        if inactive.iter().any(|p| p == path) {
            inactive.retain(|p| p != path);
        } else {
            inactive.push(path.to_owned());
        }
        self.set(shell, LIBRARY_INACTIVE, json!(inactive))
    }

    /// A folder picked for the Libraries page.
    pub fn picked_folder(&mut self, shell: &mut Shell, path: &std::path::Path) {
        let path = path.to_string_lossy().replace('\\', "/");
        let _ = self.add_library_path(shell, &path);
    }
}

/// The user Library folders' key.
pub const LIBRARY_PATHS: &str = "libraries.paths";

/// The key listing the user Library folders whose Active checkbox is off.
pub const LIBRARY_INACTIVE: &str = "libraries.inactive";

/// The Starter Library's Active checkbox.
pub const LIBRARY_STARTER_ACTIVE: &str = "libraries.starter_active";

/// The prefix of the Library check severity keys, one per check.
pub const LIBRARY_SEVERITIES: &str = "libraries.severities.";

/// A value as All settings writes it: strings bare, unset blank, the rest as JSON.
fn as_text(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn value(prefs: &Preferences, key: &str) -> Value {
    preferences::value_of(prefs, key)
        .map(|(_, v)| v)
        .unwrap_or(Value::Null)
}
