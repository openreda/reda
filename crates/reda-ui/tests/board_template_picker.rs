//! The Board Template picker of New Project and Add PCB Document
//! (docs/spec/v0.2.5/testing.md#picker-ui): draw-list text snapshots of the preview
//! painter, and the dialogs driven through AccessKit with `egui_kittest`, asserting on
//! the one Command each sends in the Journal.

use std::path::{Path, PathBuf};

use egui::accesskit::{Role, Toggled};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use reda_testkit::{TempDir, assert_golden};
use reda_ui::App;
use reda_ui::engine::Engine;
use reda_ui::engine::command::{FixedClock, SeededIds, UserDirs};
use reda_ui::engine::commands::board_template::evaluate_choice;
use reda_ui::engine::library::board_template::BoardTemplateChoice;
use reda_ui::new_project::preview::{Mark, draw_list, to_text};
use reda_ui::shell::Shell;
use serde_json::{Value, json};

const PI: &str = "40f34815-ae6e-4549-bf0e-a54bb293ca59";
const PI_NAME: &str = "Add-on board for Raspberry Pi";
/// The Pi template's GPIO socket row.
const PI_GPIO: &str = "bb81444c-0296-459b-afd4-7bf7f8b60ce4";
const RECTANGLE: &str = "4b5a3569-5fcc-49b3-8af9-5d5fdda27d41";
const PCIE: &str = "4ca6b4ca-11f8-45f1-96c1-c610ece56af9";
const PCIE_NAME: &str = "PCI Express add-in card";

fn starter() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../library/starter")
}

fn engine(home: &Path) -> Engine {
    Engine::builder()
        .ids(SeededIds::new(0))
        .clock(FixedClock::EPOCH)
        .user_dirs(Some(UserDirs::under(home)))
        .starter_library(Some(starter()))
        .headless(false)
        .build()
}

// ---------------------------------------------------------------------------------------
// Draw-list snapshots.

/// The preview's marks for the starter template `element` at `values`, unticking
/// `exclude`, in the dialog's 660 × 300 box.
fn marks(element: &str, values: Value, exclude: &[&str]) -> Vec<Mark> {
    let home = TempDir::new("picker-home");
    let index = engine(home.path()).libraries();
    let choice: BoardTemplateChoice = serde_json::from_value(json!({
        "element": element, "values": values, "exclude": exclude,
    }))
    .unwrap();
    let frame = evaluate_choice(&index, &choice).unwrap();
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(660.0, 300.0));
    draw_list(&frame, rect)
}

fn golden(name: &str, marks: &[Mark]) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(format!("tests/snapshots/board_template_preview_{name}.txt"));
    assert_golden(path, to_text(marks).as_bytes());
}

fn count(marks: &[Mark], line_starts: &str) -> usize {
    to_text(marks)
        .lines()
        .filter(|l| l.starts_with(line_starts))
        .count()
}

#[test]
fn the_preview_draws_the_full_pi_board_with_its_cutouts() {
    let marks = marks(PI, json!({}), &[]);
    // The display notch is part of the outline; the camera slot is the one cutout.
    assert_eq!(count(&marks, "fill cutout"), 1);
    // Four holes, each a drilled pad; the 40 socket pads on the bottom; the body dashed.
    assert_eq!(count(&marks, "fill bottom"), 40);
    assert_eq!(count(&marks, "line bottom closed dashed"), 1);
    assert_eq!(count(&marks, "fill keepout"), 4);
    // Width, height and the hole-to-GPIO datum, measured by the preview.
    let text = to_text(&marks);
    assert!(text.contains("\"65 mm\""), "{text}");
    assert!(text.contains("\"56.5 mm\""), "{text}");
    assert!(text.contains("text drawing") || text.contains("line drawing"));
    golden("pi_full", &marks);
}

#[test]
fn the_preview_draws_the_micro_pi_board() {
    let marks = marks(PI, json!({"size": "micro"}), &[]);
    // Micro has no cutouts, whatever the hidden Cutouts parameter holds.
    assert_eq!(count(&marks, "fill cutout"), 0);
    assert!(to_text(&marks).contains("\"30.5 mm\""));
    golden("pi_micro", &marks);
}

