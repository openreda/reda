//! The schematic canvas in the window, driven with `egui_kittest`: a real mouse drag
//! previews without touching the Journal and commits exactly one move Command on release;
//! the keys and the Edit menu run their Commands.

use std::path::{Path, PathBuf};

use egui::accesskit::Role;
use egui::{Key, Modifiers, Pos2, vec2};
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
const R_10K: &str = "a0000000-0000-4000-8000-000000000100";
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

fn run(h: &mut Harness<'_, App>, id: &str, params: Value) -> Value {
    h.state_mut().shell_mut().run(id, params).unwrap()
}

fn sheet() -> Uuid {
    SHEET.parse().unwrap()
}

/// A resistor at the origin and a Wire up from its Pin 1, at (0, 150) mil.
fn resistor_and_wire(h: &mut Harness<'_, App>) -> Value {
    let unit = run(
        h,
        "schematic.component.place",
        json!({"document": SHEET, "part": R_10K, "at": {"x": 0, "y": 0}}),
    )["result"]["unit"]
        .clone();
    run(
        h,
        "schematic.wire.add",
        json!({"document": SHEET, "points": [{"x": 0, "y": "150mil"}, {"x": 0, "y": "500mil"}]}),
    );
    h.run();
    unit
}

/// Where the sheet point (`x`, `y`) mil is on screen.
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

/// The ids of the Journal's successful entries from `from` on.
fn journal_since(h: &Harness<'_, App>, from: usize) -> Vec<String> {
    h.state().shell().engine().journal()[from..]
        .iter()
        .filter(|e| e.outcome.is_ok())
        .map(|e| e.id.clone())
        .collect()
}

#[test]
fn a_drag_commits_exactly_one_move_command() {
    let dir = TempDir::new("canvas-drag");
    let mut h = harness(&dir);
    let unit = resistor_and_wire(&mut h);
    let before = h.state().shell().engine().journal().len();
    let project = h.state().shell().engine().project().unwrap().clone();

    // Grab the resistor by its Pin 1 stub and drag it right in several steps.
    let start = screen(&h, 0.0, 120.0);
    h.hover_at(start);
    h.run();
    h.drag_at(start);
    h.run();
    for step in 1..=6 {
        h.hover_at(start + vec2(15.0 * step as f32, 0.0));
        h.run();
    }
    // In flight: drawn moved, but nothing is committed and the Journal only selected.
    let tab = h.state().shell().document_tab(sheet()).unwrap();
    assert!(tab.moving_offset.0 > 0.0);
    assert!(h.state().shell().engine().project().unwrap() == &project);
    assert_eq!(journal_since(&h, before), ["selection.set"]);

    h.drop_at(start + vec2(90.0, 0.0));
    h.run();
    h.run();
    let ids = journal_since(&h, before);
    assert_eq!(ids, ["selection.set", "schematic.drag"]);
    let moved = h
        .state()
        .shell()
        .engine()
        .project()
        .unwrap()
        .schematic_sheet(sheet())
        .unwrap()
        .unit(unit.as_str().unwrap().parse().unwrap())
        .unwrap()
        .at;
    // Snapped: the grabbed Pin lands on the 50 mil grid.
    assert!(moved.x.0 > 0 && moved.x.0 % 1_270_000 == 0 && moved.y.0 % 1_270_000 == 0);
    // Rubber-banded: the Wire now turns a corner to the moved Pin.
    let wires = h
        .state()
        .shell()
        .engine()
        .project()
        .unwrap()
        .schematic_sheet(sheet())
        .unwrap()
        .wires()
        .count();
    assert_eq!(wires, 2);
    let tab = h.state().shell().document_tab(sheet()).unwrap();
    assert!(tab.live.moving.is_empty());
}

#[test]
fn keys_on_the_canvas_run_commands_and_the_edit_menu_aligns() {
    let dir = TempDir::new("canvas-keys");
    let mut h = harness(&dir);
    resistor_and_wire(&mut h);
    let before = h.state().shell().engine().journal().len();
    // A click on the canvas gives it the keyboard and selects the resistor.
    h.get_by_role(Role::Canvas).hover();
    let pin = screen(&h, 0.0, 120.0);
    h.hover_at(pin);
    h.run();
    h.drag_at(pin);
    h.run();
    h.drop_at(pin);
    h.run();
    h.key_press(Key::G);
    h.run();
    h.key_press_modifiers(Modifiers::COMMAND, Key::D);
    h.run();
    h.key_press(Key::Delete);
    h.run();
    let ids = journal_since(&h, before);
    assert!(ids.contains(&"view.grid.cycle".to_owned()), "{ids:?}");
    assert!(ids.contains(&"schematic.duplicate".to_owned()), "{ids:?}");
    assert_eq!(ids.last().unwrap(), "schematic.delete");

    // Select everything and align it from the Edit menu.
    h.hover_at(pin);
    h.key_press_modifiers(Modifiers::COMMAND, Key::A);
    h.run();
    h.get_by_role_and_label(Role::Button, "Edit").click();
    h.run();
    h.get_by_label("Align").hover();
    h.run();
    h.get_by_label("Align Left").click();
    h.run();
    assert_eq!(journal_since(&h, before).last().unwrap(), "schematic.align");
}
