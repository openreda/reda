//! The Wire Tool's modes on a Shell, without a window: Shift+Space cycles 90°, 45°, Any
//! Angle and Auto Wire; clicks fix corners with a look-ahead; Insert, Enter, Del and Esc;
//! the one-level Tool stack; and the in-Tool keys (`tool.mirror_x`, `tool.mirror_y`,
//! `tool.flip_side`, `tool.edit_properties`), each checked in the Journal.

use std::path::{Path, PathBuf};

use egui::Modifiers;
use reda_testkit::TempDir;
use reda_ui::bars;
use reda_ui::canvas::interact::Pointer;
use reda_ui::engine::Engine;
use reda_ui::engine::command::{FixedClock, SeededIds};
use reda_ui::engine::model::Uuid;
use reda_ui::engine::model::geom::{Nm, Point};
use reda_ui::keymap::{self, Chord};
use reda_ui::schematic_editor::{self, Interaction, Scale};
use reda_ui::shell::Shell;
use reda_ui::tools::Tool;
use serde_json::json;

const SHEET: &str = "00000000-0000-4000-8000-000000000002";
const R_10K: &str = "a0000000-0000-4000-8000-000000000100";
const MIL: f64 = 25_400.0;

fn library() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../reda-schematic/tests/library/Test")
}

fn shell(dir: &TempDir) -> Shell {
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
}

fn sheet() -> Uuid {
    SHEET.parse().unwrap()
}

/// A point in mil, as the camera gives it.
fn at(x: f64, y: f64) -> (f64, f64) {
    (x * MIL, y * MIL)
}

/// A sheet point in mil.
fn pt(x: i64, y: i64) -> Point {
    Point::new(Nm(x * 25_400), Nm(y * 25_400))
}

fn place(shell: &mut Shell, x: i64, y: i64) -> Uuid {
    let out = shell
        .run(
            "schematic.component.place",
            json!({"document": SHEET, "part": R_10K,
                   "at": {"x": format!("{x}mil"), "y": format!("{y}mil")}}),
        )
        .unwrap();
    out["result"]["unit"].as_str().unwrap().parse().unwrap()
}

/// Feeds `events` to the editor and runs the Commands it asks for; returns their ids.
fn pointer(shell: &mut Shell, events: &[Pointer]) -> Vec<String> {
    let commands = schematic_editor::pointer(shell, sheet(), events, Scale::at(MIL));
    commands
        .into_iter()
        .map(|(id, params)| {
            shell.run(&id, params).unwrap();
            id
        })
        .collect()
}

fn hover(shell: &mut Shell, p: (f64, f64)) -> Vec<String> {
    pointer(shell, &[Pointer::Hover(p)])
}

fn click(shell: &mut Shell, p: (f64, f64)) -> Vec<String> {
    pointer(
        shell,
        &[Pointer::Click {
            at: p,
            modifiers: Modifiers::NONE,
        }],
    )
}

/// Presses `key` on the sheet's canvas: runs what the default Keymap binds it to there.
fn press(shell: &mut Shell, key: &str) -> Vec<String> {
    let chord = Chord::parse(key).unwrap();
    let active = keymap::active_for(shell, Some(sheet()));
    let Some(action) = shell.keymap().lookup(&chord, &active).cloned() else {
        return Vec::new();
    };
    match bars::resolve_for(shell, &action, Some(sheet())).run {
        Ok((id, params)) => {
            shell.run(&id, params).unwrap();
            vec![id]
        }
        Err(_) => Vec::new(),
    }
}

/// The Journal's successful entries' ids from entry `from` on.
fn journal_since(shell: &Shell, from: usize) -> Vec<String> {
    shell.engine().journal()[from..]
        .iter()
        .filter(|e| e.outcome.is_ok())
        .map(|e| e.id.clone())
        .collect()
}

/// The segments of the Wires on the sheet, as (from, to) in mil.
fn wires(shell: &Shell) -> Vec<((i64, i64), (i64, i64))> {
    let mil = |p: Point| (p.x.0 / 25_400, p.y.0 / 25_400);
    let project = shell.engine().project().unwrap();
    let mut out: Vec<_> = project
        .schematic_sheet(sheet())
        .unwrap()
        .wires()
        .flat_map(|w| w.segments())
        .map(|(a, b)| (mil(a), mil(b)))
        .collect();
    out.sort();
    out
}

/// The run in flight as the canvas draws it.
fn overlay(shell: &Shell) -> Vec<(Point, Point)> {
    shell.document_tab(sheet()).unwrap().overlay.wires.to_vec()
}

/// The Wire Tool's mode as the status bar reads it out.
fn mode(shell: &Shell) -> Option<String> {
    reda_ui::status_bar::readout(shell).prompt
}

