//! Check results as view state: `view.checks.clear`, and the `checks.max_markers` cap on
//! the markers drawn.

use std::path::Path;

use egui_kittest::Harness;
use reda_testkit::TempDir;
use reda_ui::App;
use reda_ui::engine::Engine;
use reda_ui::engine::command::UserDirs;
use reda_ui::engine::model::Uuid;
use reda_ui::messages::{CheckKind, LineKey};
use reda_ui::shell::Shell;
use serde_json::{Value, json};

fn at(x: i64, y: i64) -> Value {
    json!({"x": format!("{x}mil"), "y": format!("{y}mil")})
}

/// A Project whose first sheet has three lone Net Labels, so ERC finds three Violations
/// there. Returns the shell and that sheet.
fn three_labels(home: &Path, work: &Path) -> (Shell, Uuid) {
    let engine = Engine::builder()
        .user_dirs(Some(UserDirs::under(home)))
        .starter_library(None)
        .headless(false)
        .build();
    let mut shell = Shell::new(engine);
    shell.new_project(work, "Demo").unwrap();
    let sheet = shell.document_tabs()[0].document;
    for (i, name) in ["A", "B", "C"].iter().enumerate() {
        shell
            .run(
                "schematic.net_label.add",
                json!({"document": sheet, "name": name, "at": at(500, 500 + 500 * i as i64)}),
            )
            .unwrap();
    }
    (shell, sheet)
}

fn violation_lines(shell: &Shell) -> usize {
    shell
        .message_groups()
        .iter()
        .flat_map(|g| &g.lines)
        .filter(|l| matches!(l.key, LineKey::Violation(..)))
        .count()
}

#[test]
fn clearing_checks_empties_the_list_and_markers_and_the_next_run_lists_them_again() {
    let home = TempDir::new("checks-clear-home");
    let work = TempDir::new("checks-clear-work");
    let (mut shell, sheet) = three_labels(home.path(), work.path());
    shell.run_check(CheckKind::Erc).unwrap();
    let listed = violation_lines(&shell);
    assert!(listed >= 3, "{listed}");
    assert!(!shell.markers(sheet).is_empty());

    let dirty = shell.engine().dirty_scopes();
    shell.run("view.checks.clear", json!({})).unwrap();
    assert_eq!(violation_lines(&shell), 0);
    assert!(shell.messages().run(CheckKind::Erc).is_none());
    assert!(shell.markers(sheet).is_empty());
    // View state: it edits nothing.
    assert_eq!(shell.engine().dirty_scopes(), dirty);

    shell.run_check(CheckKind::Erc).unwrap();
    assert_eq!(violation_lines(&shell), listed);
    assert!(!shell.markers(sheet).is_empty());
}

#[test]
fn max_markers_defaults_to_500() {
    let home = TempDir::new("checks-cap-default");
    let work = TempDir::new("checks-cap-default-work");
    let (mut shell, _) = three_labels(home.path(), work.path());
    let pref = shell
        .run("app.preferences.get", json!({"key": "checks.max_markers"}))
        .unwrap();
    assert_eq!(pref["value"], 500);
    assert_eq!(pref["default"], 500);
    assert_eq!(
        reda_ui::engine::preferences::pages::page_of("checks.max_markers"),
        "checks"
    );
}

#[test]
fn markers_beyond_max_markers_are_not_drawn_but_counted_and_still_listed() {
    let home = TempDir::new("checks-cap-home");
    let work = TempDir::new("checks-cap-work");
    let (mut shell, sheet) = three_labels(home.path(), work.path());
    shell.run_check(CheckKind::Erc).unwrap();
    let all = shell.markers(sheet).len();
    assert!(all >= 3);
    assert_eq!(shell.markers_not_shown(sheet), 0);

    shell
        .run(
            "app.preferences.set",
            json!({"key": "checks.max_markers", "value": 2}),
        )
        .unwrap();
    assert_eq!(shell.markers(sheet).len(), 2);
    assert_eq!(shell.markers_not_shown(sheet), all - 2);
    // The list is never capped.
    assert_eq!(violation_lines(&shell), all);

    // The Violation last gone to always keeps its marker.
    let last = shell
        .message_groups()
        .iter()
        .flat_map(|g| g.lines.clone())
        .rfind(|l| matches!(l.key, LineKey::Violation(..)))
        .unwrap()
        .key;
    shell.go_to(last);
    assert_eq!(shell.markers(sheet).len(), 2);
    assert!(shell.markers(sheet).iter().any(|m| m.current));
}

/// Every text the last frame painted.
fn painted_texts(h: &Harness<'_, App>) -> Vec<String> {
    let mut out = Vec::new();
    for clipped in &h.output().shapes {
        let mut shapes = vec![&clipped.shape];
        while let Some(shape) = shapes.pop() {
            match shape {
                egui::Shape::Vec(inner) => shapes.extend(inner),
                egui::Shape::Text(text) => out.push(text.galley.text().to_owned()),
                _ => {}
            }
        }
    }
    out
}

#[test]
fn the_canvas_says_how_many_markers_are_not_shown() {
    let home = TempDir::new("checks-cap-canvas");
    let work = TempDir::new("checks-cap-canvas-work");
    let (mut shell, sheet) = three_labels(home.path(), work.path());
    shell
        .run(
            "app.preferences.set",
            json!({"key": "checks.max_markers", "value": 1}),
        )
        .unwrap();
    shell.run_check(CheckKind::Erc).unwrap();
    let hidden = shell.markers_not_shown(sheet);
    assert!(hidden >= 2);
    let mut h = Harness::builder()
        .with_size([1200.0, 800.0])
        .build_ui_state(|ui, app: &mut App| app.ui(ui), App::new(shell));
    h.run();
    let label = format!("{hidden} not shown");
    assert!(painted_texts(&h).contains(&label), "no {label:?} drawn");
}
