//! The Command Palette in the window: `Mod+Shift+P` opens it, typing narrows it, Enter
//! runs the top entry, and a Command that needs params opens its generated form, whose
//! Run button waits until the fields are valid.

use std::path::Path;

use egui::accesskit::Role;
use egui::{Key, Modifiers};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use reda_testkit::TempDir;
use reda_ui::App;
use reda_ui::engine::Engine;
use reda_ui::engine::command::UserDirs;
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
    let app = App::new(Shell::new(engine_in(home)));
    Harness::builder()
        .with_size([900.0, 560.0])
        .build_ui_state(|ui, app: &mut App| app.ui(ui), app)
}

fn last_command(h: &Harness<'_, App>) -> Option<(String, serde_json::Value)> {
    h.state()
        .shell()
        .engine()
        .journal()
        .last()
        .map(|e| (e.id.clone(), e.params.clone()))
}

fn open_palette(h: &mut Harness<'_, App>) {
    h.key_press_modifiers(Modifiers::COMMAND | Modifiers::SHIFT, Key::P);
    h.run();
    assert!(
        h.query_by_role_and_label(Role::TextInput, "Command Palette")
            .is_some(),
        "the Palette did not open"
    );
}

#[test]
fn mod_shift_p_opens_the_palette_and_enter_runs_the_best_match() {
    let home = TempDir::new("palette-home");
    let mut h = harness(home.path());
    h.run();
    open_palette(&mut h);
    h.get_by_role_and_label(Role::TextInput, "Command Palette")
        .type_text("show journal");
    h.run();
    h.key_press(Key::Enter);
    h.run();
    assert_eq!(
        last_command(&h),
        Some(("panel.show:journal".to_owned(), json!({})))
    );
    assert!(
        h.query_by_role_and_label(Role::TextInput, "Command Palette")
            .is_none()
    );
}

#[test]
fn unavailable_entries_show_why_and_escape_closes_the_palette() {
    let home = TempDir::new("palette-home");
    let mut h = harness(home.path());
    h.run();
    open_palette(&mut h);
    h.get_by_role_and_label(Role::TextInput, "Command Palette")
        .type_text("save all");
    h.run();
    assert!(h.query_by_label_contains("no Project is open").is_some());
    // Enter on an entry that cannot run does nothing.
    h.key_press(Key::Enter);
    h.run();
    assert_eq!(last_command(&h), None);
    h.key_press(Key::Escape);
    h.run();
    assert!(
        h.query_by_role_and_label(Role::TextInput, "Command Palette")
            .is_none()
    );
}

#[test]
fn the_palette_preset_opens_the_palette_too() {
    let home = TempDir::new("palette-home");
    let mut h = harness(home.path());
    h.run();
    h.state_mut()
        .shell_mut()
        .run("app.dialog.open:command_palette", json!({}))
        .unwrap();
    h.run();
    h.run();
    assert!(
        h.query_by_role_and_label(Role::TextInput, "Command Palette")
            .is_some()
    );
}

#[test]
fn a_command_that_needs_params_opens_its_form_which_runs_once_the_fields_fit() {
    let home = TempDir::new("palette-home");
    let mut h = harness(home.path());
    h.run();
    open_palette(&mut h);
    h.get_by_role_and_label(Role::TextInput, "Command Palette")
        .type_text("set theme");
    h.run();
    h.key_press(Key::Enter);
    h.run();
    // No params: the form opens instead of a run.
    assert_eq!(last_command(&h), None);
    assert!(h.query_by_label("Set Theme").is_some());
    assert!(h.query_by_label("Required").is_some());
    assert!(h.get_by_label("Run").accesskit_node().is_disabled());

    h.get_by_role_and_label(Role::ComboBox, "Theme").click();
    h.run();
    h.get_by_label("dark").click();
    h.run();
    assert!(h.query_by_label("Required").is_none());
    h.get_by_label("Run").click();
    h.run();
    assert_eq!(
        last_command(&h),
        Some(("view.theme.set".to_owned(), json!({"theme": "dark"})))
    );
    assert!(h.query_by_label("Set Theme").is_none());
}

#[test]
fn a_form_flags_a_bad_value_under_its_field() {
    let home = TempDir::new("palette-home");
    let mut h = harness(home.path());
    h.run();
    open_palette(&mut h);
    h.get_by_role_and_label(Role::TextInput, "Command Palette")
        .type_text("show tab");
    h.run();
    h.key_press(Key::Enter);
    h.run();
    assert!(h.query_by_label("Required").is_some());
    let field = h.get_by_role_and_label(Role::TextInput, "Tab");
    field.focus();
    field.type_text("nowhere");
    h.run();
    h.get_by_label("Run").click();
    h.run();
    // The layout refuses an unknown tab: the form stays open and says why.
    assert!(h.query_by_label("Run").is_some());
    // Once in the form, once in the status bar.
    assert_eq!(h.query_all_by_label_contains("is not a tab").count(), 2);
    h.get_by_label("Cancel").click();
    h.run();
    assert!(h.query_by_label("Run").is_none());
}