fn dotted(shell: &Shell) -> Vec<(Point, Point)> {
    shell.document_tab(sheet()).unwrap().overlay.dotted.to_vec()
}

/// The fixed corners of the run in flight.
fn run_points(shell: &Shell) -> Vec<Point> {
    match &shell.document_tab(sheet()).unwrap().interaction {
        Interaction::Wiring(w) => w.points.clone(),
        other => panic!("not wiring: {other:?}"),
    }
}

#[test]
fn shift_space_cycles_the_wire_mode_and_the_session_keeps_it() {
    let dir = TempDir::new("wire-mode-cycle");
    let mut shell = shell(&dir);
    let before = shell.engine().journal().len();
    assert_eq!(mode(&shell), None);
    press(&mut shell, "W");
    assert_eq!(mode(&shell).as_deref(), Some("Wire: 90°"));
    let mut seen = Vec::new();
    for _ in 0..4 {
        assert_eq!(press(&mut shell, "Shift+Space"), ["tool.cycle_wire_mode"]);
        seen.push(mode(&shell).unwrap());
    }
    assert_eq!(
        seen,
        [
            "Wire: 45°",
            "Wire: Any Angle",
            "Wire: Auto Wire",
            "Wire: 90°"
        ]
    );
    // It changes during a run, and the run follows it.
    click(&mut shell, at(0.0, 0.0));
    hover(&mut shell, at(1000.0, 400.0));
    assert_eq!(overlay(&shell).len(), 2);
    press(&mut shell, "Shift+Space");
    press(&mut shell, "Shift+Space");
    assert_eq!(overlay(&shell), [(pt(0, 0), pt(1000, 400))]);
    // Leaving the Tool and coming back keeps the mode.
    press(&mut shell, "Esc");
    press(&mut shell, "Esc");
    assert_eq!(shell.tool(), Tool::Select);
    press(&mut shell, "W");
    assert_eq!(mode(&shell).as_deref(), Some("Wire: Any Angle"));
    // Outside the Wire Tool the Command is refused.
    press(&mut shell, "Esc");
    assert!(shell.run("tool.cycle_wire_mode", json!({})).is_err());
    let journal = journal_since(&shell, before);
    assert_eq!(
        journal
            .iter()
            .filter(|id| *id == "tool.cycle_wire_mode")
            .count(),
        6
    );
}

#[test]
fn forty_five_degrees_turns_one_diagonal_corner_and_space_swaps_its_ends() {
    let dir = TempDir::new("wire-mode-45");
    let mut shell = shell(&dir);
    press(&mut shell, "W");
    press(&mut shell, "Shift+Space");
    click(&mut shell, at(0.0, 0.0));
    hover(&mut shell, at(1000.0, 400.0));
    // Straight first, then the 45° stretch.
    assert_eq!(
        overlay(&shell),
        [(pt(0, 0), pt(600, 0)), (pt(600, 0), pt(1000, 400))]
    );
    assert_eq!(press(&mut shell, "Space"), ["tool.cycle_corner"]);
    assert_eq!(
        overlay(&shell),
        [(pt(0, 0), pt(400, 400)), (pt(400, 400), pt(1000, 400))]
    );
    // A click fixes the run up to the corner; the segment on the pointer is a look-ahead.
    assert!(click(&mut shell, at(1000.0, 400.0)).is_empty());
    assert_eq!(run_points(&shell), [pt(0, 0), pt(400, 400)]);
    // The same point again ends the run there.
    assert_eq!(click(&mut shell, at(1000.0, 400.0)), ["schematic.wire.add"]);
    assert_eq!(
        wires(&shell),
        [((0, 0), (400, 400)), ((400, 400), (1000, 400))]
    );
}

#[test]
fn ninety_degrees_keeps_the_segment_on_the_pointer_as_a_look_ahead() {
    let dir = TempDir::new("wire-mode-90");
    let mut shell = shell(&dir);
    press(&mut shell, "W");
    click(&mut shell, at(0.0, 0.0));
    assert!(click(&mut shell, at(1000.0, 500.0)).is_empty());
    assert_eq!(run_points(&shell), [pt(0, 0), pt(1000, 0)]);
    hover(&mut shell, at(1000.0, 500.0));
    assert_eq!(
        overlay(&shell),
        [(pt(0, 0), pt(1000, 0)), (pt(1000, 0), pt(1000, 500))]
    );
    // Enter fixes a corner where the pointer is, as a click does.
    hover(&mut shell, at(1000.0, 800.0));
    assert_eq!(press(&mut shell, "Enter"), ["tool.finish"]);
    assert_eq!(run_points(&shell), [pt(0, 0), pt(1000, 0), pt(1000, 800)]);
    // Enter again on the same point ends the run there.
    assert_eq!(press(&mut shell, "Enter"), ["tool.finish"]);
    assert_eq!(
        wires(&shell),
        [((0, 0), (1000, 0)), ((1000, 0), (1000, 800))]
    );
}

