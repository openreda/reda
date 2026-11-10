//! The Shell's Preferences and keymap: reading them, reloading them when their files
//! change, their warnings, and the UI scale.

use super::*;

impl Shell {
    /// The Keymap in force: the base Keymap the Preferences pick, with the user's
    /// `keymap.toml` laid over it.
    pub fn keymap(&self) -> &Keymap {
        &self.keymap
    }

    /// Reads the Keymap again (after `keymap.toml` or `keymap.base` changed) and lists its
    /// problems in Messages, replacing the earlier ones.
    pub fn reload_keymap(&mut self) {
        let dirs = self.engine.user_dirs();
        let base = preferences::current(dirs).keymap.base;
        let mut problems = Vec::new();
        let user = dirs.and_then(|d| {
            let path = d.config.join(keymap::USER_FILE);
            match std::fs::read_to_string(&path) {
                Ok(text) => Some(text),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
                Err(e) => {
                    problems.push(tr!(
                        "keymap-unreadable",
                        file = keymap::USER_FILE,
                        error = e.to_string()
                    ));
                    None
                }
            }
        });
        let (mut map, found) = Keymap::load(base, user.as_deref());
        problems.extend(found);
        problems.extend(map.check(self.engine.registry()));
        self.keymap = map;
        let problems = problems
            .into_iter()
            .map(|text| (Severity::Error, text))
            .collect();
        self.set_load_messages(KEYMAP, problems);
    }

    /// The held modifier of each mouse gesture role: the Keymap's `[modifiers]` tables.
    pub fn modifier_roles(&self) -> keymap::ModifierRoles {
        *self.keymap.modifiers()
    }

    /// How strongly dimmed and masked objects fade, each 0..1: this session's `[` / `]`
    /// strengths, else the shared highlight Preference's.
    pub fn highlight_strengths(&self) -> (f64, f64) {
        let session = (self.engine.view())
            .get::<crate::highlight::SessionStrengths>()
            .copied()
            .unwrap_or_default();
        session.over(&self.preferences().editing.highlight)
    }

    /// The Preferences as last read: after this window's own changes at once, and after
    /// another instance's once [`Shell::poll_preferences`] notices them.
    pub fn preferences(&self) -> &Preferences {
        self.preferences.values()
    }

    /// Moves every time [`Shell::preferences`] change, so the window applies the theme and
    /// UI scale once per change.
    pub fn preferences_generation(&self) -> u64 {
        self.preferences.generation()
    }

    /// Reads `preferences.toml` again if it changed on disk (another instance, or a hand
    /// edit) and applies what changed. Returns whether the Preferences changed.
    pub fn poll_preferences(&mut self) -> bool {
        match self.engine.user_dirs() {
            Some(dirs) if self.preferences.changed_on_disk(dirs) => {
                let changed = self.reload_preferences();
                if changed {
                    // Layer colours and the Inactive-Layer mode are in the Scenes.
                    self.refresh();
                    self.sync_selection();
                }
                changed
            }
            _ => false,
        }
    }

    /// Reads `preferences.toml` again and applies what changed: the undo depth, the base
    /// Keymap, the Layer colours and the warnings in Messages. Returns whether the
    /// Preferences changed.
    pub(super) fn reload_preferences(&mut self) -> bool {
        let Some(dirs) = self.engine.user_dirs().cloned() else {
            return false;
        };
        let Some(reloaded) = self.preferences.reload(&dirs) else {
            return false;
        };
        let now = self.preferences.values().clone();
        self.sync_ui_scale();
        if now.general.undo_depth != reloaded.before.general.undo_depth {
            (self.engine).set_undo_depth(now.general.undo_depth.map(|d| d.get() as usize));
        }
        if now.keymap != reloaded.before.keymap {
            self.reload_keymap();
        }
        self.generation += 1;
        self.palette = pcb::view::palette_handle(&now, self.generation);
        self.set_preference_warnings(reloaded.warnings);
        if now.checks != reloaded.before.checks {
            self.update_markers();
        }
        true
    }

    /// Lists the warnings reading `preferences.toml` gave.
    pub(super) fn load_preference_warnings(&mut self) {
        let (watched, warnings) = Watched::load(self.engine.user_dirs());
        self.preferences = watched;
        self.sync_ui_scale();
        if self.engine.user_dirs().is_some() {
            self.set_preference_warnings(warnings);
        }
    }

    /// Keeps the UI scale view state, which `view.ui_scale.step` steps from, equal to the
    /// Preference.
    pub(super) fn sync_ui_scale(&mut self) {
        let scale = self.preferences.values().general.ui_scale;
        self.engine
            .view_mut()
            .set(crate::bars::commands::UiScale(scale));
    }

    pub(super) fn set_preference_warnings(&mut self, warnings: Vec<String>) {
        let notes = warnings
            .into_iter()
            .map(|w| Note {
                source: PREFERENCES.to_owned(),
                document: None,
                severity: Severity::Warning,
                text: tr!("message-preferences", warning = w),
            })
            .collect();
        self.messages.set_notes(PREFERENCES, notes);
    }
}
