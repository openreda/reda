//! The Fab Preset pages in the windows, driven through AccessKit with `egui_kittest`: a
//! Project copy's Gerber, Drill, Stackup and CPL pages in Project Settings, and a user
//! preset's pages in Preferences ▸ Fab Presets, written at once.

use std::path::Path;

use egui::accesskit::Role;
use egui::{Key, Modifiers};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use reda_testkit::TempDir;
use reda_ui::App;
use reda_ui::engine::Engine;
use reda_ui::engine::command::UserDirs;
use reda_ui::engine::model::{FileOrigin, ProjectFabPreset, SlotCommand};
use reda_ui::shell::Shell;

fn harness(home: &Path, work: Option<&Path>) -> Harness<'static, App> {
    let engine = Engine::builder()
        .user_dirs(Some(UserDirs::under(home)))
        .starter_library(None)
        .headless(false)
        .build();
    let mut shell = Shell::new(engine);
    if let Some(work) = work {
        shell.new_project(work, "Dlg").unwrap();
    }
    let mut h = Harness::builder()
        .with_size([1200.0, 900.0])
        .build_ui_state(|ui, app: &mut App| app.ui(ui), App::new(shell));
    h.run();
    h
}

fn open_settings(h: &mut Harness<'static, App>) {
    h.query_all_by_label("Dlg")
        .find(|n| n.accesskit_node().role() != Role::TreeItem)
        .expect("the Project's row")
        .click_secondary();
    h.run();
    h.get_by_label("Project Settings…").click();
    h.run();
}

fn page(h: &mut Harness<'static, App>, title: &str) {
    h.get_by_role_and_label(Role::TreeItem, title).click();
    h.run();
}

fn choose(h: &mut Harness<'static, App>, label: &str, option: &str) {
    h.get_by_role_and_label(Role::ComboBox, label).click();
    h.run();
    h.get_all_by_label(option).last().unwrap().click();
    h.run();
}

fn retype(h: &mut Harness<'static, App>, label: &str, text: &str) {
    h.get_by_role_and_label(Role::TextInput, label).click();
    h.run();
    h.key_press_modifiers(Modifiers::COMMAND, Key::A);
    h.run();
    h.get_by_role_and_label(Role::TextInput, label)
        .type_text(text);
    h.run();
}

fn copy(h: &Harness<'static, App>) -> ProjectFabPreset {
    let project = h.state().shell().engine().project().unwrap();
    project.fab_presets().next().unwrap().clone()
}

fn last_journal_id(h: &Harness<'static, App>) -> String {
    let journal = h.state().shell().engine().journal();
    journal.last().map(|e| e.id.clone()).unwrap_or_default()
}

#[test]
fn the_gerber_page_sets_the_file_origin_and_the_drill_and_cpl_pages_show_it() {
    let (home, work) = (TempDir::new("fpw-home"), TempDir::new("fpw-work"));
    let mut h = harness(home.path(), Some(work.path()));
    open_settings(&mut h);
    page(&mut h, "Gerber");
    h.get_by_role_and_label(Role::Button, "Reset Page to Source");
    h.get_by_label_contains("also places the drill and CPL");
    choose(
        &mut h,
        "File origin",
        "Output Origin, else the board's lower-left corner",
    );
    h.get_by_label("Board Outline on every plot").click();
    h.run();

    page(&mut h, "Drill");
    h.get_by_label("Output Origin, else the board's lower-left corner");
    h.get_by_label("set on the Gerber page");
    assert!(
        h.query_by_role_and_label(Role::ComboBox, "File origin")
            .is_none()
    );
    choose(&mut h, "Slot command", "Canned slot (G85)");
    h.get_by_label("Merge plated and non-plated holes into one file")
        .click();
    h.run();
    h.get_by_label("Writes one drill file, <project>.drl.");

    page(&mut h, "CPL");
    h.get_by_label("Output Origin, else the board's lower-left corner");
    assert!(
        h.query_by_role_and_label(Role::ComboBox, "File origin")
            .is_none()
    );

    h.get_by_label("Apply").click();
    h.run();
    assert_eq!(last_journal_id(&h), "batch");
    let g = copy(&h).gerber;
    assert_eq!(g.file_origin, FileOrigin::Relative);
    assert_eq!(g.slot_command, SlotCommand::G85);
    assert!(g.merge_drill && g.outline_on_every_plot);
}

#[test]
fn the_stackup_page_edits_finish_and_colour_suggestions_as_free_text() {
    let (home, work) = (TempDir::new("fpw-home"), TempDir::new("fpw-work"));
    let mut h = harness(home.path(), Some(work.path()));
    open_settings(&mut h);
    page(&mut h, "Stackup");
    h.get_by_label_contains("never checked");
    retype(&mut h, "Finishes", "ENIG, Purple dip");
    retype(&mut h, "Silkscreen colours", "Pink");
    assert!(
        h.query_by_label("Fix the flagged values first.").is_none(),
        "suggestions are never checked"
    );
    h.get_by_label("OK").click();
    h.run();
    let s = copy(&h).stackup_suggestions;
    assert_eq!(s.finish, ["ENIG", "Purple dip"]);
    assert_eq!(s.silk_color, ["Pink"]);
}

#[test]
fn a_user_presets_pages_in_preferences_write_its_file_at_once() {
    let home = TempDir::new("fpw-prefs");
    let dirs = UserDirs::under(home.path());
    let mut preset = reda_ui::engine::output::shipped().remove(1);
    preset.name = "My JLC".into();
    reda_ui::engine::fab_presets::save_user(&dirs, &preset).unwrap();
    let mut h = harness(home.path(), None);
    h.get_by_label("File").click();
    h.run();
    h.get_all_by_label_contains("Preferences…")
        .last()
        .unwrap()
        .click();
    h.run();
    h.get_by_role_and_label(Role::TreeItem, "Fab Presets")
        .click();
    h.run();
    h.get_by_role_and_label(Role::Button, "Edit the pages of My JLC")
        .click();
    h.run();
    h.get_by_label("Drill").click();
    h.run();
    choose(&mut h, "Slot command", "Canned slot (G85)");
    h.run();
    assert_eq!(last_journal_id(&h), "fab_preset.set_user");
    let read = || {
        reda_ui::engine::fab_presets::user_preset(&dirs, "My JLC")
            .unwrap()
            .0
    };
    assert_eq!(read().gerber.slot_command, SlotCommand::G85);

    h.get_by_label("Packaging").click();
    h.run();
    h.get_by_label("Put the BOM and CPL inside the zip").click();
    h.run();
    assert!(read().zip_assembly);
    h.get_by_role_and_label(Role::Button, "Reset Page to Source")
        .click();
    h.run();
    assert!(!read().zip_assembly, "Generic keeps the assembly files out");
    assert_eq!(
        read().gerber.slot_command,
        SlotCommand::G85,
        "only the shown page resets"
    );
}
