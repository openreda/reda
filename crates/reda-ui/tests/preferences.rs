//! The Preferences dialog's model, driven through the Shell with no window: its pages,
//! changes going through `app.preferences.set` and applying at once, per-setting Reset,
//! search over every setting, and reloading when another instance changes the file.

use std::path::Path;

use reda_testkit::TempDir;
use reda_ui::engine::Engine;
use reda_ui::engine::command::UserDirs;
use reda_ui::engine::preferences::Theme;
use reda_ui::preferences_dialog::{Kind, PageId, PreferencesDialog};
use reda_ui::shell::Shell;
use serde_json::{Value, json};

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
fn the_tree_lists_the_pages_with_settings_and_indents_sub_pages() {
    let tree: Vec<(usize, String)> = PageId::tree()
        .into_iter()
        .map(|p| (p.depth(), p.title()))
        .collect();
    let expect = [
        (0, "General"),
        (0, "Window & Layouts"),
        (0, "Editing"),
        (1, "Snap"),
        (1, "Grid"),
        (1, "Auto-pan"),
        (1, "HUD"),
        (0, "Schematic"),
        (1, "Wires"),
        (1, "Names & Units"),
        (0, "PCB"),
        (1, "Interactive Routing"),
        (1, "Placement"),
        (1, "Zones"),
        (1, "View"),
        (0, "Checks"),
        (0, "Display"),
        (0, "Libraries"),
        (0, "Fab Presets"),
        (0, "Agents"),
        (0, "Keyboard Shortcuts"),
        (0, "All settings"),
    ];
    let expect: Vec<(usize, String)> = expect.iter().map(|(d, t)| (*d, t.to_string())).collect();
    assert_eq!(tree, expect);
}

