//! The Wire Tool in the window, driven with `egui_kittest`: real keys and clicks cycle the
//! mode, suspend a run under another Tool and come back to it, open the Auto Wire options
//! with Tab, and end a run with a right-click; the Journal holds each Command.

use std::path::{Path, PathBuf};

use egui::accesskit::Role;
use egui::{Key, Modifiers, PointerButton, Pos2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use reda_testkit::TempDir;
use reda_ui::App;
use reda_ui::engine::Engine;
use reda_ui::engine::command::{FixedClock, SeededIds, UserDirs};
use reda_ui::engine::model::Uuid;
use reda_ui::shell::Shell;
use reda_ui::tools::{Tool, WireTool};

const SHEET: &str = "00000000-0000-4000-8000-000000000002";
const MIL: f64 = 25_400.0;

fn library() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../reda-schematic/tests/library/Test")
}

fn harness(dir: &TempDir) -> Harness<'static, App> {
    let engine = Engine::builder()
        .ids(SeededIds::new(0))
        .clock(FixedClock::EPOCH)
        .user_dirs(Some(UserDirs::under(&dir.path().join("home"))))
        .starter_library(Some(library()))
        .headless(false)
        .build();
    let mut shell = Shell::new(engine);
    shell.new_project(dir.path(), "Demo").unwrap();
    Harness::builder()
        .with_step_dt(0.25)
        .with_max_steps(100)
        .with_size([1200.0, 800.0])
        .build_ui_state(|ui, app: &mut App| app.ui(ui), App::new(shell))
}

fn sheet() -> Uuid {
    SHEET.parse().unwrap()
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

fn click(h: &mut Harness<'_, App>, x: f64, y: f64) {
    let at = screen(h, x, y);
    h.hover_at(at);
    h.run();
    h.drag_at(at);
    h.drop_at(at);
    h.run();
}

fn right_click(h: &mut Harness<'_, App>, x: f64, y: f64) {
    let pos = screen(h, x, y);
    h.hover_at(pos);
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton {
            pos,
            button: PointerButton::Secondary,
            pressed,
            modifiers: Modifiers::NONE,
        });
    }
    h.run();
}

fn key(h: &mut Harness<'_, App>, modifiers: Modifiers, key: Key) {
    h.key_press_modifiers(modifiers, key);
    h.run();
}

/// The ids of the Journal's successful entries from `from` on.
fn journal_since(h: &Harness<'_, App>, from: usize) -> Vec<String> {
    h.state().shell().engine().journal()[from..]
        .iter()
        .filter(|e| e.outcome.is_ok())
        .map(|e| e.id.clone())
        .collect()
}

fn wire_tool(h: &Harness<'_, App>) -> WireTool {
    *h.state().shell().engine().view().get::<WireTool>().unwrap()
}

#[test]
fn the_wire_tool_runs_its_commands_from_real_keys_and_clicks() {
    let dir = TempDir::new("wire-tool-canvas");
    let mut h = harness(&dir);
    h.run();
    // A click on the canvas gives it the keyboard.
    click(&mut h, 2000.0, 2000.0);
    let before = h.state().shell().engine().journal().len();

    key(&mut h, Modifiers::NONE, Key::W);
    assert_eq!(h.state().shell().tool(), Tool::Wire);
    key(&mut h, Modifiers::SHIFT, Key::Space);
    // The status bar shows the mode.
    assert!(h.query_by_label("Wire: 45°").is_some());
    key(&mut h, Modifiers::SHIFT, Key::Space);
    key(&mut h, Modifiers::SHIFT, Key::Space);
    key(&mut h, Modifiers::SHIFT, Key::Space);
    assert!(h.query_by_label("Wire: 90°").is_some());

    // A run, suspended by the Net Label Tool and back on Esc.
    click(&mut h, 0.0, 0.0);
    click(&mut h, 1000.0, 0.0);
    key(&mut h, Modifiers::NONE, Key::N);
    assert_eq!(h.state().shell().tool(), Tool::NetLabel);
    click(&mut h, 500.0, 1000.0);
    key(&mut h, Modifiers::NONE, Key::Escape);
    assert_eq!(h.state().shell().tool(), Tool::Wire);
    // Insert adds a vertex where the pointer is.
    h.hover_at(screen(&h, 1000.0, 500.0));
    h.run();
    key(&mut h, Modifiers::NONE, Key::Insert);

    // Tab opens the Auto Wire options; their controls run `tool.auto_wire.set`.
    key(&mut h, Modifiers::NONE, Key::Tab);
    assert!(wire_tool(&h).options_open);
    h.get_by_role_and_label(Role::Slider, "Time Out After")
        .set_value_accesskit(1.5);
    h.run();
    h.get_by_role_and_label(Role::Slider, "Avoid cutting wires")
        .set_value_accesskit(2.0);
    h.run();
    assert_eq!(wire_tool(&h).timeout_ms, 1500);
    assert_eq!(wire_tool(&h).avoid, reda_ui::engine::schematic::Avoid::High);
    h.get_by_role_and_label(Role::Button, "Close").click();
    h.run();
    assert!(!wire_tool(&h).options_open);

    // A right-click ends the run with its fixed corners; the Tool stays.
    right_click(&mut h, 1500.0, 500.0);
    assert_eq!(h.state().shell().tool(), Tool::Wire);
    // With no run, a right-click leaves the Tool.
    right_click(&mut h, 1500.0, 500.0);
    assert_eq!(h.state().shell().tool(), Tool::Select);

    let journal = journal_since(&h, before);
    assert_eq!(
        journal,
        [
            "tool.activate:wire",
            "tool.cycle_wire_mode",
            "tool.cycle_wire_mode",
            "tool.cycle_wire_mode",
            "tool.cycle_wire_mode",
            "tool.activate:net_label",
            "schematic.net_label.add",
            "tool.activate:select",
            "tool.insert_vertex",
            "tool.edit_properties",
            "tool.auto_wire.set",
            "tool.auto_wire.set",
            "tool.edit_properties",
            "tool.activate:select",
            "schematic.wire.add",
            "tool.activate:select",
        ]
    );
    let sheet = h
        .state()
        .shell()
        .engine()
        .project()
        .unwrap()
        .schematic_sheet(sheet())
        .unwrap();
    let wire = sheet.wires().next().unwrap();
    assert_eq!(wire.vertices.len(), 3, "{:?}", wire.vertices);
}
