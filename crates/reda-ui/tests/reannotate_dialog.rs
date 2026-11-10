//! The Re-annotate dialog's model, without a window: the settings (source, mode, scope with
//! a sheet grid, order, location, threshold, start indices, sides), an old → new preview
//! with Only changed, sorting and a lock switch per row, the last settings it opens with,
//! and Re-annotate (last settings), which confirms a count and runs them.

use std::path::{Path, PathBuf};

use reda_testkit::TempDir;
use reda_ui::engine::Engine;
use reda_ui::engine::command::{FixedClock, SeededIds};
use reda_ui::engine::model::Uuid;
use reda_ui::engine::model::annotate::{Mode, Order, ScopeKind, Source};
use reda_ui::schematic_dialogs::reannotate_labels::StartIndices;
use reda_ui::schematic_dialogs::{ReannotateDialog, ReannotateLast, RowSort};
use reda_ui::shell::Shell;
use serde_json::{Value, json};

const SHEET: &str = "00000000-0000-4000-8000-000000000002";
const R_10K: &str = "a0000000-0000-4000-8000-000000000100";

fn fixture_library() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../reda-schematic/tests/library/Test")
}

fn engine() -> Engine {
    Engine::builder()
        .ids(SeededIds::new(1000))
        .clock(FixedClock::EPOCH)
        .user_dirs(None)
        .starter_library(Some(fixture_library()))
        .headless(false)
        .build()
}

