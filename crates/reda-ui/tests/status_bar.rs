//! The status bar's readout, through the Shell with no window: the active Document's
//! kind, the cursor's X/Y in that Document's units at the display precision, ΔX/ΔY from
//! the last click and its keys, the grid, object snap mode, Layer and prompt.

use reda_testkit::TempDir;
use reda_ui::bars;
use reda_ui::engine::Engine;
use reda_ui::engine::command::UserDirs;
use reda_ui::engine::model::Uuid;
use reda_ui::engine::model::geom::{Nm, Point};
use reda_ui::keymap::{self, Chord};
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
    // A locked object says so.
    let via = shell
        .engine()
        .project()
        .unwrap()
        .pcb()
        .unwrap()
        .vias()
        .next()
        .unwrap()
        .uuid;
    shell
        .run("pcb.lock", json!({"objects": [via], "locked": true}))
        .unwrap();
    assert_eq!(readout(&shell).prompt.as_deref(), Some("Via · Locked"));
}

#[test]
fn the_delta_counts_from_the_last_click_and_its_keys_hide_and_reset_it() {
    let home = TempDir::new("status-home");
    let work = TempDir::new("status-work");
    let mut shell = shell(&home);
    shell.new_project(work.path(), "Status").unwrap();
    let pcb = shell.engine().project().unwrap().pcb().unwrap().uuid;
    shell
        .run("document.open", json!({"document": pcb}))
        .unwrap();
    assert_eq!(readout(&shell).delta, None, "no cursor yet");
    shell.pcb_hover(Point::new(Nm::from_mm(2), Nm::from_mm(1)));
    let shown = readout(&shell);
    assert_eq!(shown.cursor.as_deref(), Some("X 2.000 mm  Y 1.000 mm"));
    assert_eq!(shown.delta.as_deref(), Some("ΔX 2.000 mm  ΔY 1.000 mm"));

    // Insert puts the delta origin at the cursor, as a click does.
    press(&mut shell, pcb, "Insert");
    assert_eq!(
        readout(&shell).delta.as_deref(),
        Some("ΔX 0.000 mm  ΔY 0.000 mm")
    );
    shell.pcb_hover(Point::new(Nm::from_mm(3), Nm::from_mm(0)));
    assert_eq!(
        readout(&shell).delta.as_deref(),
        Some("ΔX 1.000 mm  ΔY -1.000 mm")
    );

    // Shift+D hides it for the session; the `editing.delta` Preference sets the default.
    press(&mut shell, pcb, "Shift+D");
    assert_eq!(readout(&shell).delta, None);
    press(&mut shell, pcb, "Shift+D");
    assert!(readout(&shell).delta.is_some());
    shell
        .run(
            "app.preferences.set",
            json!({"key": "editing.delta", "value": false}),
        )
        .unwrap();
    assert_eq!(readout(&shell).delta, None);
}

#[test]
fn the_delta_runs_on_a_sheet_and_insert_stays_add_vertex_in_the_wire_tool() {
    let home = TempDir::new("status-home");
    let work = TempDir::new("status-work");
    let mut shell = shell(&home);
    shell.new_project(work.path(), "Status").unwrap();
    let sheet = shell.active_document().unwrap().document;
    shell.document_tab_mut(sheet).unwrap().cursor = Some((254_000.0, 0.0));
    assert_eq!(
        readout(&shell).delta.as_deref(),
        Some("ΔX 10.0 mil  ΔY 0.0 mil")
    );
    // With the Select Tool and no Wire segment held, Insert resets the delta.
    press(&mut shell, sheet, "Insert");
    assert_eq!(
        readout(&shell).delta.as_deref(),
        Some("ΔX 0.0 mil  ΔY 0.0 mil")
    );
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

#[test]
fn the_delta_runs_in_the_symbol_and_footprint_editors() {
    const LIBRARY: &str = "000000aa-0000-4000-8000-000000000000";
    let home = TempDir::new("status-home");
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
    let mut shell = shell(&home);
    shell
        .run(
            "app.preferences.set",
            json!({"key": "libraries.paths", "value": [lib]}),
        )
        .unwrap();
    for kind in ["symbol", "footprint"] {
        let out = shell
            .run(
                "library.element.new",
                json!({"library": LIBRARY, "kind": kind, "name": kind}),
            )
            .unwrap();
        let element: Uuid = out["element"].as_str().unwrap().parse().unwrap();
        shell.document_tab_mut(element).unwrap().cursor = Some((1_000_000.0, 0.0));
        let delta = readout(&shell)
            .delta
            .unwrap_or_else(|| panic!("{kind}: no delta"));
        assert!(
            delta.starts_with("ΔX ") && !delta.starts_with("ΔX 0"),
            "{kind}: {delta}"
        );
        press(&mut shell, element, "Insert");
        let delta = readout(&shell).delta.unwrap();
        assert!(delta.starts_with("ΔX 0.0"), "{kind}: {delta}");
    }
}
