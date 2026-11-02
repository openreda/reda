//! The Preferences dialog's model, driven through the Shell with no window: its pages,
//! changes going through `app.preferences.set` and applying at once, per-setting Reset,
//! search over every setting, and reloading when another instance changes the file.

use std::path::Path;

use reda_testkit::TempDir;
use reda_ui::engine::Engine;
use reda_ui::engine::command::UserDirs;
use reda_ui::engine::preferences::Theme;
use reda_ui::preferences_dialog::{PageId, PreferencesDialog};
use reda_ui::shell::Shell;
use serde_json::json;

fn shell_in(home: &Path) -> Shell {
    let engine = Engine::builder()
        .user_dirs(Some(UserDirs::under(home)))
        .starter_library(None)
        .headless(false)
        .build();
    Shell::new(engine)
}

fn prefs_text(home: &Path) -> String {
    std::fs::read_to_string(UserDirs::under(home).preferences_file()).unwrap_or_default()
}

fn last_journal_id(shell: &Shell) -> String {
    let journal = shell.engine().journal();
    journal.last().map(|e| e.id.clone()).unwrap_or_default()
}

#[test]
fn the_dialog_has_every_v01_page() {
    let titles: Vec<String> = PageId::ALL.iter().map(|p| p.title()).collect();
    assert_eq!(
        titles,
        [
            "General",
            "Schematic",
            "PCB",
            "Display",
            "Libraries",
            "Fab Presets",
            "Agents",
            "Keyboard Shortcuts"
        ]
    );
}

#[test]
fn every_preference_is_on_a_page() {
    let shown: Vec<String> = PageId::ALL
        .iter()
        .flat_map(|p| p.settings())
        .map(|s| s.key)
        .collect();
    for key in reda_ui::preferences_dialog::preference_keys() {
        // The base Keymap has only one choice in v0.1, so it stays hidden.
        if key == "keymap.base" {
            continue;
        }
        assert!(shown.contains(&key), "{key} is on no page");
    }
}

#[test]
fn a_change_goes_through_the_set_command_and_applies_at_once() {
    let home = TempDir::new("prefs-dialog-set");
    let mut shell = shell_in(home.path());
    let mut dialog = PreferencesDialog::new();
    dialog
        .set(&mut shell, "schematic.repeat_placement", json!(false))
        .unwrap();
    assert_eq!(last_journal_id(&shell), "app.preferences.set");
    assert!(!shell.preferences().schematic.repeat_placement);
    assert_eq!(
        prefs_text(home.path()),
        "[schematic]\nrepeat_placement = false\n"
    );

    dialog
        .set(&mut shell, "general.theme", json!("light"))
        .unwrap();
    assert_eq!(shell.preferences().general.theme, Theme::Light);
}

#[test]
fn reset_puts_a_setting_back_to_its_default() {
    let home = TempDir::new("prefs-dialog-reset");
    let mut shell = shell_in(home.path());
    let mut dialog = PreferencesDialog::new();
    dialog
        .set(&mut shell, "pcb.auto_pan", json!(false))
        .unwrap();
    assert!(dialog.can_reset(&shell, "pcb.auto_pan"));
    assert!(!dialog.can_reset(&shell, "pcb.electrical_snap"));
    dialog.reset(&mut shell, "pcb.auto_pan").unwrap();
    assert!(shell.preferences().pcb.auto_pan);
    assert!(!prefs_text(home.path()).contains("auto_pan"));
    assert!(!dialog.can_reset(&shell, "pcb.auto_pan"));
}

#[test]
fn typed_values_apply_when_valid_and_are_flagged_when_not() {
    let home = TempDir::new("prefs-dialog-typed");
    let mut shell = shell_in(home.path());
    let mut dialog = PreferencesDialog::new();

    // Undo depth: a count, blank for unlimited. It caps the undo stacks at once.
    dialog.commit_text(&mut shell, "general.undo_depth", "40");
    assert_eq!(shell.engine().history().depth(), Some(40));
    assert_eq!(dialog.problem("general.undo_depth"), None);
    dialog.commit_text(&mut shell, "general.undo_depth", "0");
    assert!(dialog.problem("general.undo_depth").is_some());
    assert_eq!(shell.engine().history().depth(), Some(40));
    dialog.commit_text(&mut shell, "general.undo_depth", "");
    assert_eq!(dialog.problem("general.undo_depth"), None);
    assert_eq!(shell.engine().history().depth(), None);

    // UI scale: a number from 0.5 to 3.
    dialog.commit_text(&mut shell, "general.ui_scale", "1.5");
    assert_eq!(shell.preferences().general.ui_scale, 1.5);
    dialog.commit_text(&mut shell, "general.ui_scale", "9");
    assert!(dialog.problem("general.ui_scale").is_some());
    dialog.commit_text(&mut shell, "general.ui_scale", "big");
    assert!(dialog.problem("general.ui_scale").is_some());
    assert_eq!(shell.preferences().general.ui_scale, 1.5);
}

#[test]
fn search_finds_settings_on_every_page_by_name_and_description() {
    let found = |query: &str| -> Vec<String> {
        reda_ui::preferences_dialog::search(query)
            .into_iter()
            .map(|(_, s)| s.key)
            .collect()
    };
    assert_eq!(
        found("electrical"),
        ["schematic.electrical_snap", "pcb.electrical_snap"]
    );
    assert_eq!(found("COLOUR theme"), ["general.theme"]);
    // By description: "Pan the view when the cursor reaches the canvas edge…".
    assert!(found("canvas edge").contains(&"pcb.auto_pan".to_owned()));
    assert!(found("").is_empty());
    assert!(found("no such setting").is_empty());
}

#[test]
fn library_paths_are_added_and_removed() {
    let home = TempDir::new("prefs-dialog-libraries");
    let mut shell = shell_in(home.path());
    let mut dialog = PreferencesDialog::new();
    dialog.add_library_path(&mut shell, "/libs/a").unwrap();
    dialog.add_library_path(&mut shell, "/libs/b").unwrap();
    assert_eq!(shell.preferences().libraries.paths, ["/libs/a", "/libs/b"]);
    dialog.remove_library_path(&mut shell, 0).unwrap();
    assert_eq!(shell.preferences().libraries.paths, ["/libs/b"]);
}

#[test]
fn another_instance_changing_the_file_is_picked_up() {
    let home = TempDir::new("prefs-dialog-watch");
    let mut shell = shell_in(home.path());
    let before = shell.preferences_generation();
    assert!(!shell.poll_preferences());

    // Another instance writes the file.
    let mut other = shell_in(home.path());
    PreferencesDialog::new()
        .set(&mut other, "general.undo_depth", json!(7))
        .unwrap();

    assert!(shell.poll_preferences());
    assert_eq!(
        shell.preferences().general.undo_depth.map(|d| d.get()),
        Some(7)
    );
    assert_eq!(shell.engine().history().depth(), Some(7));
    assert_ne!(shell.preferences_generation(), before);
    assert!(!shell.poll_preferences());
}
