//! Schematic editing in the window, driven with `egui_kittest`: F2 opens a text field on
//! the canvas and Enter commits it; Tab while placing a Net Label shows "Paused" with the
//! next name; hovering a Wire shows its Net; Copy As Text reaches the system clipboard.

use std::path::{Path, PathBuf};

use egui::accesskit::Role;
use egui::{Key, Pos2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use reda_testkit::TempDir;
use reda_ui::App;
use reda_ui::engine::Engine;
use reda_ui::engine::command::{FixedClock, SeededIds};
use reda_ui::engine::model::Uuid;
use reda_ui::shell::Shell;
use serde_json::{Value, json};

const SHEET: &str = "00000000-0000-4000-8000-000000000002";
const MIL: f64 = 25_400.0;

fn library() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../reda-schematic/tests/library/Test")
}

fn harness(dir: &TempDir) -> Harness<'static, App> {
    let engine = Engine::builder()
        .ids(SeededIds::new(0))
        .clock(FixedClock::EPOCH)
        .user_dirs(None)
        .starter_library(Some(library()))
        .headless(false)
        .build();
    let mut shell = Shell::new(engine);
    shell.new_project(dir.path(), "Demo").unwrap();
    Harness::builder()
        .with_size([1200.0, 800.0])
        .build_ui_state(|ui, app: &mut App| app.ui(ui), App::new(shell))
}

fn sheet() -> Uuid {
    SHEET.parse().unwrap()
}

fn run(h: &mut Harness<'_, App>, id: &str, params: Value) -> Value {
    h.state_mut().shell_mut().run(id, params).unwrap()
}

fn screen(h: &Harness<'_, App>, x: f64, y: f64) -> Pos2 {
    let rect = h.get_by_role(Role::Canvas).rect();
    let camera = h
        .state()
        .shell()
        .document_tab(sheet())
        .unwrap()
        .camera
        .unwrap();
    camera.screen_point(rect, (x * MIL, y * MIL))
}

fn labels(h: &Harness<'_, App>) -> Vec<String> {
    let project = h.state().shell().engine().project().unwrap();
    let mut names: Vec<String> = (project.schematic_sheet(sheet()).unwrap().net_labels())
        .map(|l| l.name.clone())
        .collect();
    names.sort();
    names
}

#[test]
fn f2_edits_a_net_label_on_the_canvas_and_enter_commits() {
    let dir = TempDir::new("in-place-window");
    let mut h = harness(&dir);
    let label = run(
        &mut h,
        "schematic.net_label.add",
        json!({"document": SHEET, "name": "SDA", "at": {"x": 0, "y": 0}}),
    )["result"]["net_label"]
        .clone();
    run(&mut h, "selection.set", json!({"objects": [label]}));
    h.run();
    h.hover_at(screen(&h, 2000.0, 2000.0));
    h.run();
    h.key_press(Key::F2);
    h.run();
    h.run();
    h.get_by_role_and_label(Role::TextInput, "Edit in place");
    h.key_press_modifiers(egui::Modifiers::COMMAND, Key::A);
    h.run();
    h.get_by_role_and_label(Role::TextInput, "Edit in place")
        .type_text("SCL");
    h.run();
    h.key_press(Key::Enter);
    h.run();
    h.run();
    assert_eq!(labels(&h), ["SCL"]);
    assert!(
        h.query_by_role_and_label(Role::TextInput, "Edit in place")
            .is_none()
    );
}

#[test]
fn tab_while_placing_a_net_label_shows_paused_and_the_next_name() {
    let dir = TempDir::new("paused-window");
    let mut h = harness(&dir);
    h.run();
    h.hover_at(screen(&h, 1000.0, 1000.0));
    h.run();
    h.key_press(Key::N);
    h.run();
    h.hover_at(screen(&h, 1000.0, 1000.0));
    h.run();
    h.key_press(Key::Tab);
    h.run();
    h.run();
    h.get_by_label("Paused");
    h.get_by_role_and_label(Role::TextInput, "Next name");
    h.key_press(Key::Escape);
    h.run();
    h.run();
    assert!(h.query_by_label("Paused").is_none());
}

#[test]
fn hovering_a_wire_shows_its_net_name() {
    let dir = TempDir::new("tip-window");
    let mut h = harness(&dir);
    run(
        &mut h,
        "schematic.wire.add",
        json!({"document": SHEET, "points": [
            {"x": "0mil", "y": "0mil"}, {"x": "1000mil", "y": "0mil"}]}),
    );
    run(
        &mut h,
        "schematic.net_label.add",
        json!({"document": SHEET, "name": "SDA", "at": {"x": 0, "y": 0}}),
    );
    h.run();
    h.hover_at(screen(&h, 500.0, 0.0));
    h.run();
    h.run();
    h.get_by_label("Net: SDA");
}

#[test]
fn copy_as_text_reaches_the_system_clipboard() {
    let dir = TempDir::new("copy-text-window");
    let mut h = harness(&dir);
    let label = run(
        &mut h,
        "schematic.net_label.add",
        json!({"document": SHEET, "name": "SDA", "at": {"x": 0, "y": 0}}),
    )["result"]["net_label"]
        .clone();
    run(&mut h, "selection.set", json!({"objects": [label]}));
    run(
        &mut h,
        "schematic.copy_as_text",
        json!({"objects": [label]}),
    );
    h.step();
    let copied = h
        .output()
        .platform_output
        .commands
        .iter()
        .any(|c| matches!(c, egui::OutputCommand::CopyText(t) if t == "SDA"));
    assert!(copied, "{:?}", h.output().platform_output.commands);
}
