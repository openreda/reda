//! The schematic editor on a Shell, without a window: pointer events and keys in, Commands
//! out. Selection, box selection, drag versus move, the Move and Drag Tools, editing keys
//! and the snap grid.

use std::path::{Path, PathBuf};

use egui::Modifiers;
use reda_testkit::TempDir;
use reda_ui::bars;
use reda_ui::canvas::interact::Pointer;
use reda_ui::engine::Engine;
use reda_ui::engine::command::{FixedClock, SeededIds};
use reda_ui::engine::model::Uuid;
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

fn uuid(v: &Value) -> Uuid {
    v.as_str().unwrap().parse().unwrap()
}

/// A point in mil, as the camera gives it.
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
    uuid(&out["result"]["unit"])
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
    uuid(&out["result"]["wires"][0])
}

/// One mil per screen point: picks reach 5 mil, snaps 10 mil.
fn scale() -> Scale {
    Scale::at(MIL)
}

/// Feeds `events` to the editor and runs the Commands it asks for; returns their ids.
fn pointer(shell: &mut Shell, events: &[Pointer]) -> Vec<String> {
    let commands = schematic_editor::pointer(shell, sheet(), events, scale());
    run_all(shell, commands)
}

/// Presses `key` on the sheet's canvas: runs what the default Keymap binds it to there,
/// as the window does; nothing when the binding cannot run now.
fn press_keys(shell: &mut Shell, key: &str) -> Vec<String> {
    let chord = Chord::parse(key).unwrap();
    let active = keymap::active_for(shell, Some(sheet()));
    let Some(action) = shell.keymap().lookup(&chord, &active).cloned() else {
        return Vec::new();
    };
    match bars::resolve_for(shell, &action, Some(sheet())).run {
        Ok(command) => run_all(shell, vec![command]),
        Err(_) => Vec::new(),
    }
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

fn click(shell: &mut Shell, p: (f64, f64), modifiers: Modifiers) -> Vec<String> {
    pointer(shell, &[Pointer::Click { at: p, modifiers }])
}

fn selected(shell: &Shell) -> Vec<Uuid> {
    shell.selection().objects.clone()
}

/// The Journal's successful entries' ids from entry `from` on.
fn journal_since(shell: &Shell, from: usize) -> Vec<String> {
    shell.engine().journal()[from..]
        .iter()
        .filter(|e| e.outcome.is_ok())
        .map(|e| e.id.clone())
        .collect()
}

fn wire_ends(shell: &Shell, wire: Uuid) -> ((i64, i64), (i64, i64)) {
    let sheet = shell
        .engine()
        .project()
        .unwrap()
        .schematic_sheet(sheet())
        .unwrap();
    let w = sheet.wire(wire).unwrap();
    let mil = |p: reda_ui::engine::model::geom::Point| (p.x.0 / 25_400, p.y.0 / 25_400);
    let [first, last] = w.ends();
    (mil(first), mil(last))
}

fn wire_count(shell: &Shell) -> usize {
    let project = shell.engine().project().unwrap();
    project.schematic_sheet(sheet()).unwrap().wires().count()
}

#[test]
fn click_selects_shift_adds_mod_toggles_and_empty_clears() {
    let dir = TempDir::new("editor-click");
    let mut shell = shell(&dir);
    let r1 = place(&mut shell, 0, 0);
    let w = wire(&mut shell, &[(1000, 0), (1500, 0)]);

    assert_eq!(
        click(&mut shell, at(0.0, 120.0), Modifiers::NONE),
        ["selection.set"]
    );
    assert_eq!(selected(&shell), [r1]);
    click(&mut shell, at(1200.0, 2.0), Modifiers::SHIFT);
    assert_eq!(selected(&shell), [r1, w]);
    click(&mut shell, at(0.0, 120.0), Modifiers::COMMAND);
    assert_eq!(selected(&shell), [w]);
    // Clicking the same again changes nothing, so it runs nothing.
    assert!(click(&mut shell, at(1200.0, 0.0), Modifiers::NONE).is_empty());
    // A Shift-click on nothing keeps the selection; a plain one clears it.
    click(&mut shell, at(3000.0, 3000.0), Modifiers::SHIFT);
    assert_eq!(selected(&shell), [w]);
    click(&mut shell, at(3000.0, 3000.0), Modifiers::NONE);
    assert!(selected(&shell).is_empty());
}

#[test]
fn a_box_left_to_right_encloses_and_right_to_left_touches() {
    let dir = TempDir::new("editor-box");
    let mut shell = shell(&dir);
    let w1 = wire(&mut shell, &[(1000, 0), (1500, 0)]);
    let w2 = wire(&mut shell, &[(1000, 500), (2500, 500)]);
    let drag = |from: (f64, f64), to: (f64, f64)| {
        [
            Pointer::DragStart {
                from,
                at: from,
                modifiers: Modifiers::NONE,
            },
            Pointer::Drag(to),
            Pointer::DragEnd(to),
        ]
    };
    // Mid-drag the box shows, and nothing is selected yet.
    let box_events = drag(at(900.0, -100.0), at(2000.0, 600.0));
    pointer(&mut shell, &box_events[..2]);
    let tab = shell.document_tab(sheet()).unwrap();
    assert!(matches!(tab.interaction, Interaction::Boxing { .. }));
    assert!(tab.overlay.rubber_band.is_some());
    pointer(&mut shell, &box_events[2..]);
    assert_eq!(selected(&shell), [w1]);
    assert!(
        shell
            .document_tab(sheet())
            .unwrap()
            .overlay
            .rubber_band
            .is_none()
    );

    pointer(&mut shell, &drag(at(2000.0, 600.0), at(900.0, -100.0)));
    assert_eq!(selected(&shell), [w1, w2]);
}

#[test]
fn a_plain_drag_previews_then_commits_one_drag_that_keeps_connections() {
    let dir = TempDir::new("editor-drag");
    let mut shell = shell(&dir);
    let r1 = place(&mut shell, 0, 0);
    let up = wire(&mut shell, &[(0, 150), (0, 500)]);
    let before = shell.engine().journal().len();
    let project = shell.engine().project().unwrap().clone();

    // Press on the Unit (its Pin 1 stub), drag right by 304 mil: it snaps to the 50 mil
    // grid (300).
    pointer(
        &mut shell,
        &[
            Pointer::DragStart {
                from: at(0.0, 120.0),
                at: at(100.0, 120.0),
                modifiers: Modifiers::NONE,
            },
            Pointer::Drag(at(304.0, 120.0)),
        ],
    );
    // In flight: drawn at the offset, the attached Wire redrawn bent; nothing committed.
    let tab = shell.document_tab(sheet()).unwrap();
    assert_eq!(tab.moving_offset, (300.0 * MIL, 0.0));
    assert!(tab.live.moving.contains(&r1));
    assert_eq!(&tab.live.hidden[..], [up]);
    assert_eq!(tab.overlay.wires.len(), 2);
    assert!(shell.engine().project().unwrap() == &project);
    assert_eq!(journal_since(&shell, before), ["selection.set"]);

    let ran = pointer(&mut shell, &[Pointer::DragEnd(at(304.0, 120.0))]);
    assert_eq!(ran, ["schematic.drag"]);
    assert_eq!(
        journal_since(&shell, before),
        ["selection.set", "schematic.drag"]
    );
    // The Wire's far end stayed, and it turns a corner to the moved Pin.
    assert_eq!(wire_ends(&shell, up), ((300, 150), (0, 500)));
    assert_eq!(wire_count(&shell), 1);
    let tab = shell.document_tab(sheet()).unwrap();
    assert_eq!(tab.interaction, Interaction::Idle);
    assert!(tab.live.moving.is_empty() && tab.overlay.wires.is_empty());
}

#[test]
fn mod_drag_detaches_and_commits_one_move() {
    let dir = TempDir::new("editor-move");
    let mut shell = shell(&dir);
    place(&mut shell, 0, 0);
    let up = wire(&mut shell, &[(0, 150), (0, 500)]);
    let ran = pointer(
        &mut shell,
        &[
            Pointer::DragStart {
                from: at(0.0, 120.0),
                at: at(0.0, 20.0),
                modifiers: Modifiers::COMMAND,
            },
            Pointer::DragEnd(at(0.0, -80.0)),
        ],
    );
    assert_eq!(ran, ["selection.set", "schematic.move"]);
    assert_eq!(wire_ends(&shell, up), ((0, 150), (0, 500)));
    assert_eq!(wire_count(&shell), 1);
}

#[test]
fn the_move_and_drag_tools_carry_the_selection_until_a_click() {
    let dir = TempDir::new("editor-tools");
    let mut shell = shell(&dir);
    let r1 = place(&mut shell, 0, 0);
    let up = wire(&mut shell, &[(0, 150), (0, 500)]);
    // With nothing selected, M is refused.
    assert!(shell.run("tool.activate:move", json!({})).is_err());
    shell
        .run("selection.set", json!({ "objects": [r1] }))
        .unwrap();
    pointer(&mut shell, &[Pointer::Hover(at(0.0, 0.0))]);

    assert_eq!(press_keys(&mut shell, "D"), ["tool.activate:drag"]);
    assert_eq!(shell.tool(), Tool::Drag);
    pointer(&mut shell, &[Pointer::Hover(at(0.0, -100.0))]);
    let tab = shell.document_tab(sheet()).unwrap();
    assert_eq!(tab.moving_offset, (0.0, -100.0 * MIL));
    // Escape puts it back and returns to Select.
    assert_eq!(press_keys(&mut shell, "Esc"), ["tool.activate:select"]);
    pointer(&mut shell, &[]);
    assert_eq!(
        shell.document_tab(sheet()).unwrap().moving_offset,
        (0.0, 0.0)
    );

    press_keys(&mut shell, "M");
    pointer(&mut shell, &[Pointer::Hover(at(0.0, 0.0))]);
    let ran = click(&mut shell, at(200.0, 0.0), Modifiers::NONE);
    assert_eq!(ran, ["tool.activate:select", "schematic.move"]);
    assert_eq!(shell.tool(), Tool::Select);
    assert_eq!(wire_ends(&shell, up), ((0, 150), (0, 500)));
}

#[test]
fn keys_delete_copy_paste_duplicate_and_select_all() {
    let dir = TempDir::new("editor-keys");
    let mut shell = shell(&dir);
    let r1 = place(&mut shell, 0, 0);
    let w = wire(&mut shell, &[(1000, 0), (1500, 0)]);

    assert_eq!(press_keys(&mut shell, "Mod+A"), ["selection.set"]);
    let mut all = vec![r1, w];
    all.sort();
    assert_eq!(selected(&shell), all);

    shell
        .run("selection.set", json!({ "objects": [w] }))
        .unwrap();
    assert_eq!(press_keys(&mut shell, "Mod+C"), ["schematic.copy"]);
    assert!(shell.clipboard().is_some());
    // Paste goes at the pointer, snapped to the grid, and selects what it made.
    pointer(&mut shell, &[Pointer::Hover(at(1010.0, 990.0))]);
    assert_eq!(press_keys(&mut shell, "Mod+V"), ["schematic.paste"]);
    let pasted = selected(&shell);
    assert_eq!(pasted.len(), 1);
    assert_eq!(wire_ends(&shell, pasted[0]), ((1000, 1000), (1500, 1000)));

    assert_eq!(press_keys(&mut shell, "Mod+D"), ["schematic.duplicate"]);
    let copy = selected(&shell);
    assert_ne!(copy, pasted);

    assert_eq!(press_keys(&mut shell, "Del"), ["schematic.delete"]);
    assert!(selected(&shell).is_empty());
    assert!(shell.engine().project().unwrap().object(copy[0]).is_none());
    // Cut removes the selection and fills the clipboard.
    shell
        .run("selection.set", json!({ "objects": [r1] }))
        .unwrap();
    assert_eq!(press_keys(&mut shell, "Mod+X"), ["schematic.cut"]);
    assert!(shell.engine().project().unwrap().object(r1).is_none());
    assert!(
        shell.clipboard().unwrap()["units"]
            .as_array()
            .is_some_and(|u| !u.is_empty())
    );
}

#[test]
fn the_grid_starts_at_50_mil_and_g_cycles_within_its_unit() {
    let dir = TempDir::new("editor-grid");
    let mut shell = shell(&dir);
    let grid = |shell: &Shell| shell.document_tab(sheet()).unwrap().grid.0;
    assert_eq!(grid(&shell), 1_270_000);
    press_keys(&mut shell, "G");
    assert_eq!(grid(&shell), 2_540_000);
    press_keys(&mut shell, "G");
    assert_eq!(grid(&shell), 254_000);
    shell
        .run("view.grid.set", json!({"document": SHEET, "grid": "1mm"}))
        .unwrap();
    press_keys(&mut shell, "G");
    assert_eq!(grid(&shell), 2_500_000);
    press_keys(&mut shell, "G");
    assert_eq!(grid(&shell), 250_000);
    let err = shell
        .run("view.grid.set", json!({"document": SHEET, "grid": "3mil"}))
        .unwrap_err();
    assert_eq!(err.details.as_ref().unwrap()["pointer"], "/grid");
    // A drag snaps to the grid in force: 0.25 mm.
    let r1 = place(&mut shell, 0, 0);
    pointer(
        &mut shell,
        &[Pointer::DragStart {
            from: at(0.0, 120.0),
            at: (300_000.0, 120.0 * MIL),
            modifiers: Modifiers::NONE,
        }],
    );
    let tab = shell.document_tab(sheet()).unwrap();
    assert!(tab.live.moving.contains(&r1));
    assert_eq!(tab.moving_offset, (250_000.0, 0.0));
}

#[test]
fn the_pointer_snaps_onto_a_nearby_pin_off_the_grid() {
    let dir = TempDir::new("editor-snap");
    let mut shell = shell(&dir);
    // A Wire whose end is off the 50 mil grid.
    let w = wire(&mut shell, &[(1000, 0), (1000, 520)]);
    place(&mut shell, 0, 0);
    // Pick the Unit by its body, then bring its upper Pin (0, 150) near the Wire's end
    // (1000, 520): it lands on the end, not on the grid.
    pointer(
        &mut shell,
        &[
            Pointer::DragStart {
                from: at(0.0, 150.0),
                at: at(0.0, 150.0),
                modifiers: Modifiers::NONE,
            },
            Pointer::Drag(at(995.0, 516.0)),
        ],
    );
    let tab = shell.document_tab(sheet()).unwrap();
    assert_eq!(tab.moving_offset, (1000.0 * MIL, 370.0 * MIL));
    let _ = w;
}
