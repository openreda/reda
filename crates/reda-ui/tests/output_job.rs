//! The Output Job dialog and job runs, without a window: the dialog edits one job and
//! applies as one `batch`; a job whose PCB Document is gone is broken until re-pointed; a
//! run goes to another thread with progress and Cancel, is journalled, and its blockers and
//! warnings land in the Messages panel.

use reda_testkit::TempDir;
use reda_ui::engine::Engine;
use reda_ui::engine::command::{CancelToken, UserDirs};
use reda_ui::engine::model::{DocumentKind, Scope, Severity, Uuid};
use reda_ui::job_run::{JobRunner, RunOutcome};
use reda_ui::settings::output_job::{self, JobPage};
use reda_ui::shell::Shell;
use serde_json::json;

fn shell(home: &TempDir) -> Shell {
    let engine = Engine::builder()
        .user_dirs(Some(UserDirs::under(home.path())))
        .starter_library(None)
        .headless(false)
        .build();
    Shell::new(engine)
}

/// A Project "Board" with a 30 × 20 mm Board Outline, so its job runs cleanly.
fn shell_with_board(home: &TempDir, work: &TempDir) -> Shell {
    let mut shell = shell(home);
    shell
        .run(
            "project.new",
            json!({"path": work.path(), "name": "Board",
                   "board": {"width": "30mm", "height": "20mm"}}),
        )
        .unwrap();
    shell
}

fn job(shell: &Shell) -> Uuid {
    shell
        .engine()
        .project()
        .unwrap()
        .output_jobs()
        .next()
        .unwrap()
        .uuid
}

fn pcb(shell: &Shell) -> Uuid {
    shell
        .engine()
        .project()
        .unwrap()
        .documents()
        .find(|d| d.kind == DocumentKind::Pcb)
        .unwrap()
        .uuid
}

fn journal_ids(shell: &Shell) -> Vec<String> {
    let journal = shell.engine().journal();
    journal.iter().map(|e| e.id.clone()).collect()
}

#[test]
fn the_dialog_edits_one_job_and_applies_as_one_batch() {
    let (home, work) = (TempDir::new("job-home"), TempDir::new("job-work"));
    let mut shell = shell_with_board(&home, &work);
    let uuid = job(&shell);
    let mut d = output_job::dialog(shell.engine().project().unwrap(), uuid).unwrap();
    assert_eq!(d.page_titles(), ["Job"]);
    assert!(!d.has_changes());

    let page = d.page_mut::<JobPage>().unwrap();
    page.name.set("Fab".into());
    page.bom.set(false);
    page.destination.set("out/fab".into());
    assert!(d.can_apply());
    let before = journal_ids(&shell).len();
    d.apply(&mut shell).unwrap();
    assert_eq!(journal_ids(&shell)[before..], ["batch"]);
    let job = shell.engine().project().unwrap().output_job(uuid).unwrap();
    assert_eq!(job.name, "Fab");
    assert!(!job.outputs.bom && job.outputs.gerber);
    assert_eq!(job.destination.as_deref(), Some("out/fab"));
    assert!(!d.has_changes());

    shell.undo(false, Some(Scope::Project)).unwrap();
    let job = shell.engine().project().unwrap().output_job(uuid).unwrap();
    assert_ne!(job.name, "Fab");
}

#[test]
fn a_taken_or_blank_name_and_an_escaping_destination_are_flagged() {
    let (home, work) = (TempDir::new("job-home"), TempDir::new("job-work"));
    let mut shell = shell_with_board(&home, &work);
    let uuid = job(&shell);
    shell
        .run("output.job.duplicate", json!({"job": uuid}))
        .unwrap();
    let other = shell
        .engine()
        .project()
        .unwrap()
        .output_jobs()
        .find(|j| j.uuid != uuid)
        .unwrap()
        .name
        .clone();
    let mut d = output_job::dialog(shell.engine().project().unwrap(), uuid).unwrap();
    let page = d.page_mut::<JobPage>().unwrap();
    page.name.set(other.to_uppercase());
    page.destination.set("../elsewhere".into());
    let fields: Vec<String> = d.problems().into_iter().map(|(_, p)| p.field).collect();
    assert_eq!(fields, ["name", "destination"]);
    assert!(!d.can_ok());

    let page = d.page_mut::<JobPage>().unwrap();
    page.name.set("  ".into());
    page.destination.set(String::new());
    let fields: Vec<String> = d.problems().into_iter().map(|(_, p)| p.field).collect();
    assert_eq!(fields, ["name"]);
}