#[test]
fn any_angle_goes_straight_and_insert_and_del_add_and_take_vertices() {
    let dir = TempDir::new("wire-mode-any");
    let mut shell = shell(&dir);
    let before = shell.engine().journal().len();
    press(&mut shell, "W");
    press(&mut shell, "Shift+Space");
    press(&mut shell, "Shift+Space");
    click(&mut shell, at(0.0, 0.0));
    click(&mut shell, at(1000.0, 400.0));
    assert_eq!(run_points(&shell), [pt(0, 0), pt(1000, 400)]);
    // Insert adds a vertex at the pointer, in 90° with its corner on the way.
    press(&mut shell, "Shift+Space");
    press(&mut shell, "Shift+Space");
    assert_eq!(mode(&shell).as_deref(), Some("Wire: 90°"));
    hover(&mut shell, at(1500.0, 900.0));
    assert_eq!(press(&mut shell, "Insert"), ["tool.insert_vertex"]);
    assert_eq!(
        run_points(&shell),
        [pt(0, 0), pt(1000, 400), pt(1500, 400), pt(1500, 900)]
    );
    // Del takes the last segment off, as Backspace does.
    assert_eq!(press(&mut shell, "Del"), ["tool.undo_segment"]);
    assert_eq!(run_points(&shell), [pt(0, 0), pt(1000, 400), pt(1500, 400)]);
    assert!(
        shell
            .engine()
            .project()
            .unwrap()
            .schematic_sheet(sheet())
            .unwrap()
            .wires()
            .next()
            .is_none()
    );
    // Esc commits the fixed corners, any angle and all, as one Wire.
    press(&mut shell, "Esc");
    assert_eq!(
        wires(&shell),
        [((0, 0), (1000, 400)), ((1000, 400), (1500, 400))]
    );
    assert_eq!(
        journal_since(&shell, before)
            .into_iter()
            .filter(|id| id != "tool.cycle_wire_mode")
            .collect::<Vec<_>>(),
        [
            "tool.activate:wire",
            "tool.insert_vertex",
            "tool.undo_segment",
            "tool.activate:select",
            "schematic.wire.add"
        ]
    );
}

#[test]
fn auto_wire_routes_a_click_around_a_unit_and_draws_it_dotted() {
    let dir = TempDir::new("wire-mode-auto");
    let mut shell = shell(&dir);
    // A resistor body stands between the two ends, along the y axis at x = 0.
    place(&mut shell, 0, 0);
    let before = shell.engine().journal().len();
    press(&mut shell, "W");
    for _ in 0..3 {
        press(&mut shell, "Shift+Space");
    }
    assert_eq!(mode(&shell).as_deref(), Some("Wire: Auto Wire"));
    click(&mut shell, at(-500.0, 0.0));
    hover(&mut shell, at(500.0, 0.0));
    assert_eq!(shell.message(), None);
    // On the way to a click, the line to the pointer is dotted.
    assert_eq!(dotted(&shell), [(pt(-500, 0), pt(500, 0))]);
    assert!(click(&mut shell, at(500.0, 0.0)).is_empty());
    assert_eq!(shell.message(), None);
    let points = run_points(&shell);
    assert_eq!(points.first(), Some(&pt(-500, 0)));
    assert_eq!(points.last(), Some(&pt(500, 0)));
    assert!(points.len() >= 4, "it goes round the body: {points:?}");
    // The route is drawn dotted, and nothing is committed yet.
    assert_eq!(dotted(&shell).len(), points.len() - 1);
    assert!(wires(&shell).is_empty());
    assert_eq!(click(&mut shell, at(500.0, 0.0)), ["schematic.wire.add"]);
    assert_eq!(
        journal_since(&shell, before)
            .into_iter()
            .filter(|id| id != "tool.cycle_wire_mode")
            .collect::<Vec<_>>(),
        [
            "tool.activate:wire",
            "schematic.wire.route",
            "schematic.wire.add"
        ]
    );
}

