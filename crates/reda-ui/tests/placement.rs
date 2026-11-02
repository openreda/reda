//! The placement Tool on a Shell, without a window: `tool.place` picks a Library Part up,
//! the pointer previews it without touching the Project or the Journal, and a click
//! places it as one Command. Space, X, Y and L turn it in flight, edits made with Tab
//! carry to later placements with the next Designator, placement repeats per Preference,
//! and Esc returns to Select.

use std::path::{Path, PathBuf};

use reda_testkit::TempDir;
use reda_ui::bars;
use reda_ui::engine::Engine;
use reda_ui::engine::command::{ErrorCode, SeededIds, UserDirs};
use reda_ui::engine::model::geom::{Millideg, Nm, Point};
use reda_ui::engine::model::{FootprintInstance, Side, UnitInstance, Uuid};
use reda_ui::keymap::{self, Chord};
use reda_ui::placement;
use reda_ui::shell::Shell;
use reda_ui::tools::Tool;
use serde_json::{Value, json};

const SHEET: &str = "00000000-0000-4000-8000-000000000002";
/// The starter Library's Resistor (Standard, with a Symbol).
const RESISTOR: &str = "84d5651a-bf4b-462e-9480-39e8f454a042";
/// The starter Library's MountingHole_M3 (Mechanical, no Symbol).
const MOUNTING_HOLE_M3: &str = "326ace02-ef44-4af4-8f9d-21d6c65a74ef";

fn starter() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../library/starter")
}

struct Fixture {
    shell: Shell,
    _home: TempDir,
    _work: TempDir,
}

fn fixture(name: &str) -> Fixture {
    let home = TempDir::new(&format!("{name}-home"));
    let work = TempDir::new(&format!("{name}-work"));
    let engine = Engine::builder()
        .ids(SeededIds::new(0))
        .user_dirs(Some(UserDirs::under(home.path())))
        .starter_library(Some(starter()))
        .headless(false)
        .build();
    let mut shell = Shell::new(engine);
    shell.new_project(work.path(), "Demo").unwrap();
    Fixture {
        shell,
        _home: home,
        _work: work,
    }
}

fn sheet() -> Uuid {
    SHEET.parse().unwrap()
}

fn pcb(shell: &Shell) -> Uuid {
    shell.engine().project().unwrap().pcb().unwrap().uuid
}

fn run(shell: &mut Shell, id: &str, params: Value) -> Value {
    match shell.run(id, params.clone()) {
        Ok(out) => out,
        Err(e) => panic!("{id} {params} failed: {e}"),
    }
}

fn mil(x: i64, y: i64) -> Point {
    Point::new(Nm(x * 25_400), Nm(y * 25_400))
}

/// Presses `key` over `document`'s canvas, as the window does: runs what the Keymap binds
/// it to there. Returns the Command run, if any.
fn press(shell: &mut Shell, document: Uuid, key: &str) -> Option<String> {
    let chord = Chord::parse(key).unwrap();
    let active = keymap::active_for(shell, Some(document));
    let action = shell.keymap().lookup(&chord, &active).cloned()?;
    let (id, params) = bars::resolve_for(shell, &action, Some(document)).run.ok()?;
    shell.run(&id, params).unwrap();
    Some(id)
}

fn units(shell: &Shell) -> Vec<UnitInstance> {
    let sheet = shell.engine().project().unwrap().schematic_sheet(sheet());
    sheet.unwrap().units().cloned().collect()
}

fn footprints(shell: &Shell) -> Vec<FootprintInstance> {
    let pcb = shell.engine().project().unwrap().pcb().unwrap();
    pcb.footprints().cloned().collect()
}

/// The ids the Journal recorded from entry `from` on.
fn journal_since(shell: &Shell, from: usize) -> Vec<String> {
    shell.engine().journal()[from..]
        .iter()
        .map(|e| e.id.clone())
        .collect()
}