#[test]
fn the_preview_marks_an_invalid_frame() {
    // A corner radius over half a side folds the outline over itself.
    let marks = marks(
        RECTANGLE,
        json!({"width": "100mm", "height": "80mm", "corner_radius": "60mm"}),
        &[],
    );
    assert!(count(&marks, "circle problem") >= 1);
    golden("rectangle_invalid", &marks);
}

#[test]
fn the_preview_draws_pcie_x16_at_the_short_end_of_its_range() {
    // x16's card_length starts where its tab ends: 57.15 + 72.15.
    let marks = marks(PCIE, json!({"lanes": "x16", "card_length": "129.3mm"}), &[]);
    assert_eq!(count(&marks, "circle problem"), 0);
    let text = to_text(&marks);
    assert!(text.contains("\"129.3 mm\""), "{text}");
    // The key-to-bracket datum.
    assert!(text.contains("\"57.15 mm\""), "{text}");
    golden("pcie_x16_short", &marks);
}

#[test]
fn the_dialog_shows_and_checks_the_range_of_the_lanes_chosen() {
    let (home, work) = (TempDir::new("picker-home"), TempDir::new("picker-work"));
    let mut h = harness(Shell::new(engine(home.path())));
    open_new_project(&mut h, work.path());
    click(&mut h, PCIE_NAME);
    h.get_by_label("65.3 – 167.65 mm");
    h.get_by_role_and_label(Role::ComboBox, "Lanes").click();
    h.run();
    h.get_all_by_label("x16").last().unwrap().click();
    h.run();
    h.get_by_label("129.3 – 167.65 mm");
    h.get_by_role_and_label(Role::TextInput, "Card length")
        .set_text_value_accesskit("100");
    h.run();
    h.get_by_label("Card length must be from 129.3 to 167.65 mm.");
    assert!(!create_enabled(&h, "Create"));
    h.get_by_role_and_label(Role::TextInput, "Card length")
        .set_text_value_accesskit("129.3");
    h.run();
    assert!(create_enabled(&h, "Create"));
    click(&mut h, "Create");
    let sent = sent(&h, "project.new");
    assert_eq!(sent.len(), 1);
    assert_eq!(
        sent[0]["board_template"]["values"],
        json!({"lanes": "x16", "bracket": "full", "card_length": "129.3mm"})
    );
}

#[test]
fn the_preview_draws_an_unticked_connector_faint() {
    let marks = marks(PI, json!({}), &[PI_GPIO]);
    assert_eq!(count(&marks, "fill bottom faint"), 40);
    assert_eq!(count(&marks, "line bottom closed dashed faint"), 1);
    // Its designator goes with it.
    assert!(!to_text(&marks).contains("\"J1\""));
    golden("pi_unticked_connector", &marks);
}

// ---------------------------------------------------------------------------------------
// The dialogs.

fn harness(shell: Shell) -> Harness<'static, App> {
    let mut h = Harness::builder()
        .with_size([1400.0, 1200.0])
        .with_max_steps(200)
        .build_ui_state(|ui, app: &mut App| app.ui(ui), App::new(shell));
    h.run();
    h
}

fn click(h: &mut Harness<'_, App>, label: &str) {
    h.get_by_label(label).click();
    h.run();
}

/// Opens New Project and fills its name and location.
fn open_new_project(h: &mut Harness<'_, App>, work: &Path) {
    h.get_all_by_label("New Project…").last().unwrap().click();
    h.run();
    h.get_by_role_and_label(Role::TextInput, "Name")
        .set_text_value_accesskit("Hat");
    h.get_by_role_and_label(Role::TextInput, "Location")
        .set_text_value_accesskit(&work.display().to_string());
    h.run();
}

/// The params of every `id` in the Journal.
fn sent(h: &Harness<'_, App>, id: &str) -> Vec<Value> {
    (h.state().shell().engine().journal().iter())
        .filter(|e| e.id == id)
        .map(|e| e.params.clone())
        .collect()
}

fn create_enabled(h: &Harness<'_, App>, label: &str) -> bool {
    !h.get_by_role_and_label(Role::Button, label)
        .accesskit_node()
        .is_disabled()
}

