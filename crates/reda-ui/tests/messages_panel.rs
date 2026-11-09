//! The Messages panel in the window, driven through AccessKit with `egui_kittest`: running
//! ERC, going to a Violation, and the right-click menu's Waive and Set severity.

use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use reda_testkit::TempDir;
use reda_ui::App;
use reda_ui::engine::Engine;
use reda_ui::engine::command::UserDirs;
use reda_ui::shell::Shell;
use serde_json::{Value, json};

fn at(x: i64, y: i64) -> Value {
    json!({"x": format!("{x}mil"), "y": format!("{y}mil")})
}

/// A window on a Project whose second sheet holds a lone Wire, so ERC finds its two
/// dangling ends there.
fn harness(home: &TempDir, work: &TempDir) -> (Harness<'static, App>, String) {
    let engine = Engine::builder()
        .user_dirs(Some(UserDirs::under(home.path())))
        .starter_library(None)
        .headless(false)
        .build();
    let mut shell = Shell::new(engine);
    shell.new_project(work.path(), "Demo").unwrap();
    let out = shell
        .run("document.add", json!({"kind": "schematic"}))
        .unwrap();
    let sheet2 = out["result"]["uuid"].as_str().unwrap().to_owned();
    shell
        .run(
            "schematic.wire.add",
            json!({"document": sheet2, "points": [at(1000, 1000), at(2000, 1000)]}),
        )
        .unwrap();
    let h = Harness::builder()
        .with_size([1100.0, 800.0])
        .build_ui_state(|ui, app: &mut App| app.ui(ui), App::new(shell));
    (h, sheet2)
}

/// The ids of the Commands the Journal recorded, successful ones only.
fn journal(h: &Harness<'_, App>) -> Vec<String> {
    h.state()
        .shell()
        .engine()
        .journal()
        .iter()
        .filter(|e| e.outcome.is_ok())
        .map(|e| e.id.clone())
        .collect()
}

const FIRST_END: &str = "Wire end at (25.4mm, 25.4mm) touches nothing";
const SECOND_END: &str = "Wire end at (50.8mm, 25.4mm) touches nothing";

#[test]
fn running_erc_lists_violations_under_their_document_and_a_click_goes_to_one() {
    let (home, work) = (TempDir::new("panel-home"), TempDir::new("panel-work"));
    let (mut h, sheet2) = harness(&home, &work);
    h.run();
    h.get_all_by_label("Run ERC").last().unwrap().click();
    h.run();
    assert!(journal(&h).contains(&"erc.run".to_owned()));
    h.get_by_label("Sheet 2 (2)");
    h.get_by_label(FIRST_END);

    h.get_by_label(SECOND_END).click();
    h.run();
    let shell = h.state().shell();
    assert_eq!(
        shell.layout().active_document().map(|u| u.to_string()),
        Some(sheet2)
    );
    assert_eq!(shell.selection().objects.len(), 1);
}

#[test]
fn right_click_waive_asks_for_a_reason_and_the_waived_group_brings_it_back() {
    let (home, work) = (TempDir::new("panel-home"), TempDir::new("panel-work"));
    let (mut h, _) = harness(&home, &work);
    h.run();
    h.get_all_by_label("Run ERC").last().unwrap().click();
    h.run();

    h.get_by_label(FIRST_END).click_secondary();
    h.run();
    h.get_by_label("Waive").click();
    h.run();
    // Waive opens a small window for the reason: nothing is waived yet.
    assert!(!journal(&h).contains(&"erc.waive".to_owned()));
    h.get_by_label("Waive 1 Violation with this reason:");
    h.get_by_role_and_label(Role::TextInput, "Reason").click();
    h.run();
    h.get_by_role_and_label(Role::TextInput, "Reason")
        .type_text("Test point");
    h.run();
    h.get_all_by_label("Waive").last().unwrap().click();
    h.run();
    h.run();
    assert!(journal(&h).contains(&"erc.waive".to_owned()));
    let shell = h.state().shell();
    let waiver = (shell
        .messages()
        .run(reda_ui::messages::CheckKind::Erc)
        .unwrap()
        .entries)
        .iter()
        .find_map(|e| e.violation.waiver)
        .unwrap();
    let object = shell.engine().project().unwrap().object(waiver);
    let Some(reda_ui::engine::model::Object::Waiver(w)) = object else {
        panic!("a Waiver");
    };
    assert_eq!(w.object.reason, "Test point");
    // A Waiver matches by check and objects, so it waives both ends of the one Wire;
    // they sit in the collapsed Waived group.
    assert!(h.query_by_label(FIRST_END).is_none());
    h.get_by_label("Waived (2)").click();
    h.run();
    h.get_by_label(&format!("{FIRST_END} (waived)"));
    h.get_by_label(&format!("{SECOND_END} (waived)"));

    h.get_by_label("Show waived").click();
    h.run();
    assert!(journal(&h).contains(&"view.messages.show_waived".to_owned()));
    assert!(h.query_by_label("Waived (2)").is_none());
    h.get_by_label("Show waived").click();
    h.run();

    h.get_by_label(&format!("{FIRST_END} (waived)"))
        .click_secondary();
    h.run();
    h.get_by_label("Unwaive").click();
    h.run();
    assert!(journal(&h).contains(&"erc.unwaive".to_owned()));
    h.get_by_label(FIRST_END);
}

