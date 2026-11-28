//! Board Grids in the editor (docs/spec/v0.3/grids-guides.md#board-grids), through the
//! Shell with no window: G and Shift+G cycle the board's In-cycle Board Grids together
//! with the user's presets in either units, a board with no view state opens on its
//! Default Board Grid, the custom-value dialog's Keep on this board tick, and cycling
//! never dirtying the Document.

use std::path::PathBuf;

use reda_testkit::TempDir;
use reda_ui::bars;
use reda_ui::command_dialog::ParamsDialog;
use reda_ui::engine::Engine;
use reda_ui::engine::command::UserDirs;
use reda_ui::engine::model::Uuid;
use reda_ui::engine::model::geom::Nm;
use reda_ui::engine::preferences::Units;
use reda_ui::grid::Step;
use reda_ui::grid::custom_dialog::CustomGridDialog;
use reda_ui::keymap::{self, Chord};
use reda_ui::shell::Shell;
use serde_json::{Value, json};

const MIL: i64 = 25_400;
const UM: i64 = 1_000;

fn engine(home: &TempDir) -> Engine {
    Engine::builder()
        .user_dirs(Some(UserDirs::under(home.path())))
        .starter_library(None)
        .headless(false)
        .build()
}

struct Board {
    shell: Shell,
    pcb: Uuid,
    project: PathBuf,
    home: TempDir,
    _work: TempDir,
}

/// A new Project with its board open, in mm on the shipped presets (0.1, 0.25, 0.5 and
/// 1 mm; 5, 10, 25 and 50 mil), starting on 0.5 mm.
fn board(label: &str) -> Board {
    let home = TempDir::new(&format!("{label}-home"));
    let work = TempDir::new(&format!("{label}-work"));
    let mut shell = Shell::new(engine(&home));
    shell.new_project(work.path(), "Board").unwrap();
    let pcb = shell.engine().project().unwrap().pcb().unwrap().uuid;
    shell
        .run("document.open", json!({"document": pcb}))
        .unwrap();
    Board {
        shell,
        pcb,
        project: work.path().join("Board"),
        home,
        _work: work,
    }
}

fn run(shell: &mut Shell, id: &str, params: Value) -> Value {
    match shell.run(id, params.clone()) {
        Ok(out) => out,
        Err(e) => panic!("{id} {params} failed: {e}"),
    }
}

/// Runs what `key` is bound to on `document`, as the window does.
fn press(shell: &mut Shell, document: Uuid, key: &str) {
    let chord = Chord::parse(key).unwrap();
    let active = keymap::active_for(shell, Some(document));
    let action = shell.keymap().lookup(&chord, &active).cloned().unwrap();
    let (id, params) = bars::resolve_for(shell, &action, Some(document))
        .run
        .unwrap();
    run(shell, &id, params);
}

/// The Step X each of `n` presses of `key` lands on, in µm.
fn cycle(b: &mut Board, key: &str, n: usize) -> Vec<i64> {
    (0..n)
        .map(|_| {
            press(&mut b.shell, b.pcb, key);
            let step = b.shell.grid(b.pcb);
            assert!(step.is_square(), "{step:?}");
            step.x.0 / UM
        })
        .collect()
}

fn add_grid(shell: &mut Shell, params: Value) -> Uuid {
    let out = run(shell, "pcb.board_grid.add", params);
    out["result"]["uuid"].as_str().unwrap().parse().unwrap()
}

