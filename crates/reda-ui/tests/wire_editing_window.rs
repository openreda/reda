//! Wire editing in the window, driven through AccessKit with `egui_kittest`: the vertex
//! list in the Properties panel, the Break Wire Tool's options (Tab) writing straight back
//! to Preferences, and Convert Cross Junctions from the Design menu.

use std::path::{Path, PathBuf};

use egui::Key;
use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use reda_testkit::TempDir;
use reda_ui::App;
use reda_ui::engine::Engine;
use reda_ui::engine::command::{FixedClock, SeededIds, UserDirs};
use reda_ui::engine::model::Uuid;
use reda_ui::shell::Shell;
use serde_json::{Value, json};

const SHEET: &str = "00000000-0000-4000-8000-000000000002";
const MIL: i64 = 25_400;

fn library() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../reda-schematic/tests/library/Test")
}

fn harness(dir: &TempDir, home: &TempDir) -> Harness<'static, App> {
    let engine = Engine::builder()
        .ids(SeededIds::new(0))
        .clock(FixedClock::EPOCH)
        .user_dirs(Some(UserDirs::under(home.path())))
        .starter_library(Some(library()))
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

fn wire(h: &mut Harness<'_, App>, points: &[(i64, i64)], wiring: Value) -> Uuid {
    let points: Vec<Value> = points
        .iter()
        .map(|(x, y)| json!({"x": format!("{x}mil"), "y": format!("{y}mil")}))
        .collect();
    let out = run(
        h,
        "schematic.wire.add",
        json!({"document": SHEET, "points": points, "wiring": wiring}),
    );
    out["result"]["wires"][0].as_str().unwrap().parse().unwrap()
}

fn last_entry(h: &Harness<'_, App>) -> (String, Value) {
    let entry = (h.state().shell().engine().journal().iter())
        .rfind(|e| e.outcome.is_ok())
        .unwrap();
    (entry.id.clone(), entry.params.clone())
}

fn point(x: i64, y: i64) -> Value {
    json!({"x": x * MIL, "y": y * MIL})
}

#[test]
fn the_properties_panel_lists_a_wires_vertices_and_edits_them() {
    let (dir, home) = (
        TempDir::new("vertex-list"),
        TempDir::new("vertex-list-home"),
    );
    let mut h = harness(&dir, &home);
    let w = wire(&mut h, &[(0, 0), (1000, 0), (1000, 1000)], json!({}));
    run(&mut h, "selection.set", json!({"objects": [w]}));
    h.run();
    h.get_by_label("Vertices");
    for header in ["Index", "X", "Y"] {
        h.get_by_label(header);
    }
    let x1 = h.get_by_role_and_label(Role::TextInput, "Vertex 1 X");
    assert_eq!(x1.value().as_deref(), Some("1000 mil"));
    x1.focus();
    h.run();
    h.get_by_role_and_label(Role::TextInput, "Vertex 1 X")
        .set_text_value_accesskit("800");
    h.run();
    h.key_press(Key::Enter);
    h.run();
    assert_eq!(
        last_entry(&h).0,
        "schematic.wire.set_vertices",
        "a bare number is in the sheet grid's units"
    );
    assert_eq!(
        last_entry(&h).1["vertices"],
        json!([point(0, 0), point(800, 0), point(1000, 1000)])
    );
    h.run();
    h.get_by_role_and_label(Role::Button, "Add").click();
    h.run();
    assert_eq!(
        last_entry(&h).1["vertices"],
        json!([
            point(0, 0),
            point(800, 0),
            point(1000, 1000),
            point(1050, 1000)
        ])
    );
    h.run();
    let removes: Vec<_> = h
        .get_all_by_role_and_label(Role::Button, "Remove")
        .collect();
    assert_eq!(removes.len(), 4, "one per vertex");
    removes[1].click();
    h.run();
    assert_eq!(
        last_entry(&h).1["vertices"],
        json!([point(0, 0), point(1000, 1000), point(1050, 1000)])
    );
}

#[test]
fn the_break_wire_options_write_straight_back_to_the_preferences() {
    let (dir, home) = (
        TempDir::new("break-options"),
        TempDir::new("break-options-home"),
    );
    let mut h = harness(&dir, &home);
    h.run();
    run(&mut h, "tool.activate:break_wire", json!({}));
    run(&mut h, "tool.edit_properties", json!({}));
    h.run();
    h.get_by_label("Break Wire");
    h.get_by_role_and_label(Role::RadioButton, "Fixed Length")
        .click();
    h.run();
    h.get_by_role_and_label(Role::RadioButton, "Show Cutter Box: On Wire")
        .click();
    h.run();
    h.get_by_role_and_label(Role::RadioButton, "Show Extremity Markers: Never")
        .click();
    h.run();
    let prefs = h.state().shell().preferences().schematic.break_wire.clone();
    let prefs = serde_json::to_value(prefs).unwrap();
    assert_eq!(prefs["cutting_length"], "fixed");
    assert_eq!(prefs["show_cutter_box"], "on_wire");
    assert_eq!(prefs["show_extremity_markers"], "never");
    h.get_by_role_and_label(Role::Button, "Close").click();
    h.run();
    assert!(h.query_by_label("Show Cutter Box: On Wire").is_none());
}

#[test]
fn design_convert_cross_junctions_opens_its_dialog_and_converts() {
    let (dir, home) = (
        TempDir::new("convert-cross-window"),
        TempDir::new("cc-home"),
    );
    let mut h = harness(&dir, &home);
    let off = json!({"convert_cross_junctions": false});
    for points in [
        [(0, 500), (500, 500)],
        [(500, 500), (1000, 500)],
        [(500, 0), (500, 500)],
        [(500, 500), (500, 900)],
    ] {
        wire(&mut h, &points, off.clone());
    }
    h.run();
    h.get_by_role_and_label(Role::Button, "Design").click();
    h.run();
    h.get_by_label("Convert Cross Junctions…").click();
    h.run();
    let miter = h.get_by_role_and_label(Role::TextInput, "Miter Size");
    assert_eq!(miter.value().as_deref(), Some("50 mil"));
    for radio in ["This sheet", "Project", "All", "Selected Objects"] {
        h.get_by_role_and_label(Role::RadioButton, radio);
    }
    h.get_by_role_and_label(Role::Button, "Convert").click();
    h.run();
    let (id, params) = last_entry(&h);
    assert_eq!(id, "schematic.junctions.convert_cross");
    assert_eq!(params["document"], SHEET);
    assert_eq!(params["miter"], 50 * MIL);
    assert!(
        h.query_by_label("Miter Size").is_none(),
        "the dialog closes"
    );
}
