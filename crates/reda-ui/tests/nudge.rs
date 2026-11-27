//! Nudging the selection (docs/spec/v0.3/input-and-defaults.md#nudging) on a Shell
//! without a window: `Mod`+arrows nudge it one grid step and Shift+`Mod`+arrows ten in
//! the PCB, Footprint and Symbol editors as in the Schematic editor. The step is measured
//! at the selection's first object (from off the grid the first press lands on the next
//! grid line), each press is one undo step, connections rubber-band as in a drag and
//! locked objects stay put.

use std::path::{Path, PathBuf};

use reda_testkit::TempDir;
use reda_ui::bars;
use reda_ui::engine::Engine;
use reda_ui::engine::command::{FixedClock, SeededIds, UserDirs};
use reda_ui::engine::model::geom::{Nm, Point};
use reda_ui::engine::model::{LibraryElement, Shape, Uuid};
use reda_ui::keymap::{self, Chord};
use reda_ui::shell::Shell;
use serde_json::{Value, json};

const MM: i64 = 1_000_000;

fn library() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../reda-schematic/tests/library/Test")
}

struct Board {
    shell: Shell,
    pcb: Uuid,
    _home: TempDir,
    _work: TempDir,
}

/// A Shell on an empty board, its tab active on a 1 mm grid.
fn board() -> Board {
    let home = TempDir::new("nudge-home");
    let work = TempDir::new("nudge-work");
    let engine = Engine::builder()
        .ids(SeededIds::new(0))
        .clock(FixedClock::EPOCH)
        .user_dirs(Some(UserDirs::under(home.path())))
        .starter_library(Some(library()))
        .commands(reda_testkit::probe_commands)
        .headless(false)
        .build();
    let mut shell = Shell::new(engine);
    shell.new_project(work.path(), "Board").unwrap();
    let pcb = shell.engine().project().unwrap().pcb().unwrap().uuid;
    for (id, params) in [
        ("document.open", json!({"document": pcb})),
        ("layout.tab.activate", json!({"tab": pcb.to_string()})),
        ("view.grid.set", json!({"document": pcb, "grid": "1mm"})),
    ] {
        shell.run(id, params).unwrap();
    }
    Board {
        shell,
        pcb,
        _home: home,
        _work: work,
    }
}

fn uuid(v: &Value) -> Uuid {
    v.as_str().unwrap().parse().unwrap()
}

fn mm(x: f64, y: f64) -> Point {
    Point::new(
        Nm((x * MM as f64).round() as i64),
        Nm((y * MM as f64).round() as i64),
    )
}

/// Places a Footprint with no Pads at `at` (mm).
fn footprint(b: &mut Board, x: f64, y: f64) -> Uuid {
    let out = b
        .shell
        .run(
            "test.footprint.add",
            json!({"at": {"x": format!("{x}mm"), "y": format!("{y}mm")}, "pads": []}),
        )
        .unwrap();
    uuid(&out["result"]["footprint"])
}

fn footprint_at(b: &Board, f: Uuid) -> Point {
    let project = b.shell.engine().project().unwrap();
    project.pcb().unwrap().footprint(f).unwrap().at
}

fn select(shell: &mut Shell, objects: &[Uuid]) {
    shell
        .run("selection.set", json!({"objects": objects}))
        .unwrap();
}

/// Presses `key` on `document`'s canvas: runs what the Keymap binds it to there and
/// returns the Command's ID.
fn press(shell: &mut Shell, document: Uuid, key: &str) -> String {
    let chord = Chord::parse(key).unwrap();
    let active = keymap::active_for(shell, Some(document));
    let action = shell.keymap().lookup(&chord, &active).cloned().unwrap();
    let (id, params) = match bars::resolve_for(shell, &action, Some(document)).run {
        Ok(command) => command,
        Err(why) => panic!("{key} cannot run: {why}"),
    };
    shell.run(&id, params).unwrap();
    id
}

/// The Commands run since the Journal held `from` entries, with their params.
fn ran_since(shell: &Shell, from: usize) -> Vec<(String, Value)> {
    shell.engine().journal()[from..]
        .iter()
        .filter(|e| e.outcome.is_ok())
        .map(|e| (e.id.clone(), e.params.clone()))
        .collect()
}

fn edits_since(shell: &Shell, from: usize) -> Vec<String> {
    ran_since(shell, from)
        .into_iter()
        .map(|(id, _)| id)
        .filter(|id| !id.ends_with(".nudge"))
        .collect()
}

