//! The Output Job dialog and job runs in the window, driven through AccessKit with
//! `egui_kittest`: Edit… and a double-click open the dialog, Run from the Projects panel
//! or the dialog shows progress and then the run report, and a broken job is flagged.

use std::path::Path;
use std::time::{Duration, Instant};

use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use reda_testkit::TempDir;
use reda_ui::App;
use reda_ui::engine::Engine;
use reda_ui::engine::command::UserDirs;
use reda_ui::engine::model::DocumentKind;
use reda_ui::shell::Shell;
use serde_json::json;

/// The job `project.new` makes, named after the Generic Fab Preset.
const JOB: &str = "Generic";

fn harness(home: &Path, work: &Path, board: bool) -> Harness<'static, App> {
    let engine = Engine::builder()
        .user_dirs(Some(UserDirs::under(home)))
        .starter_library(None)
        .headless(false)
        .build();
    let mut shell = Shell::new(engine);
    let mut params = json!({"path": work, "name": "Jobs"});
    if board {
        params["board"] = json!({"width": "30mm", "height": "20mm"});
    }
    shell.run("project.new", params).unwrap();
    let mut h = Harness::builder()
        .with_size([1100.0, 700.0])
        .build_ui_state(|ui, app: &mut App| app.ui(ui), App::new(shell));
    h.ctx
        .options_mut(|o| o.input_options.max_double_click_delay = 10.0);
    h.run();
    h
}

fn job_menu(h: &mut Harness<'static, App>, item: &str) {
    h.get_by_role_and_label(Role::TreeItem, JOB)
        .click_secondary();
    h.run();
    h.get_by_label(item).click();
    // A run keeps the window repainting until it ends, so step rather than settle.
    h.step();
}

fn disabled(h: &Harness<'static, App>, label: &str) -> bool {
    h.get_by_role_and_label(Role::Button, label)
        .accesskit_node()
        .is_disabled()
}

/// Steps frames until `label` shows; the run is on another thread.
fn wait_for(h: &mut Harness<'static, App>, label: &str) {
    let start = Instant::now();
    while h.query_by_label_contains(label).is_none() {
        assert!(
            start.elapsed() < Duration::from_secs(20),
            "{label:?} never showed"
        );
        std::thread::sleep(Duration::from_millis(10));
        h.step();
    }
}

fn last_journal_id(h: &Harness<'static, App>) -> String {
    let journal = h.state().shell().engine().journal();
    journal.last().map(|e| e.id.clone()).unwrap_or_default()
}

#[test]
fn edit_and_a_double_click_open_the_output_job_dialog() {
    let (home, work) = (TempDir::new("jd-home"), TempDir::new("jd-work"));
    let mut h = harness(home.path(), work.path(), true);
    let row = h.get_by_role_and_label(Role::TreeItem, JOB);
    row.click();
    row.click();
    h.run();
    h.get_by_label("Output Job: Generic");
    h.get_by_label("Cancel").click();
    h.run();

    job_menu(&mut h, "Edit…");
    h.run();
    h.get_by_label("Output Job: Generic");
    for field in ["Name", "Destination folder"] {
        h.get_by_role_and_label(Role::TextInput, field);
    }
    h.get_by_label("CSV BOM");
    assert!(!disabled(&h, "OK") && disabled(&h, "Apply"));
    h.get_by_label("Cancel").click();
    h.run();
    assert!(h.query_by_label("Output Job: Generic").is_none());
}

#[test]
fn ok_applies_the_dialog_as_one_batch() {
    let (home, work) = (TempDir::new("jd-home"), TempDir::new("jd-work"));
    let mut h = harness(home.path(), work.path(), true);
    job_menu(&mut h, "Edit…");
    h.run();
    h.get_by_label("CSV BOM").click();
    h.run();
    assert!(!disabled(&h, "Apply"));
    h.get_by_label("OK").click();
    h.run();
    assert!(h.query_by_label("Output Job: Generic").is_none());
    assert_eq!(last_journal_id(&h), "batch");
    let project = h.state().shell().engine().project().unwrap();
    assert!(!project.output_jobs().next().unwrap().outputs.bom);
}

