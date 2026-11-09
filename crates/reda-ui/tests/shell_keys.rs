//! The UI scale keys and View menu, and the Keys in Tool card, in the window, driven with
//! `egui_kittest` (docs/spec/v0.1/app.md#window-shell, #keys-in-tool, #keymap).

use egui::accesskit::Role;
use egui::{Key, Modifiers};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use reda_testkit::TempDir;
use reda_ui::App;
use reda_ui::engine::Engine;
use reda_ui::engine::command::UserDirs;
use reda_ui::shell::Shell;
use serde_json::json;

fn harness(dir: &TempDir, project: bool) -> Harness<'static, App> {
    let engine = Engine::builder()
        .user_dirs(Some(UserDirs::under(&dir.path().join("home"))))
        .starter_library(None)
        .headless(false)
        .build();
    let mut shell = Shell::new(engine);
    if project {
        shell.new_project(dir.path(), "Keys").unwrap();
    }
    Harness::builder()
        .with_size([1200.0, 800.0])
        .build_ui_state(|ui, app: &mut App| app.ui(ui), App::new(shell))
}

fn scale(h: &Harness<'_, App>) -> (f64, f32) {
    (
        h.state().shell().preferences().general.ui_scale,
        h.ctx.zoom_factor(),
    )
}

#[test]
fn mod_equals_minus_and_zero_step_the_ui_scale_preference() {
    let dir = TempDir::new("ui-scale-keys");
    let mut h = harness(&dir, false);
    h.run();
    assert_eq!(scale(&h), (1.0, 1.0));
    h.key_press_modifiers(Modifiers::COMMAND, Key::Equals);
    h.run();
    assert_eq!(scale(&h), (1.15, 1.15));
    h.key_press_modifiers(Modifiers::COMMAND, Key::Equals);
    h.run();
    assert_eq!(scale(&h), (1.25, 1.25));
    h.key_press_modifiers(Modifiers::COMMAND, Key::Minus);
    h.run();
    assert_eq!(scale(&h), (1.15, 1.15));
    h.key_press_modifiers(Modifiers::COMMAND, Key::Num0);
    h.run();
    assert_eq!(scale(&h), (1.0, 1.0));
    // At the bottom step it stays.
    for _ in 0..4 {
        h.key_press_modifiers(Modifiers::COMMAND, Key::Minus);
        h.run();
    }
    assert_eq!(scale(&h), (0.75, 0.75));
}

#[test]
fn the_view_menu_has_ui_scale_with_the_preference_steps() {
    let dir = TempDir::new("ui-scale-menu");
    let mut h = harness(&dir, false);
    h.run();
    h.get_all_by_label("View").last().unwrap().click();
    h.run();
    h.get_by_label("UI Scale").hover();
    h.run();
    for step in [75, 85, 100, 115, 125, 150, 200] {
        h.get_by_label_contains(&format!("UI Scale: {step} %"));
    }
    h.get_by_label_contains("UI Scale: 200 %").click();
    h.run();
    assert_eq!(scale(&h), (2.0, 2.0));
}

#[test]
fn shift_f1_toggles_a_card_of_the_active_tools_keys_from_the_keymap() {
    let dir = TempDir::new("keys-card");
    let mut h = harness(&dir, true);
    h.run();
    h.state_mut()
        .shell_mut()
        .run("tool.activate:wire", json!({}))
        .unwrap();
    h.run();
    assert!(h.query_by_label("Wire keys").is_none());
    h.key_press_modifiers(Modifiers::SHIFT, Key::F1);
    h.run();
    h.get_by_role_and_label(Role::Label, "Wire keys");
    // The Wire Tool's own keys, then the sheet's it leaves in force.
    let card = reda_ui::keys_card::card(h.state().shell(), false);
    assert_eq!(card.title, "Wire keys");
    let rows: Vec<(&str, &str)> = card
        .rows
        .iter()
        .map(|(k, a)| (k.as_str(), a.as_str()))
        .collect();
    assert_eq!(rows[0].0, "/");
    assert_eq!(rows[1].0, "Backspace");
    let del = reda_ui::keymap::Chord::parse("Del").unwrap().label(false);
    assert!(rows.iter().any(|(k, _)| *k == del), "{rows:?}");
    assert!(rows.iter().any(|(k, _)| *k == "W"), "{rows:?}");
    // Each chord once: the Tool's binding wins over the sheet's.
    let mut chords: Vec<&str> = rows.iter().map(|(k, _)| *k).collect();
    chords.sort();
    chords.dedup();
    assert_eq!(chords.len(), rows.len());
    for (chord, action) in &rows {
        assert!(h.query_all_by_label(chord).count() > 0, "{chord} shows");
        assert!(h.query_all_by_label(action).count() > 0, "{action} shows");
    }
    h.key_press_modifiers(Modifiers::SHIFT, Key::F1);
    h.run();
    assert!(h.query_by_label("Wire keys").is_none());
}
