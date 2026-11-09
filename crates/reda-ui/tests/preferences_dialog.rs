//! The Preferences dialog in the window, driven through AccessKit with `egui_kittest`:
//! opened from the File menu or Mod+Comma, its pages, the theme and UI scale switching
//! live, search, recording a shortcut, the Agents page, and another instance's change
//! reloading here.

use std::path::Path;

use egui::accesskit::Role;
use egui::{Key, Modifiers};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use reda_testkit::TempDir;
use reda_ui::App;
use reda_ui::engine::Engine;
use reda_ui::engine::command::UserDirs;
use reda_ui::engine::state;
use reda_ui::shell::Shell;
use serde_json::json;

fn engine_in(home: &Path) -> Engine {
    Engine::builder()
        .user_dirs(Some(UserDirs::under(home)))
        .starter_library(None)
        .headless(false)
        .build()
}

fn harness(home: &Path) -> Harness<'static, App> {
    let mut h = Harness::builder()
        .with_size([1100.0, 700.0])
        .build_ui_state(
            |ui, app: &mut App| app.ui(ui),
            App::new(Shell::new(engine_in(home))),
        );
    h.run();
    h
}

fn open_preferences(h: &mut Harness<'static, App>) {
    h.get_by_label("File").click();
    h.run();
    // The menu item is drawn after the top row's gear of the same name.
    h.get_all_by_label_contains("Preferences…")
        .last()
        .unwrap()
        .click();
    h.run();
}

fn page(h: &mut Harness<'static, App>, title: &str) {
    h.get_by_role_and_label(Role::TreeItem, title).click();
    h.run();
}

fn last_journal_id(h: &Harness<'static, App>) -> String {
    let journal = h.state().shell().engine().journal();
    journal.last().map(|e| e.id.clone()).unwrap_or_default()
}

fn type_into(h: &mut Harness<'static, App>, role: Role, field: &str, text: &str) {
    h.get_by_role_and_label(role, field).click();
    h.run();
    h.get_by_role_and_label(role, field).type_text(text);
    h.run();
}

#[test]
fn the_dialog_opens_from_the_file_menu_and_lists_every_page() {
    let home = TempDir::new("pref-dialog-pages");
    let mut h = harness(home.path());
    open_preferences(&mut h);
    for title in [
        "General",
        "Editing",
        "Snap",
        "Schematic",
        "Wires",
        "Display",
        "Libraries",
        "Fab Presets",
        "Agents",
        "Keyboard Shortcuts",
        "All settings",
    ] {
        assert!(
            h.query_by_role_and_label(Role::TreeItem, title).is_some(),
            "{title} is listed"
        );
    }
    // Live: there is Close, but no OK, Cancel or Apply.
    assert!(h.query_by_label("OK").is_none());
    assert!(h.query_by_label("Apply").is_none());
    h.get_by_label("Close").click();
    h.run();
    assert!(h.state().preferences_dialog().is_none());
}

#[test]
fn mod_comma_opens_the_dialog() {
    let home = TempDir::new("pref-dialog-key");
    let mut h = harness(home.path());
    h.key_press_modifiers(Modifiers::COMMAND, Key::Comma);
    h.run();
    assert!(h.state().preferences_dialog().is_some());
}

#[test]
fn the_theme_switches_live() {
    let home = TempDir::new("pref-dialog-theme");
    let mut h = harness(home.path());
    open_preferences(&mut h);
    h.get_by_role_and_label(Role::ComboBox, "Theme").click();
    h.run();
    h.get_by_label("Light").click();
    h.run();
    assert_eq!(last_journal_id(&h), "app.preferences.set");
    assert_eq!(
        h.ctx.options(|o| o.theme_preference),
        egui::ThemePreference::Light
    );
    assert!(h.state().preferences_dialog().is_some(), "it stays open");

    h.get_by_role_and_label(Role::Button, "Reset Theme to default")
        .click();
    h.run();
    assert_eq!(
        h.ctx.options(|o| o.theme_preference),
        egui::ThemePreference::System
    );
}

#[test]
fn the_ui_scale_switches_live() {
    let home = TempDir::new("pref-dialog-scale");
    let mut h = harness(home.path());
    open_preferences(&mut h);
    // A list of the steps, 75 to 200 %.
    h.get_by_role_and_label(Role::ComboBox, "UI scale").click();
    h.run();
    for step in ["75 %", "85 %", "100 %", "115 %", "150 %", "200 %"] {
        h.get_by_label(step);
    }
    h.get_by_label("125 %").click();
    h.run();
    h.run();
    assert_eq!(h.state().shell().preferences().general.ui_scale, 1.25);
    assert_eq!(h.ctx.zoom_factor(), 1.25);
}

