//! The wiring Tools in the window, driven with `egui_kittest`: real keys and clicks on the
//! sheet's canvas draw a Wire run (one `schematic.wire.add` when it reaches a Pin) and
//! place a Net Label from the Active Bar.

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
use reda_ui::tools::Tool;
use serde_json::json;

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
    shell
        .run(
            "schematic.component.place",
            json!({"document": SHEET, "part": R_10K, "at": {"x": 0, "y": 0}}),
        )
        .unwrap();
    Harness::builder()
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

/// A click with the mouse at the sheet point (`x`, `y`) mil.
fn click(h: &mut Harness<'_, App>, x: f64, y: f64) {
    let at = screen(h, x, y);
    h.hover_at(at);
    h.run();
    h.drag_at(at);
    h.run();
    h.drop_at(at);
    h.run();
}

fn key(h: &mut Harness<'_, App>, key: Key) {
    h.key_press(key);
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

#[test]
fn keys_and_clicks_draw_a_wire_run_that_ends_on_a_pin() {
    let dir = TempDir::new("wiring-canvas-run");
    let mut h = harness(&dir);
    h.run();
    // A click on empty sheet gives the canvas the keyboard.
    click(&mut h, 2000.0, 2000.0);
    let before = h.state().shell().engine().journal().len();

    key(&mut h, Key::W);
    assert_eq!(h.state().shell().tool(), Tool::Wire);
    click(&mut h, 1000.0, 1000.0);
    click(&mut h, 1000.0, 500.0);
    click(&mut h, 500.0, 500.0);
    key(&mut h, Key::Backspace);
    h.hover_at(screen(&h, 0.0, 300.0));
    h.run();
    key(&mut h, Key::Slash);
    // In flight, drawn over the canvas: down from (1000, 1000) in one straight stretch
    // (vertical first), then across. Nothing in the Project yet.
    let tab = h.state().shell().document_tab(sheet()).unwrap();
    assert_eq!(tab.overlay.wires.len(), 2);
    let wires = |h: &Harness<'_, App>| {
        h.state()
            .shell()
            .engine()
            .project()
            .unwrap()
            .schematic_sheet(sheet())
            .unwrap()
            .wires()
            .count()
    };
    assert_eq!(wires(&h), 0);

    // Onto Pin 1 at (0, 150) mil: the run ends.
    click(&mut h, 0.0, 150.0);
    h.run();
    assert_eq!(
        journal_since(&h, before),
        [
            "tool.activate:wire",
            "tool.undo_segment",
            "tool.cycle_corner",
            "schematic.wire.add"
        ]
    );
    // The run is one Wire.
    assert_eq!(wires(&h), 1);
    assert!(
        h.state()
            .shell()
            .document_tab(sheet())
            .unwrap()
            .overlay
            .wires
            .is_empty()
    );

    // Escape returns to Select.
    key(&mut h, Key::Escape);
    assert_eq!(h.state().shell().tool(), Tool::Select);
}

#[test]
fn the_active_bar_picks_the_net_label_tool_and_a_click_places_one() {
    let dir = TempDir::new("wiring-canvas-label");
    let mut h = harness(&dir);
    h.run();
    let before = h.state().shell().engine().journal().len();
    h.get_by_label("Net Label").click();
    h.run();
    assert_eq!(h.state().shell().tool(), Tool::NetLabel);
    click(&mut h, 500.0, 500.0);
    h.run();
    assert_eq!(
        journal_since(&h, before),
        ["tool.activate:net_label", "schematic.net_label.add"]
    );
    let names: Vec<String> = h
        .state()
        .shell()
        .engine()
        .project()
        .unwrap()
        .schematic_sheet(sheet())
        .unwrap()
        .net_labels()
        .map(|l| l.name.clone())
        .collect();
    assert_eq!(names, ["NET1"]);
}