#[test]
fn the_fab_notes_drill_report_and_job_file_each_have_a_toggle() {
    let (home, work) = (TempDir::new("jd-home"), TempDir::new("jd-work"));
    let mut h = harness(home.path(), work.path(), true);
    job_menu(&mut h, "Edit…");
    h.run();
    for label in ["Fab Notes", "Drill report", "Gerber job file"] {
        h.get_by_label(label).click();
        h.run();
    }
    h.get_by_label("OK").click();
    h.run();
    let project = h.state().shell().engine().project().unwrap();
    let outputs = project.output_jobs().next().unwrap().outputs;
    assert!(!outputs.fab_notes && !outputs.drill_report && !outputs.gerber_job);
    assert!(outputs.gerber && outputs.drill && outputs.bom && outputs.cpl);
}

#[test]
fn run_from_the_projects_panel_shows_the_report_and_offers_the_zip() {
    let (home, work) = (TempDir::new("jd-home"), TempDir::new("jd-work"));
    let mut h = harness(home.path(), work.path(), true);
    job_menu(&mut h, "Run");
    wait_for(&mut h, "Generic finished");
    h.get_by_label_contains("Jobs.GTL");
    h.get_by_label("Show Zip");
    assert_eq!(last_journal_id(&h), "output.job.run");
    h.get_by_label("Close").click();
    h.run();
    assert!(h.query_by_label_contains("Generic finished").is_none());
}

#[test]
fn run_from_the_dialog_applies_first_and_lists_blockers() {
    let (home, work) = (TempDir::new("jd-home"), TempDir::new("jd-work"));
    // No Board Outline: the run is blocked.
    let mut h = harness(home.path(), work.path(), false);
    job_menu(&mut h, "Edit…");
    h.run();
    h.get_by_label("CSV CPL (pick and place)").click();
    h.run();
    h.get_by_role_and_label(Role::Button, "Run").click();
    h.step();
    wait_for(&mut h, "Generic did not run");
    h.get_by_label("Blocking problems");
    assert!(
        h.get_all_by_label_contains("Board Outline is missing")
            .count()
            >= 1
    );
    let journal: Vec<String> = h
        .state()
        .shell()
        .engine()
        .journal()
        .iter()
        .map(|e| e.id.clone())
        .collect();
    assert_eq!(journal[journal.len() - 2..], ["batch", "output.job.run"]);
    let project = h.state().shell().engine().project().unwrap();
    assert!(!project.output_jobs().next().unwrap().outputs.cpl);
    let messages = h.state().shell().message_groups();
    assert!(
        messages
            .iter()
            .flat_map(|g| &g.lines)
            .any(|l| l.text.contains("Board Outline")),
        "{messages:?}"
    );
}

#[test]
fn a_job_whose_pcb_is_gone_is_flagged_broken_and_does_not_run() {
    let (home, work) = (TempDir::new("jd-home"), TempDir::new("jd-work"));
    let mut h = harness(home.path(), work.path(), true);
    let pcb = {
        let project = h.state().shell().engine().project().unwrap();
        let pcb = project.documents().find(|d| d.kind == DocumentKind::Pcb);
        pcb.unwrap().uuid
    };
    h.state_mut()
        .shell_mut()
        .run("document.delete", json!({"document": pcb}))
        .unwrap();
    h.run();
    h.get_by_role_and_label(Role::TreeItem, "Generic (broken)")
        .click_secondary();
    h.run();
    assert!(disabled(&h, "Run"));
    h.get_by_label("Edit…").click();
    h.run();
    h.get_by_label("This job's PCB Document is gone. Pick another to repair it.");
    assert!(disabled(&h, "OK"));
}