#[test]
fn mod_arrows_nudge_footprints_from_the_first_ones_grid_step_one_undo_step_each() {
    let mut b = board();
    // The first selected Footprint is off the grid; the second is on it.
    let first = footprint(&mut b, 2.1, 0.1);
    let second = footprint(&mut b, 5.0, 5.0);
    select(&mut b.shell, &[first, second]);
    let before = b.shell.engine().journal().len();
    assert_eq!(press(&mut b.shell, b.pcb, "Mod+Right"), "pcb.nudge");
    // The first lands on the next grid line to the right; the second keeps its distance.
    assert_eq!(footprint_at(&b, first), mm(3.0, 0.1));
    assert_eq!(footprint_at(&b, second), mm(5.9, 5.0));
    assert_eq!(press(&mut b.shell, b.pcb, "Shift+Mod+Up"), "pcb.nudge");
    // Up from y = 0.1: the first of the ten steps lands on y = 1.
    assert_eq!(footprint_at(&b, first), mm(3.0, 10.0));
    assert_eq!(footprint_at(&b, second), mm(5.9, 14.9));
    assert_eq!(press(&mut b.shell, b.pcb, "Mod+Left"), "pcb.nudge");
    assert_eq!(footprint_at(&b, first), mm(2.0, 10.0));
    assert_eq!(press(&mut b.shell, b.pcb, "Mod+Down"), "pcb.nudge");
    assert_eq!(footprint_at(&b, first), mm(2.0, 9.0));
    // One edit per press.
    assert_eq!(
        edits_since(&b.shell, before),
        ["pcb.move", "pcb.move", "pcb.move", "pcb.move"]
    );
    let pcb = b.pcb;
    b.shell.run("edit.undo", json!({"document": pcb})).unwrap();
    assert_eq!(footprint_at(&b, first), mm(2.0, 10.0));
    assert_eq!(footprint_at(&b, second), mm(4.9, 14.9));
}

/// A Via at (4, 4) mm with a Track from (0, 4) mm ending on it.
fn via_with_track(b: &mut Board) -> (Uuid, Uuid) {
    b.shell
        .run(
            "pcb.track.add",
            json!({"layer": "copper_top", "width": "0.3mm",
                   "points": [{"x": 0, "y": "4mm"}, {"x": "4mm", "y": "4mm"}]}),
        )
        .unwrap();
    b.shell
        .run("pcb.via.add", json!({"at": {"x": "4mm", "y": "4mm"}}))
        .unwrap();
    let pcb = b.shell.engine().project().unwrap().pcb().unwrap();
    let via = pcb.vias().next().unwrap().uuid;
    let track = pcb.tracks().next().unwrap().uuid;
    (via, track)
}

fn track_ends(b: &Board, track: Uuid) -> (Point, Point) {
    let project = b.shell.engine().project().unwrap();
    project.pcb().unwrap().track(track).unwrap().geometry.ends()
}

fn via_at(b: &Board, via: Uuid) -> Point {
    let project = b.shell.engine().project().unwrap();
    project.pcb().unwrap().via(via).unwrap().at
}

#[test]
fn a_nudged_via_keeps_its_tracks_attached_as_a_drag_does() {
    let mut b = board();
    let (via, track) = via_with_track(&mut b);
    select(&mut b.shell, &[via]);
    let before = b.shell.engine().journal().len();
    press(&mut b.shell, b.pcb, "Mod+Right");
    assert_eq!(via_at(&b, via), mm(5.0, 4.0));
    // The Track's end on the Via follows it (angle keeping on, its free far end comes
    // along too).
    assert_eq!(track_ends(&b, track).1, mm(5.0, 4.0));
    assert_eq!(edits_since(&b.shell, before), ["pcb.copper.drag"]);
    let pcb = b.pcb;
    b.shell.run("edit.undo", json!({"document": pcb})).unwrap();
    assert_eq!(via_at(&b, via), mm(4.0, 4.0));
    assert_eq!(track_ends(&b, track), (mm(0.0, 4.0), mm(4.0, 4.0)));
    // With angle keeping off, as for a drag, the far end stays and the Track turns.
    b.shell
        .run(
            "app.preferences.set",
            json!({"key": "pcb.drag.preserve_angle", "value": false}),
        )
        .unwrap();
    press(&mut b.shell, b.pcb, "Mod+Up");
    assert_eq!(via_at(&b, via), mm(4.0, 5.0));
    assert_eq!(track_ends(&b, track), (mm(0.0, 4.0), mm(4.0, 5.0)));
}

