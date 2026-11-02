//! The Library panel in the window, driven through AccessKit with `egui_kittest`: what it
//! lists on a sheet and on the board, search and filters, the previews, and its buttons,
//! which start the placement Tool and run the Library element Commands.

use std::path::{Path, PathBuf};

use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use reda_testkit::TempDir;
use reda_ui::App;
use reda_ui::engine::Engine;
use reda_ui::engine::command::{FixedClock, SeededIds, UserDirs};
use reda_ui::shell::Shell;
use reda_ui::tools::Tool;
use serde_json::{Value, json};

const SHEET: &str = "00000000-0000-4000-8000-000000000002";
/// The starter Library's Resistor and MountingHole_M3.
const RESISTOR: &str = "84d5651a-bf4b-462e-9480-39e8f454a042";
const RESISTOR_SYMBOL: &str = "06c71621-e7e4-49a4-9b90-88485bb936b5";

fn starter() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../library/starter")
}

fn harness(home: &TempDir, work: &TempDir) -> Harness<'static, App> {
    let engine = Engine::builder()
        .ids(SeededIds::new(0))
        .clock(FixedClock::EPOCH)
        .user_dirs(Some(UserDirs::under(home.path())))
        .starter_library(Some(starter()))
        .headless(false)
        .build();
    let mut shell = Shell::new(engine);
    shell.new_project(work.path(), "Demo").unwrap();
    shell
        .run("document.open", json!({"document": SHEET}))
        .unwrap();
    shell
        .run("panel.show", json!({"panel": "library"}))
        .unwrap();
    let mut h = Harness::builder()
        .with_size([1400.0, 1000.0])
        .with_max_steps(200)
        .build_ui_state(|ui, app: &mut App| app.ui(ui), App::new(shell));
    h.run();
    h
}

fn run(h: &mut Harness<'_, App>, id: &str, params: Value) -> Value {
    h.state_mut().shell_mut().run(id, params).unwrap()
}

/// Types `text` into the search box (after what is there).
fn search(h: &mut Harness<'_, App>, text: &str) {
    h.get_by_role_and_label(Role::TextInput, "Search Parts")
        .click();
    h.run();
    h.get_by_role_and_label(Role::TextInput, "Search Parts")
        .type_text(text);
    h.run();
}

/// Searches for `name` and clicks its row.
fn choose(h: &mut Harness<'_, App>, name: &str) {
    search(h, name);
    h.get_by_role_and_label(Role::ListItem, name).click();
    h.run();
}

fn listed(h: &Harness<'_, App>, name: &str) -> bool {
    h.query_by_role_and_label(Role::ListItem, name).is_some()
}

/// The last successful Journal entry, as `(id, params)`.
fn last_entry(h: &Harness<'_, App>) -> (String, Value) {
    let entry = h
        .state()
        .shell()
        .engine()
        .journal()
        .iter()
        .rfind(|e| e.outcome.is_ok())
        .unwrap();
    (entry.id.clone(), entry.params.clone())
}

#[test]
fn a_sheet_lists_parts_with_a_symbol_and_the_board_mechanical_parts() {
    let (home, work) = (TempDir::new("lib-panel-home"), TempDir::new("lib-panel"));
    let mut h = harness(&home, &work);
    assert!(listed(&h, "Resistor"));
    assert!(!listed(&h, "MountingHole_M3"));

    let pcb = h
        .state()
        .shell()
        .engine()
        .project()
        .unwrap()
        .pcb()
        .unwrap()
        .uuid;
    run(&mut h, "document.open", json!({"document": pcb}));
    h.run();
    assert!(listed(&h, "MountingHole_M3"));
    assert!(!listed(&h, "Resistor"));
}

#[test]
fn search_and_the_component_type_filter_narrow_the_list() {
    let (home, work) = (TempDir::new("lib-search-home"), TempDir::new("lib-search"));
    let mut h = harness(&home, &work);
    search(&mut h, "capac");
    assert!(listed(&h, "Capacitor"));
    assert!(!listed(&h, "Resistor"));

    search(&mut h, "itor_pol");
    assert!(listed(&h, "Capacitor_Polarised"));
    assert!(!listed(&h, "Capacitor"));

    h.get_by_role_and_label(Role::ComboBox, "Component Type")
        .click();
    h.run();
    h.get_by_label("Mechanical").click();
    h.run();
    assert!(!listed(&h, "Capacitor_Polarised"));
    h.get_by_label("No Parts match.");
}

#[test]
fn clicking_a_part_previews_it_and_starts_placing_it() {
    let (home, work) = (TempDir::new("lib-click-home"), TempDir::new("lib-click"));
    let mut h = harness(&home, &work);
    choose(&mut h, "OpAmp_Dual");

    assert_eq!(h.state().shell().tool(), Tool::Place);
    let (id, params) = last_entry(&h);
    assert_eq!(id, "tool.place");
    assert_eq!(params["document"], SHEET);
    h.get_by_role_and_label(Role::Image, "Symbol preview");
    h.get_by_role_and_label(Role::Image, "Footprint preview");

    // Units are tabs; choosing one places that Unit.
    h.get_by_role_and_label(Role::Tab, "Unit B").click();
    h.run();
    let (id, params) = last_entry(&h);
    assert_eq!(id, "tool.place");
    assert_eq!(params["unit"], "B");
}

#[test]
fn the_footprint_picker_shows_the_alternates() {
    let (home, work) = (TempDir::new("lib-fp-home"), TempDir::new("lib-fp"));
    let mut h = harness(&home, &work);
    choose(&mut h, "Resistor");
    h.get_by_role_and_label(Role::ComboBox, "Alternate Footprint")
        .click();
    h.run();
    h.get_by_label("RESC1608X55N").click();
    h.run();
    let (id, params) = last_entry(&h);
    assert_eq!(id, "tool.place");
    assert_eq!(params["part"], RESISTOR);
    assert_eq!(params["footprint"], "0691e5e7-9eb6-49f3-9f3e-d4159258693e");
}

#[test]
fn edit_opens_the_parts_symbol_and_new_asks_for_the_new_symbols_name() {
    let (home, work) = (TempDir::new("lib-edit-home"), TempDir::new("lib-edit"));
    let mut h = harness(&home, &work);
    choose(&mut h, "Resistor");
    h.get_by_role_and_label(Role::Button, "Edit Symbol").click();
    h.run();
    // The Part's Symbol opens, then its Symbol editor tab.
    let journal = h.state().shell().engine().journal();
    let entry = journal[journal.len() - 2].clone();
    assert_eq!(entry.id, "library.element.open");
    assert_eq!(entry.params, json!({"element": RESISTOR_SYMBOL}));
    assert_eq!(journal.last().unwrap().id, "document.open");
    let symbol = RESISTOR_SYMBOL.parse().unwrap();
    assert_eq!(h.state().shell().layout().active_document(), Some(symbol));

    h.get_by_role_and_label(Role::Button, "New…").click();
    h.run();
    // library.element.new lacks the Library and name: its params form asks for them.
    h.get_by_label("New Library Element");
    let last = h
        .state()
        .shell()
        .engine()
        .journal()
        .last()
        .unwrap()
        .id
        .clone();
    assert_eq!(last, "document.open", "nothing ran yet");
}
