//! The schematic wiring Tools on a Shell, without a window: the Wire Tool's runs (corner
//! toggle, Backspace, ending on a Pin or Wire, one `schematic.wire.add` per run) and the
//! Net Label, Port, Power Port and No-Connect Tools, with their keys from the Keymap.

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
use serde_json::{Value, json};

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

fn wire(shell: &mut Shell, points: &[(i64, i64)]) {
    let points: Vec<Value> = points
        .iter()
        .map(|(x, y)| json!({"x": format!("{x}mil"), "y": format!("{y}mil")}))
        .collect();
    shell
        .run(
            "schematic.wire.add",
            json!({"document": SHEET, "points": points}),
        )
        .unwrap();
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

/// The Wires on the sheet, as (from, to) in mil.
fn wires(shell: &Shell) -> Vec<((i64, i64), (i64, i64))> {
    let mil = |p: Point| (p.x.0 / 25_400, p.y.0 / 25_400);
    let project = shell.engine().project().unwrap();
    let mut out: Vec<_> = project
        .schematic_sheet(sheet())
        .unwrap()
        .wires()
        .map(|w| (mil(w.from), mil(w.to)))
        .collect();
    out.sort();
    out
}

/// The run in flight as the canvas draws it.
fn overlay(shell: &Shell) -> Vec<(Point, Point)> {
    shell.document_tab(sheet()).unwrap().overlay.wires.to_vec()
}

#[test]
fn a_wire_run_previews_toggles_its_corner_and_commits_once_on_reaching_a_wire() {
    let dir = TempDir::new("wiring-run");
    let mut shell = shell(&dir);
    wire(&mut shell, &[(2000, 0), (2000, 1000)]);
    let before = shell.engine().journal().len();
    let project = shell.engine().project().unwrap().clone();

    assert_eq!(press(&mut shell, "W"), ["tool.activate:wire"]);
    assert_eq!(shell.tool(), Tool::Wire);
    hover(&mut shell, at(1000.0, 0.0));
    assert!(click(&mut shell, at(1003.0, -4.0)).is_empty());
    // Horizontal first, then up to the pointer.
    hover(&mut shell, at(1500.0, 500.0));
    assert_eq!(
        overlay(&shell),
        [(pt(1000, 0), pt(1500, 0)), (pt(1500, 0), pt(1500, 500))]
    );
    // `/` turns the corner the other way.
    assert_eq!(press(&mut shell, "/"), ["tool.cycle_corner"]);
    assert_eq!(
        overlay(&shell),
        [(pt(1000, 0), pt(1000, 500)), (pt(1000, 500), pt(1500, 500))]
    );
    assert!(click(&mut shell, at(1500.0, 500.0)).is_empty());
    // Nothing is committed while the run goes on.
    assert!(shell.engine().project().unwrap() == &project);

    // Clicking onto the Wire at x = 2000 ends the run: one Command for the whole run,
    // whose straight stretch through (1500, 500) is one Wire.
    let ran = click(&mut shell, at(1998.0, 503.0));
    assert_eq!(ran, ["schematic.wire.add"]);
    assert_eq!(
        journal_since(&shell, before),
        [
            "tool.activate:wire",
            "tool.cycle_corner",
            "schematic.wire.add"
        ]
    );
    assert_eq!(
        wires(&shell),
        [
            ((1000, 0), (1000, 500)),
            ((1000, 500), (2000, 500)),
            ((2000, 0), (2000, 1000)),
        ]
    );
    // The Tool stays, ready for the next run.
    assert_eq!(shell.tool(), Tool::Wire);
    hover(&mut shell, at(2500.0, 500.0));
    assert!(overlay(&shell).is_empty());
}

/// The fixed corners of the run in flight.
fn run_points(shell: &Shell) -> Vec<Point> {
    match &shell.document_tab(sheet()).unwrap().interaction {
        Interaction::Wiring(w) => w.points.clone(),
        other => panic!("not wiring: {other:?}"),
    }
}

#[test]
fn backspace_takes_the_last_segment_off_and_a_run_ends_on_a_pin() {
    let dir = TempDir::new("wiring-backspace");
    let mut shell = shell(&dir);
    // The resistor's Pin 1 is at (0, 150) mil.
    place(&mut shell, 0, 0);
    let before = shell.engine().journal().len();
    press(&mut shell, "W");
    click(&mut shell, at(1000.0, 1000.0));
    click(&mut shell, at(1000.0, 500.0));
    click(&mut shell, at(500.0, 500.0));
    hover(&mut shell, at(500.0, 800.0));
    assert_eq!(
        run_points(&shell),
        [pt(1000, 1000), pt(1000, 500), pt(500, 500)]
    );

    assert_eq!(press(&mut shell, "Backspace"), ["tool.undo_segment"]);
    assert_eq!(run_points(&shell), [pt(1000, 1000), pt(1000, 500)]);
    // The run now goes on from (1000, 500), along x first.
    assert_eq!(
        overlay(&shell),
        [
            (pt(1000, 1000), pt(1000, 500)),
            (pt(1000, 500), pt(500, 500)),
            (pt(500, 500), pt(500, 800)),
        ]
    );

    // Near the Pin, the pointer snaps onto it off the grid, and the click ends the run.
    let ran = click(&mut shell, at(4.0, 146.0));
    assert_eq!(ran, ["schematic.wire.add"]);
    assert_eq!(
        wires(&shell),
        [
            ((0, 500), (0, 150)),
            ((1000, 500), (0, 500)),
            ((1000, 1000), (1000, 500)),
        ]
    );
    assert_eq!(
        journal_since(&shell, before),
        [
            "tool.activate:wire",
            "tool.undo_segment",
            "schematic.wire.add"
        ]
    );
}

#[test]
fn backspace_down_to_nothing_and_escape_end_a_run_without_drawing() {
    let dir = TempDir::new("wiring-escape");
    let mut shell = shell(&dir);
    let project = shell.engine().project().unwrap().clone();
    press(&mut shell, "W");
    click(&mut shell, at(0.0, 0.0));
    click(&mut shell, at(500.0, 0.0));
    press(&mut shell, "Backspace");
    press(&mut shell, "Backspace");
    assert!(run_points(&shell).is_empty());
    assert!(overlay(&shell).is_empty());
    assert_eq!(shell.tool(), Tool::Wire);

    click(&mut shell, at(0.0, 0.0));
    click(&mut shell, at(500.0, 0.0));
    hover(&mut shell, at(500.0, 500.0));
    assert_eq!(press(&mut shell, "Esc"), ["tool.activate:select"]);
    assert_eq!(shell.tool(), Tool::Select);
    let tab = shell.document_tab(sheet()).unwrap();
    assert_eq!(tab.interaction, Interaction::Idle);
    assert!(tab.overlay.wires.is_empty());
    assert!(shell.engine().project().unwrap() == &project);
}

#[test]
fn clicking_the_last_corner_again_ends_a_run_in_open_space() {
    let dir = TempDir::new("wiring-double");
    let mut shell = shell(&dir);
    press(&mut shell, "W");
    click(&mut shell, at(0.0, 0.0));
    click(&mut shell, at(500.0, 0.0));
    assert!(click(&mut shell, at(500.0, 300.0)).is_empty());
    // A double-click: the same point twice.
    assert_eq!(click(&mut shell, at(500.0, 300.0)), ["schematic.wire.add"]);
    assert_eq!(wires(&shell), [((0, 0), (500, 0)), ((500, 0), (500, 300))]);
}

#[test]
fn the_wire_tool_snaps_onto_pins_wire_ends_and_wires() {
    let dir = TempDir::new("wiring-snap");
    let mut shell = shell(&dir);
    place(&mut shell, 0, 0);
    // A Wire off the 50 mil grid: ends at (1013, 0) and (1013, 1007).
    wire(&mut shell, &[(1013, 0), (1013, 1007)]);
    press(&mut shell, "W");
    let cursor = |shell: &Shell| shell.document_tab(sheet()).unwrap().overlay.cursor;
    // Off everything: the grid.
    hover(&mut shell, at(2012.0, 2040.0));
    assert_eq!(cursor(&shell), Some(pt(2000, 2050)));
    // Near a Pin (0, -150).
    hover(&mut shell, at(6.0, -144.0));
    assert_eq!(cursor(&shell), Some(pt(0, -150)));
    // Near a Wire end.
    hover(&mut shell, at(1010.0, 1000.0));
    assert_eq!(cursor(&shell), Some(pt(1013, 1007)));
    // Near the Wire partway: onto it, at a grid step along it.
    hover(&mut shell, at(1018.0, 498.0));
    assert_eq!(cursor(&shell), Some(pt(1013, 500)));
    // A run started on the Pin ends on the Wire partway: a T.
    click(&mut shell, at(6.0, -144.0));
    assert!(click(&mut shell, at(500.0, -150.0)).is_empty());
    assert!(click(&mut shell, at(500.0, 500.0)).is_empty());
    let ran = click(&mut shell, at(1018.0, 498.0));
    assert_eq!(ran, ["schematic.wire.add"]);
    assert!(wires(&shell).contains(&((500, 500), (1013, 500))));
}

/// The objects the canvas draws that are not in the Project: what a Tool previews.
fn previewed(shell: &Shell) -> Vec<Uuid> {
    let project = shell.engine().project().unwrap();
    let tab = shell.document_tab(sheet()).unwrap();
    let mut out: Vec<Uuid> = tab
        .scene
        .scene
        .items
        .iter()
        .map(|i| i.object)
        .filter(|o| *o != sheet() && project.object(*o).is_none())
        .collect();
    out.dedup();
    out
}

/// The sheet's Net Labels as (name, at in mil, rotation in degrees), in name order.
fn labels(shell: &Shell) -> Vec<(String, (i64, i64), i64)> {
    let project = shell.engine().project().unwrap();
    let mut out: Vec<_> = project
        .schematic_sheet(sheet())
        .unwrap()
        .net_labels()
        .map(|l| {
            (
                l.name.clone(),
                (l.at.x.0 / 25_400, l.at.y.0 / 25_400),
                l.rotation.0 / 1000,
            )
        })
        .collect();
    out.sort();
    out
}

#[test]
fn the_net_label_tool_previews_turns_and_places_one_label_per_click() {
    let dir = TempDir::new("wiring-label");
    let mut shell = shell(&dir);
    let before = shell.engine().journal().len();
    assert_eq!(press(&mut shell, "N"), ["tool.activate:net_label"]);
    assert_eq!(shell.tool(), Tool::NetLabel);
    hover(&mut shell, at(510.0, 490.0));
    // Previewed in the Scene, not in the Project.
    assert_eq!(previewed(&shell).len(), 1);
    assert!(labels(&shell).is_empty());

    assert_eq!(
        click(&mut shell, at(510.0, 490.0)),
        ["schematic.net_label.add"]
    );
    // Space turns the next one; the names count up.
    assert_eq!(press(&mut shell, "Space"), ["tool.rotate"]);
    click(&mut shell, at(1000.0, 0.0));
    assert_eq!(press(&mut shell, "X"), ["tool.mirror:x"]);
    click(&mut shell, at(1500.0, 0.0));
    assert_eq!(
        labels(&shell),
        [
            ("NET1".to_owned(), (500, 500), 0),
            ("NET2".to_owned(), (1000, 0), 90),
            ("NET3".to_owned(), (1500, 0), 90),
        ]
    );
    assert_eq!(
        journal_since(&shell, before),
        [
            "tool.activate:net_label",
            "schematic.net_label.add",
            "tool.rotate",
            "schematic.net_label.add",
            "tool.mirror:x",
            "schematic.net_label.add",
        ]
    );
    // Escape ends the Tool and the preview.
    hover(&mut shell, at(2000.0, 0.0));
    assert_eq!(previewed(&shell).len(), 1);
    press(&mut shell, "Esc");
    pointer(&mut shell, &[]);
    assert!(previewed(&shell).is_empty());
    assert_eq!(labels(&shell).len(), 3);
}

#[test]
fn the_port_power_port_and_no_connect_tools_place_on_click() {
    let dir = TempDir::new("wiring-ports");
    let mut shell = shell(&dir);
    place(&mut shell, 0, 0);
    let before = shell.engine().journal().len();

    assert_eq!(press(&mut shell, "P"), ["tool.activate:port"]);
    hover(&mut shell, at(500.0, 0.0));
    assert_eq!(click(&mut shell, at(500.0, 0.0)), ["schematic.port.add"]);
    assert_eq!(press(&mut shell, "B"), ["tool.activate:power_port"]);
    hover(&mut shell, at(1000.0, 0.0));
    assert_eq!(press(&mut shell, "Y"), ["tool.mirror:y"]);
    assert_eq!(press(&mut shell, "Space"), ["tool.rotate"]);
    click(&mut shell, at(1000.0, 0.0));
    click(&mut shell, at(1500.0, 0.0));
    // A No-Connect Marker goes on the Pin the pointer snaps to.
    assert_eq!(press(&mut shell, "Q"), ["tool.activate:no_connect"]);
    hover(&mut shell, at(3.0, -147.0));
    assert_eq!(
        click(&mut shell, at(3.0, -147.0)),
        ["schematic.no_connect.add"]
    );

    let project = shell.engine().project().unwrap();
    let body = project.schematic_sheet(sheet()).unwrap();
    let ports: Vec<_> = body.ports().map(|p| (p.name.clone(), p.at)).collect();
    assert_eq!(ports, [("PORT1".to_owned(), pt(500, 0))]);
    let mut power: Vec<_> = body
        .power_ports()
        .map(|p| (p.name.clone(), p.at, p.rotation.0 / 1000))
        .collect();
    power.sort_by_key(|p| p.1);
    // Mirror Y then a quarter turn: 0 -> 0 -> 90; Power Ports keep their name.
    assert_eq!(
        power,
        [
            ("GND".to_owned(), pt(1000, 0), 90),
            ("GND".to_owned(), pt(1500, 0), 90),
        ]
    );
    let markers: Vec<_> = body.no_connects().map(|m| m.at).collect();
    assert_eq!(markers, [pt(0, -150)]);
    assert_eq!(
        journal_since(&shell, before)
            .iter()
            .filter(|id| id.starts_with("schematic."))
            .count(),
        4
    );
}

#[test]
fn space_x_and_y_turn_a_move_in_flight_and_it_commits_as_one_batch() {
    let dir = TempDir::new("wiring-move-turn");
    let mut shell = shell(&dir);
    let r1 = place(&mut shell, 0, 0);
    shell
        .run("selection.set", json!({ "objects": [r1] }))
        .unwrap();
    hover(&mut shell, at(0.0, 0.0));
    let project = shell.engine().project().unwrap().clone();
    let before = shell.engine().journal().len();

    assert_eq!(press(&mut shell, "M"), ["tool.activate:move"]);
    hover(&mut shell, at(0.0, 0.0));
    let generation = shell.document_tab(sheet()).unwrap().scene.generation;
    assert_eq!(press(&mut shell, "Space"), ["tool.rotate"]);
    hover(&mut shell, at(1000.0, 0.0));
    // Drawn from a preview Scene now; still nothing committed.
    let tab = shell.document_tab(sheet()).unwrap();
    assert_ne!(tab.scene.generation, generation);
    assert!(tab.live.moving.is_empty());
    assert!(shell.engine().project().unwrap() == &project);
    assert_eq!(press(&mut shell, "Y"), ["tool.mirror:y"]);

    let ran = click(&mut shell, at(1000.0, 0.0));
    assert_eq!(ran, ["tool.activate:select", "batch"]);
    let unit = shell
        .engine()
        .project()
        .unwrap()
        .schematic_sheet(sheet())
        .unwrap()
        .unit(r1)
        .unwrap()
        .clone();
    assert_eq!(unit.at, pt(1000, 0));
    // A quarter turn, then flipped top to bottom: mirrored, at 180 - 90 = 90 degrees.
    assert!(unit.mirror);
    assert_eq!(unit.rotation.0, 90_000);
    let ids = journal_since(&shell, before);
    assert_eq!(
        ids,
        [
            "tool.activate:move",
            "tool.rotate",
            "tool.mirror:y",
            "tool.activate:select",
            "batch"
        ]
    );
    // One undo puts it all back.
    shell
        .run("edit.undo", json!({ "document": SHEET }))
        .unwrap();
    assert!(shell.engine().project().unwrap() == &project);
}

#[test]
fn space_with_nothing_in_flight_turns_the_selection() {
    let dir = TempDir::new("wiring-turn-selection");
    let mut shell = shell(&dir);
    let r1 = place(&mut shell, 0, 0);
    // Nothing selected: nothing to turn.
    press(&mut shell, "Space");
    shell
        .run("selection.set", json!({ "objects": [r1] }))
        .unwrap();
    let before = shell.engine().journal().len();
    press(&mut shell, "Space");
    press(&mut shell, "X");
    assert_eq!(
        journal_since(&shell, before),
        [
            "tool.rotate",
            "schematic.rotate",
            "tool.mirror:x",
            "schematic.mirror"
        ]
    );
}

#[test]
fn an_agent_deleting_the_wire_a_run_starts_on_cancels_the_run() {
    use reda_ui::engine::Source;
    use reda_ui::engine::command::{CancelToken, Signals};
    let dir = TempDir::new("wiring-bridged");
    let mut shell = shell(&dir);
    wire(&mut shell, &[(0, 0), (500, 0)]);
    let w = shell
        .engine()
        .project()
        .unwrap()
        .schematic_sheet(sheet())
        .unwrap()
        .wires()
        .next()
        .unwrap()
        .uuid;
    press(&mut shell, "W");
    click(&mut shell, at(500.0, 0.0));
    assert!(shell.tool_preview().is_some());

    shell
        .run_bridged(
            Source::Mcp,
            "schematic.delete",
            json!({ "objects": [w] }),
            &Signals::new(CancelToken::default()),
        )
        .unwrap();
    assert!(shell.note().is_some());
    hover(&mut shell, at(500.0, 500.0));
    assert!(run_points(&shell).is_empty());
    assert!(overlay(&shell).is_empty());
    assert_eq!(shell.tool(), Tool::Wire);
}
