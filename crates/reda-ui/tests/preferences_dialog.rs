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
        "Schematic",
        "PCB",
        "Display",
        "Libraries",
        "Fab Presets",
        "Agents",
        "Keyboard Shortcuts",
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
    h.get_by_role_and_label(Role::TextInput, "UI scale").click();
    h.run();
    // Select all and replace: Mod+A, then type.
    h.key_press_modifiers(Modifiers::COMMAND, Key::A);
    h.run();
    h.get_by_role_and_label(Role::TextInput, "UI scale")
        .type_text("1.25");
    h.run();
    h.key_press(Key::Enter);
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
