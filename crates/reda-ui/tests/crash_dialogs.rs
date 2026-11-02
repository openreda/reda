//! The recovery prompt and the Crash Report offer in the window, driven through AccessKit
//! with `egui_kittest`.

use std::path::Path;
use std::time::{Duration, Instant};

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use reda_testkit::TempDir;
use reda_ui::App;
use reda_ui::crash::{self, Report};
use reda_ui::engine::Engine;
use reda_ui::engine::command::UserDirs;
use reda_ui::shell::Shell;
use serde_json::json;

fn engine_in(home: &Path) -> Engine {
    Engine::builder()
        .user_dirs(Some(UserDirs::under(home)))
        .starter_library(None)
        .headless(false)
        .build()
}

fn harness(shell: Shell) -> Harness<'static, App> {
    let mut h = Harness::builder()
        .with_size([1100.0, 700.0])
        .build_ui_state(|ui, app: &mut App| app.ui(ui), App::new(shell));
    h.run();
    h
}

#[test]
fn the_recovery_prompt_restores_all_and_closes() {
    let home = TempDir::new("dialog-rec-home");
    let work = TempDir::new("dialog-rec-work");
    let mut shell = Shell::new(engine_in(home.path()));
    shell.new_project(work.path(), "Demo").unwrap();
    let t0 = Instant::now();
    shell.tick_recovery(t0);
    shell
        .run("project.properties.set", json!({"title": "Amp"}))
        .unwrap();
    shell.tick_recovery(t0 + Duration::from_secs(1));
    shell.tick_recovery(t0 + Duration::from_secs(10));
    shell.flush_recovery();
    drop(shell);

    let mut shell = Shell::new(engine_in(home.path()));
    shell.open_project(&work.path().join("Demo")).unwrap();
    let mut h = harness(shell);
    h.get_by_label("Recover Unsaved Work");
    assert!(h.query_by_label("Demo.reda").is_some());
    h.get_by_label("Restore All").click();
    h.run();
    assert!(h.query_by_label("Recover Unsaved Work").is_none());
    let engine = h.state().shell().engine();
    assert_eq!(engine.project().unwrap().props().properties.title, "Amp");
    assert!(engine.is_dirty());
}

#[test]
fn a_crash_report_is_offered_once_on_the_next_start() {
    let home = TempDir::new("dialog-crash-home");
    let dirs = UserDirs::under(home.path());
    let report = Report {
        version: "0.1.0".into(),
        os: crash::os(),
        message: "boom".into(),
        location: "a.rs:1:1".into(),
        backtrace: String::new(),
        journal: vec![],
    };
    crash::write(
        &crash::reports_dir(&dirs),
        &report,
        std::time::SystemTime::now(),
    )
    .unwrap();

    let mut h = harness(Shell::new(engine_in(home.path())));
    h.get_by_label("REDA Closed Unexpectedly");
    h.get_by_label("Open Folder");
    h.get_by_label("Report on GitHub…");
    h.get_by_label("Close").click();
    h.run();
    assert!(h.query_by_label("REDA Closed Unexpectedly").is_none());

    let h = harness(Shell::new(engine_in(home.path())));
    assert!(
        h.query_by_label("REDA Closed Unexpectedly").is_none(),
        "offered once"
    );
}
