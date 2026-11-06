//! The Board Setup dialog in the window, driven through AccessKit with `egui_kittest`:
//! opened from the PCB Document's menu in the Projects panel, its rule pages, Net Classes
//! and DRC pages, and OK committing one `batch`.

use std::path::Path;

use egui::Key;
use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use reda_testkit::TempDir;
use reda_ui::App;
use reda_ui::engine::Engine;
use reda_ui::engine::command::UserDirs;
use reda_ui::engine::model::geom::Nm;
use reda_ui::engine::model::{DrcCheck, PcbDocument, RuleKind, Severity};
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
        .with_size([1100.0, 760.0])
        .build_ui_state(|ui, app: &mut App| app.ui(ui), App::new(shell));
    h.run();
    h
}

/// Opens Board Setup from the PCB Document's menu: its row is the tree item named after
/// the Project.
fn open_board_setup(h: &mut Harness<'static, App>, project: &str) {
    h.get_by_role_and_label(Role::TreeItem, project)
        .click_secondary();
    h.run();
    h.get_by_label("Board Setup…").click();
    h.run();
}

fn pcb(h: &Harness<'static, App>) -> PcbDocument {
    h.state()
        .shell()
        .engine()
        .project()
        .unwrap()
        .pcb()
        .unwrap()
        .clone()
}

fn disabled(h: &Harness<'static, App>, label: &str) -> bool {
    h.get_by_role_and_label(Role::Button, label)
        .accesskit_node()
        .is_disabled()
}

fn page(h: &mut Harness<'static, App>, title: &str) {
    h.get_by_role_and_label(Role::TreeItem, title).click();
    h.run();
}

#[test]
fn board_setup_opens_from_the_pcb_menu_with_a_page_per_rule() {
    let (home, work) = (TempDir::new("bsd-home"), TempDir::new("bsd-work"));
    let mut h = harness(home.path(), work.path(), "Dlg");
    open_board_setup(&mut h, "Dlg");
    h.get_by_label("Board Setup");
    for title in [
        "Clearance",
        "Track Width",
        "Via",
        "Hole Size",
        "Annular Ring",
        "Hole Spacing",
        "Edge Clearance",
        "Net Classes",
        "DRC",
    ] {
        h.get_by_role_and_label(Role::TreeItem, title);
    }
    // The Clearance page: the board-wide value and the check's severity. The Default Net
    // Class's values are the board-wide ones, so it has no override row.
    h.get_by_role_and_label(Role::TextInput, "Board-wide minimum Clearance (mm)");
    assert!(h.query_by_label("Default: Clearance").is_none());
    h.get_by_label("Clearance: Error");
    h.get_by_label("Apply Generic Rules…");
    assert!(disabled(&h, "Apply"));
    h.get_by_label("Cancel").click();
    h.run();
    assert!(h.query_by_label("Board Setup").is_none());
}

#[test]
fn an_invalid_rule_disables_ok_until_fixed_and_ok_commits_one_batch() {
    let (home, work) = (TempDir::new("bsd-home"), TempDir::new("bsd-work"));
    let mut h = harness(home.path(), work.path(), "Dlg");
    open_board_setup(&mut h, "Dlg");
    let field = "Board-wide minimum Clearance (mm)";
    h.get_by_role_and_label(Role::TextInput, field).click();
    h.run();
    h.get_by_role_and_label(Role::TextInput, field)
        .type_text("x");
    h.run();
    h.get_by_role_and_label(Role::TreeItem, "Clearance (has invalid values)");
    assert!(disabled(&h, "OK") && disabled(&h, "Apply"));
    h.key_press(Key::Backspace);
    h.run();
    h.get_by_role_and_label(Role::TextInput, field)
        .type_text("5");
    h.run();

    page(&mut h, "DRC");
    h.get_by_label("Courtyard Overlap: Off").click();
    h.run();
    page(&mut h, "Net Classes");
    h.get_by_label("Add Net Class").click();
    h.run();
    h.get_by_role_and_label(Role::TextInput, "Net Class 1 name");

    let journal = h.state().shell().engine().journal().len();
    h.get_by_label("OK").click();
    h.run();
    assert!(h.query_by_label("Board Setup").is_none(), "OK closes");
    let entries = h.state().shell().engine().journal();
    assert_eq!(entries.len(), journal + 1);
    assert_eq!(entries.last().unwrap().id, "batch");
    let board = pcb(&h);
    assert_eq!(
        board.board_rule(RuleKind::Clearance).values.value,
        Some(Nm::from_um(165))
    );
    assert_eq!(
        board.setup().severities.get(DrcCheck::CourtyardOverlap),
        Severity::Off
    );
    assert!(board.net_class_named("Class 1").is_some());
}