#[test]
fn g_cycles_in_cycle_board_grids_with_the_presets_by_size_in_either_units() {
    let mut b = board("board-grid-cycle");
    let s = &mut b.shell;
    add_grid(s, json!({"step_x": "0.3mm"}));
    add_grid(s, json!({"step_x": "25mil"}));
    // Equal to the 0.5 mm preset: it shows once.
    add_grid(s, json!({"step_x": "0.5mm"}));
    // Not In cycle: never reached by G.
    add_grid(s, json!({"step_x": "0.7mm", "in_cycle": false}));
    assert_eq!(b.shell.grid(b.pcb), Step::square(Nm(500 * UM)));
    // In mm: the metric presets with every In-cycle Board Grid, 25 mil among them.
    assert_eq!(
        cycle(&mut b, "G", 6),
        [635, 1_000, 100, 250, 300, 500],
        "sorted by size, wrapping round"
    );
    assert_eq!(cycle(&mut b, "Shift+G", 2), [300, 250]);
    assert_eq!(
        b.shell.board_view().units,
        Units::Mm,
        "a Board Grid keeps the units"
    );

    // In mil: the imperial presets with the same Board Grids, 0.3 mm among them.
    run(&mut b.shell, "view.units.toggle", json!({}));
    run(
        &mut b.shell,
        "view.grid.set",
        json!({"document": b.pcb, "grid": "5mil"}),
    );
    let mils = [10 * MIL, 300 * UM, 500 * UM, 25 * MIL, 50 * MIL, 5 * MIL];
    assert_eq!(
        cycle(&mut b, "G", 6),
        mils.map(|nm| nm / UM),
        "10 mil, 0.3 mm, 0.5 mm, 25 mil, 50 mil, then round to 5 mil"
    );
    assert_eq!(b.shell.board_view().units, Units::Mil);
}

#[test]
fn a_board_grid_with_a_step_y_cycles_after_the_square_one_and_draws_its_visible() {
    let mut b = board("board-grid-xy");
    add_grid(
        &mut b.shell,
        json!({"step_x": "0.5mm", "step_y": "1.27mm", "visible": "1mm"}),
    );
    press(&mut b.shell, b.pcb, "G");
    let step = b.shell.grid(b.pcb);
    assert_eq!(step, Step::new(Nm(500 * UM), Some(Nm(1_270 * UM))));
    // Visible follows the pair: twice Step X across, twice Step Y down.
    let visible = b.shell.document_tab(b.pcb).unwrap().grid_visible.pitch;
    assert_eq!(visible, Step::new(Nm(1_000 * UM), Some(Nm(2_540 * UM))));
    press(&mut b.shell, b.pcb, "G");
    assert_eq!(b.shell.grid(b.pcb), Step::square(Nm(1_000 * UM)));
}

#[test]
fn cycling_never_dirties_the_document_or_adds_an_undo_step() {
    let mut b = board("board-grid-clean");
    add_grid(&mut b.shell, json!({"step_x": "0.3mm"}));
    run(&mut b.shell, "project.save", json!({}));
    let file = b.project.join("Board.redapcb");
    let saved = std::fs::read(&file).unwrap();
    cycle(&mut b, "G", 5);
    cycle(&mut b, "Shift+G", 3);
    assert!(!b.shell.engine().is_dirty());
    run(&mut b.shell, "project.save", json!({}));
    assert_eq!(std::fs::read(&file).unwrap(), saved);
    // The board's last undo step is still the Board Grid's add.
    run(&mut b.shell, "edit.undo", json!({"document": b.pcb}));
    let pcb = b.shell.engine().project().unwrap().pcb().unwrap();
    assert_eq!(pcb.board_grids().count(), 0);
}

/// Reopens `b`'s Project in a fresh Shell, as on another machine when `view` is false (no
/// `.reda/view.json`).
fn reopen(b: &mut Board, view: bool) -> Shell {
    run(&mut b.shell, "project.save", json!({}));
    b.shell.save_view();
    let view_file = b.project.join(".reda/view.json");
    if !view {
        let _ = std::fs::remove_file(&view_file);
    }
    let mut again = Shell::new(engine(&b.home));
    again.open_project(&b.project).unwrap();
    again
}