#[test]
fn no_outline_is_the_default_and_sends_no_template() {
    let (home, work) = (TempDir::new("picker-home"), TempDir::new("picker-work"));
    let mut h = harness(Shell::new(engine(home.path())));
    open_new_project(&mut h, work.path());
    let none = h.get_by_role_and_label(Role::ListItem, "No outline (draw it later)");
    assert_eq!(
        none.accesskit_node().toggled(),
        Some(Toggled::True),
        "No outline is preselected"
    );
    click(&mut h, "Create");
    let sent = sent(&h, "project.new");
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].get("board_template"), None);
    assert_eq!(sent[0]["layers"], json!(2));
}

#[test]
fn create_sends_one_project_new_with_the_changed_parameter_and_unticked_include() {
    let (home, work) = (TempDir::new("picker-home"), TempDir::new("picker-work"));
    let mut h = harness(Shell::new(engine(home.path())));
    open_new_project(&mut h, work.path());
    click(&mut h, PI_NAME);
    // Full size shows the Cutouts choice; micro hides it.
    assert!(h.query_by_label("Cutouts").is_some());
    click(&mut h, "Micro (65 x 30)");
    assert!(h.query_by_label("Cutouts").is_none());
    assert!(h.query_by_label("Display and camera").is_none());
    // The suggestion fills the layer count.
    h.get_by_label("The template suggests 2 copper layers.");
    // Untick the GPIO socket.
    let socket = "J1 Socket_2x20 (bottom)";
    click(&mut h, socket);
    assert_eq!(
        h.get_by_role_and_label(Role::CheckBox, socket)
            .accesskit_node()
            .toggled(),
        Some(Toggled::False)
    );
    assert!(create_enabled(&h, "Create"));
    click(&mut h, "Create");

    let sent = sent(&h, "project.new");
    assert_eq!(sent.len(), 1, "exactly one project.new");
    assert_eq!(
        sent[0]["board_template"],
        json!({"element": PI, "values": {"size": "micro", "gpio": "smt"}, "exclude": [PI_GPIO]})
    );
    // The field holds the suggestion: the Command takes it itself.
    assert_eq!(sent[0].get("layers"), None);
    let project = h.state().shell().engine().project().expect("created");
    assert!(project.pcb().unwrap().outline().is_some());
}

#[test]
fn changing_a_parameter_keeps_the_ticks() {
    let (home, work) = (TempDir::new("picker-home"), TempDir::new("picker-work"));
    let mut h = harness(Shell::new(engine(home.path())));
    open_new_project(&mut h, work.path());
    click(&mut h, PI_NAME);
    click(&mut h, "J1 Socket_2x20 (bottom)");
    click(&mut h, "Micro (65 x 30)");
    click(&mut h, "Full (65 x 56)");
    assert_eq!(
        h.get_by_role_and_label(Role::CheckBox, "J1 Socket_2x20 (bottom)")
            .accesskit_node()
            .toggled(),
        Some(Toggled::False)
    );
}

#[test]
fn an_invalid_frame_disables_create() {
    let (home, work) = (TempDir::new("picker-home"), TempDir::new("picker-work"));
    let mut h = harness(Shell::new(engine(home.path())));
    open_new_project(&mut h, work.path());
    click(&mut h, "Custom rectangle");
    assert!(create_enabled(&h, "Create"));
    h.get_by_role_and_label(Role::TextInput, "Corner radius")
        .set_text_value_accesskit("60");
    h.run();
    assert!(!create_enabled(&h, "Create"));
    h.get_by_label("This board is invalid at these values:");
    // Out of range is refused before the preview runs.
    h.get_by_role_and_label(Role::TextInput, "Corner radius")
        .set_text_value_accesskit("600");
    h.run();
    assert!(!create_enabled(&h, "Create"));
    h.get_by_label("Corner radius must be from 0 to 500 mm.");
    h.get_by_role_and_label(Role::TextInput, "Corner radius")
        .set_text_value_accesskit("5");
    h.run();
    assert!(create_enabled(&h, "Create"));
}

