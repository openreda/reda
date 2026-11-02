//! The status bar's readout, through the Shell with no window: the active Document's kind,
//! the cursor's X/Y in that Document's units, and its grid.

use reda_testkit::TempDir;
use reda_ui::engine::Engine;
use reda_ui::engine::command::UserDirs;
use reda_ui::engine::model::geom::{Nm, Point};
use reda_ui::shell::Shell;
use reda_ui::status_bar::{Readout, readout};
use serde_json::json;

fn shell(home: &TempDir) -> Shell {
    let engine = Engine::builder()
        .user_dirs(Some(UserDirs::under(home.path())))
        .starter_library(None)
        .headless(false)
        .build();
    Shell::new(engine)
}

#[test]
fn with_no_document_the_readout_is_empty() {
    let home = TempDir::new("status-home");
    let shell = shell(&home);
    assert_eq!(readout(&shell), Readout::default());
}

#[test]
fn a_sheet_reads_its_kind_grid_and_cursor_in_the_grids_units() {
    let home = TempDir::new("status-home");
    let work = TempDir::new("status-work");
    let mut shell = shell(&home);
    shell.new_project(work.path(), "Status").unwrap();
    let sheet = shell.active_document().unwrap().document;
    let shown = readout(&shell);
    assert_eq!(shown.kind.as_deref(), Some("Active: Schematic Sheet"));
    assert_eq!(shown.grid.as_deref(), Some("Grid: 50 mil"));
    // Until the pointer has been over the canvas there is no cursor.
    assert_eq!(shown.cursor, None);

    shell.document_tab_mut(sheet).unwrap().cursor = Some((1_270_000.0, -2_540_000.0));
    assert_eq!(
        readout(&shell).cursor.as_deref(),
        Some("X 50 mil  Y -100 mil")
    );
    // A metric grid reads in mm.
    shell
        .run(
            "view.grid.set",
            json!({"document": sheet, "grid": 1_000_000}),
        )
        .unwrap();
    assert_eq!(
        readout(&shell).cursor.as_deref(),
        Some("X 1.27 mm  Y -2.54 mm")
    );
    assert_eq!(readout(&shell).grid.as_deref(), Some("Grid: 1 mm"));
}

#[test]
fn a_board_reads_where_the_cursor_snapped_in_its_units() {
    let home = TempDir::new("status-home");
    let work = TempDir::new("status-work");
    let mut shell = shell(&home);
    shell.new_project(work.path(), "Status").unwrap();
    let pcb = shell.engine().project().unwrap().pcb().unwrap().uuid;
    shell
        .run("document.open", json!({"document": pcb}))
        .unwrap();
    shell
        .run("view.grid.set", json!({"document": pcb, "grid": 500_000}))
        .unwrap();
    shell.pcb_hover(Point::new(Nm(1_100_000), Nm(2_400_000)));
    let shown = readout(&shell);
    assert_eq!(shown.kind.as_deref(), Some("Active: PCB Document"));
    assert_eq!(shown.cursor.as_deref(), Some("X 1.00 mm  Y 2.50 mm"));
    assert_eq!(shown.grid.as_deref(), Some("Grid: 0.5 mm"));
}
