//! The status bar's readout and the HUD, through the Shell with no window: the active
//! Document's kind, the cursor's X/Y in that Document's units at the display precision,
//! its grid, object snap mode, Layer and prompt; and the HUD's items and keys.

use reda_testkit::TempDir;
use reda_ui::engine::Engine;
use reda_ui::engine::command::UserDirs;
use reda_ui::engine::model::Uuid;
use reda_ui::engine::model::geom::{Nm, Point};
use reda_ui::keymap::{self, Chord};
use reda_ui::shell::Shell;
use reda_ui::status_bar::{Readout, readout};
use reda_ui::{bars, hud};
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
        Some("X 50.0 mil  Y -100.0 mil")
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
        Some("X 1.270 mm  Y -2.540 mm")
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
    assert_eq!(shown.cursor.as_deref(), Some("X 1.000 mm  Y 2.500 mm"));
    assert_eq!(shown.grid.as_deref(), Some("Grid: 0.5 mm"));
}

fn press(shell: &mut Shell, document: Uuid, key: &str) {
    let chord = Chord::parse(key).unwrap();
    let active = keymap::active_for(shell, Some(document));
    let action = shell.keymap().lookup(&chord, &active).cloned().unwrap();
    let (id, params) = bars::resolve_for(shell, &action, Some(document))
        .run
        .unwrap();
    shell.run(&id, params).unwrap();
}

#[test]
fn coordinates_follow_the_display_precision_preferences() {
    let home = TempDir::new("status-home");
    let work = TempDir::new("status-work");
    let mut shell = shell(&home);
    shell.new_project(work.path(), "Status").unwrap();
    let sheet = shell.active_document().unwrap().document;
    shell.document_tab_mut(sheet).unwrap().cursor = Some((1_272_540.0, 0.0));
    // 3 decimals for mm and 1 for mil by default.
    assert_eq!(
        readout(&shell).cursor.as_deref(),
        Some("X 50.1 mil  Y 0.0 mil")
    );
    for (key, value) in [
        ("editing.display_precision.mil", 3),
        ("editing.display_precision.mm", 1),
    ] {
        shell
            .run("app.preferences.set", json!({"key": key, "value": value}))
            .unwrap();
    }
    assert_eq!(
        readout(&shell).cursor.as_deref(),
        Some("X 50.100 mil  Y 0.000 mil")
    );
    shell
        .run("view.grid.set", json!({"document": sheet, "grid": "1mm"}))
        .unwrap();
    assert_eq!(
        readout(&shell).cursor.as_deref(),
        Some("X 1.3 mm  Y 0.0 mm"),
        "rounding is for display only"
    );
    assert_eq!(
        shell.document_tab(sheet).unwrap().cursor,
        Some((1_272_540.0, 0.0))
    );
}

#[test]
fn the_snap_mode_layer_and_prompt_segments_read_the_active_document() {
    let home = TempDir::new("status-home");
    let work = TempDir::new("status-work");
    let mut shell = shell(&home);
    shell.new_project(work.path(), "Status").unwrap();
    let sheet = shell.active_document().unwrap().document;
    let shown = readout(&shell);
    assert_eq!(shown.snap.as_deref(), Some("On"));
    assert_eq!(shown.layer, None, "a sheet has no Layer swatch");
    assert_eq!(shown.prompt, None, "nothing under the cursor, no Tool");
    // Object snap Off is blank, as in the leading suite.
    shell
        .run("view.snap.set", json!({"document": sheet, "mode": "off"}))
        .unwrap();
    assert_eq!(readout(&shell).snap.as_deref(), Some(""));
    // A Tool shows its prompt.
    shell.run("tool.activate:wire", json!({})).unwrap();
    assert_eq!(readout(&shell).prompt.as_deref(), Some("Wire: 90°"));
    shell.run("tool.activate:select", json!({})).unwrap();

    let pcb = shell.engine().project().unwrap().pcb().unwrap().uuid;
    shell
        .run("document.open", json!({"document": pcb}))
        .unwrap();
    let shown = readout(&shell);
    assert_eq!(shown.snap.as_deref(), Some("Active Layer"));
    assert!(shown.layer.unwrap().contains("Top"));
    shell.run("tool.activate:route", json!({})).unwrap();
    assert!(readout(&shell).prompt.unwrap().starts_with("Tool: "));
}