#[test]
fn a_board_with_no_view_state_opens_on_its_default_board_grid() {
    let mut b = board("board-grid-default");
    // Without a Default Board Grid: the user's default preset.
    assert_eq!(
        reopen(&mut b, false).grid(b.pcb),
        Step::square(Nm(500 * UM))
    );
    add_grid(&mut b.shell, json!({"step_x": "0.3mm"}));
    add_grid(
        &mut b.shell,
        json!({"step_x": "20mil", "step_y": "0.5mm", "default": true, "in_cycle": false}),
    );
    let fresh = reopen(&mut b, false);
    assert_eq!(
        fresh.grid(b.pcb),
        Step::new(Nm(20 * MIL), Some(Nm(500 * UM)))
    );
    // After that the view state remembers the last grid used, a Board Grid too.
    run(
        &mut b.shell,
        "view.grid.set",
        json!({"document": b.pcb, "grid": "0.3mm"}),
    );
    let again = reopen(&mut b, true);
    assert_eq!(again.grid(b.pcb), Step::square(Nm(300 * UM)));
}

/// Runs what the dialog gives back, and what it runs next, as the window does.
fn submit(shell: &mut Shell, dialog: &mut CustomGridDialog) -> String {
    let id = dialog.command().to_owned();
    let out = run(shell, &id, dialog.params().unwrap());
    if let Some((next, params)) = dialog.then(&out) {
        run(shell, &next, params);
    }
    id
}

#[test]
fn keep_on_this_board_makes_the_custom_value_a_board_grid_that_cycling_keeps() {
    let mut b = board("board-grid-keep");
    let doc = b.pcb;
    // Unticked: a custom grid, which cycling discards.
    let mut dialog = CustomGridDialog::new(&b.shell, json!({"document": doc}));
    dialog.step = "0.3".into();
    assert_eq!(submit(&mut b.shell, &mut dialog), "view.grid.custom");
    assert_eq!(
        b.shell
            .engine()
            .project()
            .unwrap()
            .pcb()
            .unwrap()
            .board_grids()
            .count(),
        0
    );
    press(&mut b.shell, doc, "G");
    press(&mut b.shell, doc, "Shift+G");
    assert_eq!(b.shell.grid(doc), Step::square(Nm(250 * UM)), "discarded");

    // Ticked: the value, in the unit it was typed in, becomes a Board Grid and the grid.
    let mut dialog = CustomGridDialog::new(&b.shell, json!({"document": doc}));
    dialog.step = "12mil, 0.4".into();
    dialog.keep = true;
    assert_eq!(submit(&mut b.shell, &mut dialog), "pcb.board_grid.add");
    let pcb = b.shell.engine().project().unwrap().pcb().unwrap();
    let grids: Vec<_> = pcb.board_grids().cloned().collect();
    assert_eq!(grids.len(), 1);
    assert_eq!(grids[0].step_x.text().as_deref(), Some("12mil"));
    assert_eq!(
        grids[0].step_y.and_then(|y| y.text()).as_deref(),
        Some("0.4mm")
    );
    assert!(grids[0].in_cycle);
    let kept = Step::new(Nm(12 * MIL), Some(Nm(400 * UM)));
    assert_eq!(b.shell.grid(doc), kept);
    assert!(!b.shell.grids().is_custom(doc));
    // Cycling comes back round to it.
    press(&mut b.shell, doc, "G");
    press(&mut b.shell, doc, "Shift+G");
    assert_eq!(b.shell.grid(doc), kept);
}

#[test]
fn keep_on_this_board_is_offered_on_a_board_only() {
    let b = board("board-grid-keep-offer");
    let dialog = CustomGridDialog::new(&b.shell, json!({"document": b.pcb}));
    assert!(dialog.offers_keep());
    let sheet = (b.shell.engine().project().unwrap().documents())
        .find(|d| d.uuid != b.pcb)
        .unwrap()
        .uuid;
    let dialog = CustomGridDialog::new(&b.shell, json!({"document": sheet}));
    assert!(!dialog.offers_keep());
}
