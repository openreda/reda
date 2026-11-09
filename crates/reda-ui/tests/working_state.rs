//! The working state of each Document: the Selection Filter (`selection.filter.set`), the
//! object snap mode and Gridless (`view.snap.set`), the grid and the units. It is kept in
//! `.reda/view.json` per Document, never in the Document. Also `view.jump_to`.

use reda_ui::working_state::FilterKind;
use std::path::{Path, PathBuf};

use reda_testkit::TempDir;
use reda_ui::bars::{self, Request};
use reda_ui::canvas::Camera;
use reda_ui::engine::Engine;
use reda_ui::engine::command::{ErrorCode, UserDirs};
use reda_ui::engine::model::Uuid;
use reda_ui::engine::model::geom::{Nm, Point};
use reda_ui::engine::preferences::Units;
use reda_ui::shell::Shell;
use serde_json::{Value, json};

fn engine_in(home: &Path) -> Engine {
    Engine::builder()
        .user_dirs(Some(UserDirs::under(home)))
        .starter_library(None)
        .headless(false)
        .build()
}

/// Creates the Project `name` in `work` and returns its `.reda` file.
fn make_project(home: &Path, work: &Path, name: &str) -> PathBuf {
    let mut shell = Shell::new(engine_in(home));
    shell.new_project(work, name).unwrap();
    work.join(name).join(format!("{name}.reda"))
}

fn open(home: &Path, project: &Path) -> Shell {
    let mut shell = Shell::new(engine_in(home));
    shell.open_project(project).unwrap();
    shell
}

fn document(shell: &Shell, index: usize) -> Uuid {
    let project = shell.engine().project().unwrap();
    project.documents().nth(index).unwrap().uuid
}

/// The kinds the Filter lets through.
fn on(out: &Value) -> Vec<String> {
    let kinds = out["kinds"].as_array().unwrap();
    kinds
        .iter()
        .filter(|k| k["on"] == json!(true))
        .map(|k| k["kind"].as_str().unwrap().to_owned())
        .collect()
}

fn kinds(out: &Value) -> Vec<String> {
    let kinds = out["kinds"].as_array().unwrap();
    kinds
        .iter()
        .map(|k| k["kind"].as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn the_filter_has_a_toggle_per_kind_of_the_document_type_plus_all_and_none() {
    let home = TempDir::new("filter-home");
    let work = TempDir::new("filter-work");
    let project = make_project(home.path(), work.path(), "Filter");
    let mut shell = open(home.path(), &project);
    let sheet = document(&shell, 0);
    let board = document(&shell, 1);

    let out = shell
        .run("selection.filter.set", json!({"document": sheet}))
        .unwrap();
    let sheet_kinds = kinds(&out);
    assert_eq!(on(&out), sheet_kinds, "everything starts on");
    assert!(sheet_kinds.contains(&"wire".to_owned()));
    assert!(!sheet_kinds.contains(&"track".to_owned()));

    let out = shell
        .run(
            "selection.filter.set",
            json!({"document": sheet, "kinds": {"wire": false, "net_label": false}}),
        )
        .unwrap();
    let mut expected = sheet_kinds.clone();
    expected.retain(|k| k != "wire" && k != "net_label");
    assert_eq!(on(&out), expected);
    assert!(!shell.selection_filter(sheet).allows(FilterKind::Wire));
    assert!(shell.selection_filter(sheet).allows(FilterKind::Unit));

    // None, then one back.
    let out = shell
        .run(
            "selection.filter.set",
            json!({"document": sheet, "all": false, "kinds": {"unit": true}}),
        )
        .unwrap();
    assert_eq!(on(&out), vec!["unit".to_owned()]);
    // All.
    let out = shell
        .run(
            "selection.filter.set",
            json!({"document": sheet, "all": true}),
        )
        .unwrap();
    assert_eq!(on(&out), sheet_kinds);

    // A board has its own list, Pads included; filtering a sheet leaves the board alone.
    let out = shell
        .run("selection.filter.set", json!({"document": board}))
        .unwrap();
    assert!(kinds(&out).contains(&"pad".to_owned()));
    assert!(kinds(&out).contains(&"track".to_owned()));
}

#[test]
fn the_filter_refuses_kinds_the_document_type_lacks() {
    let home = TempDir::new("filter-home");
    let work = TempDir::new("filter-work");
    let project = make_project(home.path(), work.path(), "Kinds");
    let mut shell = open(home.path(), &project);
    let sheet = document(&shell, 0);
    let err = shell
        .run(
            "selection.filter.set",
            json!({"document": sheet, "kinds": {"track": false}}),
        )
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidParams);
    assert_eq!(err.details, Some(json!({"pointer": "/kinds/track"})));
    let err = shell
        .run(
            "selection.filter.set",
            json!({"document": Uuid::from_u128(5)}),
        )
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::NotFound);
}

