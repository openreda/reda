//! The Command Palette: a fuzzy search over every Command and Preset in the current
//! context, showing each one's shortcut and why any unavailable one cannot run.

use std::path::{Path, PathBuf};

use reda_testkit::TempDir;
use reda_ui::engine::Engine;
use reda_ui::engine::command::UserDirs;
use reda_ui::keymap::{self, Chord};
use reda_ui::palette::{self, Entry};
use reda_ui::shell::Shell;
use serde_json::json;

fn engine_in(home: &Path) -> Engine {
    Engine::builder()
        .user_dirs(Some(UserDirs::under(home)))
        .starter_library(None)
        .headless(false)
        .build()
}

fn make_project(folder: &Path, name: &str) -> PathBuf {
    let mut engine = Engine::builder()
        .user_dirs(None)
        .starter_library(None)
        .build();
    engine
        .run("project.new", json!({"path": folder, "name": name}))
        .unwrap();
    folder.join(name).join(format!("{name}.reda"))
}

fn no_shortcuts(_: &str) -> Option<String> {
    None
}

fn entry<'e>(entries: &'e [Entry], id: &str) -> &'e Entry {
    entries
        .iter()
        .find(|e| e.id == id)
        .unwrap_or_else(|| panic!("{id} is not in the Palette"))
}

fn ids<'e>(found: &[&'e Entry]) -> Vec<&'e str> {
    found.iter().map(|e| e.id.as_str()).collect()
}

#[test]
fn every_command_and_preset_is_listed_with_its_title() {
    let home = TempDir::new("palette-home");
    let shell = Shell::new(engine_in(home.path()));
    let entries = palette::entries(&shell, &no_shortcuts);
    let registry = shell.engine().registry();
    for meta in registry.commands() {
        let e = entry(&entries, meta.id);
        assert!(!e.title.is_empty(), "{}", meta.id);
    }
    assert_eq!(entry(&entries, "project.save").title, "Save All");
    // Presets, the engine's and the app's, with their own titles and icons.
    let messages = entry(&entries, "panel.show:messages");
    assert_eq!(messages.title, "Show Messages");
    assert_eq!(messages.icon, "panel_messages");
    // A Preset keeps its full title here, where no menu around it says what it is.
    assert_eq!(entry(&entries, "view.theme.set:dark").title, "Theme: Dark");
}

#[test]
fn unavailable_entries_carry_the_reason_and_available_ones_none() {
    let (home, work) = (TempDir::new("palette-home"), TempDir::new("palette-work"));
    let mut shell = Shell::new(engine_in(home.path()));
    let entries = palette::entries(&shell, &no_shortcuts);
    assert_eq!(
        entry(&entries, "project.save").unavailable.as_deref(),
        Some("no Project is open")
    );
    assert_eq!(entry(&entries, "app.quit").unavailable, None);

    shell
        .open_project(&make_project(work.path(), "Demo"))
        .unwrap();
    let entries = palette::entries(&shell, &no_shortcuts);
    assert_eq!(entry(&entries, "project.save").unavailable, None);
}

#[test]
fn entries_show_the_shortcut_the_keymap_gives() {
    let home = TempDir::new("palette-home");
    let shell = Shell::new(engine_in(home.path()));
    let keymap = |id: &str| (id == "project.save").then(|| "Ctrl+S".to_owned());
    let entries = palette::entries(&shell, &keymap);
    assert_eq!(
        entry(&entries, "project.save").shortcut.as_deref(),
        Some("Ctrl+S")
    );
    assert_eq!(entry(&entries, "app.quit").shortcut, None);
}

#[test]
fn the_window_s_lookup_reads_the_keymap() {
    let home = TempDir::new("palette-home");
    let shell = Shell::new(engine_in(home.path()));
    let lookup = |id: &str| keymap::shortcut_of(&shell, id);
    let entries = palette::entries(&shell, &lookup);
    // Labelled as on this OS.
    let shown = |key: &str| Chord::parse(key).unwrap().label(cfg!(target_os = "macos"));
    assert_eq!(
        entry(&entries, "project.save").shortcut,
        Some(shown("Mod+S"))
    );
    assert_eq!(
        entry(&entries, "app.dialog.open:command_palette").shortcut,
        Some(shown("Mod+Shift+P"))
    );
    assert_eq!(entry(&entries, "app.quit").shortcut, None);
}

#[test]
fn the_search_is_fuzzy_on_titles_and_ids_and_ranks_the_closest_first() {
    let home = TempDir::new("palette-home");
    let shell = Shell::new(engine_in(home.path()));
    let entries = palette::entries(&shell, &no_shortcuts);

    // Initials and skipped letters match; the tightest match ranks first.
    let found = palette::search(&entries, "sav al");
    assert_eq!(found[0].id, "project.save");
    let found = palette::search(&entries, "zf");
    assert_eq!(found[0].id, "view.zoom_fit", "{:?}", ids(&found));
    let found = palette::search(&entries, "Show Mess");
    assert_eq!(found[0].id, "panel.show:messages");
    // Ids match too, and case does not matter.
    let found = palette::search(&entries, "PROJECT.SAVE");
    assert_eq!(found[0].id, "project.save");
    // Letters out of order do not match.
    assert!(palette::search(&entries, "xqzj").is_empty());
    // Every hit has every query letter in order in its title or id.
    for e in palette::search(&entries, "drc") {
        assert!(
            in_order("drc", &e.title.to_lowercase()) || in_order("drc", &e.id),
            "{} matched",
            e.id
        );
    }
}

fn in_order(query: &str, text: &str) -> bool {
    let mut chars = text.chars();
    query.chars().all(|q| chars.any(|c| c == q))
}

#[test]
fn an_empty_search_lists_available_entries_before_unavailable_ones() {
    let home = TempDir::new("palette-home");
    let shell = Shell::new(engine_in(home.path()));
    let entries = palette::entries(&shell, &no_shortcuts);
    let found = palette::search(&entries, "");
    assert_eq!(found.len(), entries.len());
    let first_unavailable = found.iter().position(|e| e.unavailable.is_some()).unwrap();
    assert!(
        found[first_unavailable..]
            .iter()
            .all(|e| e.unavailable.is_some())
    );
    // Among equals, by title.
    let titles: Vec<&str> = found[..first_unavailable]
        .iter()
        .map(|e| e.title.as_str())
        .collect();
    let mut sorted = titles.clone();
    sorted.sort_by_key(|t| t.to_lowercase());
    assert_eq!(titles, sorted);
}
