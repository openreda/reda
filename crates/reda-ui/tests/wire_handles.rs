//! Editing a placed Wire on a Shell, without a window: segment selection, the endpoint,
//! vertex and segment handles, Delete and Insert while holding one, deleting segments and
//! the vertex list in Properties.

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
use reda_ui::properties::{Edit, Properties};
use reda_ui::schematic_editor::{self, Scale};
use reda_ui::selection::WireSegment;
use reda_ui::shell::Shell;
use serde_json::{Value, json};

const SHEET: &str = "00000000-0000-4000-8000-000000000002";
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

fn at(x: f64, y: f64) -> (f64, f64) {
    (x * MIL, y * MIL)
}

fn mil(x: i64, y: i64) -> Point {
    Point::new(Nm(x * 25_400), Nm(y * 25_400))
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

fn vertices(shell: &Shell, wire: Uuid) -> Vec<(i64, i64)> {
    let sheet = shell
        .engine()
        .project()
        .unwrap()
        .schematic_sheet(sheet())
        .unwrap();
    (sheet.wire(wire).unwrap().vertices.iter())
        .map(|p| (p.x.0 / 25_400, p.y.0 / 25_400))
        .collect()
}

fn pointer(shell: &mut Shell, events: &[Pointer]) -> Vec<String> {
    let commands = schematic_editor::pointer(shell, sheet(), events, Scale::at(MIL));
    run_all(shell, commands)
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

fn press(shell: &mut Shell, key: &str) -> Vec<String> {
    let chord = Chord::parse(key).unwrap();
    let active = keymap::active_for(shell, Some(sheet()));
    let Some(action) = shell.keymap().lookup(&chord, &active).cloned() else {
        return Vec::new();
    };
    match bars::resolve_for(shell, &action, Some(sheet())).run {
        Ok(command) => run_all(shell, vec![command]),
        Err(why) => panic!("{key} cannot run: {why}"),
    }
}

fn click(shell: &mut Shell, p: (f64, f64), modifiers: Modifiers) {
    pointer(shell, &[Pointer::Click { at: p, modifiers }]);
}

/// Drags from `from` to `to` in one frame.
fn drag(shell: &mut Shell, from: (f64, f64), to: (f64, f64)) -> Vec<String> {
    pointer(
        shell,
        &[
            Pointer::DragStart {
                from,
                at: from,
                modifiers: Modifiers::NONE,
            },
            Pointer::Drag(to),
            Pointer::DragEnd(to),
        ],
    )
}

fn segment(wire: Uuid, index: usize) -> WireSegment {
    WireSegment { wire, index }
}

/// The handles the canvas draws, as (x, y, red) in mil, sorted.
fn handles(shell: &Shell) -> Vec<(i64, i64, bool)> {
    let tab = shell.document_tab(sheet()).unwrap();
    let mut out: Vec<_> = (tab.overlay.handles.iter())
        .map(|(p, red)| (p.x.0 / 25_400, p.y.0 / 25_400, *red))
        .collect();
    out.sort();
    out
}

/// The last successful Journal entry's id.
fn last_entry(shell: &Shell) -> String {
    (shell.engine().journal().iter().rev())
        .find(|e| e.outcome.is_ok())
        .map(|e| e.id.clone())
        .unwrap()
}

const RUN: [(i64, i64); 3] = [(0, 0), (1000, 0), (1000, 1000)];

#[test]
fn a_click_on_a_segment_of_the_selected_wire_selects_only_that_segment_with_red_handles() {
    let dir = TempDir::new("wire-segment-click");
    let mut shell = shell(&dir);
    let w = wire(&mut shell, &RUN);
    click(&mut shell, at(500.0, 0.0), Modifiers::NONE);
    assert_eq!(shell.selection().objects, [w]);
    assert!(shell.selection().segments.is_empty());
    // The selected Wire shows a handle at each vertex and on each segment.
    pointer(&mut shell, &[Pointer::Hover(at(3000.0, 3000.0))]);
    assert_eq!(
        handles(&shell),
        [
            (0, 0, false),
            (500, 0, false),
            (1000, 0, false),
            (1000, 500, false),
            (1000, 1000, false)
        ]
    );
    click(&mut shell, at(1000.0, 400.0), Modifiers::NONE);
    assert!(shell.selection().objects.is_empty());
    assert_eq!(shell.selection().segments, [segment(w, 1)]);
    pointer(&mut shell, &[Pointer::Hover(at(3000.0, 3000.0))]);
    assert_eq!(
        handles(&shell),
        [(1000, 0, true), (1000, 500, true), (1000, 1000, true)]
    );
    // Another segment of the same Wire takes its place; a click elsewhere clears it.
    click(&mut shell, at(200.0, 0.0), Modifiers::NONE);
    assert_eq!(shell.selection().segments, [segment(w, 0)]);
    click(&mut shell, at(3000.0, 3000.0), Modifiers::NONE);
    assert!(shell.selection().is_empty());
}

#[test]
fn shift_click_twice_adds_a_segment_from_another_wire() {
    let dir = TempDir::new("wire-segment-shift");
    let mut shell = shell(&dir);
    let a = wire(&mut shell, &RUN);
    let b = wire(&mut shell, &[(0, 2000), (1000, 2000)]);
    click(&mut shell, at(500.0, 0.0), Modifiers::NONE);
    click(&mut shell, at(500.0, 0.0), Modifiers::NONE);
    assert_eq!(shell.selection().segments, [segment(a, 0)]);
    // The first Shift+click adds the other Wire whole, the second only its segment.
    click(&mut shell, at(500.0, 2000.0), Modifiers::SHIFT);
    assert_eq!(shell.selection().objects, [b]);
    assert_eq!(shell.selection().segments, [segment(a, 0)]);
    click(&mut shell, at(500.0, 2000.0), Modifiers::SHIFT);
    assert!(shell.selection().objects.is_empty());
    assert_eq!(shell.selection().segments, [segment(a, 0), segment(b, 0)]);
    // Shift+click on a selected segment drops it.
    click(&mut shell, at(500.0, 0.0), Modifiers::SHIFT);
    assert_eq!(shell.selection().segments, [segment(b, 0)]);
}

#[test]
fn dragging_an_endpoint_handle_moves_that_end_only() {
    let dir = TempDir::new("wire-drag-endpoint");
    let mut shell = shell(&dir);
    let w = wire(&mut shell, &RUN);
    click(&mut shell, at(500.0, 0.0), Modifiers::NONE);
    let ran = drag(&mut shell, at(0.0, 0.0), at(0.0, 300.0));
    assert_eq!(ran, ["schematic.wire.set_vertices"]);
    assert_eq!(vertices(&shell, w), [(0, 300), (1000, 0), (1000, 1000)]);
}

#[test]
fn dragging_a_vertex_handle_keeps_the_ends_where_they_are() {
    let dir = TempDir::new("wire-drag-vertex");
    let mut shell = shell(&dir);
    let w = wire(&mut shell, &RUN);
    click(&mut shell, at(500.0, 0.0), Modifiers::NONE);
    // In flight, the Wire is drawn with the vertex where the pointer is.
    pointer(
        &mut shell,
        &[
            Pointer::DragStart {
                from: at(1000.0, 0.0),
                at: at(1000.0, 0.0),
                modifiers: Modifiers::NONE,
            },
            Pointer::Drag(at(800.0, 200.0)),
        ],
    );
    let overlay = shell.document_tab(sheet()).unwrap().overlay.clone();
    assert_eq!(
        overlay.wires[..],
        [(mil(0, 0), mil(800, 200)), (mil(800, 200), mil(1000, 1000))]
    );
    assert_eq!(
        vertices(&shell, w),
        RUN,
        "nothing changes before the release"
    );
    pointer(&mut shell, &[Pointer::DragEnd(at(800.0, 200.0))]);
    assert_eq!(vertices(&shell, w), [(0, 0), (800, 200), (1000, 1000)]);
    shell.run("edit.undo", json!({"document": SHEET})).unwrap();
    assert_eq!(vertices(&shell, w), RUN, "one undo step");
}

#[test]
fn dragging_a_segment_moves_its_two_vertices_and_leaves_the_others() {
    let dir = TempDir::new("wire-drag-segment");
    let mut shell = shell(&dir);
    let w = wire(&mut shell, &[(0, 0), (1000, 0), (1000, 1000), (2000, 1000)]);
    // A selected segment drags by its body.
    click(&mut shell, at(1000.0, 500.0), Modifiers::NONE);
    click(&mut shell, at(1000.0, 500.0), Modifiers::NONE);
    assert_eq!(shell.selection().segments, [segment(w, 1)]);
    drag(&mut shell, at(1000.0, 400.0), at(1200.0, 400.0));
    assert_eq!(
        vertices(&shell, w),
        [(0, 0), (1200, 0), (1200, 1000), (2000, 1000)]
    );
    // A selected Wire's segment drags by the handle on it.
    click(&mut shell, at(3000.0, 3000.0), Modifiers::NONE);
    click(&mut shell, at(500.0, 0.0), Modifiers::NONE);
    drag(&mut shell, at(600.0, 0.0), at(600.0, -200.0));
    assert_eq!(
        vertices(&shell, w),
        [(0, -200), (1200, -200), (1200, 1000), (2000, 1000)]
    );
}

#[test]
fn dragging_an_unselected_wire_moves_the_whole_wire() {
    let dir = TempDir::new("wire-drag-unselected");
    let mut shell = shell(&dir);
    let w = wire(&mut shell, &RUN);
    let ran = drag(&mut shell, at(500.0, 0.0), at(500.0, 200.0));
    assert_eq!(ran, ["selection.set", "schematic.drag"]);
    assert_eq!(vertices(&shell, w), [(0, 200), (1000, 200), (1000, 1200)]);
}

#[test]
fn delete_while_holding_a_vertex_removes_it() {
    let dir = TempDir::new("wire-delete-held-vertex");
    let mut shell = shell(&dir);
    let w = wire(&mut shell, &[(0, 0), (1000, 0), (1000, 1000), (2000, 1000)]);
    click(&mut shell, at(500.0, 0.0), Modifiers::NONE);
    pointer(
        &mut shell,
        &[
            Pointer::DragStart {
                from: at(1000.0, 1000.0),
                at: at(1000.0, 1000.0),
                modifiers: Modifiers::NONE,
            },
            Pointer::Drag(at(1100.0, 900.0)),
        ],
    );
    press(&mut shell, "Del");
    assert_eq!(last_entry(&shell), "schematic.wire.set_vertices");
    assert_eq!(vertices(&shell, w), [(0, 0), (1000, 0), (2000, 1000)]);
    // The drag is over: releasing does nothing more.
    assert!(pointer(&mut shell, &[Pointer::DragEnd(at(1100.0, 900.0))]).is_empty());
    assert_eq!(shell.selection().objects, [w], "the Wire stays selected");
}

#[test]
fn insert_while_holding_a_segment_adds_a_vertex_that_follows_the_pointer() {
    let dir = TempDir::new("wire-insert-held-segment");
    let mut shell = shell(&dir);
    let w = wire(&mut shell, &RUN);
    click(&mut shell, at(500.0, 0.0), Modifiers::NONE);
    pointer(
        &mut shell,
        &[Pointer::DragStart {
            from: at(500.0, 0.0),
            at: at(500.0, 0.0),
            modifiers: Modifiers::NONE,
        }],
    );
    press(&mut shell, "Insert");
    pointer(
        &mut shell,
        &[
            Pointer::Drag(at(500.0, 300.0)),
            Pointer::DragEnd(at(500.0, 300.0)),
        ],
    );
    assert_eq!(
        vertices(&shell, w),
        [(0, 0), (500, 300), (1000, 0), (1000, 1000)]
    );
}

#[test]
fn delete_with_a_segment_selected_deletes_only_that_segment() {
    let dir = TempDir::new("wire-delete-selected-segment");
    let mut shell = shell(&dir);
    let w = wire(&mut shell, &[(0, 0), (1000, 0), (1000, 1000), (2000, 1000)]);
    click(&mut shell, at(1000.0, 500.0), Modifiers::NONE);
    click(&mut shell, at(1000.0, 500.0), Modifiers::NONE);
    press(&mut shell, "Del");
    assert_eq!(last_entry(&shell), "schematic.delete");
    assert_eq!(vertices(&shell, w), [(0, 0), (1000, 0)]);
    assert!(shell.selection().is_empty());
    let sheet = shell
        .engine()
        .project()
        .unwrap()
        .schematic_sheet(sheet())
        .unwrap();
    assert_eq!(sheet.wires().count(), 2);
}

#[test]
fn properties_lists_the_vertices_with_index_x_and_y_plus_add_and_remove() {
    let dir = TempDir::new("wire-vertex-list");
    let mut shell = shell(&dir);
    let w = wire(&mut shell, &RUN);
    shell.run("selection.set", json!({"objects": [w]})).unwrap();
    let properties = Properties::of(shell.engine().project().unwrap(), shell.selection());
    let list = properties.vertices.clone().expect("one Wire selected");
    assert_eq!(list.wire, w);
    assert_eq!(list.vertices, [mil(0, 0), mil(1000, 0), mil(1000, 1000)]);
    // A selected segment lists its Wire's vertices too.
    shell
        .run(
            "selection.set",
            json!({"segments": [{"wire": w, "index": 1}]}),
        )
        .unwrap();
    let by_segment = Properties::of(shell.engine().project().unwrap(), shell.selection());
    assert_eq!(by_segment.vertices, Some(list));

    let set = |edit: Edit| {
        let (id, params) = properties.command(&edit).unwrap();
        assert_eq!(id, "schematic.wire.set_vertices");
        assert_eq!(params["wire"], json!(w));
        let mils: Vec<(i64, i64)> = (params["vertices"].as_array().unwrap().iter())
            .map(|p| {
                (
                    p["x"].as_i64().unwrap() / 25_400,
                    p["y"].as_i64().unwrap() / 25_400,
                )
            })
            .collect();
        mils
    };
    assert_eq!(
        set(Edit::Vertex(1, mil(800, 0))),
        [(0, 0), (800, 0), (1000, 1000)]
    );
    assert_eq!(set(Edit::RemoveVertex(1)), [(0, 0), (1000, 1000)]);
    // Add puts a new end one snap grid on, turning a corner off the last segment.
    assert_eq!(
        set(Edit::AddVertex(Nm(50 * 25_400))),
        [(0, 0), (1000, 0), (1000, 1000), (1050, 1000)]
    );
    // Two Wires selected have no vertex list.
    let other = wire(&mut shell, &[(0, 2000), (500, 2000)]);
    shell
        .run("selection.set", json!({"objects": [w, other]}))
        .unwrap();
    let two = Properties::of(shell.engine().project().unwrap(), shell.selection());
    assert_eq!(two.vertices, None);
}