#[test]
fn snap_mode_and_gridless_per_document_type() {
    let home = TempDir::new("snap-home");
    let work = TempDir::new("snap-work");
    let project = make_project(home.path(), work.path(), "Snap");
    let mut shell = open(home.path(), &project);
    let sheet = document(&shell, 0);
    let board = document(&shell, 1);

    let out = shell
        .run("view.snap.set", json!({"document": sheet}))
        .unwrap();
    assert_eq!(
        out,
        json!({"mode": "on", "gridless": false, "hotspots_off": []})
    );
    let out = shell
        .run("view.snap.set", json!({"document": board}))
        .unwrap();
    assert_eq!(
        out,
        json!({"mode": "active_layer", "gridless": false, "hotspots_off": []})
    );

    let out = shell
        .run(
            "view.snap.set",
            json!({"document": board, "mode": "all_layers", "gridless": true}),
        )
        .unwrap();
    assert_eq!(
        out,
        json!({"mode": "all_layers", "gridless": true, "hotspots_off": []})
    );
    // The sheet is untouched.
    let out = shell
        .run("view.snap.set", json!({"document": sheet}))
        .unwrap();
    assert_eq!(
        out,
        json!({"mode": "on", "gridless": false, "hotspots_off": []})
    );

    let err = shell
        .run(
            "view.snap.set",
            json!({"document": sheet, "mode": "active_layer"}),
        )
        .unwrap_err();
    assert_eq!(err.details, Some(json!({"pointer": "/mode"})));
}

#[test]
fn the_working_state_is_kept_in_view_json_and_not_in_the_documents() {
    let home = TempDir::new("working-home");
    let work = TempDir::new("working-work");
    let project = make_project(home.path(), work.path(), "Working");
    let folder = project.parent().unwrap().to_path_buf();
    let files_before: Vec<(PathBuf, Vec<u8>)> =
        ["Working.reda", "Working.redasch", "Working.redapcb"]
            .iter()
            .map(|f| (folder.join(f), std::fs::read(folder.join(f)).unwrap()))
            .collect();

    let mut shell = open(home.path(), &project);
    let sheet = document(&shell, 0);
    let board = document(&shell, 1);
    shell
        .run("view.grid.set", json!({"document": sheet, "grid": "25mil"}))
        .unwrap();
    shell
        .run("view.grid.set", json!({"document": board, "grid": "10mil"}))
        .unwrap();
    shell
        .run(
            "view.snap.set",
            json!({"document": board, "mode": "off", "gridless": true}),
        )
        .unwrap();
    shell
        .run(
            "selection.filter.set",
            json!({"document": board, "kinds": {"zone": false}}),
        )
        .unwrap();
    shell
        .run(
            "selection.filter.set",
            json!({"document": sheet, "kinds": {"wire": false}}),
        )
        .unwrap();
    shell.save_view();
    assert!(!shell.engine().is_dirty());
    for (file, bytes) in &files_before {
        assert_eq!(&std::fs::read(file).unwrap(), bytes, "{}", file.display());
    }

    let saved: Value =
        serde_json::from_slice(&std::fs::read(folder.join(".reda/view.json")).unwrap()).unwrap();
    let board_view = &saved["documents"][board.to_string()];
    assert_eq!(board_view["grid"], json!(254_000));
    assert_eq!(board_view["units"], json!("mil"));
    assert_eq!(board_view["snap_mode"], json!("off"));
    assert_eq!(board_view["gridless"], json!(true));
    assert_eq!(board_view["selection_filter_off"], json!(["zone"]));

    let mut again = open(home.path(), &project);
    assert_eq!(again.grid(sheet), Nm(25 * 25_400));
    assert_eq!(again.grid(board), Nm(10 * 25_400));
    assert_eq!(again.board_view().units, Units::Mil);
    let out = again
        .run("view.snap.set", json!({"document": board}))
        .unwrap();
    assert_eq!(
        out,
        json!({"mode": "off", "gridless": true, "hotspots_off": []})
    );
    assert!(!again.selection_filter(board).allows(FilterKind::Zone));
    assert!(again.selection_filter(board).allows(FilterKind::Track));
    assert!(!again.selection_filter(sheet).allows(FilterKind::Wire));
}

