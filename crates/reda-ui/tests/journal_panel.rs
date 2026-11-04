//! The Journal panel: it lists every Command of the session with its source and result or
//! error, and exports the session as JSON lines that `reda run --script` replays.

use std::path::{Path, PathBuf};

use egui_kittest::Harness;
use egui_kittest::kittest::{By, Queryable};
use reda_testkit::TempDir;
use reda_ui::App;
use reda_ui::engine::journal::Source;
use reda_ui::engine::{Engine, journal};
use reda_ui::shell::Shell;
use serde_json::json;

/// Creates a Project named `name` in `folder` with a throwaway engine; returns its file.
fn make_project(folder: &Path, name: &str) -> PathBuf {
    let mut engine = Engine::builder()
        .user_dirs(None)
        .starter_library(None)
        .build();
    engine
        .run("project.new", json!({"path": folder, "name": name}))
        .unwrap();
    folder.join(name).join(format!("{name}.reda"))
}

fn app_engine() -> Engine {
    Engine::builder()
        .source(Source::Gui)
        .user_dirs(None)
        .starter_library(None)
        .headless(false)
        .build()
}

fn harness(shell: Shell) -> Harness<'static, App> {
    Harness::builder()
        .with_size([1100.0, 700.0])
        .build_ui_state(|ui, app: &mut App| app.ui(ui), App::new(shell))
}

#[test]
fn the_panel_lists_each_command_with_its_source_and_result_or_error() {
    let work = TempDir::new("journal-list");
    let file = make_project(work.path(), "Demo");
    let mut shell = Shell::new(app_engine());
    shell.open_project(&file).unwrap();
    shell
        .run("document.rename", json!({"document": "project"}))
        .unwrap_err();
    shell.run("panel.show:journal", json!({})).unwrap();
    let mut h = harness(shell);
    h.run();

    h.get_by_label("project.open");
    h.get_by_label("document.rename");
    h.get_by_label("panel.show:journal");
    assert_eq!(h.query_all_by_label("App").count(), 3, "every row's source");
    assert!(h.query_all_by_label("OK").count() >= 2, "the successes");
    let error = h
        .query_all(By::new().label_contains("invalid_params"))
        .count();
    assert_eq!(error, 1, "the failed rename shows its error code");
}

#[test]
fn an_exported_journal_replays_on_a_fresh_headless_engine() {
    let work = TempDir::new("journal-export");
    let file = make_project(work.path(), "Demo");
    let mut shell = Shell::new(app_engine());
    shell.open_project(&file).unwrap();
    let added = shell
        .run("document.add", json!({"kind": "schematic"}))
        .unwrap();
    let sheet = added["result"]["uuid"].as_str().unwrap().to_owned();
    shell
        .run(
            "document.rename",
            json!({"document": sheet, "title": "Power"}),
        )
        .unwrap();
    shell
        .run("document.rename", json!({"document": sheet}))
        .unwrap_err();
    shell.run("panel.show:journal", json!({})).unwrap();

    let out = work.path().join("session.jsonl");
    shell.export_journal(&out).unwrap();
    let text = std::fs::read_to_string(&out).unwrap();

    let steps = journal::parse(&text).unwrap();
    let ids: Vec<&str> = steps.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(
        ids,
        [
            "project.open",
            "document.add",
            "document.rename",
            "document.rename"
        ],
        "view Commands need the app, so they stay out of the script"
    );
    let mut fresh = Engine::builder()
        .user_dirs(None)
        .starter_library(None)
        .build();
    journal::replay(&mut fresh, &steps).unwrap();
    let titles: Vec<String> = fresh
        .project()
        .unwrap()
        .documents()
        .map(|d| d.title.clone())
        .collect();
    assert!(titles.contains(&"Power".to_owned()), "{titles:?}");
}