#[test]
fn search_shows_matching_settings_from_every_page() {
    let home = TempDir::new("pref-dialog-search");
    let mut h = harness(home.path());
    open_preferences(&mut h);
    type_into(&mut h, Role::SearchInput, "Search settings", "electrical");
    assert_eq!(
        h.query_all_by_role_and_label(Role::CheckBox, "Electrical snap")
            .count(),
        2,
        "the Schematic and PCB settings both show"
    );
    // A found setting is changed from the results.
    h.query_all_by_role_and_label(Role::CheckBox, "Electrical snap")
        .last()
        .unwrap()
        .click();
    h.run();
    assert!(!h.state().shell().preferences().pcb.electrical_snap);
}

#[test]
fn a_shortcut_is_recorded_from_the_keyboard_shortcuts_page() {
    let home = TempDir::new("pref-dialog-shortcut");
    let mut h = harness(home.path());
    open_preferences(&mut h);
    page(&mut h, "Keyboard Shortcuts");
    type_into(&mut h, Role::SearchInput, "Search shortcuts", "Save All");
    h.get_by_role_and_label(Role::Button, "Add a key for Save All")
        .click();
    h.run();
    // The key goes to the recording, not to the Keymap: Mod+N would otherwise open New
    // Project.
    h.key_press_modifiers(Modifiers::COMMAND, Key::N);
    h.run();
    assert!(
        h.query_by_label_contains("runs New Project").is_some(),
        "the conflict is flagged"
    );
    assert!(h.state().preferences_dialog().is_some());
    h.get_by_label("Assign").click();
    h.run();
    let file =
        std::fs::read_to_string(UserDirs::under(home.path()).config.join("keymap.toml")).unwrap();
    assert_eq!(
        file,
        "[[bind]]\nkey = \"Mod+N\"\ncommand = \"project.save\"\n"
    );

    h.get_by_role_and_label(Role::Button, "Reset Save All to default")
        .click();
    h.run();
    let file =
        std::fs::read_to_string(UserDirs::under(home.path()).config.join("keymap.toml")).unwrap();
    assert_eq!(file, "");
}

#[test]
fn the_agents_page_lists_and_forgets_allowed_agents() {
    let home = TempDir::new("pref-dialog-agents");
    let dirs = UserDirs::under(home.path());
    state::update(&dirs, |s| s.allowed_agents = vec!["claude-code".into()]).unwrap();
    let mut h = harness(home.path());
    open_preferences(&mut h);
    page(&mut h, "Agents");
    assert!(
        h.query_by_role_and_label(Role::CheckBox, "Let agents connect")
            .is_some()
    );
    h.get_by_role_and_label(Role::Button, "Forget claude-code")
        .click();
    h.run();
    assert!(state::read(&dirs).allowed_agents.is_empty());
}

#[test]
fn another_instance_changing_the_theme_applies_here() {
    let home = TempDir::new("pref-dialog-watch");
    let mut h = harness(home.path());
    let mut other = engine_in(home.path());
    other
        .run(
            "app.preferences.set",
            json!({"key": "general.theme", "value": "dark"}),
        )
        .unwrap();
    h.run();
    assert_eq!(
        h.ctx.options(|o| o.theme_preference),
        egui::ThemePreference::Dark
    );
}

#[test]
fn the_fab_presets_page_renames_and_deletes_user_presets() {
    let home = TempDir::new("prefs-fab");
    let dirs = UserDirs::under(home.path());
    let mut preset = reda_ui::engine::output::shipped().remove(0);
    preset.name = "My JLC".into();
    reda_ui::engine::fab_presets::save_user(&dirs, &preset).unwrap();
    let mut h = harness(home.path());
    open_preferences(&mut h);
    page(&mut h, "Fab Presets");

    let field = "Name of My JLC";
    h.get_by_role_and_label(Role::TextInput, field).click();
    h.run();
    h.key_press_modifiers(Modifiers::COMMAND, Key::A);
    h.get_by_role_and_label(Role::TextInput, field)
        .type_text("JLC Fast");
    h.key_press(Key::Enter);
    h.run();
    assert_eq!(last_journal_id(&h), "fab_preset.rename_user");
    assert!(dirs.config.join("fab-presets/JLC Fast.redafab").exists());

    h.get_by_role_and_label(Role::Button, "Delete JLC Fast")
        .click();
    h.run();
    assert_eq!(last_journal_id(&h), "fab_preset.delete_user");
}

