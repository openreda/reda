//! Board Setup ▸ Grids (docs/spec/v0.3/grids-guides.md#board-grids), without a window:
//! it lists the board's Board Grids with add, remove, In cycle and the Default mark, and
//! Apply turns the edits into `pcb.board_grid.*` steps on the board's undo stack.

use reda_testkit::TempDir;
use reda_ui::engine::Engine;
use reda_ui::engine::command::UserDirs;
use reda_ui::engine::model::{BoardGrid, PcbDocument, Uuid};
use reda_ui::settings::board_setup::{self, BoardGridRow, GridsPage};
use reda_ui::settings::{SettingsDialog, Step};
use reda_ui::shell::Shell;
use serde_json::json;

struct Setup {
    shell: Shell,
    _home: TempDir,
    _work: TempDir,
}

fn setup(label: &str) -> Setup {
    let home = TempDir::new(&format!("{label}-home"));
    let work = TempDir::new(&format!("{label}-work"));
    let engine = Engine::builder()
        .user_dirs(Some(UserDirs::under(home.path())))
        .starter_library(None)
        .headless(false)
        .build();
    let mut shell = Shell::new(engine);
    shell.new_project(work.path(), "Board").unwrap();
    Setup {
        shell,
        _home: home,
        _work: work,
    }
}

fn dialog(shell: &Shell) -> SettingsDialog {
    board_setup::dialog(shell.engine().project().unwrap())
}

fn pcb(shell: &Shell) -> &PcbDocument {
    shell.engine().project().unwrap().pcb().unwrap()
}

fn grids(shell: &Shell) -> Vec<BoardGrid> {
    pcb(shell).board_grids().cloned().collect()
}

fn add(shell: &mut Shell, params: serde_json::Value) -> Uuid {
    let out = shell.run("pcb.board_grid.add", params).unwrap();
    out["result"]["uuid"].as_str().unwrap().parse().unwrap()
}

fn page(d: &mut SettingsDialog) -> &mut GridsPage {
    d.page_mut::<GridsPage>().expect("a Grids page")
}

#[test]
fn the_grids_page_lists_the_board_grids_with_their_steps_as_typed() {
    let mut s = setup("bsg-list");
    let a = add(&mut s.shell, json!({"step_x": "0.3mm", "default": true}));
    let b = add(
        &mut s.shell,
        json!({"step_x": "25mil", "step_y": "1.27mm", "visible": "100mil", "in_cycle": false}),
    );
    let mut d = dialog(&s.shell);
    assert!(d.page_titles().contains(&"Grids".to_owned()));
    let rows = page(&mut d).rows().to_vec();
    assert_eq!(
        rows,
        [
            BoardGridRow {
                uuid: Some(a),
                step: "0.3mm".into(),
                visible: String::new(),
                in_cycle: true,
                default: true,
            },
            BoardGridRow {
                uuid: Some(b),
                step: "25mil, 1.27mm".into(),
                visible: "100mil".into(),
                in_cycle: false,
                default: false,
            },
        ]
    );
    assert!(!d.has_changes());
}

#[test]
fn add_remove_in_cycle_and_default_apply_as_board_grid_steps() {
    let mut s = setup("bsg-apply");
    let a = add(&mut s.shell, json!({"step_x": "0.3mm", "default": true}));
    let b = add(&mut s.shell, json!({"step_x": "0.4mm"}));
    let c = add(&mut s.shell, json!({"step_x": "0.6mm"}));
    let mut d = dialog(&s.shell);
    let p = page(&mut d);
    p.remove(2);
    p.set_in_cycle(0, false);
    // Marking another row moves the mark: the old one loses it first.
    p.set_default(1, true);
    p.add();
    p.set_step(2, "12mil, 0.5");
    p.set_visible(2, "1mm");
    assert_eq!(
        d.steps(),
        [
            Step::new("pcb.board_grid.delete", json!({"uuid": c})),
            Step::new(
                "pcb.board_grid.set",
                json!({"uuid": a, "in_cycle": false, "default": false})
            ),
            Step::new("pcb.board_grid.set", json!({"uuid": b, "default": true})),
            Step::new(
                "pcb.board_grid.add",
                json!({"step_x": "12mil", "step_y": "0.5mm", "visible": "1mm",
                       "in_cycle": true, "default": false})
            ),
        ]
    );
    d.apply(&mut s.shell).unwrap();
    let after = grids(&s.shell);
    assert_eq!(after.len(), 3);
    assert_eq!(pcb(&s.shell).default_board_grid().map(|g| g.uuid), Some(b));
    assert!(!pcb(&s.shell).board_grid(a).unwrap().in_cycle);
    let new = after.iter().find(|g| ![a, b].contains(&g.uuid)).unwrap();
    assert_eq!(new.step_x.text().as_deref(), Some("12mil"));
    assert_eq!(new.step_y.and_then(|y| y.text()).as_deref(), Some("0.5mm"));
    // Applied, the page has nothing pending, and it is one undo step on the board.
    assert!(!d.has_changes());
    let doc = pcb(&s.shell).uuid;
    s.shell.run("edit.undo", json!({"document": doc})).unwrap();
    assert_eq!(grids(&s.shell).len(), 3);
    assert_eq!(pcb(&s.shell).default_board_grid().map(|g| g.uuid), Some(a));
    assert!(pcb(&s.shell).board_grid(c).is_some());
}

#[test]
fn editing_a_step_sets_it_and_clearing_step_y_or_visible_sends_null() {
    let mut s = setup("bsg-edit");
    let a = add(
        &mut s.shell,
        json!({"step_x": "0.5mm", "step_y": "1mm", "visible": "2mm"}),
    );
    let mut d = dialog(&s.shell);
    let p = page(&mut d);
    p.set_step(0, "0.5mm");
    p.set_visible(0, "");
    assert_eq!(
        d.steps(),
        [Step::new(
            "pcb.board_grid.set",
            json!({"uuid": a, "step_y": null, "visible": null})
        )]
    );
    page(&mut d).set_step(0, "0.6 mm, 1 mm");
    page(&mut d).set_visible(0, "2mm");
    assert_eq!(
        d.steps(),
        [Step::new(
            "pcb.board_grid.set",
            json!({"uuid": a, "step_x": "0.6mm"})
        )]
    );
    // Ticking Default on the marked row again clears it.
    page(&mut d).set_default(0, true);
    page(&mut d).set_default(0, false);
    d.apply(&mut s.shell).unwrap();
    assert_eq!(
        pcb(&s.shell)
            .board_grid(a)
            .unwrap()
            .step_x
            .text()
            .as_deref(),
        Some("0.6mm")
    );
}

#[test]
fn a_bad_step_is_flagged_on_the_page_and_blocks_apply() {
    let s = setup("bsg-bad");
    let mut d = dialog(&s.shell);
    page(&mut d).add();
    assert!(!d.can_ok(), "a new row needs a step");
    page(&mut d).set_step(0, "0");
    let problems = d.problems();
    assert_eq!(problems.len(), 1);
    assert_eq!(problems[0].0, "Grids");
    assert_eq!(problems[0].1.field, "grid/0/step");
    page(&mut d).set_step(0, "1mm, 2mm, 3mm");
    assert_eq!(d.problems()[0].1.field, "grid/0/step");
    page(&mut d).set_step(0, "1mm");
    page(&mut d).set_visible(0, "-1mm");
    assert_eq!(d.problems()[0].1.field, "grid/0/visible");
    page(&mut d).set_visible(0, "5mm");
    assert!(d.can_apply());
}
