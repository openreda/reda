//! The Preferences window's Mouse Wheel page (docs/spec/v0.3/input-and-defaults.md#wheel-mapping,
//! testing.md "Mouse Wheel page"), driven through AccessKit with `egui_kittest`: a
//! clashing combination is refused naming the action that holds it, and choosing a
//! preset rewrites the table, which reads Custom once edited.

use std::path::Path;

use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use reda_testkit::TempDir;
use reda_ui::App;
use reda_ui::engine::Engine;
use reda_ui::engine::command::UserDirs;
use reda_ui::engine::preferences::{Wheel, WheelBinding, WheelPreset};
use reda_ui::preferences_dialog::PageId;
use reda_ui::shell::Shell;

fn harness(home: &Path) -> Harness<'static, App> {
    let engine = Engine::builder()
        .user_dirs(Some(UserDirs::under(home)))
        .starter_library(None)
        .headless(false)
        .build();
    let mut h = Harness::builder()
        .with_size([1100.0, 760.0])
        .build_ui_state(|ui, app: &mut App| app.ui(ui), App::new(Shell::new(engine)));
    h.run();
    h.state_mut().open_preferences(Some(PageId::MOUSE_WHEEL));
    h.run();
    h
}

fn wheel(h: &Harness<'static, App>) -> Wheel {
    h.state().shell().preferences().canvas.wheel.clone()
}

fn journal_len(h: &Harness<'static, App>) -> usize {
    h.state().shell().engine().journal().len()
}

/// What the dropdown `combo` shows.
fn shown(h: &Harness<'static, App>, combo: &str) -> String {
    let node = h.get_by_role_and_label(Role::ComboBox, combo);
    node.accesskit_node().value().unwrap_or_default().to_owned()
}

/// Opens the dropdown `combo` and picks `option` from it.
fn choose(h: &mut Harness<'static, App>, combo: &str, option: &str) {
    h.get_by_role_and_label(Role::ComboBox, combo).click();
    h.run();
    h.get_all_by_label(option).last().unwrap().click();
    h.run();
    h.run();
}

#[test]
fn the_page_is_listed_under_editing_with_every_wheel_setting() {
    let home = TempDir::new("wheel-page-rows");
    let h = harness(home.path());
    assert!(
        h.query_by_role_and_label(Role::TreeItem, "Mouse Wheel")
            .is_some()
    );
    for combo in [
        "Preset",
        "Zoom at the cursor",
        "Scroll sideways",
        "Scroll up and down",
        "Step Layer",
        "Touchpad",
    ] {
        assert!(
            h.query_by_role_and_label(Role::ComboBox, combo).is_some(),
            "no {combo}"
        );
    }
    assert!(
        h.query_by_role_and_label(Role::TextInput, "Zoom speed")
            .is_some()
    );
    assert!(
        h.query_by_role_and_label(Role::CheckBox, "Reverse zoom direction")
            .is_some()
    );
    assert_eq!(shown(&h, "Preset"), "REDA");
    assert_eq!(shown(&h, "Zoom at the cursor"), "Plain wheel");
    assert_eq!(shown(&h, "Scroll sideways"), "Shift");
    assert_eq!(shown(&h, "Scroll up and down"), "Ctrl");
    assert_eq!(shown(&h, "Step Layer"), "Unbound");
    assert!(h.query_by_label("PCB editor").is_some());
}

#[test]
fn a_clashing_combination_is_refused_naming_the_action_that_holds_it() {
    let home = TempDir::new("wheel-page-clash");
    let mut h = harness(home.path());
    let before = journal_len(&h);
    choose(&mut h, "Scroll up and down", "Shift");
    assert_eq!(wheel(&h), Wheel::default(), "nothing is written");
    assert_eq!(journal_len(&h), before);
    assert!(
        h.query_by_label_contains("Shift is held by Scroll sideways")
            .is_some(),
        "the refusal names the holder"
    );
    // A free combination is taken, through app.preferences.set.
    choose(&mut h, "Step Layer", "Alt");
    assert_eq!(wheel(&h).step_layer, WheelBinding::parse("alt").unwrap());
    let journal = h.state().shell().engine().journal();
    assert_eq!(journal.last().unwrap().id, "app.preferences.set");
}

#[test]
fn choosing_a_preset_rewrites_the_table_and_an_edited_table_reads_custom() {
    let home = TempDir::new("wheel-page-preset");
    let mut h = harness(home.path());
    choose(&mut h, "Preset", "Wheel scrolls");
    let now = wheel(&h);
    assert_eq!(now.preset(), Some(WheelPreset::WheelScrolls));
    assert_eq!(now.scroll_y, WheelBinding::PLAIN);
    assert_eq!(now.zoom, WheelBinding::CTRL);
    assert_eq!(shown(&h, "Preset"), "Wheel scrolls");
    assert_eq!(shown(&h, "Scroll up and down"), "Plain wheel");

    choose(&mut h, "Step Layer", "Alt");
    assert_eq!(wheel(&h).preset(), None);
    assert_eq!(shown(&h, "Preset"), "Custom", "an edited table");
    let file = std::fs::read_to_string(UserDirs::under(home.path()).preferences_file()).unwrap();
    assert!(!file.contains("preset"), "{file}");

    choose(&mut h, "Preset", "REDA");
    let back = wheel(&h);
    assert_eq!(back.preset(), Some(WheelPreset::Reda));
    // A preset is the bindings: the Step Layer binding goes with it.
    assert_eq!(back.step_layer, WheelBinding::Unbound);
}

#[test]
fn a_reset_dot_that_would_clash_is_refused_naming_the_holder() {
    let home = TempDir::new("wheel-page-reset");
    let mut h = harness(home.path());
    choose(&mut h, "Zoom at the cursor", "Alt");
    choose(&mut h, "Scroll up and down", "Plain wheel");
    let before = wheel(&h);
    h.get_by_label("Reset Zoom at the cursor to default")
        .click();
    h.run();
    assert_eq!(wheel(&h), before);
    assert!(
        h.query_by_label_contains("Plain wheel is held by Scroll up and down")
            .is_some()
    );
}