#[test]
fn search_filters_the_list() {
    let (home, work) = (TempDir::new("picker-home"), TempDir::new("picker-work"));
    let mut h = harness(Shell::new(engine(home.path())));
    open_new_project(&mut h, work.path());
    assert!(h.query_by_label("Eurocard").is_some());
    h.get_by_role_and_label(Role::SearchInput, "Search templates")
        .set_text_value_accesskit("express");
    h.run();
    assert!(h.query_by_label("PCI Express add-in card").is_some());
    assert!(h.query_by_label("Eurocard").is_none());
    assert!(h.query_by_label(PI_NAME).is_none());
    // Search reads descriptions and categories too.
    h.get_by_role_and_label(Role::SearchInput, "Search templates")
        .set_text_value_accesskit("arduino");
    h.run();
    assert!(h.query_by_label("Shield for Arduino Uno").is_some());
    assert!(h.query_by_label("Custom rectangle").is_none());
}

/// A Project Library folder `lib` in the Project holding a copy of Custom rectangle
/// named "Project panel", registered with the open Project.
fn project_library(shell: &mut Shell) -> &'static str {
    const ELEMENT: &str = "000000bb-0000-4000-8000-000000000001";
    let folder = shell.engine().location().unwrap().join("lib");
    std::fs::create_dir_all(folder.join("board-templates")).unwrap();
    std::fs::write(
        folder.join("Local.redalib"),
        r#"{"format": 1, "library": {"uuid": "000000bb-0000-4000-8000-000000000000", "name": "Local"}}"#,
    )
    .unwrap();
    let text =
        std::fs::read_to_string(starter().join("board-templates/Custom_rectangle.redaboard"))
            .unwrap()
            .replace(RECTANGLE, ELEMENT)
            .replace("\"Custom rectangle\"", "\"Project panel\"")
            .replace("\"Generic\"", "\"Local\"");
    std::fs::write(folder.join("board-templates/Project_panel.redaboard"), text).unwrap();
    shell
        .run("project.library.add", json!({"path": "lib"}))
        .unwrap();
    ELEMENT
}

#[test]
fn add_pcb_document_lists_project_libraries_and_sends_one_document_add() {
    let (home, work) = (TempDir::new("picker-home"), TempDir::new("picker-work"));
    let mut shell = Shell::new(engine(home.path()));
    shell
        .run("project.new", json!({"path": work.path(), "name": "Panel"}))
        .unwrap();
    let element = project_library(&mut shell);
    let pcb = shell.engine().project().unwrap().pcb().unwrap().uuid;
    shell
        .run("document.delete", json!({"document": pcb}))
        .unwrap();
    let mut h = harness(shell);

    // New Project hides the Project's Libraries.
    (h.state_mut().shell_mut())
        .run("app.dialog.open", json!({"dialog": "new_project"}))
        .unwrap();
    h.run();
    assert!(h.query_by_label("Project panel").is_none());
    click(&mut h, "Cancel");
    assert!(
        h.query_by_role_and_label(Role::TextInput, "Location")
            .is_none(),
        "New Project closed"
    );

    match h.query_by_role_and_label(Role::TreeItem, "PCB") {
        Some(node) => node.click_secondary(),
        None => h.get_by_label("PCB").click_secondary(),
    }
    h.run();
    h.get_all_by_label("Add PCB Document")
        .last()
        .unwrap()
        .click();
    h.run();
    // No Name or Location row.
    assert!(
        h.query_by_role_and_label(Role::TextInput, "Location")
            .is_none()
    );
    // The modal settles its size over a frame or two after it opens.
    h.run();
    h.run();
    click(&mut h, "Project panel");
    h.get_by_role_and_label(Role::TextInput, "Width")
        .set_text_value_accesskit("42.5");
    h.run();
    h.get_by_role_and_label(Role::Button, "Add PCB").click();
    h.run();

    let sent = sent(&h, "document.add");
    assert_eq!(sent.len(), 1);
    assert_eq!(
        sent[0]["board_template"],
        json!({"element": element, "values": {"width": "42.5mm", "height": "80mm", "corner_radius": "0mm"}})
    );
    let pcb = h.state().shell().engine().project().unwrap().pcb().unwrap();
    let xs: Vec<i64> = pcb
        .outline()
        .unwrap()
        .vertices
        .iter()
        .map(|v| v.x.0)
        .collect();
    assert_eq!(xs.iter().max(), Some(&42_500_000));
}
