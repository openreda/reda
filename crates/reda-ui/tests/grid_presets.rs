//! Editable grid presets (docs/spec/v0.1/editing.md#grid), through the Shell with no
//! window: the `editing.grid` tables' Snap, Electrical Range, Visible and In cycle
//! columns, G and Shift+G, custom values, and the visible grid's toggle and style.

use reda_testkit::TempDir;
use reda_ui::bars;
use reda_ui::engine::Engine;
use reda_ui::engine::command::UserDirs;
use reda_ui::engine::model::Uuid;
use reda_ui::engine::model::geom::Nm;
use reda_ui::engine::preferences::GridStyle;
use reda_ui::keymap::{self, Chord};
use reda_ui::shell::Shell;
use serde_json::{Value, json};

const MIL: i64 = 25_400;

fn shell(home: &TempDir) -> Shell {
    let engine = Engine::builder()
        .user_dirs(Some(UserDirs::under(home.path())))
        .starter_library(None)
        .headless(false)
        .build();
    Shell::new(engine)
}

fn set_pref(shell: &mut Shell, key: &str, value: Value) {
    shell
        .run("app.preferences.set", json!({"key": key, "value": value}))
        .unwrap();
}

/// Runs what `key` is bound to on the active Document, as the window does.
fn press(shell: &mut Shell, document: Uuid, key: &str) -> Option<(String, Value)> {
    let chord = Chord::parse(key).unwrap();
    let active = keymap::active_for(shell, Some(document));
    let action = shell.keymap().lookup(&chord, &active).cloned()?;
    let (id, params) = bars::resolve_for(shell, &action, Some(document)).run.ok()?;
    let _ = shell.run(&id, params.clone());
    Some((id, params))
}

fn sheet_of(shell: &Shell) -> Uuid {
    shell.active_document().unwrap().document
}

#[test]
fn g_and_shift_g_step_through_the_in_cycle_rows_both_ways() {
    let home = TempDir::new("grid-cycle-home");
    let work = TempDir::new("grid-cycle-work");
    let mut s = shell(&home);
    s.new_project(work.path(), "Grid").unwrap();
    let sheet = sheet_of(&s);
    assert_eq!(s.grid(sheet), Nm(50 * MIL));
    press(&mut s, sheet, "G");
    assert_eq!(s.grid(sheet), Nm(100 * MIL));
    press(&mut s, sheet, "G");
    assert_eq!(s.grid(sheet), Nm(10 * MIL), "wraps to the finest");
    press(&mut s, sheet, "Shift+G");
    assert_eq!(s.grid(sheet), Nm(100 * MIL), "Shift+G goes back");
    press(&mut s, sheet, "Shift+G");
    assert_eq!(s.grid(sheet), Nm(50 * MIL));

    // Take 25 mil out of the cycle: G and Shift+G skip it, but it can still be picked.
    let mut rows = s
        .grids()
        .tables(bars::DocumentType::Schematic)
        .imperial
        .clone();
    rows[1].in_cycle = false;
    set_pref(
        &mut s,
        "editing.grid.schematic.imperial",
        serde_json::to_value(&rows).unwrap(),
    );
    press(&mut s, sheet, "Shift+G");
    assert_eq!(s.grid(sheet), Nm(10 * MIL));
    press(&mut s, sheet, "G");
    assert_eq!(s.grid(sheet), Nm(50 * MIL));
    s.run("view.grid.set", json!({"document": sheet, "grid": "25mil"}))
        .unwrap();
    assert_eq!(s.grid(sheet), Nm(25 * MIL));
}

#[test]
fn a_rows_electrical_range_and_visible_grid_come_from_its_table() {
    let home = TempDir::new("grid-rows-home");
    let work = TempDir::new("grid-rows-work");
    let mut s = shell(&home);
    s.new_project(work.path(), "Grid").unwrap();
    let sheet = sheet_of(&s);
    assert_eq!(s.electrical_range(sheet), Nm(40 * MIL));
    assert_eq!(
        s.document_tab(sheet).unwrap().grid_visible.pitch,
        Nm(50 * MIL),
        "Visible starts equal to Snap"
    );
    // A new row: 20 mil snap, 15 mil range, a 100 mil visible grid.
    set_pref(
        &mut s,
        "editing.grid.schematic.imperial",
        json!([{"snap": "20mil", "range": "15mil", "visible": "100mil", "in_cycle": true}]),
    );
    s.run("view.grid.set", json!({"document": sheet, "grid": "20mil"}))
        .unwrap();
    assert_eq!(s.grid(sheet), Nm(20 * MIL));
    assert_eq!(s.electrical_range(sheet), Nm(15 * MIL));
    assert_eq!(
        s.document_tab(sheet).unwrap().grid_visible.pitch,
        Nm(100 * MIL)
    );
    // The board's rows have no range; their Visible column draws too.
    let pcb = s.engine().project().unwrap().pcb().unwrap().uuid;
    s.run("document.open", json!({"document": pcb})).unwrap();
    set_pref(
        &mut s,
        "editing.grid.pcb.metric",
        json!([{"snap": "0.5mm", "visible": "1mm"}]),
    );
    s.run("view.grid.set", json!({"document": pcb, "grid": "0.5mm"}))
        .unwrap();
    assert_eq!(
        s.document_tab(pcb).unwrap().grid_visible.pitch,
        Nm(1_000_000)
    );
    let err = s
        .run("view.grid.set", json!({"document": pcb, "grid": "0.25mm"}))
        .unwrap_err();
    assert!(err.message.contains("0.5 mm"), "{}", err.message);
}