#[test]
fn jump_to_moves_the_canvas_to_a_point_or_by_a_delta() {
    let home = TempDir::new("jump-home");
    let work = TempDir::new("jump-work");
    let project = make_project(home.path(), work.path(), "Jump");
    let mut shell = open(home.path(), &project);
    let sheet = document(&shell, 0);
    bars::take_requests(shell.engine());

    let out = shell
        .run(
            "view.jump_to",
            json!({"document": sheet, "x": "10mm", "y": "-5mm"}),
        )
        .unwrap();
    let target = Point::new(Nm::from_mm(10), Nm::from_mm(-5));
    assert_eq!(out["at"], json!({"x": 10_000_000, "y": -5_000_000}));
    let requests = bars::take_requests(shell.engine());
    assert_eq!(requests.len(), 1);
    let Request::JumpTo(document, jump) = requests[0].clone() else {
        panic!("{requests:?}");
    };
    assert_eq!(document, sheet);

    shell.document_tab_mut(sheet).unwrap().camera = Some(Camera {
        center: (1.0, 2.0),
        nm_per_point: 5_000.0,
    });
    shell.jump(sheet, jump);
    let camera = shell.document_tab(sheet).unwrap().camera.unwrap();
    assert_eq!(camera.center, (target.x.0 as f64, target.y.0 as f64));
    assert_eq!(camera.nm_per_point, 5_000.0, "the zoom stays");

    shell
        .run(
            "view.jump_to",
            json!({"document": sheet, "x": "1mm", "y": "1mm", "delta": true}),
        )
        .unwrap();
    let Request::JumpTo(_, jump) = bars::take_requests(shell.engine())[0].clone() else {
        panic!();
    };
    shell.jump(sheet, jump);
    let camera = shell.document_tab(sheet).unwrap().camera.unwrap();
    assert_eq!(camera.center, (11_000_000.0, -4_000_000.0));
}

#[test]
fn jump_to_measures_from_the_output_origin_on_a_board() {
    let home = TempDir::new("jump-home");
    let work = TempDir::new("jump-work");
    let project = make_project(home.path(), work.path(), "Origin");
    let mut shell = open(home.path(), &project);
    let sheet = document(&shell, 0);
    let board = document(&shell, 1);

    // No Output Origin placed: design zero.
    let out = shell
        .run(
            "view.jump_to",
            json!({"document": board, "x": "1mm", "y": 0, "origin": "output"}),
        )
        .unwrap();
    assert_eq!(out["at"], json!({"x": 1_000_000, "y": 0}));
    shell
        .run(
            "pcb.output_origin.set",
            json!({"at": {"x": "20mm", "y": "30mm"}}),
        )
        .unwrap();
    let out = shell
        .run(
            "view.jump_to",
            json!({"document": board, "x": "1mm", "y": 0, "origin": "output"}),
        )
        .unwrap();
    assert_eq!(out["at"], json!({"x": 21_000_000, "y": 30_000_000}));

    let err = shell
        .run(
            "view.jump_to",
            json!({"document": sheet, "x": 0, "y": 0, "origin": "output"}),
        )
        .unwrap_err();
    assert_eq!(err.details, Some(json!({"pointer": "/origin"})));
}