/// Saves, rewrites Designators in the first sheet's file (duplicates can't be made by a
/// Command) and opens the Project again.
fn reopen_with(shell: &mut Shell, rewrites: &[(&str, &str)]) -> Shell {
    shell.run("project.save", json!({})).unwrap();
    let folder = shell.engine().location().unwrap().to_path_buf();
    let sheet = std::fs::read_dir(&folder)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "redasch"))
        .min()
        .unwrap();
    let mut text = std::fs::read_to_string(&sheet).unwrap();
    for (from, to) in rewrites {
        let from = format!(r#""designator":"{from}""#);
        assert!(text.contains(&from), "{from}");
        text = text.replace(&from, &format!(r#""designator":"{to}""#));
    }
    std::fs::write(&sheet, text).unwrap();
    let mut reopened = Shell::new(engine());
    reopened.open_project(&folder).unwrap();
    reopened
}

fn shell(dir: &TempDir) -> Shell {
    let engine = Engine::builder()
        .ids(SeededIds::new(0))
        .clock(FixedClock::EPOCH)
        .user_dirs(None)
        .starter_library(Some(fixture_library()))
        .headless(false)
        .build();
    let mut shell = Shell::new(engine);
    shell
        .new_project(&dir.path().join("project"), "Demo")
        .unwrap();
    shell
}

fn sheet() -> Uuid {
    SHEET.parse().unwrap()
}

fn uuid(v: &Value) -> Uuid {
    v.as_str().unwrap().parse().unwrap()
}

/// Places R_10K on `document` at (`x`, `y`) mil; returns (Unit, Component).
fn place(shell: &mut Shell, document: Uuid, x: i64, y: i64) -> (Uuid, Uuid) {
    let out = shell
        .run(
            "schematic.component.place",
            json!({"document": document, "part": R_10K,
                   "at": {"x": format!("{x}mil"), "y": format!("{y}mil")}}),
        )
        .unwrap();
    (
        uuid(&out["result"]["unit"]),
        uuid(&out["result"]["component"]["uuid"]),
    )
}

fn add_sheet(shell: &mut Shell) -> Uuid {
    let added = shell
        .run("document.add", json!({"kind": "schematic"}))
        .unwrap();
    uuid(&added["result"]["uuid"])
}

fn designator(shell: &Shell, unit: Uuid) -> String {
    let project = shell.engine().project().unwrap();
    project
        .documents()
        .filter_map(|d| project.schematic_sheet(d.uuid))
        .find_map(|s| s.unit(unit))
        .unwrap()
        .designator
        .clone()
}

fn pairs(dialog: &ReannotateDialog) -> Vec<(String, String)> {
    dialog
        .rows()
        .iter()
        .map(|r| (r.old.clone(), r.new.clone()))
        .collect()
}

#[test]
fn modes_orders_and_the_threshold_change_the_preview() {
    let dir = TempDir::new("rd-modes");
    let mut shell = shell(&dir);
    // R1 at the origin, R2 to its left a little lower, R3 well below R1.
    let (a, _) = place(&mut shell, sheet(), 0, 0);
    let (b, _) = place(&mut shell, sheet(), -1000, -60);
    let (c, _) = place(&mut shell, sheet(), 0, -2000);
    let mut d = ReannotateDialog::new(&mut shell);
    assert_eq!(d.settings.source, Source::Schematic);
    assert_eq!(d.settings.order, Order::AcrossThenDown);
    assert_eq!(d.threshold_text(), "100 mil");
    // Across then down: R2 is within 100 mil of R1's row, and left of it.
    let changes = |d: &ReannotateDialog| -> Vec<(String, String)> {
        d.changes()
            .map(|r| (r.old.clone(), r.new.clone()))
            .collect()
    };
    let swap = [("R2", "R1"), ("R1", "R2")].map(|(a, b)| (a.to_owned(), b.to_owned()));
    assert_eq!(changes(&d), swap);

    // A smaller threshold gives R2 a row of its own, below R1's.
    d.set_threshold("50mil");
    d.refresh(&mut shell);
    assert!(changes(&d).is_empty(), "{:?}", changes(&d));
    // An unreadable threshold is flagged and blocks Apply.
    d.set_threshold("lots");
    assert!(d.threshold_problem().is_some());
    assert!(!d.can_apply());
    d.set_threshold("100mil");
    assert!(d.threshold_problem().is_none());

    // Down then across numbers the left column first.
    d.settings.order = Order::DownThenAcross;
    d.refresh(&mut shell);
    d.apply(&mut shell).unwrap();
    assert_eq!(
        [
            designator(&shell, b),
            designator(&shell, a),
            designator(&shell, c)
        ],
        ["R1", "R2", "R3"]
    );

    // Fix duplicates only renumbers just the later of two R1s.
    let mut shell = reopen_with(&mut shell, &[("R3", "R1")]);
    let mut d = ReannotateDialog::new(&mut shell);
    d.settings.mode = Mode::FixDuplicatesOnly;
    d.refresh(&mut shell);
    assert_eq!(changes(&d), [("R1".to_owned(), "R3".to_owned())]);
}

#[test]
fn the_sheet_grid_picks_sheets_and_start_indices_number_by_sheet() {
    let dir = TempDir::new("rd-sheets");
    let mut shell = shell(&dir);
    let (a, _) = place(&mut shell, sheet(), 0, 0);
    let second = add_sheet(&mut shell);
    let (b, _) = place(&mut shell, second, 0, 0);
    let (c, _) = place(&mut shell, second, 1000, 0);

    let mut d = ReannotateDialog::new(&mut shell);
    let grid = d.sheet_rows();
    assert_eq!(grid.len(), 2);
    assert_eq!((grid[0].number, grid[1].number), (1, 2));
    assert!(grid.iter().all(|s| s.start == 1));

    d.set_start_indices(StartIndices::Hundreds);
    let grid = d.sheet_rows();
    assert_eq!((grid[0].start, grid[1].start), (101, 201));
    d.refresh(&mut shell);
    d.apply(&mut shell).unwrap();
    assert_eq!(
        [
            designator(&shell, a),
            designator(&shell, b),
            designator(&shell, c)
        ],
        ["R101", "R201", "R202"]
    );

    // Scope Sheets with only the second sheet ticked leaves the first alone.
    let mut d = ReannotateDialog::new(&mut shell);
    assert_eq!(
        d.start_indices(),
        StartIndices::Hundreds,
        "the last settings"
    );
    d.set_start_indices(StartIndices::Thousands);
    d.settings.scope = ScopeKind::Sheets;
    d.set_sheet(sheet(), false);
    d.set_sheet(second, true);
    assert_eq!(d.settings.sheets, [second]);
    d.refresh(&mut shell);
    d.apply(&mut shell).unwrap();
    assert_eq!(
        [
            designator(&shell, a),
            designator(&shell, b),
            designator(&shell, c)
        ],
        ["R101", "R2001", "R2002"]
    );
    // No sheet ticked: nothing to run.
    d.set_sheet(second, false);
    d.refresh(&mut shell);
    assert!(!d.can_apply());
}

#[test]
fn the_preview_shows_only_changed_sorts_and_locks_per_row() {
    let dir = TempDir::new("rd-rows");
    let mut shell = shell(&dir);
    let (_, ca) = place(&mut shell, sheet(), 0, 0);
    place(&mut shell, sheet(), 2000, 0);
    let (_, cc) = place(&mut shell, sheet(), 1000, 0);
    // R1, R3, R2 left to right: R3 and R2 swap; R1 stays.
    let mut d = ReannotateDialog::new(&mut shell);
    d.only_changed = false;
    assert_eq!(
        pairs(&d),
        [("R1", "R1"), ("R3", "R2"), ("R2", "R3")].map(|(a, b)| (a.to_owned(), b.to_owned()))
    );
    d.only_changed = true;
    assert_eq!(pairs(&d).len(), 2);

    // The lock switch on a row locks it at once and the preview follows.
    d.set_locked(&mut shell, cc, true);
    d.only_changed = false;
    let rows = d.rows();
    let locked: Vec<&str> = rows
        .iter()
        .filter(|r| r.locked)
        .map(|r| r.old.as_str())
        .collect();
    assert_eq!(locked, ["R3"]);
    // Sorting by locked puts locked rows first; by changed, changed rows first.
    d.sort = RowSort::Locked;
    assert!(d.rows()[0].locked);
    d.sort = RowSort::Changed;
    let rows = d.rows();
    assert!(rows[0].old != rows[0].new || rows.iter().all(|r| r.old == r.new));
    let _ = ca;
}

#[test]
fn locked_duplicates_stay_and_the_preview_says_so() {
    let dir = TempDir::new("rd-kept");
    let mut shell = shell(&dir);
    let (_, ca) = place(&mut shell, sheet(), 0, 0);
    let (_, cb) = place(&mut shell, sheet(), 1000, 0);
    shell
        .run(
            "schematic.component.set",
            json!({"components": [ca, cb], "designator_locked": true}),
        )
        .unwrap();
    let mut shell = reopen_with(&mut shell, &[("R1", "R7"), ("R2", "R7")]);
    let d = ReannotateDialog::new(&mut shell);
    assert_eq!(d.kept_duplicates(), ["R7"]);
}

#[test]
fn the_dialog_opens_with_the_last_settings_and_reannotate_last_confirms_a_count() {
    let dir = TempDir::new("rd-last");
    let mut shell = shell(&dir);
    // No settings saved yet.
    assert!(ReannotateLast::new(&mut shell).is_err());

    let (a, _) = place(&mut shell, sheet(), 0, 0);
    let (b, _) = place(&mut shell, sheet(), 0, -1000);
    let mut d = ReannotateDialog::new(&mut shell);
    d.settings.order = Order::UpThenAcross;
    d.refresh(&mut shell);
    d.apply(&mut shell).unwrap();
    assert_eq!([designator(&shell, a), designator(&shell, b)], ["R2", "R1"]);

    let d = ReannotateDialog::new(&mut shell);
    assert_eq!(d.settings.order, Order::UpThenAcross, "the last settings");

    // Two more, placed out of order: Re-annotate (last settings) counts them first.
    let (c, _) = place(&mut shell, sheet(), 0, -2000);
    let (e, _) = place(&mut shell, sheet(), 0, 1000);
    let last = ReannotateLast::new(&mut shell).unwrap();
    assert!(last.count() > 0);
    assert!(last.message().contains(&last.count().to_string()));
    let journal = shell.engine().journal().len();
    last.run(&mut shell).unwrap();
    assert_eq!(shell.engine().journal().len(), journal + 1);
    assert_eq!(
        shell.engine().journal().last().unwrap().params["saved"],
        true
    );
    assert_eq!(
        [
            designator(&shell, c),
            designator(&shell, b),
            designator(&shell, a),
            designator(&shell, e)
        ],
        ["R1", "R2", "R3", "R4"]
    );
}

#[test]
fn the_board_source_offers_board_orders_sides_and_a_start_index() {
    let dir = TempDir::new("rd-board");
    let mut shell = shell(&dir);
    place(&mut shell, sheet(), 0, 0);
    let mut d = ReannotateDialog::new(&mut shell);
    d.set_source(Source::Board);
    assert_eq!(d.settings.order, Order::DescendingYThenAscendingX);
    assert_eq!(d.orders()[0], Order::DescendingYThenAscendingX);
    d.settings.start_index = 10;
    let params = d.params(&shell);
    assert_eq!(params["source"], "board");
    assert_eq!(params["start_index"], 10);
    assert_eq!(params["sides"], "top_then_bottom");
    assert!(params.get("per_sheet").is_none());
    d.set_source(Source::Schematic);
    assert_eq!(d.settings.order, Order::AcrossThenDown);
}