#[test]
fn a_changed_setting_gets_a_dot_and_its_page_a_count() {
    let home = TempDir::new("pref-dialog-dot");
    let mut h = harness(home.path());
    open_preferences(&mut h);
    page(&mut h, "Wires");
    assert!(
        h.query_by_role_and_label(Role::Button, "Reset Optimize Wires to default")
            .is_none(),
        "an unchanged setting has no dot"
    );
    h.get_by_role_and_label(Role::CheckBox, "Optimize Wires")
        .click();
    h.run();
    assert!(!h.state().shell().preferences().schematic.optimize_wires);
    assert!(
        h.query_by_role_and_label(Role::TreeItem, "Wires, 1 changed")
            .is_some()
    );
    h.get_by_role_and_label(Role::Button, "Reset Optimize Wires to default")
        .click();
    h.run();
    assert_eq!(last_journal_id(&h), "app.preferences.reset");
    assert!(h.state().shell().preferences().schematic.optimize_wires);
    assert!(h.query_by_role_and_label(Role::TreeItem, "Wires").is_some());
}

#[test]
fn reset_page_branch_and_all_from_the_window() {
    let home = TempDir::new("pref-dialog-reset-tree");
    let mut h = harness(home.path());
    {
        let shell = h.state_mut().shell_mut();
        for (key, value) in [
            ("schematic.auto_increment.primary", json!(2)),
            ("schematic.optimize_wires", json!(false)),
            ("general.theme", json!("dark")),
        ] {
            shell
                .run("app.preferences.set", json!({"key": key, "value": value}))
                .unwrap();
        }
    }
    h.run();
    open_preferences(&mut h);
    h.get_by_role_and_label(Role::TreeItem, "Schematic, 1 changed")
        .click();
    h.run();
    h.get_by_label("Reset page").click();
    h.run();
    let prefs = h.state().shell().preferences().schematic.clone();
    assert_eq!(prefs.auto_increment.primary, 1);
    assert!(!prefs.optimize_wires, "the sub-page is left");
    h.get_by_label("Reset branch").click();
    h.run();
    assert!(h.state().shell().preferences().schematic.optimize_wires);
    h.get_by_label("Reset all").click();
    h.run();
    assert_eq!(last_journal_id(&h), "app.preferences.reset");
    assert_eq!(
        h.ctx.options(|o| o.theme_preference),
        egui::ThemePreference::System
    );
}

#[test]
fn modified_only_and_all_settings() {
    let home = TempDir::new("pref-dialog-all");
    let mut h = harness(home.path());
    h.state_mut()
        .shell_mut()
        .run(
            "app.preferences.set",
            json!({"key": "pcb.auto_pan", "value": false}),
        )
        .unwrap();
    h.run();
    open_preferences(&mut h);
    h.get_by_role_and_label(Role::CheckBox, "Modified only")
        .click();
    h.run();
    assert!(
        h.query_by_role_and_label(Role::TreeItem, "Display")
            .is_none()
    );
    assert!(
        h.query_by_role_and_label(Role::TreeItem, "Auto-pan, 1 changed")
            .is_some()
    );
    h.get_by_role_and_label(Role::CheckBox, "Modified only")
        .click();
    h.run();

    h.get_by_role_and_label(Role::TreeItem, "All settings, 1 changed")
        .click();
    h.run();
    assert!(h.query_by_label("schematic.single_negation").is_some());
    type_into(&mut h, Role::SearchInput, "Search settings", "pcb.auto");
    // The search filters the table rather than leaving the page.
    assert!(h.query_by_label("schematic.single_negation").is_none());
    assert!(h.query_by_label("pcb.auto_pan").is_some());
    h.get_by_role_and_label(Role::Button, "Reset Auto-pan at the canvas edge to default")
        .click();
    h.run();
    assert!(h.state().shell().preferences().pcb.auto_pan);
}

/// How far `label`'s text starts from the left edge of its page tree row, in points.
fn tree_label_indent(h: &Harness<'static, App>, label: &str) -> f32 {
    let row = h.get_by_role_and_label(Role::TreeItem, label).rect();
    let mut found = None;
    for clipped in &h.output().shapes {
        let mut shapes = vec![&clipped.shape];
        while let Some(shape) = shapes.pop() {
            match shape {
                egui::Shape::Vec(inner) => shapes.extend(inner),
                egui::Shape::Text(text)
                    if text.galley.text() == label && row.contains(text.pos) =>
                {
                    found = Some(text.pos.x - row.left());
                }
                _ => {}
            }
        }
    }
    found.unwrap_or_else(|| panic!("no text {label:?} in its row {row:?}"))
}