#[test]
fn auto_wire_options_open_on_tab_and_feed_the_router() {
    let dir = TempDir::new("wire-auto-options");
    let mut shell = shell(&dir);
    let wire_tool = |shell: &Shell| {
        *shell
            .engine()
            .view()
            .get::<reda_ui::tools::WireTool>()
            .unwrap()
    };
    assert_eq!(wire_tool(&shell).timeout_ms, 3000);
    press(&mut shell, "W");
    assert_eq!(press(&mut shell, "Tab"), ["tool.edit_properties"]);
    assert!(wire_tool(&shell).options_open);
    shell
        .run(
            "tool.auto_wire.set",
            json!({"timeout_ms": 500, "avoid_cutting_wires": "high"}),
        )
        .unwrap();
    assert!(
        shell
            .run("tool.auto_wire.set", json!({"timeout_ms": 50}))
            .is_err()
    );
    for _ in 0..3 {
        press(&mut shell, "Shift+Space");
    }
    let before = shell.engine().journal().len();
    click(&mut shell, at(0.0, 0.0));
    click(&mut shell, at(500.0, 500.0));
    let route = shell.engine().journal()[before..]
        .iter()
        .find(|e| e.id == "schematic.wire.route")
        .unwrap()
        .params
        .clone();
    assert_eq!(route["timeout_ms"], 500);
    assert_eq!(route["avoid_cutting_wires"], "high");
    assert_eq!(press(&mut shell, "Tab"), ["tool.edit_properties"]);
    assert!(!wire_tool(&shell).options_open);
}

#[test]
fn another_tool_suspends_a_run_and_esc_returns_to_it_one_level_deep() {
    let dir = TempDir::new("wire-tool-stack");
    let mut shell = shell(&dir);
    let before = shell.engine().journal().len();
    press(&mut shell, "W");
    click(&mut shell, at(0.0, 0.0));
    click(&mut shell, at(1000.0, 0.0));
    // N starts the Net Label Tool on top of the run, which waits.
    assert_eq!(press(&mut shell, "N"), ["tool.activate:net_label"]);
    assert_eq!(shell.tool(), Tool::NetLabel);
    assert!(overlay(&shell).is_empty());
    assert_eq!(
        click(&mut shell, at(500.0, 1000.0)),
        ["schematic.net_label.add"]
    );
    // A third Tool replaces the one on top; the run still waits under it.
    assert_eq!(press(&mut shell, "P"), ["tool.activate:port"]);
    // Finishing it with Esc brings the run back as it was.
    assert_eq!(press(&mut shell, "Esc"), ["tool.activate:select"]);
    assert_eq!(shell.tool(), Tool::Wire);
    assert_eq!(run_points(&shell), [pt(0, 0), pt(1000, 0)]);
    hover(&mut shell, at(1000.0, 500.0));
    assert_eq!(
        overlay(&shell),
        [(pt(0, 0), pt(1000, 0)), (pt(1000, 0), pt(1000, 500))]
    );
    click(&mut shell, at(1000.0, 500.0));
    assert_eq!(click(&mut shell, at(1000.0, 500.0)), ["schematic.wire.add"]);
    assert_eq!(
        wires(&shell),
        [((0, 0), (1000, 0)), ((1000, 0), (1000, 500))]
    );
    // With no run, Esc leaves the Tool: nothing is stacked any more.
    press(&mut shell, "N");
    assert_eq!(press(&mut shell, "Esc"), ["tool.activate:select"]);
    assert_eq!(shell.tool(), Tool::Select);
    assert_eq!(
        journal_since(&shell, before),
        [
            "tool.activate:wire",
            "tool.activate:net_label",
            "schematic.net_label.add",
            "tool.activate:port",
            "tool.activate:select",
            "schematic.wire.add",
            "tool.activate:net_label",
            "tool.activate:select",
        ]
    );
}

#[test]
fn mirror_x_mirror_y_flip_side_and_edit_properties_are_tool_keys() {
    let dir = TempDir::new("wire-tool-keys");
    let mut shell = shell(&dir);
    press(&mut shell, "N");
    assert_eq!(press(&mut shell, "X"), ["tool.mirror_x"]);
    assert_eq!(press(&mut shell, "Y"), ["tool.mirror_y"]);
    // Tab is Properties only in a Tool that has them; in the Net Label Tool it edits the
    // next name in place.
    assert!(shell.run("tool.edit_properties", json!({})).is_ok());
    press(&mut shell, "Esc");
    assert!(shell.run("tool.edit_properties", json!({})).is_err());
    let registry = shell.engine().registry();
    for id in [
        "tool.mirror_x",
        "tool.mirror_y",
        "tool.flip_side",
        "tool.edit_properties",
        "tool.cycle_wire_mode",
        "tool.insert_vertex",
    ] {
        assert!(registry.get(id).is_some(), "{id}");
    }
    assert!(registry.get("tool.mirror").is_none());
    assert!(registry.get("tool.flip").is_none());
}