#[test]
fn a_job_whose_pcb_is_gone_is_broken_until_re_pointed() {
    let (home, work) = (TempDir::new("job-home"), TempDir::new("job-work"));
    let mut shell = shell_with_board(&home, &work);
    let uuid = job(&shell);
    let old = pcb(&shell);
    shell
        .run("document.delete", json!({"document": old}))
        .unwrap();
    assert!(output_job::broken(shell.engine().project().unwrap(), uuid));

    let mut d = output_job::dialog(shell.engine().project().unwrap(), uuid).unwrap();
    assert!(d.page::<JobPage>().unwrap().broken());
    let fields: Vec<String> = d.problems().into_iter().map(|(_, p)| p.field).collect();
    assert_eq!(fields, ["pcb"]);
    assert!(
        JobRunner::start(&shell, uuid).is_err(),
        "a broken job does not run"
    );

    shell.run("document.add", json!({"kind": "pcb"})).unwrap();
    let new = pcb(&shell);
    d.sync(shell.engine().project().unwrap());
    let page = d.page_mut::<JobPage>().unwrap();
    assert_eq!(page.pcbs(), [new]);
    page.pcb.set(new);
    assert!(d.problems().is_empty());
    d.apply(&mut shell).unwrap();
    assert!(!output_job::broken(shell.engine().project().unwrap(), uuid));
}

#[test]
fn a_run_writes_its_files_and_zip_and_is_journalled() {
    let (home, work) = (TempDir::new("job-home"), TempDir::new("job-work"));
    let mut shell = shell_with_board(&home, &work);
    let uuid = job(&shell);
    let runner = JobRunner::start(&shell, uuid).unwrap();
    let report = runner.wait(&mut shell);
    let RunOutcome::Written { zip, files, .. } = &report.outcome else {
        panic!("{report:?}");
    };
    assert!(zip.exists(), "{zip:?}");
    assert!(files.iter().any(|f| f.name == "Board.GTL"), "{files:?}");
    assert_eq!(journal_ids(&shell).last().unwrap(), "output.job.run");
    assert!(report.blockers.is_empty());
}

#[test]
fn blockers_refuse_the_run_and_land_in_the_messages_panel_under_the_pcb() {
    let (home, work) = (TempDir::new("job-home"), TempDir::new("job-work"));
    let mut shell = shell(&home);
    // No Board Outline: the run is blocked.
    shell.new_project(work.path(), "Board").unwrap();
    let uuid = job(&shell);
    let report = JobRunner::start(&shell, uuid).unwrap().wait(&mut shell);
    assert_eq!(report.outcome, RunOutcome::Blocked);
    assert_eq!(report.blockers.len(), 1);
    assert!(report.blockers[0].contains("Board Outline"), "{report:?}");

    let groups = shell.message_groups();
    let group = groups
        .iter()
        .find(|g| g.document == Some(pcb(&shell)))
        .expect("the PCB's group");
    let line = &group.lines[0];
    assert_eq!(line.severity, Severity::Error);
    assert!(line.text.contains("Board Outline"), "{}", line.text);

    // A clean run clears them.
    shell
        .run(
            "pcb.board_outline.add_rect",
            json!({"width": "30mm", "height": "20mm"}),
        )
        .unwrap();
    let report = JobRunner::start(&shell, uuid).unwrap().wait(&mut shell);
    assert!(
        matches!(report.outcome, RunOutcome::Written { .. }),
        "{report:?}"
    );
    let errors = shell
        .message_groups()
        .iter()
        .flat_map(|g| g.lines.clone())
        .filter(|l| l.severity == Severity::Error)
        .count();
    assert_eq!(errors, 0);
}

#[test]
fn a_cancelled_run_writes_nothing() {
    let (home, work) = (TempDir::new("job-home"), TempDir::new("job-work"));
    let mut shell = shell_with_board(&home, &work);
    let uuid = job(&shell);
    // Cancelled before the thread gets going, so the run stops at its first check.
    let token = CancelToken::new();
    token.cancel();
    let runner = JobRunner::start_with(&shell, uuid, token, None).unwrap();
    assert!(runner.cancelling());
    let report = runner.wait(&mut shell);
    assert_eq!(report.outcome, RunOutcome::Cancelled);
    assert!(!work.path().join("Board/outputs").exists());
}