#[test]
fn a_nudged_track_segment_slides_across_itself() {
    let mut b = board();
    let (_, track) = via_with_track(&mut b);
    select(&mut b.shell, &[track]);
    let before = b.shell.engine().journal().len();
    press(&mut b.shell, b.pcb, "Mod+Up");
    assert_eq!(edits_since(&b.shell, before), ["pcb.track.drag"]);
    assert_eq!(track_ends(&b, track).0, mm(0.0, 5.0));
}

/// The Command `key` gives on `document`'s canvas, with its params.
fn bound(shell: &Shell, document: Uuid, key: &str) -> (String, Value) {
    let chord = Chord::parse(key).unwrap();
    let active = keymap::active_for(shell, Some(document));
    let action = shell.keymap().lookup(&chord, &active).cloned().unwrap();
    bars::resolve_for(shell, &action, Some(document))
        .run
        .unwrap()
}

#[test]
fn locked_objects_stay_put_and_the_step_is_measured_at_the_first_that_moves() {
    let mut b = board();
    let locked = footprint(&mut b, 0.5, 0.5);
    let free = footprint(&mut b, 2.25, 3.0);
    b.shell
        .run("pcb.lock", json!({"objects": [locked], "locked": true}))
        .unwrap();
    // Even with Protect Locked Objects off a nudge leaves a locked object behind.
    b.shell
        .run(
            "app.preferences.set",
            json!({"key": "editing.protect_locked_objects", "value": false}),
        )
        .unwrap();
    select(&mut b.shell, &[locked, free]);
    press(&mut b.shell, b.pcb, "Mod+Right");
    assert_eq!(footprint_at(&b, locked), mm(0.5, 0.5));
    assert_eq!(footprint_at(&b, free), mm(3.0, 3.0));
    // With only locked objects selected the nudge is refused and nothing moves.
    select(&mut b.shell, &[locked]);
    let before = b.shell.engine().journal().len();
    let (id, params) = bound(&b.shell, b.pcb, "Mod+Right");
    assert!(b.shell.run(&id, params).is_err());
    assert_eq!(footprint_at(&b, locked), mm(0.5, 0.5));
    assert!(edits_since(&b.shell, before).is_empty());
}

#[test]
fn on_a_board_seen_from_below_mod_right_moves_towards_smaller_x() {
    let mut b = board();
    let f = footprint(&mut b, 2.0, 0.0);
    select(&mut b.shell, &[f]);
    b.shell
        .run("pcb.view.from_bottom", json!({"from_bottom": true}))
        .unwrap();
    press(&mut b.shell, b.pcb, "Mod+Right");
    assert_eq!(footprint_at(&b, f), mm(1.0, 0.0));
}

const LIBRARY: &str = "000000aa-0000-4000-8000-000000000000";

/// A Shell with no Project, a user Library "Mine" and a new `kind` element in it, its tab
/// active on a `grid` grid.
fn element(home: &TempDir, kind: &str, grid: &str) -> (Shell, Uuid) {
    let lib = home.path().join("lib");
    std::fs::create_dir_all(&lib).unwrap();
    let info = reda_ui::engine::model::LibraryInfo {
        uuid: LIBRARY.parse().unwrap(),
        name: "Mine".into(),
        description: String::new(),
    };
    std::fs::write(
        lib.join("Mine.redalib"),
        reda_ui::engine::format::library::write_library_info(&info).unwrap(),
    )
    .unwrap();
    let engine = Engine::builder()
        .ids(SeededIds::new(0))
        .clock(FixedClock::EPOCH)
        .user_dirs(Some(UserDirs::under(&home.path().join("home"))))
        .starter_library(Some(library()))
        .headless(false)
        .build();
    let mut shell = Shell::new(engine);
    shell
        .run(
            "app.preferences.set",
            json!({"key": "libraries.paths", "value": [lib]}),
        )
        .unwrap();
    let out = shell
        .run(
            "library.element.new",
            json!({"library": LIBRARY, "kind": kind, "name": "Nudged"}),
        )
        .unwrap();
    let element = uuid(&out["element"]);
    shell
        .run("view.grid.set", json!({"document": element, "grid": grid}))
        .unwrap();
    (shell, element)
}

fn open(shell: &Shell, element: Uuid) -> LibraryElement {
    shell.engine().element(element).unwrap().clone()
}