#[test]
fn a_custom_grid_takes_any_value_with_a_range_from_0_8_and_cycling_discards_it() {
    let home = TempDir::new("grid-custom-home");
    let work = TempDir::new("grid-custom-work");
    let mut s = shell(&home);
    s.new_project(work.path(), "Grid").unwrap();
    let sheet = sheet_of(&s);
    // Mod+Shift+G asks for the value: its binding leaves the grid out.
    let (id, params) = press(&mut s, sheet, "Mod+Shift+G").unwrap();
    assert_eq!(id, "view.grid.custom");
    assert!(reda_ui::command_dialog::lacks_required(
        s.engine().registry(),
        &id,
        &params
    ));

    let out = s
        .run(
            "view.grid.custom",
            json!({"document": sheet, "grid": "30mil"}),
        )
        .unwrap();
    assert_eq!(out["grid"], 30 * MIL);
    assert_eq!(out["range"], 24 * MIL, "0.8 of the step");
    assert!(out.get("warning").is_none());
    assert_eq!(s.grid(sheet), Nm(30 * MIL));
    assert_eq!(s.electrical_range(sheet), Nm(24 * MIL));

    // A range at or above the step warns and is kept as given.
    let out = s
        .run(
            "view.grid.custom",
            json!({"document": sheet, "grid": "30mil", "range": "40mil"}),
        )
        .unwrap();
    assert_eq!(out["range"], 40 * MIL);
    assert!(out["warning"].as_str().unwrap().contains("at or above"));
    assert!(
        s.note().unwrap().contains("at or above"),
        "the window shows the warning"
    );
    assert_eq!(s.electrical_range(sheet), Nm(40 * MIL));

    // G discards it for the next row up; the range is the row's again.
    press(&mut s, sheet, "G");
    assert_eq!(s.grid(sheet), Nm(50 * MIL));
    assert_eq!(s.electrical_range(sheet), Nm(40 * MIL));
    s.run(
        "view.grid.custom",
        json!({"document": sheet, "grid": "30mil"}),
    )
    .unwrap();
    press(&mut s, sheet, "Shift+G");
    assert_eq!(s.grid(sheet), Nm(25 * MIL), "Shift+G: the next row down");

    // A board takes a custom step but has no range.
    let pcb = s.engine().project().unwrap().pcb().unwrap().uuid;
    s.run(
        "view.grid.custom",
        json!({"document": pcb, "grid": "0.2mm"}),
    )
    .unwrap();
    assert_eq!(s.grid(pcb), Nm(200_000));
    let err = s
        .run(
            "view.grid.custom",
            json!({"document": pcb, "grid": "0.2mm", "range": "0.1mm"}),
        )
        .unwrap_err();
    assert_eq!(err.details.unwrap()["pointer"], "/range");
}

#[test]
fn the_visible_grid_toggles_and_draws_dots_or_lines_with_a_coarse_multiplier() {
    let home = TempDir::new("grid-visible-home");
    let work = TempDir::new("grid-visible-work");
    let mut s = shell(&home);
    s.new_project(work.path(), "Grid").unwrap();
    let sheet = sheet_of(&s);
    let visible = |s: &Shell| s.document_tab(sheet).unwrap().grid_visible;
    assert!(visible(&s).shown);
    assert_eq!(visible(&s).style, GridStyle::Dots);
    assert_eq!(visible(&s).coarse, 5);
    press(&mut s, sheet, "Alt+G");
    assert!(!visible(&s).shown, "the toggle key hides it");
    press(&mut s, sheet, "Alt+G");
    assert!(visible(&s).shown);
    s.run(
        "view.grid.show",
        json!({"document": sheet, "visible": false}),
    )
    .unwrap();
    assert!(!visible(&s).shown);

    set_pref(&mut s, "editing.grid.style", json!("lines"));
    set_pref(&mut s, "editing.grid.coarse_multiplier", json!(10));
    assert_eq!(visible(&s).style, GridStyle::Lines);
    assert_eq!(visible(&s).coarse, 10);
}
