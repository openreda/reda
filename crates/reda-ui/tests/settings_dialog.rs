//! The Project Settings dialog in the window, driven through AccessKit with
//! `egui_kittest`: opened from the Projects panel, its page tree, invalid values flagged
//! inline with OK and Apply disabled, and OK and Apply each committing one `batch`.

use std::path::Path;

use egui::Key;
use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use reda_testkit::TempDir;
use reda_ui::App;
use reda_ui::engine::Engine;
use reda_ui::engine::command::UserDirs;
use reda_ui::engine::model::{ErcCheck, Severity};
use reda_ui::shell::Shell;

fn harness(home: &Path, work: &Path, name: &str) -> Harness<'static, App> {
    let engine = Engine::builder()
        .user_dirs(Some(UserDirs::under(home)))
        .starter_library(None)
        .headless(false)
        .build();
    let mut shell = Shell::new(engine);
    shell.new_project(work, name).unwrap();
    let mut h = Harness::builder()
        .with_size([1100.0, 700.0])
        .build_ui_state(|ui, app: &mut App| app.ui(ui), App::new(shell));
    h.run();
    h
}

/// Opens Project Settings from the Project's menu in the Projects panel. The PCB Document
/// shares the Project's name, so the root is the one that is not a tree item.
fn open_settings(h: &mut Harness<'static, App>, project: &str) {
    h.query_all_by_label(project)
        .find(|n| n.accesskit_node().role() != Role::TreeItem)
        .expect("the Project's row")
        .click_secondary();
    h.run();
    h.get_by_label("Project Settings…").click();
    h.run();
}

fn last_journal_id(h: &Harness<'static, App>) -> String {
    let journal = h.state().shell().engine().journal();
    journal.last().map(|e| e.id.clone()).unwrap_or_default()
}

fn disabled(h: &Harness<'static, App>, label: &str) -> bool {
    h.get_by_role_and_label(Role::Button, label)
        .accesskit_node()
        .is_disabled()
}

fn type_into(h: &mut Harness<'static, App>, field: &str, text: &str) {
    h.get_by_role_and_label(Role::TextInput, field).click();
    h.run();
    h.get_by_role_and_label(Role::TextInput, field)
        .type_text(text);
    h.run();
}

#[test]
fn project_settings_opens_from_the_projects_panel_with_its_page_tree() {
    let (home, work) = (TempDir::new("sd-home"), TempDir::new("sd-work"));
    let mut h = harness(home.path(), work.path(), "Dlg");
    open_settings(&mut h, "Dlg");
    h.get_by_label("Project Settings");
    for page in ["Properties", "ERC", "Fab Presets", "Libraries"] {
        h.get_by_role_and_label(Role::TreeItem, page);
    }
    // Nothing changed yet: OK closes, Apply has nothing to do.
    assert!(!disabled(&h, "OK"));
    assert!(disabled(&h, "Apply"));
    h.get_by_label("Cancel").click();
    h.run();
    assert!(h.query_by_label("Project Settings").is_none());
}

#[test]
fn an_invalid_value_is_flagged_and_disables_ok_and_apply_until_fixed() {
    let (home, work) = (TempDir::new("sd-home"), TempDir::new("sd-work"));
    let mut h = harness(home.path(), work.path(), "Dlg");
    open_settings(&mut h, "Dlg");
    type_into(&mut h, "Title", "Motor driver");
    type_into(&mut h, "Board thickness (mm)", "1.6x");
    h.get_by_label("Not a length. Give mm, such as 1.6, or a unit, such as 63mil.");
    h.get_by_role_and_label(Role::TreeItem, "Properties (has invalid values)");
    assert!(disabled(&h, "OK") && disabled(&h, "Apply"));

    h.key_press(Key::Backspace);
    h.run();
    assert!(!disabled(&h, "OK") && !disabled(&h, "Apply"));
    let journal = h.state().shell().engine().journal().len();
    h.get_by_label("OK").click();
    h.run();
    assert!(h.query_by_label("Project Settings").is_none(), "OK closes");
    assert_eq!(h.state().shell().engine().journal().len(), journal + 1);
    assert_eq!(last_journal_id(&h), "batch");
    let props = h
        .state()
        .shell()
        .engine()
        .project()
        .unwrap()
        .props()
        .clone();
    assert_eq!(props.properties.title, "Motor driver");
    assert_eq!(
        props.properties.board_thickness.map(|t| t.0),
        Some(1_600_000)
    );
}

#[test]
fn apply_on_the_erc_page_commits_and_keeps_the_dialog_open() {
    let (home, work) = (TempDir::new("sd-home"), TempDir::new("sd-work"));
    let mut h = harness(home.path(), work.path(), "Dlg");
    open_settings(&mut h, "Dlg");
    h.get_by_role_and_label(Role::TreeItem, "ERC").click();
    h.run();
    h.get_by_label("Pin-type matrix (fixed): E is a Pin conflict, W is a Mixed pin conflict.");
    h.get_by_label("Single-pin Net: Off").click();
    h.run();
    h.get_by_label("Apply").click();
    h.run();
    assert_eq!(last_journal_id(&h), "batch");
    let erc = h.state().shell().engine().project().unwrap().props().erc;
    assert_eq!(erc.get(ErcCheck::SinglePinNet), Severity::Off);
    h.get_by_label("Project Settings");
    assert!(disabled(&h, "Apply"), "nothing left to apply");
}