#[test]
fn right_click_sets_the_severity_of_the_check() {
    let (home, work) = (TempDir::new("panel-home"), TempDir::new("panel-work"));
    let (mut h, _) = harness(&home, &work);
    h.run();
    h.get_all_by_label("Run ERC").last().unwrap().click();
    h.run();

    h.get_by_label(FIRST_END).click_secondary();
    h.run();
    h.get_by_label("Set severity of “Dangling Wire”").hover();
    h.run();
    // The submenu is drawn last: Properties' Snap Options has an "Off" too.
    h.get_all_by_label("Off").last().unwrap().click();
    h.run();
    assert!(journal(&h).contains(&"erc.set_severity".to_owned()));
    assert!(h.query_by_label(FIRST_END).is_none());
    assert!(h.query_by_label(SECOND_END).is_none());
    h.get_by_label("No messages.");
}

#[test]
fn an_edit_marks_the_results_out_of_date() {
    let (home, work) = (TempDir::new("panel-home"), TempDir::new("panel-work"));
    let (mut h, sheet2) = harness(&home, &work);
    h.run();
    h.get_all_by_label("Run ERC").last().unwrap().click();
    h.run();
    h.state_mut()
        .shell_mut()
        .run(
            "schematic.wire.add",
            json!({"document": sheet2, "points": [at(0, 0), at(0, 500)]}),
        )
        .unwrap();
    h.run();
    h.get_by_label("ERC results are out of date: the design changed after the run.");
}

/// Set severity on a Pin Matrix check's line sets the cell of its pair of types.
#[test]
fn set_severity_on_an_unconnected_pin_sets_its_pin_matrix_cell() {
    let (home, work) = (TempDir::new("panel-home"), TempDir::new("panel-work"));
    let engine = Engine::builder()
        .user_dirs(Some(UserDirs::under(home.path())))
        .starter_library(Some(reda_testkit::starter_library()))
        .headless(false)
        .build();
    let mut shell = Shell::new(engine);
    shell.new_project(work.path(), "Demo").unwrap();
    let sheet = shell.engine().project().unwrap().props().documents[0];
    let parts = shell.run("library.search", json!({})).unwrap();
    let resistor = (parts["parts"].as_array().unwrap().iter())
        .find(|p| p["name"] == "Resistor")
        .map(|p| p["uuid"].clone())
        .unwrap();
    shell
        .run(
            "schematic.component.place",
            json!({"document": sheet, "part": resistor, "at": at(2000, 2000)}),
        )
        .unwrap();
    let mut h = Harness::builder()
        .with_size([1100.0, 800.0])
        .build_ui_state(|ui, app: &mut App| app.ui(ui), App::new(shell));
    h.run();
    h.get_all_by_label("Run ERC").last().unwrap().click();
    h.run();

    h.get_by_label("Pin R1-1 is not connected")
        .click_secondary();
    h.run();
    h.get_by_label("Set severity of “Unconnected Pin”").hover();
    h.run();
    h.get_by_label("Error").click();
    h.run();
    assert!(journal(&h).contains(&"erc.set_pin_matrix".to_owned()));
    assert!(!journal(&h).contains(&"erc.set_severity".to_owned()));
    let erc = h
        .state()
        .shell()
        .engine()
        .project()
        .unwrap()
        .props()
        .erc
        .clone();
    let cell = reda_ui::engine::model::Cell::unconnected(reda_ui::engine::model::PinType::Passive);
    assert_eq!(
        erc.pin_matrix_overrides.get(cell),
        reda_ui::engine::model::Severity::Error
    );
    // Both Pins are passive, so both lines follow the cell.
    h.get_by_label("Pin R1-1 is not connected");
    h.get_by_label("Pin R1-2 is not connected");
}
