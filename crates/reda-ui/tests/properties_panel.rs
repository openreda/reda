//! The Properties panel in the window, driven through AccessKit with `egui_kittest`: it
//! follows the selection, shows `<mixed>`, and every edit reaches the engine's Journal as
//! one Command for everything selected.

use std::path::{Path, PathBuf};

use egui::Key;
use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use reda_testkit::TempDir;
use reda_ui::App;
use reda_ui::engine::Engine;
use reda_ui::engine::command::{FixedClock, SeededIds};
use reda_ui::shell::Shell;
use serde_json::{Value, json};

const SHEET: &str = "00000000-0000-4000-8000-000000000002";
const R_10K: &str = "a0000000-0000-4000-8000-000000000100";
const C_100N: &str = "a0000000-0000-4000-8000-000000000110";

fn library() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../reda-schematic/tests/library/Test")
}

fn harness(dir: &TempDir) -> Harness<'static, App> {
    let engine = Engine::builder()
        .ids(SeededIds::new(0))
        .clock(FixedClock::EPOCH)
        .user_dirs(None)
        .starter_library(Some(library()))
        .commands(reda_testkit::probe_commands)
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

/// Places `part` and returns the result: `{unit, component, …}`.
fn placed(h: &mut Harness<'_, App>, part: &str, x: i64) -> Value {
    let out = run(
        h,
        "schematic.component.place",
        json!({"document": SHEET, "part": part, "at": {"x": format!("{x}mil"), "y": 0}}),
    );
    out["result"].clone()
}

/// Places `part` and returns its Unit's UUID.
fn place(h: &mut Harness<'_, App>, part: &str, x: i64) -> Value {
    placed(h, part, x)["unit"].clone()
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
fn the_panel_follows_the_selection_and_edits_all_selected_components() {
    let dir = TempDir::new("properties-panel");
    let mut h = harness(&dir);
    let r1 = placed(&mut h, R_10K, 0);
    let r2 = placed(&mut h, R_10K, 1000);
    h.run();
    h.get_by_label("Nothing selected");

    run(
        &mut h,
        "selection.set",
        json!({"objects": [r1["unit"], r2["unit"]]}),
    );
    h.run();
    let ids = json!([r1["component"]["uuid"], r2["component"]["uuid"]]);

    // The Designators differ, so they show as mixed; the Designator is read-only for two.
    let designator = h.get_by_role_and_label(Role::TextInput, "Designator");
    assert!(designator.accesskit_node().is_disabled());

    let value = h.get_by_role_and_label(Role::TextInput, "Value");
    value.focus();
    h.run();
    h.get_by_role_and_label(Role::TextInput, "Value")
        .set_text_value_accesskit("4k7");
    h.run();
    h.key_press(Key::Enter);
    h.run();
    assert_eq!(
        last_entry(&h),
        (
            "schematic.component.set".to_owned(),
            json!({"components": ids, "value": "4k7"})
        )
    );

    h.get_by_label("Not Fitted").click();
    h.run();
    assert_eq!(
        last_entry(&h),
        (
            "schematic.component.set".to_owned(),
            json!({"components": ids, "not_fitted": true})
        )
    );

    h.get_by_label("Show MPN on the sheet").click();
    h.run();
    assert_eq!(
        last_entry(&h).1,
        json!({"components": ids, "visible": {"mpn": true}})
    );
}

#[test]
fn mixed_values_show_as_mixed() {
    let dir = TempDir::new("properties-panel-mixed");
    let mut h = harness(&dir);
    let r = place(&mut h, R_10K, 0);
    let c = place(&mut h, C_100N, 1000);
    run(&mut h, "selection.set", json!({"objects": [r, c]}));
    h.run();
    let value = h.get_by_role_and_label(Role::TextInput, "Value");
    assert_eq!(value.accesskit_node().placeholder(), Some("<mixed>"));
    assert_eq!(value.value().as_deref(), Some(""));
}

#[test]
fn the_footprint_is_picked_from_the_alternates() {
    let dir = TempDir::new("properties-panel-footprint");
    let mut h = harness(&dir);
    let u1 = place(&mut h, R_10K, 0);
    run(&mut h, "selection.set", json!({"objects": [u1]}));
    h.run();
    let alternates = {
        let project = h.state().shell().engine().project().unwrap();
        let p = reda_ui::properties::Properties::of(project, h.state().shell().selection());
        p.component.unwrap().alternates
    };
    h.get_by_role_and_label(Role::ComboBox, "Footprint").click();
    h.run();
    h.get_by_label(&alternates[1].name).click();
    h.run();
    let (id, params) = last_entry(&h);
    assert_eq!(id, "schematic.component.set");
    assert_eq!(params["footprint"], json!(alternates[1].footprint));
}

#[test]
fn the_net_class_is_set_for_the_selected_copper() {
    let dir = TempDir::new("properties-panel-net-class");
    let mut h = harness(&dir);
    let gnd = run(&mut h, "test.net.add", json!({"name": "GND"}))["result"]["uuid"].clone();
    run(&mut h, "pcb.net_class.add", json!({"name": "Power"}));
    let via = run(
        &mut h,
        "pcb.via.add",
        json!({"at": {"x": 0, "y": 0}, "net": gnd}),
    )["result"]["uuid"]
        .clone();
    run(&mut h, "selection.set", json!({"objects": [via]}));
    h.run();
    h.get_by_role_and_label(Role::ComboBox, "Net Class").click();
    h.run();
    h.get_by_label("Power").click();
    h.run();
    let (id, params) = last_entry(&h);
    assert_eq!(id, "pcb.net.add_to_class");
    assert_eq!(params["nets"], json!([gnd]));
}
