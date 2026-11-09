//! The Properties panel's Unit rows in the window: a selected Unit lists its Component's
//! unplaced Units, each with a **Place** button that picks the Unit up to place, and each
//! field moved by hand with **Reset position**.

use std::path::Path;

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use reda_testkit::TempDir;
use reda_ui::App;
use reda_ui::engine::Engine;
use reda_ui::engine::command::{FixedClock, SeededIds};
use reda_ui::engine::model::Uuid;
use reda_ui::engine::model::geom::{Nm, Point};
use reda_ui::shell::Shell;
use reda_ui::tools::Tool;
use serde_json::{Value, json};

const SHEET: &str = "00000000-0000-4000-8000-000000000002";
const LM358: &str = "a0000000-0000-4000-8000-000000000120";

fn harness(dir: &TempDir) -> Harness<'static, App> {
    let library =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../reda-schematic/tests/library/Test");
    let engine = Engine::builder()
        .ids(SeededIds::new(0))
        .clock(FixedClock::EPOCH)
        .user_dirs(None)
        .starter_library(Some(library))
        .headless(false)
        .build();
    let mut shell = Shell::new(engine);
    shell.new_project(dir.path(), "Demo").unwrap();
    Harness::builder()
        .with_size([1200.0, 900.0])
        .build_ui_state(|ui, app: &mut App| app.ui(ui), App::new(shell))
}

fn run(h: &mut Harness<'_, App>, id: &str, params: Value) -> Value {
    h.state_mut().shell_mut().run(id, params).unwrap()
}

fn last_entry(h: &Harness<'_, App>) -> (String, Value) {
    let entry = (h.state().shell().engine().journal().iter())
        .rfind(|e| e.outcome.is_ok())
        .unwrap();
    (entry.id.clone(), entry.params.clone())
}

#[test]
fn place_picks_up_an_unplaced_unit_and_a_click_places_it() {
    let dir = TempDir::new("unit-place");
    let mut h = harness(&dir);
    let placed = run(
        &mut h,
        "schematic.component.place",
        json!({"document": SHEET, "part": LM358, "at": {"x": 0, "y": 0}}),
    )["result"]
        .clone();
    run(
        &mut h,
        "selection.set",
        json!({"objects": [placed["unit"]]}),
    );
    run(&mut h, "panel.show:properties", json!({}));
    h.run();
    h.get_by_label_contains("Unit A");
    h.get_by_label("Place Unit B").click();
    h.run();
    let (id, params) = last_entry(&h);
    assert_eq!(id, "tool.place_unit");
    assert_eq!(params["unit"], "B");
    assert_eq!(h.state().shell().tool(), Tool::Place);

    let sheet: Uuid = SHEET.parse().unwrap();
    let at = Point::new(Nm::from_mil(2000), Nm(0));
    reda_ui::placement::click(h.state_mut().shell_mut(), sheet, at);
    h.run();
    let journal: Vec<(String, Value)> = (h.state().shell().engine().journal().iter())
        .filter(|e| e.outcome.is_ok())
        .map(|e| (e.id.clone(), e.params.clone()))
        .collect();
    let (id, params) = journal
        .iter()
        .find(|(id, _)| id == "schematic.unit.place")
        .expect("the Unit was placed");
    assert_eq!(id, "schematic.unit.place");
    assert_eq!(params["component"], placed["component"]["uuid"]);
    assert_eq!(params["unit"], "B");
    // One Unit was left, so the Tool ends with it.
    assert_eq!(h.state().shell().tool(), Tool::Select);
    let project = h.state().shell().engine().project().unwrap();
    assert_eq!(project.schematic_sheet(sheet).unwrap().units().count(), 2);

    // With every Unit placed, there is nothing left to place.
    run(
        &mut h,
        "selection.set",
        json!({"objects": [placed["unit"]]}),
    );
    h.run();
    assert!(h.query_by_label("Place Unit B").is_none());
}

#[test]
fn reset_position_puts_a_moved_field_back_on_autoposition() {
    let dir = TempDir::new("field-reset");
    let mut h = harness(&dir);
    let placed = run(
        &mut h,
        "schematic.component.place",
        json!({"document": SHEET, "part": LM358, "at": {"x": 0, "y": 0}}),
    )["result"]
        .clone();
    run(
        &mut h,
        "schematic.field.move",
        json!({"unit": placed["unit"], "field": "value", "at": {"x": "15mm", "y": "2mm"}}),
    );
    run(
        &mut h,
        "selection.set",
        json!({"objects": [placed["unit"]]}),
    );
    run(&mut h, "panel.show:properties", json!({}));
    h.run();
    h.get_by_label("Reset position of Value").click();
    h.run();
    let (id, params) = last_entry(&h);
    assert_eq!(id, "schematic.field.reset_position");
    assert_eq!(params, json!({"unit": placed["unit"], "field": "value"}));
    h.run();
    assert!(h.query_by_label("Reset position of Value").is_none());
}