#[test]
fn the_prompt_names_the_object_under_the_cursor_with_no_tool() {
    let home = TempDir::new("status-home");
    let work = TempDir::new("status-work");
    let mut shell = shell(&home);
    shell.new_project(work.path(), "Status").unwrap();
    let pcb = shell.engine().project().unwrap().pcb().unwrap().uuid;
    shell
        .run("document.open", json!({"document": pcb}))
        .unwrap();
    shell
        .run("pcb.via.add", json!({"at": {"x": "5mm", "y": "5mm"}}))
        .unwrap();
    shell.pcb_hover(Point::new(Nm::from_mm(5), Nm::from_mm(5)));
    assert_eq!(readout(&shell).prompt.as_deref(), Some("Via"));
}

#[test]
fn the_hud_shows_its_items_and_its_keys_toggle_it_and_reset_the_delta() {
    let home = TempDir::new("status-home");
    let work = TempDir::new("status-work");
    let mut shell = shell(&home);
    shell.new_project(work.path(), "Status").unwrap();
    let pcb = shell.engine().project().unwrap().pcb().unwrap().uuid;
    shell
        .run("document.open", json!({"document": pcb}))
        .unwrap();
    assert!(hud::lines(&shell, pcb).is_empty(), "no cursor yet");
    shell.pcb_hover(Point::new(Nm::from_mm(2), Nm::from_mm(1)));
    let lines = hud::lines(&shell, pcb);
    assert_eq!(lines.len(), 4, "{lines:?}");
    assert_eq!(lines[0], "X 2.000 mm  Y 1.000 mm");
    assert_eq!(lines[1], "ΔX 2.000 mm  ΔY 1.000 mm");
    assert!(lines[2].starts_with("Layer: "), "{lines:?}");
    assert_eq!(lines[3], "Grid: 0.5 mm");

    // Insert puts the delta origin at the cursor, as a click does.
    press(&mut shell, pcb, "Insert");
    assert_eq!(hud::lines(&shell, pcb)[1], "ΔX 0.000 mm  ΔY 0.000 mm");
    shell.pcb_hover(Point::new(Nm::from_mm(3), Nm::from_mm(0)));
    assert_eq!(hud::lines(&shell, pcb)[1], "ΔX 1.000 mm  ΔY -1.000 mm");

    // Shift+D hides the delta line; Shift+H the whole HUD.
    press(&mut shell, pcb, "Shift+D");
    assert!(!hud::lines(&shell, pcb).iter().any(|l| l.starts_with("ΔX")));
    press(&mut shell, pcb, "Shift+D");
    press(&mut shell, pcb, "Shift+H");
    assert!(!hud::shown(&shell));
    assert!(hud::lines(&shell, pcb).is_empty());
    press(&mut shell, pcb, "Shift+H");
    assert!(hud::shown(&shell));

    // Each item has its checkbox in Preferences.
    for key in ["xy", "layer"] {
        shell
            .run(
                "app.preferences.set",
                json!({"key": format!("editing.hud.{key}"), "value": false}),
            )
            .unwrap();
    }
    assert_eq!(
        hud::lines(&shell, pcb),
        ["ΔX 1.000 mm  ΔY -1.000 mm", "Grid: 0.5 mm"]
    );
}

#[test]
fn the_hud_runs_on_a_sheet_and_insert_stays_add_vertex_in_the_wire_tool() {
    let home = TempDir::new("status-home");
    let work = TempDir::new("status-work");
    let mut shell = shell(&home);
    shell.new_project(work.path(), "Status").unwrap();
    let sheet = shell.active_document().unwrap().document;
    shell.document_tab_mut(sheet).unwrap().cursor = Some((254_000.0, 0.0));
    assert_eq!(
        hud::lines(&shell, sheet),
        [
            "X 10.0 mil  Y 0.0 mil",
            "ΔX 10.0 mil  ΔY 0.0 mil",
            "Grid: 50 mil"
        ]
    );
    // With the Select Tool and no Wire segment held, Insert resets the delta.
    press(&mut shell, sheet, "Insert");
    assert_eq!(hud::lines(&shell, sheet)[1], "ΔX 0.0 mil  ΔY 0.0 mil");
    // In the Wire Tool, the Tool context wins: Insert adds a vertex.
    shell.run("tool.activate:wire", json!({})).unwrap();
    let chord = Chord::parse("Insert").unwrap();
    let active = keymap::active_for(&shell, Some(sheet));
    let action = shell.keymap().lookup(&chord, &active).cloned().unwrap();
    assert_eq!(
        bars::resolve_for(&shell, &action, Some(sheet)).command,
        "tool.insert_vertex"
    );
}
