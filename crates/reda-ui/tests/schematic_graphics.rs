//! The sheet's graphics Tools on a Shell, without a window: Line draws a polyline that
//! commits as one undo step on Enter or on its last point again, Rect draws between two
//! corners, and Text asks for its words where it is clicked, turned by Space.

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
use reda_ui::schematic_editor::{self, Scale};
use reda_ui::shell::Shell;
use reda_ui::tools::Tool;
use serde_json::json;

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

/// A point in mil, as the camera gives it.
fn at(x: f64, y: f64) -> (f64, f64) {
    (x * MIL, y * MIL)
}

/// A sheet point in mil.
fn pt(x: i64, y: i64) -> Point {
    Point::new(Nm(x * 25_400), Nm(y * 25_400))
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

/// The run in flight as the canvas draws it.
fn overlay(shell: &Shell) -> Vec<(Point, Point)> {
    shell.document_tab(sheet()).unwrap().overlay.wires.to_vec()
}

/// The sheet's free graphics: ("line" | "rect", from, to) in mil, sorted.
type Shape = (&'static str, (i64, i64), (i64, i64));

fn graphics(shell: &Shell) -> Vec<Shape> {
    use reda_ui::engine::model::SheetShape;
    let mil = |p: Point| (p.x.0 / 25_400, p.y.0 / 25_400);
    let project = shell.engine().project().unwrap();
    let mut out: Vec<_> = project
        .schematic_sheet(sheet())
        .unwrap()
        .graphics()
        .map(|g| match &g.shape {
            SheetShape::Line { from, to } => ("line", mil(*from), mil(*to)),
            SheetShape::Rect { from, to, .. } => ("rect", mil(*from), mil(*to)),
        })
        .collect();
    out.sort();
    out
}

#[test]
fn the_line_tool_draws_a_polyline_that_commits_as_one_step_on_enter() {
    let dir = TempDir::new("graphics-line");
    let mut shell = shell(&dir);
    let before = shell.engine().journal().len();
    assert_eq!(press(&mut shell, "L"), ["tool.activate:line"]);
    assert_eq!(shell.tool(), Tool::Line);
    assert!(click(&mut shell, at(1003.0, -4.0)).is_empty());
    hover(&mut shell, at(1500.0, 0.0));
    assert_eq!(overlay(&shell), [(pt(1000, 0), pt(1500, 0))]);
    assert!(click(&mut shell, at(1500.0, 0.0)).is_empty());
    // Lines go at any angle.
    assert!(click(&mut shell, at(2000.0, 500.0)).is_empty());
    assert!(click(&mut shell, at(2500.0, 500.0)).is_empty());
    // Backspace takes the last point back.
    assert_eq!(press(&mut shell, "Backspace"), ["tool.undo_segment"]);
    hover(&mut shell, at(2000.0, 500.0));
    assert_eq!(
        overlay(&shell),
        [(pt(1000, 0), pt(1500, 0)), (pt(1500, 0), pt(2000, 500))]
    );
    assert!(graphics(&shell).is_empty());
    assert_eq!(press(&mut shell, "Enter"), ["tool.finish"]);
    assert_eq!(
        graphics(&shell),
        [
            ("line", (1000, 0), (1500, 0)),
            ("line", (1500, 0), (2000, 500))
        ]
    );
    // One Journal entry and one undo step for the polyline.
    assert_eq!(
        journal_since(&shell, before),
        [
            "tool.activate:line",
            "tool.undo_segment",
            "tool.finish",
            "batch"
        ]
    );
    shell.run("edit.undo", json!({"document": SHEET})).unwrap();
    assert!(graphics(&shell).is_empty());
    assert_eq!(shell.tool(), Tool::Line);
    assert!(overlay(&shell).is_empty());
}

#[test]
fn clicking_the_last_point_again_ends_a_line_and_one_segment_is_one_command() {
    let dir = TempDir::new("graphics-line-click");
    let mut shell = shell(&dir);
    shell.run("tool.activate:line", json!({})).unwrap();
    click(&mut shell, at(0.0, 0.0));
    click(&mut shell, at(0.0, 500.0));
    assert_eq!(
        click(&mut shell, at(0.0, 500.0)),
        ["schematic.graphic.add_line"]
    );
    assert_eq!(graphics(&shell), [("line", (0, 0), (0, 500))]);
    // Escape drops a polyline in flight without drawing it.
    click(&mut shell, at(1000.0, 0.0));
    click(&mut shell, at(1500.0, 0.0));
    press(&mut shell, "Esc");
    pointer(&mut shell, &[]);
    assert_eq!(graphics(&shell).len(), 1);
    assert!(overlay(&shell).is_empty());
}

#[test]
fn the_rect_tool_draws_between_two_corners_by_clicks_or_a_drag() {
    let dir = TempDir::new("graphics-rect");
    let mut shell = shell(&dir);
    assert_eq!(press(&mut shell, "R"), ["tool.activate:rect"]);
    assert!(click(&mut shell, at(0.0, 0.0)).is_empty());
    hover(&mut shell, at(500.0, 300.0));
    // The outline in flight.
    assert_eq!(overlay(&shell).len(), 4);
    // Backspace drops the first corner.
    assert_eq!(press(&mut shell, "Backspace"), ["tool.undo_segment"]);
    hover(&mut shell, at(600.0, 300.0));
    assert!(overlay(&shell).is_empty());
    click(&mut shell, at(100.0, 100.0));
    assert_eq!(
        click(&mut shell, at(500.0, 300.0)),
        ["schematic.graphic.add_rect"]
    );
    let ran = pointer(
        &mut shell,
        &[
            Pointer::DragStart {
                from: at(1000.0, 0.0),
                at: at(1100.0, 100.0),
                modifiers: Modifiers::NONE,
            },
            Pointer::Drag(at(1300.0, 200.0)),
            Pointer::DragEnd(at(1500.0, 500.0)),
        ],
    );
    assert_eq!(ran, ["schematic.graphic.add_rect"]);
    assert_eq!(
        graphics(&shell),
        [
            ("rect", (100, 100), (500, 300)),
            ("rect", (1000, 0), (1500, 500))
        ]
    );
    assert_eq!(shell.tool(), Tool::Rect);
}

#[test]
fn the_text_tool_asks_for_its_words_where_it_is_clicked_turned_by_space() {
    let dir = TempDir::new("graphics-text");
    let mut shell = shell(&dir);
    assert_eq!(press(&mut shell, "T"), ["tool.activate:text"]);
    hover(&mut shell, at(500.0, 500.0));
    assert_eq!(press(&mut shell, "Space"), ["tool.rotate"]);
    let commands = schematic_editor::pointer(
        &mut shell,
        sheet(),
        &[Pointer::Click {
            at: at(498.0, 503.0),
            modifiers: Modifiers::NONE,
        }],
        Scale::at(MIL),
    );
    assert_eq!(commands.len(), 1);
    let (id, params) = &commands[0];
    assert_eq!(id, "schematic.text.add");
    assert_eq!(
        params,
        &json!({"document": SHEET, "at": {"x": 500 * 25_400, "y": 500 * 25_400},
                "rotation": 90_000})
    );
    // The words are left for the dialog the window opens.
    let registry = shell.engine().registry();
    assert!(reda_ui::command_dialog::lacks_required(
        registry, id, params
    ));
    let mut with_words = params.clone();
    with_words["text"] = json!("Note");
    shell.run(id, with_words).unwrap();
    let project = shell.engine().project().unwrap();
    let texts: Vec<_> = project.schematic_sheet(sheet()).unwrap().texts().collect();
    assert_eq!(texts.len(), 1);
    assert_eq!(texts[0].text, "Note");
}
