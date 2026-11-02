//! The Preferences dialog (docs/spec/v0.1/app.md#preferences): a modal with a page list, a
//! search box over every setting and "Reset to default" on each. Unlike the Project
//! settings dialogs ([`crate::settings`]) it has no OK, Cancel or undo: Preferences are
//! per-user, so every change runs `app.preferences.set` at once and applies live.
//!
//! The pages and their settings come from the Preferences schema ([`schema`]); the Fab
//! Presets, Agents and Keyboard Shortcuts pages have their own content ([`shortcuts`] for
//! the last). The model here has no drawing, so it is tested without a window; the modal is
//! drawn by [`PreferencesDialog::show`] (in `preferences_dialog/draw.rs`).

use std::collections::BTreeMap;

use reda_engine::command::CommandError;
use reda_engine::preferences::{self, Preferences};
use serde_json::{Value, json};

use crate::shell::Shell;
use crate::tr;

mod draw;
mod schema;
pub mod shortcuts;
mod user_keymap;

pub use draw::Outcome;
pub use schema::{Choice, Kind, Setting, preference_keys, search};
pub use shortcuts::Shortcuts;

/// A page of the dialog.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PageId {
    General,
    Schematic,
    Pcb,
    Display,
    Libraries,
    FabPresets,
    Agents,
    KeyboardShortcuts,
}

impl PageId {
    /// Every page, in the order the list shows them.
    pub const ALL: [PageId; 8] = [
        PageId::General,
        PageId::Schematic,
        PageId::Pcb,
        PageId::Display,
        PageId::Libraries,
        PageId::FabPresets,
        PageId::Agents,
        PageId::KeyboardShortcuts,
    ];

    pub fn title(self) -> String {
        match self {
            PageId::General => tr!("pref-page-general"),
            PageId::Schematic => tr!("pref-page-schematic"),
            PageId::Pcb => tr!("pref-page-pcb"),
            PageId::Display => tr!("pref-page-display"),
            PageId::Libraries => tr!("pref-page-libraries"),
            PageId::FabPresets => tr!("pref-page-fab-presets"),
            PageId::Agents => tr!("pref-page-agents"),
            PageId::KeyboardShortcuts => tr!("pref-page-keyboard-shortcuts"),
        }
    }

    /// The Preferences this page shows from the schema, in order. The Fab Presets and
    /// Keyboard Shortcuts pages show other things.
    pub fn settings(self) -> Vec<Setting> {
        schema::settings_on(self)
    }
}

/// The dialog's state: the page shown, the search, text being typed and the values that
/// were refused.
pub struct PreferencesDialog {
    page: PageId,
    /// The search box over every setting.
    pub query: String,
    /// Text typed into a field and not yet committed, by key.
    texts: BTreeMap<String, String>,
    /// Why the last value committed to a key was refused, by key.
    problems: BTreeMap<String, String>,
    /// A Library folder being typed on the Libraries page.
    pub new_library_path: String,
    /// The Keyboard Shortcuts page.
    pub shortcuts: Shortcuts,
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
            page: PageId::General,
            query: String::new(),
            texts: BTreeMap::new(),
            problems: BTreeMap::new(),
            new_library_path: String::new(),
            shortcuts: Shortcuts::default(),
        }
    }

    /// The dialog on `page`.
    pub fn on(page: PageId) -> PreferencesDialog {
        PreferencesDialog {
            page,
            ..PreferencesDialog::new()
        }
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
        match self.value(shell, key) {
            Value::Null => String::new(),
            Value::String(s) => s,
            other => other.to_string(),
        }
    }

    /// Keeps `text` as what is being typed into `key`'s field.
    pub fn edit_text(&mut self, key: &str, text: String) {
        self.texts.insert(key.to_owned(), text);
    }

    /// Commits the text typed into a Number, Count or Text field: blank clears an
    /// optional value. Text that is not a value of the right kind, or a value the
    /// Preferences refuse, is flagged and kept in the field.
    pub fn commit_text(&mut self, shell: &mut Shell, key: &str, text: &str) {
        let kind = schema::settings_on(page_of(key))
            .into_iter()
            .find(|s| s.key == key)
            .map(|s| s.kind)
            .unwrap_or(Kind::Text);
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
        paths.remove(index);
        self.set(shell, LIBRARY_PATHS, json!(paths))
    }

    /// A folder picked for the Libraries page.
    pub fn picked_folder(&mut self, shell: &mut Shell, path: &std::path::Path) {
        let path = path.to_string_lossy().replace('\\', "/");
        let _ = self.add_library_path(shell, &path);
    }
}

/// The user Library folders' key.
pub const LIBRARY_PATHS: &str = "libraries.paths";

fn page_of(key: &str) -> PageId {
    PageId::ALL
        .into_iter()
        .find(|p| p.settings().iter().any(|s| s.key == key))
        .unwrap_or(PageId::General)
}

fn value(prefs: &Preferences, key: &str) -> Value {
    preferences::value_of(prefs, key)
        .map(|(_, v)| v)
        .unwrap_or(Value::Null)
}
