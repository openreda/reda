//! The Projects panel, driven through AccessKit with `egui_kittest`: the tree with its
//! dirty markers, opening Documents, the item menus, renaming, reordering by drag and
//! deleting (with a confirmation when a sheet holds Components), and the undo stack that
//! focus picks.

use std::path::Path;

use egui::accesskit::Role;
use egui::{Key, Modifiers};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use reda_testkit::TempDir;
use reda_ui::App;
use reda_ui::engine::Engine;
use reda_ui::engine::command::UserDirs;
use reda_ui::engine::model::{DocumentKind, Uuid};
use reda_ui::layout::{PanelId, TabKey};
use reda_ui::shell::Shell;
use serde_json::json;

/// The starter Library's Resistor and its 0603 Footprint.
const RESISTOR: &str = "84d5651a-bf4b-462e-9480-39e8f454a042";
const RESC1608: &str = "0691e5e7-9eb6-49f3-9f3e-d4159258693e";

fn engine_in(home: &Path) -> Engine {
    let starter = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../library/starter");
    Engine::builder()
        .user_dirs(Some(UserDirs::under(home)))
        .starter_library(Some(starter))
        .headless(false)
        .build()
}

/// The window on a new Project named `name`, created in `work`.
fn harness(home: &TempDir, work: &TempDir, name: &str) -> Harness<'static, App> {
    let mut shell = Shell::new(engine_in(home.path()));
    shell.new_project(work.path(), name).unwrap();
    let app = App::new(shell);
    let mut h = Harness::builder()
        .with_size([1100.0, 700.0])
        .build_ui_state(|ui, app: &mut App| app.ui(ui), app);
    h.run();
    h
}

fn shell<'h>(h: &'h mut Harness<'static, App>) -> &'h mut Shell {
    h.state_mut().shell_mut()
}

fn titles(h: &Harness<'static, App>) -> Vec<String> {
    let project = h.state().shell().engine().project().unwrap();
    project.documents().map(|d| d.title.clone()).collect()
}

fn document(h: &Harness<'static, App>, title: &str) -> Uuid {
    let project = h.state().shell().engine().project().unwrap();
    project.documents().find(|d| d.title == title).unwrap().uuid
}

/// The tree item `label` (a Document's tab may carry the same title).
fn row<'h>(h: &'h Harness<'static, App>, label: &'h str) -> egui_kittest::Node<'h> {
    h.get_by_role_and_label(Role::TreeItem, label)
}

/// Opens the menu of `item` (a tree item, else a group heading) and clicks `action`.
fn menu(h: &mut Harness<'static, App>, item: &str, action: &str) {
    match h.query_by_role_and_label(Role::TreeItem, item) {
        Some(node) => node.click_secondary(),
        None => h.get_by_label(item).click_secondary(),
    }
    h.run();
    h.get_by_label(action).click();
    h.run();
}

#[test]
fn the_tree_lists_documents_jobs_and_libraries_with_dirty_markers() {
    let (home, work) = (TempDir::new("pp-home"), TempDir::new("pp-work"));
    let mut h = harness(&home, &work, "Tree");
    // The Project and its PCB Document share the name.
    assert_eq!(h.query_all_by_label("Tree").count(), 2);
    for heading in ["Schematic", "PCB", "Output Jobs", "Libraries"] {
        h.get_by_label(heading);
    }
    row(&h, "Sheet 1");
    row(&h, "Generic");

    shell(&mut h)
        .run("document.add", json!({"kind": "schematic"}))
        .unwrap();
    shell(&mut h)
        .run("project.library.add", json!({"path": "parts"}))
        .unwrap();
    h.run();
    // A new sheet is unsaved, and so is the Project file that lists it.
    h.get_by_label("Sheet 2 *");
    h.get_by_label("Tree *");
    h.get_by_label("parts");
    assert!(h.query_by_label("Sheet 1 *").is_none());

    menu(&mut h, "Tree *", "Save All");
    assert!(h.query_by_label("Sheet 2 *").is_none());
    assert_eq!(h.query_all_by_label("Tree").count(), 2);
}

#[test]
fn double_clicking_a_document_opens_and_focuses_it() {
    let (home, work) = (TempDir::new("pp-home"), TempDir::new("pp-work"));
    let mut h = harness(&home, &work, "Open");
    shell(&mut h)
        .run(
            "document.add",
            json!({"kind": "schematic", "title": "Power"}),
        )
        .unwrap();
    h.run();
    let power = document(&h, "Power");
    assert!(h.state().shell().document_tab(power).is_none());

    // The harness runs each click in its own frames, further apart than a person would.
    h.ctx
        .options_mut(|o| o.input_options.max_double_click_delay = 10.0);
    let power_row = row(&h, "Power *");
    power_row.click();
    power_row.click();
    h.run();
    assert_eq!(
        h.state().shell().layout().focus,
        Some(TabKey::Document(power))
    );
    assert!(h.state().shell().document_tab(power).is_some());
}

