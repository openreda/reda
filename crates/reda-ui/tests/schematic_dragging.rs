//! Dragging on a sheet in the GUI, on a Shell without a window (Display When Dragging):
//! the Junctions a drop would make and a chip naming the Net change, Alt keeping a drag
//! straight, Mod+Space turning the rubber-banded corners, Mod+arrows nudging, and the
//! grab Preferences.

use std::path::{Path, PathBuf};

use egui::Modifiers;
use reda_testkit::TempDir;
use reda_ui::bars;
use reda_ui::canvas::interact::Pointer;
use reda_ui::engine::Engine;
use reda_ui::engine::command::{FixedClock, SeededIds, UserDirs};
use reda_ui::engine::model::Uuid;
use reda_ui::engine::model::geom::{Nm, Point};
use reda_ui::keymap::{self, Chord};
use reda_ui::schematic_editor::{self, Interaction, Scale};
use reda_ui::shell::Shell;
use serde_json::{Value, json};

const SHEET: &str = "00000000-0000-4000-8000-000000000002";
const R_10K: &str = "a0000000-0000-4000-8000-000000000100";
const MIL: f64 = 25_400.0;

fn library() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../reda-schematic/tests/library/Test")
}

fn shell(dir: &TempDir, home: &TempDir) -> Shell {
    let engine = Engine::builder()
        .ids(SeededIds::new(0))
        .clock(FixedClock::EPOCH)
        .user_dirs(Some(UserDirs::under(home.path())))
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

fn at(x: f64, y: f64) -> (f64, f64) {
    (x * MIL, y * MIL)
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

fn run_all(shell: &mut Shell, commands: Vec<(String, Value)>) -> Vec<String> {
    commands
        .into_iter()
        .map(|(id, params)| {
            shell.run(&id, params).unwrap();
            id
        })
        .collect()
}

fn pointer(shell: &mut Shell, events: &[Pointer]) -> Vec<String> {
    let commands = schematic_editor::pointer(shell, sheet(), events, Scale::at(MIL));
    run_all(shell, commands)
}

fn press(shell: &mut Shell, key: &str) -> Vec<String> {
    let chord = Chord::parse(key).unwrap();
    let active = keymap::active_for(shell, Some(sheet()));
    let action = shell.keymap().lookup(&chord, &active).cloned().unwrap();
    match bars::resolve_for(shell, &action, Some(sheet())).run {
        Ok(command) => run_all(shell, vec![command]),
        Err(why) => panic!("{key} cannot run: {why}"),
    }
}

#[allow(dead_code)]
fn click(shell: &mut Shell, p: (f64, f64)) -> Vec<String> {
    let modifiers = Modifiers::NONE;
    pointer(shell, &[Pointer::Click { at: p, modifiers }])
}

/// `(designator, x, y)` of every Unit on the sheet, x and y in mil, sorted.
#[allow(dead_code)]
fn units(shell: &Shell) -> Vec<(String, i64, i64)> {
    let project = shell.engine().project().unwrap();
    let mut out: Vec<_> = project
        .schematic_sheet(sheet())
        .unwrap()
        .units()
        .map(|u| (u.designator.clone(), u.at.x.0 / 25_400, u.at.y.0 / 25_400))
        .collect();
    out.sort();
    out
}

fn interaction(shell: &Shell) -> Interaction {
    shell.document_tab(sheet()).unwrap().interaction.clone()
}

fn wire(shell: &mut Shell, points: &[(i64, i64)]) -> Uuid {
    let points: Vec<Value> = points
        .iter()
        .map(|(x, y)| json!({"x": format!("{x}mil"), "y": format!("{y}mil")}))
        .collect();
    let out = shell
        .run(
            "schematic.wire.add",
            json!({"document": SHEET, "points": points}),
        )
        .unwrap();
    out["result"]["wires"][0].as_str().unwrap().parse().unwrap()
}

fn label(shell: &mut Shell, name: &str, x: i64, y: i64) -> Uuid {
    let out = shell
        .run(
            "schematic.net_label.add",
            json!({"document": SHEET, "name": name,
                   "at": {"x": format!("{x}mil"), "y": format!("{y}mil")}}),
        )
        .unwrap();
    out["result"]["net_label"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap()
}

fn mil(x: i64, y: i64) -> Point {
    Point::new(Nm(x * 25_400), Nm(y * 25_400))
}

fn start_drag(shell: &mut Shell, from: (f64, f64), to: (f64, f64)) {
    let modifiers = Modifiers::NONE;
    pointer(
        shell,
        &[
            Pointer::DragStart {
                from,
                at: from,
                modifiers,
            },
            Pointer::Drag(to),
        ],
    );
}

fn offset(shell: &Shell) -> Point {
    match interaction(shell) {
        Interaction::Flight(f) => f.offset,
        other => panic!("no drag in flight: {other:?}"),
    }
}

fn journal(shell: &Shell) -> Vec<(String, Value)> {
    (shell.engine().journal().iter())
        .filter(|e| e.outcome.is_ok())
        .map(|e| (e.id.clone(), e.params.clone()))
        .collect()
}

/// SDA runs down x = 0; a SCL Wire, labelled, sits 1000 mil to the right of it.
fn two_nets(shell: &mut Shell) -> (Uuid, Uuid) {
    wire(shell, &[(0, 0), (0, 1000)]);
    label(shell, "SDA", 0, 1000);
    let scl = wire(shell, &[(1000, 500), (1500, 500)]);
    let l = label(shell, "SCL", 1500, 500);
    shell
        .run("selection.set", json!({"objects": [scl, l]}))
        .unwrap();
    (scl, l)
}

#[test]
fn a_drag_shows_the_junctions_it_would_make_and_names_the_nets_it_would_join() {
    let dir = TempDir::new("drag-check");
    let home = TempDir::new("drag-check-home");
    let mut shell = shell(&dir, &home);
    two_nets(&mut shell);
    // Pressed 100 mil in from its left end (held there, Hot Spot), the SCL Wire goes 200
    // mil left: OK.
    start_drag(&mut shell, at(1100.0, 500.0), at(900.0, 500.0));
    assert_eq!(offset(&shell), mil(-200, 0));
    assert_eq!(
        schematic_editor::drag_chip(&shell, sheet()).as_deref(),
        Some("OK")
    );
    let tab = shell.document_tab(sheet()).unwrap();
    assert!(tab.overlay.junctions.is_empty());
    // Its end on the SDA Wire: a T, so a Junction, and the Nets join.
    pointer(&mut shell, &[Pointer::Drag(at(100.0, 500.0))]);
    assert_eq!(offset(&shell), mil(-1000, 0));
    let chip = schematic_editor::drag_chip(&shell, sheet()).unwrap();
    assert!(
        chip.contains("joins") && chip.contains("SCL") && chip.contains("SDA"),
        "{chip}"
    );
    let tab = shell.document_tab(sheet()).unwrap();
    assert_eq!(&tab.overlay.junctions[..], [mil(0, 500)]);
    // It never blocks the drop.
    assert_eq!(
        pointer(&mut shell, &[Pointer::DragEnd(at(100.0, 500.0))]),
        ["schematic.drag"]
    );
    assert!(schematic_editor::drag_chip(&shell, sheet()).is_none());
}

#[test]
fn a_move_that_pulls_a_net_apart_names_the_split() {
    let dir = TempDir::new("drag-split");
    let home = TempDir::new("drag-split-home");
    let mut shell = shell(&dir, &home);
    wire(&mut shell, &[(0, 0), (0, 1000)]);
    label(&mut shell, "SDA", 0, 1000);
    let branch = wire(&mut shell, &[(0, 500), (500, 500)]);
    shell
        .run("selection.set", json!({"objects": [branch]}))
        .unwrap();
    // The Move Tool does not rubber-band: the branch comes off the SDA trunk.
    pointer(&mut shell, &[Pointer::Hover(at(500.0, 500.0))]);
    assert_eq!(press(&mut shell, "M"), ["tool.activate:move"]);
    pointer(&mut shell, &[Pointer::Hover(at(500.0, 500.0))]);
    assert_eq!(
        schematic_editor::drag_chip(&shell, sheet()).as_deref(),
        Some("OK")
    );
    pointer(&mut shell, &[Pointer::Hover(at(600.0, 800.0))]);
    let chip = schematic_editor::drag_chip(&shell, sheet());
    assert_eq!(
        chip.as_deref(),
        Some("splits SDA"),
        "{:?}",
        interaction(&shell)
    );
}

#[test]
fn with_display_when_dragging_off_there_is_no_chip_and_no_junctions() {
    let dir = TempDir::new("drag-off");
    let home = TempDir::new("drag-off-home");
    let mut shell = shell(&dir, &home);
    shell
        .run(
            "app.preferences.set",
            json!({"key": "schematic.display_when_dragging", "value": false}),
        )
        .unwrap();
    two_nets(&mut shell);
    start_drag(&mut shell, at(1100.0, 500.0), at(100.0, 500.0));
    assert_eq!(offset(&shell), mil(-1000, 0));
    assert!(schematic_editor::drag_chip(&shell, sheet()).is_none());
    assert!(
        shell
            .document_tab(sheet())
            .unwrap()
            .overlay
            .junctions
            .is_empty()
    );
}

#[test]
fn alt_keeps_a_drag_horizontal_or_vertical() {
    let dir = TempDir::new("drag-alt");
    let home = TempDir::new("drag-alt-home");
    let mut shell = shell(&dir, &home);
    two_nets(&mut shell);
    shell.hold_snap_keys(sheet(), Modifiers::ALT, true);
    start_drag(&mut shell, at(1100.0, 500.0), at(800.0, 650.0));
    assert_eq!(offset(&shell), mil(-300, 0));
    pointer(&mut shell, &[Pointer::Drag(at(1000.0, 1100.0))]);
    assert_eq!(offset(&shell), mil(0, 600));
    // Let go of Alt: the drag follows the pointer again.
    shell.hold_snap_keys(sheet(), Modifiers::NONE, false);
    pointer(&mut shell, &[Pointer::Drag(at(800.0, 650.0))]);
    assert_eq!(offset(&shell), mil(-300, 150));
}

fn vertices(shell: &Shell, wire: Uuid) -> Vec<(i64, i64)> {
    let project = shell.engine().project().unwrap();
    let w = project
        .schematic_sheet(sheet())
        .unwrap()
        .wire(wire)
        .unwrap();
    w.vertices
        .iter()
        .map(|p| (p.x.0 / 25_400, p.y.0 / 25_400))
        .collect()
}

#[test]
fn mod_space_during_a_drag_turns_the_rubber_banded_corners_the_other_way() {
    let corner = |flip: bool| {
        let dir = TempDir::new("drag-flip");
        let home = TempDir::new("drag-flip-home");
        let mut shell = shell(&dir, &home);
        let r1 = place(&mut shell, 0, 0);
        let up = wire(&mut shell, &[(0, 150), (0, 500)]);
        shell
            .run("selection.set", json!({"objects": [r1]}))
            .unwrap();
        start_drag(&mut shell, at(0.0, 120.0), at(300.0, 120.0));
        let shown = shell.document_tab(sheet()).unwrap().overlay.wires.clone();
        if flip {
            assert_eq!(press(&mut shell, "Mod+Space"), ["tool.cycle_corner"]);
            let flipped = shell.document_tab(sheet()).unwrap().overlay.wires.clone();
            assert_ne!(shown, flipped, "the preview turns too");
        }
        pointer(&mut shell, &[Pointer::DragEnd(at(300.0, 120.0))]);
        let (id, params) = journal(&shell).pop().unwrap();
        assert_eq!(id, "schematic.drag");
        assert_eq!(
            params.get("flip_corners").and_then(Value::as_bool),
            flip.then_some(true)
        );
        vertices(&shell, up)
    };
    let plain = corner(false);
    let flipped = corner(true);
    assert_eq!(plain.first(), flipped.first());
    assert_eq!(plain.last(), flipped.last());
    assert_ne!(plain, flipped);
}

#[test]
fn mod_arrows_nudge_the_selection_one_grid_or_ten_one_undo_step_each() {
    let dir = TempDir::new("nudge");
    let home = TempDir::new("nudge-home");
    let mut shell = shell(&dir, &home);
    let (scl, _) = two_nets(&mut shell);
    let before = journal(&shell).len();
    assert_eq!(press(&mut shell, "Mod+Right"), ["schematic.nudge"]);
    assert_eq!(vertices(&shell, scl), [(1050, 500), (1550, 500)]);
    press(&mut shell, "Shift+Mod+Up");
    assert_eq!(vertices(&shell, scl), [(1050, 1000), (1550, 1000)]);
    let drags: Vec<_> = journal(&shell)[before..]
        .iter()
        .filter(|(id, _)| id == "schematic.drag")
        .map(|(_, p)| p["by"].clone())
        .collect();
    assert_eq!(
        drags,
        [
            json!({"x": 50 * 25_400, "y": 0}),
            json!({"x": 0, "y": 500 * 25_400})
        ]
    );
    shell.run("edit.undo", json!({"document": SHEET})).unwrap();
    assert_eq!(vertices(&shell, scl), [(1050, 500), (1550, 500)]);
}

#[test]
fn the_grab_preferences_pick_where_a_moved_object_is_held() {
    let grabbed = |hot: bool, center: bool| {
        let dir = TempDir::new("grab");
        let home = TempDir::new("grab-home");
        let mut shell = shell(&dir, &home);
        for (key, value) in [
            ("editing.grab_hot_spot", hot),
            ("editing.grab_center", center),
        ] {
            shell
                .run("app.preferences.set", json!({"key": key, "value": value}))
                .unwrap();
        }
        let w = wire(&mut shell, &[(1000, 500), (1500, 500)]);
        shell.run("selection.set", json!({"objects": [w]})).unwrap();
        // Pressed at 1180 mil: the held point keeps its distance from the pointer and is
        // what snaps.
        start_drag(&mut shell, at(1180.0, 500.0), at(1180.0, 500.0));
        match interaction(&shell) {
            Interaction::Flight(f) => (f.anchor.x.0 / 25_400, f.anchor.y.0 / 25_400),
            other => panic!("no drag in flight: {other:?}"),
        }
    };
    // Hot Spot: the nearest end (1000), and it overrides Center.
    assert_eq!(grabbed(true, true), (1000, 500));
    // Center: the Wire's middle (1250).
    assert_eq!(grabbed(false, true), (1250, 500));
    // Neither: where it was pressed, on the grid (1200).
    assert_eq!(grabbed(false, false), (1200, 500));
}