/// How many items the canvas of `document` draws.
fn drawn(shell: &Shell, document: Uuid) -> usize {
    shell
        .document_tab(document)
        .unwrap()
        .scene
        .scene
        .items
        .len()
}

#[test]
fn a_part_follows_the_pointer_as_a_preview_and_a_click_places_it_as_one_command() {
    let Fixture { mut shell, .. } = fixture("place-click");
    let empty = drawn(&shell, sheet());
    run(
        &mut shell,
        "tool.place",
        json!({"document": SHEET, "part": RESISTOR}),
    );
    assert_eq!(shell.tool(), Tool::Place);
    let from = shell.engine().journal().len();

    placement::hover(&mut shell, sheet(), mil(1000, 520));
    assert!(drawn(&shell, sheet()) > empty, "the preview is drawn");
    assert!(units(&shell).is_empty(), "the Project is untouched");
    assert_eq!(journal_since(&shell, from), Vec::<String>::new());

    placement::click(&mut shell, sheet(), mil(1000, 520));
    assert_eq!(journal_since(&shell, from), ["schematic.component.place"]);
    let placed = units(&shell);
    assert_eq!(placed.len(), 1);
    // The point snaps to the 50 mil grid.
    assert_eq!(placed[0].at, mil(1000, 500));
    assert_eq!(placed[0].designator, "R1");

    // Placement repeats (the Preference is on by default) with the next Designator.
    assert_eq!(shell.tool(), Tool::Place);
    placement::click(&mut shell, sheet(), mil(1500, 500));
    let mut designators: Vec<String> = units(&shell).into_iter().map(|u| u.designator).collect();
    designators.sort();
    assert_eq!(designators, ["R1", "R2"]);
}

#[test]
fn space_rotates_and_x_and_y_mirror_in_flight_and_later_placements_keep_it() {
    let Fixture { mut shell, .. } = fixture("place-turn");
    run(
        &mut shell,
        "tool.place",
        json!({"document": SHEET, "part": RESISTOR}),
    );
    placement::hover(&mut shell, sheet(), mil(0, 0));
    assert_eq!(
        press(&mut shell, sheet(), "Space").as_deref(),
        Some("tool.rotate")
    );
    assert_eq!(
        press(&mut shell, sheet(), "X").as_deref(),
        Some("tool.mirror:x")
    );
    let from = shell.engine().journal().len();
    placement::click(&mut shell, sheet(), mil(0, 0));
    placement::click(&mut shell, sheet(), mil(500, 0));

    // Mirroring left to right after a quarter turn turns the other way, mirrored.
    for unit in units(&shell) {
        assert_eq!(unit.rotation, Millideg::from_deg(270));
        assert!(unit.mirror);
    }
    assert_eq!(
        journal_since(&shell, from),
        ["schematic.component.place", "schematic.component.place"]
    );

    // Y mirrors top to bottom: a half turn of the mirror about X, so from 270° mirrored
    // it comes back to 270° unmirrored (180° − 270°).
    press(&mut shell, sheet(), "Y");
    placement::click(&mut shell, sheet(), mil(1000, 0));
    let last = units(&shell)
        .into_iter()
        .find(|u| u.at == mil(1000, 0))
        .unwrap();
    assert_eq!(last.rotation, Millideg::from_deg(270));
    assert!(!last.mirror);
}

#[test]
fn edits_made_in_flight_carry_to_later_placements_with_the_next_designator() {
    let Fixture { mut shell, .. } = fixture("place-edits");
    run(
        &mut shell,
        "tool.place",
        json!({"document": SHEET, "part": RESISTOR}),
    );
    // Tab opens Properties, where the placement in flight is edited.
    assert_eq!(
        press(&mut shell, sheet(), "Tab").as_deref(),
        Some("panel.show:properties")
    );
    run(
        &mut shell,
        "tool.place.set",
        json!({"designator": "R10", "fields": {"value": "4k7"}}),
    );
    placement::click(&mut shell, sheet(), mil(0, 0));
    placement::click(&mut shell, sheet(), mil(500, 0));

    let mut placed: Vec<(String, String)> = units(&shell)
        .into_iter()
        .map(|u| (u.designator, u.fields.value))
        .collect();
    placed.sort();
    assert_eq!(
        placed,
        [("R10".into(), "4k7".into()), ("R11".into(), "4k7".into())]
    );
    let placing = placement::placing(&shell).unwrap();
    assert_eq!(placing.designator.as_deref(), Some("R12"));
}

