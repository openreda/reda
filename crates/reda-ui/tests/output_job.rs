//! The Output Job dialog and job runs, without a window: the dialog edits one job and
//! applies as one `batch`; a job whose PCB Document is gone is broken until re-pointed; a
//! run goes to another thread with progress and Cancel, is journalled, and its blockers and
//! warnings land in the Messages panel.

use reda_testkit::TempDir;
use reda_ui::engine::Engine;
use reda_ui::engine::command::{CancelToken, UserDirs};
use reda_ui::engine::model::{DocumentKind, NotFittedPaste, PdfColour, Scope, Severity, Uuid};
use reda_ui::job_run::{JobRunner, JobStatus, RunOutcome};
use std::cell::RefMut;

use reda_ui::settings::SettingsDialog;
use reda_ui::settings::output_job::{self, FileCheck, JobDraft, JobPage, LastRun};
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

/// The job's draft, shared by the window's pages.
fn draft(d: &SettingsDialog) -> RefMut<'_, JobDraft> {
    d.page::<JobPage>().unwrap().draft()
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
    assert_eq!(
        d.page_titles(),
        [
            "General", "Outputs", "Gerber", "Drill", "BOM", "CPL", "Last run"
        ]
    );
    assert!(!d.has_changes());

    {
        let mut page = draft(&d);
        page.name.set("Fab".into());
        page.bom.set(false);
        page.destination.set("out/fab".into());
    }
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
fn the_dialog_toggles_the_schematic_pdf_and_picks_its_colour() {
    let (home, work) = (TempDir::new("job-home"), TempDir::new("job-work"));
    let mut shell = shell_with_board(&home, &work);
    let uuid = job(&shell);
    let mut d = output_job::dialog(shell.engine().project().unwrap(), uuid).unwrap();
    {
        let mut page = draft(&d);
        assert!(
            *page.schematic_pdf.value() && *page.reports.value(),
            "on by default"
        );
        assert_eq!(*page.schematic_pdf_colour.value(), PdfColour::Colour);
        page.schematic_pdf_colour.set(PdfColour::Mono);
        page.reports.set(false);
    }
    d.apply(&mut shell).unwrap();
    let job = shell.engine().project().unwrap().output_job(uuid).unwrap();
    assert_eq!(job.schematic_pdf_colour, PdfColour::Mono);
    assert!(!job.outputs.reports && job.outputs.schematic_pdf);

    draft(&d).schematic_pdf.set(false);
    d.apply(&mut shell).unwrap();
    let job = shell.engine().project().unwrap().output_job(uuid).unwrap();
    assert!(!job.outputs.schematic_pdf);
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
    let d = output_job::dialog(shell.engine().project().unwrap(), uuid).unwrap();
    {
        let mut page = draft(&d);
        page.name.set(other.to_uppercase());
        page.destination.set("../elsewhere".into());
    }
    let fields: Vec<String> = d.problems().into_iter().map(|(_, p)| p.field).collect();
    assert_eq!(fields, ["name", "destination"]);
    assert!(!d.can_ok());

    {
        let mut page = draft(&d);
        page.name.set("  ".into());
        page.destination.set(String::new());
    }
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
    assert_eq!(d.page::<JobPage>().unwrap().pcbs(), [new]);
    draft(&d).pcb.set(new);
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
    assert!(files.iter().any(|f| f.name == "fab/Board.gtl"), "{files:?}");
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

#[test]
fn the_name_template_and_not_fitted_paste_apply_with_the_rest() {
    let (home, work) = (TempDir::new("job-home"), TempDir::new("job-work"));
    let mut shell = shell_with_board(&home, &work);
    let uuid = job(&shell);
    let mut d = output_job::dialog(shell.engine().project().unwrap(), uuid).unwrap();
    {
        let mut page = draft(&d);
        page.name_template.set("${project}-${nope}".into());
        page.not_fitted_paste.set(NotFittedPaste::Remove);
    }
    let fields: Vec<String> = d.problems().into_iter().map(|(_, p)| p.field).collect();
    assert_eq!(fields, ["name_template"]);
    draft(&d)
        .name_template
        .set("${project}-r${revision}".into());
    assert!(d.problems().is_empty());
    d.apply(&mut shell).unwrap();
    let job = shell.engine().project().unwrap().output_job(uuid).unwrap();
    assert_eq!(job.name_template.as_str(), "${project}-r${revision}");
    assert_eq!(job.not_fitted_paste, NotFittedPaste::Remove);
}

#[test]
fn the_previews_follow_the_drafts_before_apply() {
    let (home, work) = (TempDir::new("job-home"), TempDir::new("job-work"));
    let shell = shell_with_board(&home, &work);
    let uuid = job(&shell);
    let d = output_job::dialog(shell.engine().project().unwrap(), uuid).unwrap();
    let mut page = draft(&d);
    let preview = page.preview(shell.engine()).unwrap().clone();
    assert_eq!(preview.stem.as_deref(), Ok("Board-Generic"));
    assert!(preview.zip.contains(&"Board.gtl".to_owned()));
    page.gerber.set(false);
    page.destination.set("out/${preset}".into());
    let preview = page.preview(shell.engine()).unwrap().clone();
    assert!(!preview.zip.contains(&"Board.gtl".to_owned()));
    assert_eq!(preview.destination.as_deref(), Ok("out/Generic"));
    assert_eq!(
        page.destination_folder(shell.engine()).unwrap(),
        work.path().join("Board").join("out").join("Generic")
    );
}

#[test]
fn an_empty_column_is_one_blank_in_every_row() {
    let table = reda_ui::engine::output::Table {
        header: vec!["Ref".into(), "MPN".into(), "Value".into()],
        rows: vec![
            vec!["R1".into(), String::new(), "10k".into()],
            vec!["R2".into(), " ".into(), String::new()],
        ],
    };
    assert_eq!(output_job::empty_columns(&table), [1]);
}

#[test]
fn last_run_reads_the_manifest_and_checks_each_file_on_disk() {
    let (home, work) = (TempDir::new("job-home"), TempDir::new("job-work"));
    let mut shell = shell_with_board(&home, &work);
    let uuid = job(&shell);
    let d = output_job::dialog(shell.engine().project().unwrap(), uuid).unwrap();
    assert_eq!(draft(&d).last_run(shell.engine()), LastRun::NotRun);

    let report = JobRunner::start(&shell, uuid).unwrap().wait(&mut shell);
    let RunOutcome::Written { files, .. } = &report.outcome else {
        panic!("{report:?}");
    };
    let LastRun::Read(manifest) = draft(&d).last_run(shell.engine()) else {
        panic!("the manifest is read");
    };
    let listed: Vec<&str> = manifest.files.iter().map(|f| f.path.as_str()).collect();
    let written: Vec<&str> = files.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(listed, written);
    assert_eq!(manifest.zip, "Board-Generic.zip");
    assert_eq!(manifest.preset, "Generic");
    assert_eq!(manifest.changed(), 0);
    let gtl = manifest
        .files
        .iter()
        .find(|f| f.path == "fab/Board.gtl")
        .unwrap();
    assert_eq!(gtl.kind, "copper_top");
    assert_eq!(gtl.sha256.len(), 64);

    // Another window, as after reopening the Project, reads the same file; a file changed
    // or removed since the run shows.
    let folder = manifest.folder.clone();
    std::fs::write(folder.join("fab").join("Board.gtl"), b"edited").unwrap();
    std::fs::remove_file(folder.join("Board-Generic-BOM.csv")).unwrap();
    let d = output_job::dialog(shell.engine().project().unwrap(), uuid).unwrap();
    let LastRun::Read(manifest) = draft(&d).last_run(shell.engine()) else {
        panic!("the manifest is read");
    };
    let check = |path: &str| {
        manifest
            .files
            .iter()
            .find(|f| f.path == path)
            .unwrap()
            .check
    };
    assert_eq!(check("fab/Board.gtl"), FileCheck::Changed);
    assert_eq!(check("Board-Generic-BOM.csv"), FileCheck::Missing);
    assert_eq!(check("Board-Generic.zip"), FileCheck::Same);
    assert_eq!(manifest.changed(), 2);
}

#[test]
fn run_on_the_window_is_taken_once() {
    let (home, work) = (TempDir::new("job-home"), TempDir::new("job-work"));
    let shell = shell_with_board(&home, &work);
    let uuid = job(&shell);
    let mut d = output_job::dialog(shell.engine().project().unwrap(), uuid).unwrap();
    assert!(d.has_run() && !d.take_run());
    d.request_run();
    assert!(d.take_run());
    assert!(!d.take_run());
    assert_eq!(output_job::job_of(&d), Some(uuid));
}

#[test]
fn run_all_runs_every_job_and_reports_each() {
    let (home, work) = (TempDir::new("job-home"), TempDir::new("job-work"));
    let mut shell = shell_with_board(&home, &work);
    let uuid = job(&shell);
    shell
        .run("output.job.duplicate", json!({"job": uuid}))
        .unwrap();
    let runner = JobRunner::start_all(&shell, CancelToken::new(), None).unwrap();
    assert_eq!(runner.job(), None);
    let report = runner.wait(&mut shell);
    let RunOutcome::All { jobs, stopped } = &report.outcome else {
        panic!("{report:?}");
    };
    assert!(!stopped);
    assert_eq!(jobs.len(), 2);
    for line in jobs {
        let JobStatus::Written { destination } = &line.status else {
            panic!("{line:?}");
        };
        assert!(destination.join("manifest.json").is_file(), "{line:?}");
    }
    assert_eq!(journal_ids(&shell).last().unwrap(), "output.job.run_all");
}

#[test]
fn run_all_stops_at_a_blocked_job_and_tags_its_blockers() {
    let (home, work) = (TempDir::new("job-home"), TempDir::new("job-work"));
    let mut shell = shell(&home);
    // No Board Outline: every job is blocked.
    shell
        .run("project.new", json!({"path": work.path(), "name": "Bare"}))
        .unwrap();
    let uuid = job(&shell);
    shell
        .run("output.job.duplicate", json!({"job": uuid}))
        .unwrap();
    let report = JobRunner::start_all(&shell, CancelToken::new(), None)
        .unwrap()
        .wait(&mut shell);
    let RunOutcome::All { jobs, stopped } = &report.outcome else {
        panic!("{report:?}");
    };
    assert!(stopped);
    assert_eq!(jobs[0].status, JobStatus::Blocked);
    assert_eq!(jobs[1].status, JobStatus::NotRun);
    // The jobs run in Projects panel order; the first one's blockers carry its name.
    let tag = format!("{}: ", jobs[0].name);
    assert!(!report.blockers.is_empty());
    assert!(
        report.blockers.iter().all(|b| b.starts_with(&tag)),
        "{:?}",
        report.blockers
    );
}
