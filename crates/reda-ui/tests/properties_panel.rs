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
    // Nothing selected: the overview, with the Selection Filter region.
    assert!(h.query_all_by_label("Selection Filter").next().is_some());
    h.get_by_role_and_label(Role::CheckBox, "Select Component");

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

#[test]
fn properties_reports_lines_omitted_by_a_text_frame() {
    let dir = TempDir::new("properties-text-frame");
    let mut h = harness(&dir);
    let text=run(&mut h,"pcb.text.add",json!({"layer":"silkscreen_top","text":"A\nA","x":0,"y":0,"height":"1mm","frame":{"width":"5mm","height":"1mm"}}))["result"]["uuid"].clone();
    run(
        &mut h,
        "document.open",
        json!({"document":"00000000-0000-4000-8000-000000000003"}),
    );
    run(&mut h, "selection.set", json!({"objects":[text]}));
    h.run();
    h.get_by_label("Text Frame: 1 line omitted");
}

#[test]
fn a_numeric_expression_commits_one_batch_and_the_journal_records_the_results() {
    let dir = TempDir::new("properties-panel-expression");
    let mut h = harness(&dir);
    let text = |h: &mut Harness<'_, App>, height: &str| {
        run(
            h,
            "pcb.text.add",
            json!({"layer": "silkscreen_top", "text": "A", "x": 0, "y": 0, "height": height}),
        )["result"]["uuid"]
            .clone()
    };
    let (a, b) = (text(&mut h, "1mm"), text(&mut h, "1.5mm"));
    run(&mut h, "selection.set", json!({"objects": [a, b]}));
    h.run();

    let type_into = |h: &mut Harness<'_, App>, label: &str, typed: &str| {
        h.get_by_role_and_label(Role::TextInput, label).focus();
        h.run();
        h.get_by_role_and_label(Role::TextInput, label)
            .set_text_value_accesskit(typed);
        h.run();
        h.key_press(Key::Enter);
        h.run();
    };
    type_into(&mut h, "Height", "(! + 5mil) / 2");
    // The Journal holds the values each text resolved to, not the expression.
    assert_eq!(
        last_entry(&h),
        (
            "batch".to_owned(),
            json!({"commands": [
                {"id": "pcb.text.set", "params": {"uuid": a, "height": 563_500}},
                {"id": "pcb.text.set", "params": {"uuid": b, "height": 813_500}},
            ]})
        )
    );

    // A failure flags inline and commits nothing.
    let entries = h.state().shell().engine().journal().len();
    type_into(&mut h, "Thickness", "sqrt(!)");
    assert_eq!(h.state().shell().engine().journal().len(), entries);
    assert!(
        h.query_by_label_contains("reserved function name")
            .is_some(),
        "the field says why it was refused"
    );
}

#[test]
fn ctrl_q_with_the_panel_focused_toggles_its_units_only() {
    let dir = TempDir::new("properties-panel-units");
    let mut h = harness(&dir);
    let text = run(
        &mut h,
        "pcb.text.add",
        json!({"layer": "silkscreen_top", "text": "A", "x": 0, "y": 0, "height": "1mm"}),
    )["result"]["uuid"]
        .clone();
    run(&mut h, "selection.set", json!({"objects": [text]}));
    h.run();
    let height = |h: &Harness<'_, App>| {
        h.get_by_role_and_label(Role::TextInput, "Height")
            .accesskit_node()
            .value()
            .unwrap_or_default()
    };
    // The active Document is the sheet, whose grid is in mil.
    assert_eq!(height(&h), "39.37 mil");
    let canvas = h.state().shell().board_view().units;

    run(&mut h, "layout.tab.activate", json!({"tab": "properties"}));
    h.run();
    h.key_press_modifiers(egui::Modifiers::COMMAND, Key::Q);
    h.run();
    h.run();
    assert_eq!(height(&h), "1 mm");
    // The canvas keeps its own units.
    assert_eq!(h.state().shell().board_view().units, canvas);
    // A bare number typed now is read in the panel's units.
    h.get_by_role_and_label(Role::TextInput, "Height").focus();
    h.run();
    h.get_by_role_and_label(Role::TextInput, "Height")
        .set_text_value_accesskit("1.27");
    h.run();
    h.key_press(Key::Enter);
    h.run();
    assert_eq!(
        last_entry(&h),
        (
            "pcb.text.set".to_owned(),
            json!({"uuid": text, "height": 1_270_000})
        )
    );

    h.key_press_modifiers(egui::Modifiers::COMMAND, Key::Q);
    h.run();
    h.run();
    assert_eq!(height(&h), "50 mil");
}