#[test]
fn escape_returns_to_select_and_drops_the_preview() {
    let Fixture { mut shell, .. } = fixture("place-escape");
    let empty = drawn(&shell, sheet());
    run(
        &mut shell,
        "tool.place",
        json!({"document": SHEET, "part": RESISTOR}),
    );
    placement::hover(&mut shell, sheet(), mil(0, 0));
    let from = shell.engine().journal().len();

    press(&mut shell, sheet(), "Esc");

    assert_eq!(shell.tool(), Tool::Select);
    assert!(placement::placing(&shell).is_none());
    assert_eq!(drawn(&shell, sheet()), empty);
    assert_eq!(journal_since(&shell, from), ["tool.activate:select"]);
    // A click now places nothing.
    placement::click(&mut shell, sheet(), mil(0, 0));
    assert!(units(&shell).is_empty());
}

#[test]
fn with_repeat_placement_off_one_click_places_once_and_returns_to_select() {
    let Fixture { mut shell, .. } = fixture("place-once");
    run(
        &mut shell,
        "app.preferences.set",
        json!({"key": "schematic.repeat_placement", "value": false}),
    );
    run(
        &mut shell,
        "tool.place",
        json!({"document": SHEET, "part": RESISTOR}),
    );
    placement::click(&mut shell, sheet(), mil(0, 0));
    assert_eq!(units(&shell).len(), 1);
    assert_eq!(shell.tool(), Tool::Select);
}

#[test]
fn a_mechanical_part_places_on_the_board_turned_and_flipped_in_flight() {
    let Fixture { mut shell, .. } = fixture("place-board");
    let board = pcb(&shell);
    run(&mut shell, "document.open", json!({"document": board}));
    run(
        &mut shell,
        "tool.place",
        json!({"document": board, "part": MOUNTING_HOLE_M3}),
    );
    let from = shell.engine().journal().len();
    placement::hover(
        &mut shell,
        board,
        Point::new(Nm::from_mm(5), Nm::from_mm(5)),
    );
    assert_eq!(
        press(&mut shell, board, "Space").as_deref(),
        Some("tool.rotate")
    );
    assert_eq!(press(&mut shell, board, "L").as_deref(), Some("tool.flip"));
    assert!(footprints(&shell).is_empty());

    placement::click(
        &mut shell,
        board,
        Point::new(Nm::from_mm(5), Nm::from_mm(5)),
    );

    let placed = footprints(&shell);
    assert_eq!(placed.len(), 1);
    assert_eq!(placed[0].designator, "H1");
    assert_eq!(placed[0].rotation, Millideg::from_deg(90));
    assert_eq!(placed[0].side, Side::Bottom);
    assert_eq!(
        journal_since(&shell, from),
        ["tool.rotate", "tool.flip", "pcb.mechanical.place"]
    );
}

#[test]
fn a_part_that_does_not_fit_the_document_is_refused() {
    let Fixture { mut shell, .. } = fixture("place-refused");
    let board = pcb(&shell);
    // A Standard Part reaches the board through Sync; a Part with no Symbol has nothing
    // to draw on a sheet.
    let standard = shell.run("tool.place", json!({"document": board, "part": RESISTOR}));
    assert_eq!(standard.unwrap_err().code, ErrorCode::Rejected);
    let no_symbol = shell.run(
        "tool.place",
        json!({"document": SHEET, "part": MOUNTING_HOLE_M3}),
    );
    assert_eq!(no_symbol.unwrap_err().code, ErrorCode::Rejected);
    assert_eq!(shell.tool(), Tool::Select);
}