#[test]
fn page_tree_labels_are_left_aligned() {
    let home = TempDir::new("prefs-home");
    let mut h = harness(home.path());
    open_preferences(&mut h);
    // Short and long titles start at the row's padding, not centred in the row.
    for title in ["General", "Keyboard Shortcuts", "Schematic"] {
        let indent = tree_label_indent(&h, title);
        assert!(indent < 10.0, "{title:?} starts {indent} pt into its row");
    }
}

#[test]
fn the_libraries_page_has_a_grid_of_library_check_severities_with_reset() {
    let home = TempDir::new("pref-dialog-library-severities");
    let mut h = harness(home.path());
    open_preferences(&mut h);
    page(&mut h, "Libraries");
    let severities = || reda_ui::engine::model::LibrarySeverities::DEFAULT;
    assert_eq!(
        h.state().shell().preferences().libraries.severities,
        severities()
    );
    // One row per check, named by its title, at the spec's default.
    for check in reda_ui::engine::model::LibraryCheck::ALL {
        let title = reda_ui::messages::check_title(check.id());
        assert!(
            h.query_by_role_and_label(Role::ComboBox, &title).is_some(),
            "no row for {title}"
        );
    }
    assert!(
        h.query_by_role_and_label(Role::Label, "Library check severities")
            .is_some()
    );

    h.get_by_role_and_label(Role::ComboBox, "Missing Courtyard")
        .click();
    h.run();
    h.get_all_by_label("Error").last().unwrap().click();
    h.run();
    assert_eq!(last_journal_id(&h), "app.preferences.set");
    let set = h.state().shell().preferences().libraries.severities;
    assert_eq!(
        set.missing_courtyard,
        reda_ui::engine::model::Severity::Error
    );

    h.get_by_role_and_label(Role::ComboBox, "Missing Pin Name")
        .click();
    h.run();
    h.get_all_by_label("Warning").last().unwrap().click();
    h.run();
    assert_eq!(
        h.state()
            .shell()
            .preferences()
            .libraries
            .severities
            .missing_pin_name,
        reda_ui::engine::model::Severity::Warning
    );

    h.get_by_role_and_label(Role::Button, "Reset severities")
        .scroll_to_me();
    h.run();
    h.get_by_role_and_label(Role::Button, "Reset severities")
        .click();
    h.run();
    assert_eq!(last_journal_id(&h), "app.preferences.reset");
    assert_eq!(
        h.state().shell().preferences().libraries.severities,
        severities()
    );
}

#[test]
fn the_libraries_page_has_an_active_checkbox_for_each_user_library_and_the_starter() {
    let home = TempDir::new("pref-dialog-library-active");
    let mut h = harness(home.path());
    open_preferences(&mut h);
    page(&mut h, "Libraries");
    for path in ["/libs/a", "/libs/b"] {
        h.get_by_role_and_label(Role::TextInput, "Library folder")
            .click();
        h.run();
        h.get_by_role_and_label(Role::TextInput, "Library folder")
            .type_text(path);
        h.run();
        h.get_by_role_and_label(Role::Button, "Add").click();
        h.run();
    }
    let libraries = |h: &Harness<'static, App>| h.state().shell().preferences().libraries.clone();
    assert!(libraries(&h).starter_active);
    assert!(libraries(&h).inactive.is_empty());

    h.get_by_role_and_label(Role::CheckBox, "Active: /libs/b")
        .click();
    h.run();
    assert_eq!(last_journal_id(&h), "app.preferences.set");
    assert_eq!(libraries(&h).inactive, ["/libs/b"]);
    h.get_by_role_and_label(Role::CheckBox, "Active: Starter Library")
        .click();
    h.run();
    assert!(!libraries(&h).starter_active);

    // Turning it back on, and removing an inactive folder, leave no stale entry.
    h.get_by_role_and_label(Role::CheckBox, "Active: /libs/b")
        .click();
    h.run();
    assert!(libraries(&h).inactive.is_empty());
    h.get_by_role_and_label(Role::CheckBox, "Active: /libs/a")
        .click();
    h.run();
    assert_eq!(libraries(&h).inactive, ["/libs/a"]);
    h.get_by_role_and_label(Role::Button, "Remove /libs/a")
        .click();
    h.run();
    assert_eq!(libraries(&h).paths, ["/libs/b"]);
    assert!(libraries(&h).inactive.is_empty());
}