#[test]
fn mod_arrows_nudge_a_footprints_pads_and_graphics() {
    let home = TempDir::new("nudge-footprint");
    let (mut shell, fp) = element(&home, "footprint", "1mm");
    shell
        .run(
            "library.footprint.pad.add",
            json!({"footprint": fp, "number": "1", "kind": "smd", "shape": "rect",
                   "at": {"x": "0.3mm", "y": 0}, "width": "1mm", "height": "1mm"}),
        )
        .unwrap();
    shell
        .run(
            "library.footprint.graphic.add",
            json!({"footprint": fp, "layer": "silkscreen_top",
                   "shape": {"kind": "line", "start": {"x": "1mm", "y": "1mm"},
                             "end": {"x": "2mm", "y": "1mm"}}}),
        )
        .unwrap();
    let LibraryElement::Footprint(f) = open(&shell, fp) else {
        panic!("a Footprint");
    };
    let (pad, line) = (f.pads[0].uuid, f.graphics[0].uuid);
    select(&mut shell, &[pad, line]);
    let before = shell.engine().journal().len();
    assert_eq!(
        press(&mut shell, fp, "Mod+Right"),
        "library.footprint.nudge"
    );
    assert_eq!(
        press(&mut shell, fp, "Shift+Mod+Down"),
        "library.footprint.nudge"
    );
    let placed = |shell: &Shell| {
        let LibraryElement::Footprint(f) = open(shell, fp) else {
            panic!("a Footprint");
        };
        let Shape::Line { start, .. } = f.graphics[0].shape else {
            panic!("a line");
        };
        (f.pads[0].at, start)
    };
    // The Pad, first, lands on x = 1 mm; the line keeps its distance.
    assert_eq!(placed(&shell), (mm(1.0, -10.0), mm(1.7, -9.0)));
    assert_eq!(
        edits_since(&shell, before),
        ["library.footprint.move", "library.footprint.move"]
    );
    shell.run("edit.undo", json!({"document": fp})).unwrap();
    assert_eq!(placed(&shell), (mm(1.0, 0.0), mm(1.7, 1.0)));
}

#[test]
fn mod_arrows_nudge_a_symbols_pins_and_graphics() {
    let home = TempDir::new("nudge-symbol");
    let (mut shell, sym) = element(&home, "symbol", "100mil");
    shell
        .run(
            "library.symbol.graphic.add",
            json!({"symbol": sym, "shape": {"kind": "rect", "start": {"x": 0, "y": 0},
                                            "end": {"x": "200mil", "y": "300mil"}}}),
        )
        .unwrap();
    shell
        .run(
            "library.symbol.pin.add",
            json!({"symbol": sym, "name": "A", "number": "1",
                   "at": {"x": "-250mil", "y": 0}}),
        )
        .unwrap();
    let LibraryElement::Symbol(s) = open(&shell, sym) else {
        panic!("a Symbol");
    };
    let (pin, rect) = (s.pins[0].uuid, s.graphics[0].uuid);
    // The Pin is picked first, so the step is measured at it.
    select(&mut shell, &[pin, rect]);
    let before = shell.engine().journal().len();
    assert_eq!(press(&mut shell, sym, "Mod+Left"), "library.symbol.nudge");
    assert_eq!(press(&mut shell, sym, "Mod+Up"), "library.symbol.nudge");
    let mil = |x: i64, y: i64| Point::new(Nm(x * 25_400), Nm(y * 25_400));
    let LibraryElement::Symbol(s) = open(&shell, sym) else {
        panic!("a Symbol");
    };
    let Shape::Rect { start, .. } = s.graphics[0].shape else {
        panic!("a rectangle");
    };
    assert_eq!((s.pins[0].at, start), (mil(-300, 100), mil(-50, 100)));
    assert_eq!(
        edits_since(&shell, before),
        ["library.symbol.move", "library.symbol.move"]
    );
}

#[test]
fn a_nudge_with_nothing_movable_selected_is_refused() {
    let home = TempDir::new("nudge-empty");
    let (mut shell, _) = element(&home, "symbol", "100mil");
    let err = shell
        .run("library.symbol.nudge", json!({"dx": 1, "dy": 0}))
        .unwrap_err();
    assert!(err.message.contains("nothing is selected"), "{err:?}");
    // pcb.nudge needs the board's canvas to be active.
    let err = shell
        .run("pcb.nudge", json!({"dx": 1, "dy": 0}))
        .unwrap_err();
    assert!(err.message.contains("not the board"), "{err:?}");
}
