//! The Command Palette (`Mod+Shift+P`): a fuzzy search over every Command and Preset the
//! Shell's registry holds, each with its shortcut and, when it cannot run now, the reason.
//!
//! Picking an entry calls it with no params through
//! [`Dialogs::call`](crate::command_dialog::Dialogs::call), so a Command that needs params
//! opens its form. Shortcuts come from a lookup the window is given (the Keymap's); the
//! Palette has none of its own.

mod draw;
mod fuzzy;

use std::cmp::Reverse;

use crate::bars;
use crate::shell::Shell;

pub use draw::{Palette, PaletteOutcome};
pub use fuzzy::score;

/// The shortcut of a Command or Preset id, as the Keymap shows it, if it has one.
pub type Shortcuts<'a> = &'a dyn Fn(&str) -> Option<String>;

/// An owned shortcut lookup, as the window keeps it: the shortcut of a Command or Preset
/// id in what is active in the Shell now ([`keymap::shortcut_of`](crate::keymap::shortcut_of)).
pub type ShortcutLookup = Box<dyn Fn(&Shell, &str) -> Option<String>>;

/// One Command or Preset in the Palette.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    /// What to run: a Command id or `<command>:<preset>`.
    pub id: String,
    pub title: String,
    pub icon: String,
    pub description: String,
    pub shortcut: Option<String>,
    /// Why it cannot run now, or `None` when it can.
    pub unavailable: Option<String>,
}

/// Every Command and Preset in `shell`'s registry, as they stand now.
pub fn entries(shell: &Shell, shortcut: Shortcuts<'_>) -> Vec<Entry> {
    let registry = shell.engine().registry();
    let project = shell.engine().project();
    let entry = |id: String, title: String, icon: &str, description: &str| {
        // The window is never headless.
        let unavailable = registry.available(&id, project, false).err();
        Entry {
            shortcut: shortcut(&id),
            unavailable: unavailable.map(|e| e.message),
            title,
            icon: icon.to_owned(),
            description: description.to_owned(),
            id,
        }
    };
    let mut out = Vec::new();
    for meta in registry.commands() {
        out.push(entry(
            meta.id.to_owned(),
            bars::command_title(meta.id, meta.title),
            meta.icon,
            meta.description,
        ));
        let described = registry.describe(meta.id).unwrap_or_default();
        for preset in described["presets"].as_array().into_iter().flatten() {
            let (Some(id), Some(title)) = (preset["id"].as_str(), preset["title"].as_str()) else {
                continue;
            };
            let icon = preset["icon"].as_str().unwrap_or(meta.icon);
            out.push(entry(
                id.to_owned(),
                title.to_owned(),
                icon,
                meta.description,
            ));
        }
    }
    out
}

/// The entries that match `query`, best first: by match score, then those that can run,
/// then by title.
pub fn search<'e>(entries: &'e [Entry], query: &str) -> Vec<&'e Entry> {
    let mut found: Vec<(i32, &Entry)> = entries
        .iter()
        .filter_map(|e| {
            let best = score(query, &e.title).max(score(query, &e.id))?;
            Some((best, e))
        })
        .collect();
    found.sort_by_cached_key(|(s, e)| {
        (Reverse(*s), e.unavailable.is_some(), e.title.to_lowercase())
    });
    found.into_iter().map(|(_, e)| e).collect()
}