#[test]
fn sheets_are_added_renamed_moved_and_deleted_from_their_menus() {
    let (home, work) = (TempDir::new("pp-home"), TempDir::new("pp-work"));
    let mut h = harness(&home, &work, "Menus");

    menu(&mut h, "Schematic", "Add Sheet");
    assert_eq!(titles(&h), ["Sheet 1", "Menus", "Sheet 2"]);

    menu(&mut h, "Sheet 2 *", "Rename");
    let field = h.get_by_role_and_label(Role::TextInput, "New name for Sheet 2");
    field.focus();
    field.set_text_value_accesskit("Power");
    h.run();
    h.key_press(Key::Enter);
    h.run();
    assert_eq!(titles(&h), ["Sheet 1", "Menus", "Power"]);

    // Sheets move among the sheets.
    menu(&mut h, "Power *", "Move Up");
    assert_eq!(titles(&h), ["Power", "Sheet 1", "Menus"]);
    menu(&mut h, "Power *", "Move Down");
    assert_eq!(titles(&h), ["Sheet 1", "Power", "Menus"]);
    row(&h, "Sheet 1").click_secondary();
    h.run();
    assert!(h.get_by_label("Move Up").accesskit_node().is_disabled());
    h.key_press(Key::Escape);
    h.run();

    // An empty sheet goes without asking.
    menu(&mut h, "Power *", "Delete");
    assert_eq!(titles(&h), ["Sheet 1", "Menus"]);
}

#[test]
fn a_sheet_is_reordered_by_dragging_it_onto_another() {
    let (home, work) = (TempDir::new("pp-home"), TempDir::new("pp-work"));
    let mut h = harness(&home, &work, "Drag");
    shell(&mut h)
        .run("document.add", json!({"kind": "schematic"}))
        .unwrap();
    h.run();
    let from = row(&h, "Sheet 2 *").rect().center();
    let to = row(&h, "Sheet 1").rect().center();

    h.drag_at(from);
    h.run();
    h.hover_at(from + egui::vec2(0.0, -4.0));
    h.run();
    h.hover_at(to);
    h.run();
    h.drop_at(to);
    h.run();
    assert_eq!(titles(&h), ["Sheet 2", "Sheet 1", "Drag"]);
}

#[test]
fn deleting_a_sheet_that_holds_components_asks_first() {
    let (home, work) = (TempDir::new("pp-home"), TempDir::new("pp-work"));
    let mut h = harness(&home, &work, "Parts");
    let sheet = document(&h, "Sheet 1");
    shell(&mut h)
        .run(
            "schematic.component.place",
            json!({"document": sheet, "part": RESISTOR, "footprint": RESC1608,
                   "at": {"x": 0, "y": 0}}),
        )
        .unwrap();
    h.run();

    menu(&mut h, "Sheet 1 *", "Delete");
    h.get_by_label("Delete Sheet 1? It holds 1 Component, which is deleted with it.");
    h.get_by_label("Cancel").click();
    h.run();
    assert_eq!(titles(&h), ["Sheet 1", "Parts"]);

    menu(&mut h, "Sheet 1 *", "Delete");
    h.get_by_role_and_label(Role::Button, "Delete Sheet")
        .click();
    h.run();
    assert_eq!(titles(&h), ["Parts"]);
}

#[test]
fn the_pcb_document_is_deleted_and_added_back() {
    let (home, work) = (TempDir::new("pp-home"), TempDir::new("pp-work"));
    let mut h = harness(&home, &work, "Board");
    let pcb = |h: &Harness<'static, App>| {
        let project = h.state().shell().engine().project().unwrap();
        project.documents().any(|d| d.kind == DocumentKind::Pcb)
    };
    // The PCB Document is named after the Project.
    menu(&mut h, "Board", "Delete");
    assert!(!pcb(&h));

    menu(&mut h, "PCB", "Add PCB Document");
    assert!(pcb(&h));
}

#[test]
fn output_jobs_are_added_duplicated_and_deleted() {
    let (home, work) = (TempDir::new("pp-home"), TempDir::new("pp-work"));
    let mut h = harness(&home, &work, "Jobs");
    let jobs = |h: &Harness<'static, App>| {
        let project = h.state().shell().engine().project().unwrap();
        let mut names: Vec<String> = project.output_jobs().map(|j| j.name.clone()).collect();
        names.sort();
        names
    };
    menu(&mut h, "Output Jobs", "New Job");
    assert_eq!(jobs(&h).len(), 2);
    menu(&mut h, "Generic", "Duplicate");
    assert_eq!(jobs(&h).len(), 3);
    menu(&mut h, "Generic", "Delete");
    assert_eq!(jobs(&h).len(), 2);
}

#[test]
fn mod_z_undoes_on_the_stack_focus_picks() {
    let (home, work) = (TempDir::new("pp-home"), TempDir::new("pp-work"));
    let mut h = harness(&home, &work, "Undo");
    let sheet = document(&h, "Sheet 1");
    shell(&mut h)
        .run(
            "schematic.text.add",
            json!({"document": sheet, "text": "note", "at": {"x": 0, "y": 0}}),
        )
        .unwrap();
    menu(&mut h, "Schematic", "Add Sheet");
    assert_eq!(titles(&h).len(), 3);
    let texts = |h: &Harness<'static, App>| {
        let project = h.state().shell().engine().project().unwrap();
        project.schematic_sheet(sheet).unwrap().texts().count()
    };

    // Working in the Projects panel focused it: Mod+Z undoes the Project edit.
    assert_eq!(
        h.state().shell().layout().focus,
        Some(TabKey::Panel(PanelId::Projects))
    );
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run();
    assert_eq!((titles(&h).len(), texts(&h)), (2, 1));
    h.key_press_modifiers(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z);
    h.run();
    assert_eq!(titles(&h).len(), 3);

    // Clicking into the sheet's canvas focuses its tab: Mod+Z undoes on the sheet.
    h.get_by_role_and_label(Role::Canvas, "Schematic canvas: Sheet 1")
        .click();
    h.run();
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run();
    assert_eq!((titles(&h).len(), texts(&h)), (3, 0));
}