#[test]
fn every_preference_is_on_a_page() {
    let shown: Vec<String> = PageId::tree()
        .into_iter()
        .filter(|&p| p != PageId::ALL_SETTINGS)
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

/// The last resort: every key of the Preferences schema, read from the schema itself
/// rather than from the Preferences type, is on the generated All settings page, with
/// its value, default and description.
#[test]
fn every_schema_key_appears_on_all_settings() {
    fn schema_keys(schema: &Value, node: &Value, prefix: &str, out: &mut Vec<String>) {
        let node = match node["$ref"]
            .as_str()
            .and_then(|r| r.strip_prefix("#/$defs/"))
        {
            Some(name) => &schema["$defs"][name],
            None => node,
        };
        match node["properties"].as_object() {
            Some(props) => {
                for (name, child) in props {
                    let key = if prefix.is_empty() {
                        name.clone()
                    } else {
                        format!("{prefix}.{name}")
                    };
                    schema_keys(schema, child, &key, out);
                }
            }
            None => out.push(prefix.to_owned()),
        }
    }
    let schema = reda_ui::engine::preferences::schema();
    let mut keys = Vec::new();
    schema_keys(&schema, &schema, "", &mut keys);
    assert!(keys.len() > 40, "{keys:?}");

    let home = TempDir::new("prefs-all-settings");
    let shell = shell_in(home.path());
    let dialog = PreferencesDialog::on(PageId::ALL_SETTINGS);
    let rows = dialog.all_settings(&shell);
    for key in &keys {
        let row = rows.iter().find(|r| &r.setting.key == key);
        let row = row.unwrap_or_else(|| panic!("{key} is not on All settings"));
        assert!(
            !row.setting.description.is_empty(),
            "{key} has no description"
        );
    }
    assert_eq!(rows.len(), keys.len());
    let theme = rows
        .iter()
        .find(|r| r.setting.key == "general.theme")
        .unwrap();
    assert_eq!(
        (theme.value.as_str(), theme.default.as_str()),
        ("follow_os", "follow_os")
    );
}

#[test]
fn all_settings_follows_the_search_by_key_and_modified_only() {
    let home = TempDir::new("prefs-all-search");
    let mut shell = shell_in(home.path());
    let mut dialog = PreferencesDialog::on(PageId::ALL_SETTINGS);
    dialog
        .set(&mut shell, "pcb.auto_pan", json!(false))
        .unwrap();
    dialog.query = "pcb.auto".into();
    let keys = |d: &PreferencesDialog, s: &Shell| -> Vec<String> {
        d.all_settings(s)
            .into_iter()
            .map(|r| r.setting.key)
            .collect()
    };
    assert_eq!(keys(&dialog, &shell), ["pcb.auto_pan"]);
    let row = &dialog.all_settings(&shell)[0];
    assert_eq!(
        (row.value.as_str(), row.default.as_str()),
        ("false", "true")
    );
    dialog.query.clear();
    dialog.modified_only = true;
    assert_eq!(keys(&dialog, &shell), ["pcb.auto_pan"]);
}

#[test]
fn each_page_counts_its_modified_settings_and_modified_only_hides_the_rest() {
    let home = TempDir::new("prefs-modified");
    let mut shell = shell_in(home.path());
    let mut dialog = PreferencesDialog::new();
    dialog
        .set(&mut shell, "schematic.optimize_wires", json!(false))
        .unwrap();
    dialog
        .set(
            &mut shell,
            "schematic.convert_cross_junctions",
            json!(false),
        )
        .unwrap();
    dialog
        .set(&mut shell, "general.theme", json!("dark"))
        .unwrap();
    let wires = PageId::from_id("schematic.wires").unwrap();
    let schematic = PageId::from_id("schematic").unwrap();
    assert_eq!(dialog.modified_count(&shell, wires), 2);
    assert_eq!(dialog.modified_count(&shell, schematic), 0);
    assert_eq!(dialog.modified_count(&shell, PageId::GENERAL), 1);
    assert_eq!(dialog.modified_count(&shell, PageId::ALL_SETTINGS), 3);

    dialog.modified_only = true;
    let titles: Vec<String> = dialog
        .pages(&shell)
        .into_iter()
        .map(|p| p.title())
        .collect();
    // A branch stays listed above its modified sub-page.
    assert_eq!(titles, ["General", "Schematic", "Wires", "All settings"]);
    let rows: Vec<String> = dialog
        .rows(&shell, wires)
        .into_iter()
        .map(|s| s.key)
        .collect();
    assert_eq!(
        rows,
        [
            "schematic.convert_cross_junctions",
            "schematic.optimize_wires"
        ]
    );
    dialog.query = "theme".into();
    let found: Vec<String> = dialog
        .found(&shell)
        .into_iter()
        .map(|(_, s)| s.key)
        .collect();
    assert_eq!(found, ["general.theme"]);
}

#[test]
fn reset_page_branch_and_all_go_through_the_reset_command() {
    let home = TempDir::new("prefs-reset-tree");
    let mut shell = shell_in(home.path());
    let mut dialog = PreferencesDialog::new();
    dialog
        .set(&mut shell, "schematic.repeat_placement", json!(false))
        .unwrap();
    dialog
        .set(&mut shell, "pcb.electrical_snap", json!(false))
        .unwrap();
    dialog
        .set(&mut shell, "general.theme", json!("dark"))
        .unwrap();
    let editing = PageId::from_id("editing").unwrap();

    assert_eq!(
        dialog.reset_page(&mut shell, editing, false).unwrap(),
        ["schematic.repeat_placement"]
    );
    assert_eq!(last_journal_id(&shell), "app.preferences.reset");
    assert!(shell.preferences().schematic.repeat_placement);
    assert!(!shell.preferences().pcb.electrical_snap);

    assert_eq!(
        dialog.reset_page(&mut shell, editing, true).unwrap(),
        ["pcb.electrical_snap"]
    );
    assert!(shell.preferences().pcb.electrical_snap);

    assert_eq!(dialog.reset_all(&mut shell).unwrap(), ["general.theme"]);
    assert_eq!(shell.preferences().general.theme, Theme::FollowOs);
    assert!(!prefs_text(home.path()).contains('='), "no value is left");
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

    // UI scale: one of the steps 75 to 200 %, picked from a list.
    let Kind::Choice(steps) = reda_ui::preferences_dialog::setting("general.ui_scale").kind else {
        panic!("the UI scale is a choice of steps")
    };
    let labels: Vec<&str> = steps.iter().map(|c| c.label.as_str()).collect();
    assert_eq!(
        labels,
        ["75 %", "85 %", "100 %", "115 %", "125 %", "150 %", "200 %"]
    );
    dialog
        .set(&mut shell, "general.ui_scale", serde_json::json!(1.5))
        .unwrap();
    assert_eq!(shell.preferences().general.ui_scale, 1.5);
    assert!(
        dialog
            .set(&mut shell, "general.ui_scale", serde_json::json!(1.1))
            .is_err(),
        "a value between steps is refused"
    );
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
