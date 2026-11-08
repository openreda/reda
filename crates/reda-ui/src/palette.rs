//! The Palette (`Mod+P`, or `Mod+Shift+P` on Commands): a fuzzy search, scoped by the
//! query's first character as in VS Code ([`Scope`]), over the open Project's Documents,
//! Components and Nets and every Command and Preset the Shell's registry holds, each
//! Command with its shortcut and, when it cannot run now, the reason.
//!
//! Picking a Command calls it with no params through
//! [`Dialogs::call`](crate::command_dialog::Dialogs::call), so a Command that needs params
//! opens its form. Shortcuts come from a lookup the window is given (the Keymap's); the
//! Palette has none of its own.

mod draw;
mod fuzzy;
mod go_to;
mod rows;

use std::cmp::Reverse;
use std::collections::HashMap;

use crate::bars::{self, Binding, Item};
use crate::i18n;
use crate::shell::Shell;

pub use draw::{Palette, PaletteOutcome, remember_command_field};
pub use fuzzy::score;
pub use go_to::go_to;
pub use rows::{Row, Scope, Sources, Target};

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
    /// Where the menus of the current context have it, such as `Edit › Select Similar…`;
    /// `None` when no menu has it, or several menu items run it with different params.
    pub menu_path: Option<String>,
    /// Why it cannot run now, or `None` when it can.
    pub unavailable: Option<String>,
}

/// Every Command and Preset in `shell`'s registry, as they stand now.
pub fn entries(shell: &Shell, shortcut: Shortcuts<'_>) -> Vec<Entry> {
    let registry = shell.engine().registry();
    let paths = menu_paths(shell);
    let entry = |id: String, title: String, icon: &str, description: &str| {
        // The window is never headless.
        let unavailable = shell.engine().available(&id, false).err();
        Entry {
            shortcut: shortcut(&id),
            menu_path: paths.get(&id).cloned(),
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

/// The menu path of each Command or Preset id in the menus of the active Document's type:
/// the first menu item that runs it as a Command or Preset, or the only one that runs it
/// with fixed params.
fn menu_paths(shell: &Shell) -> HashMap<String, String> {
    fn walk(
        shell: &Shell,
        items: &[Item],
        path: &mut Vec<String>,
        plain: &mut HashMap<String, String>,
        raw: &mut HashMap<String, Vec<String>>,
    ) {
        for item in items {
            match item {
                Item::Separator => {}
                Item::Submenu(menu) => {
                    path.push(i18n::translate(&menu.title, &[]));
                    walk(shell, &menu.items, path, plain, raw);
                    path.pop();
                }
                Item::Action(action) => {
                    let title = bars::resolve(shell, action).title;
                    let full = path.iter().chain([&title]).cloned();
                    let full: Vec<String> = full.collect();
                    // `›`: the UI font has no `▸`.
                    let full = full.join(" › ");
                    match &action.binding {
                        Binding::Command(id) | Binding::Preset(id) => {
                            plain.entry(id.clone()).or_insert(full);
                        }
                        Binding::Raw { command, .. } => {
                            raw.entry(command.clone()).or_default().push(full);
                        }
                    }
                }
            }
        }
    }
    let bars = bars::for_document(bars::active_type(shell));
    let (mut plain, mut raw) = (HashMap::new(), HashMap::new());
    for menu in &bars.menus {
        let mut path = vec![i18n::translate(&menu.title, &[])];
        walk(shell, &menu.items, &mut path, &mut plain, &mut raw);
    }
    for (command, mut paths) in raw {
        if paths.len() == 1 {
            plain.entry(command).or_insert(paths.remove(0));
        }
    }
    plain
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
