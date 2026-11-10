//! Running an Output Job from the window. `output.job.run` is detached from the engine
//! ([`Engine::detach`](reda_engine::Engine::detach)) and runs on its own thread, so the
//! window keeps drawing a progress bar with Cancel; when it ends, the outcome is journalled
//! and its blockers and warnings replace the last run's lines in the Messages panel.
//!
//! The model has no drawing; `job_run/draw.rs` draws the progress and report modals.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use reda_engine::Finished;
use reda_engine::command::{CancelToken, CommandError, Progress, Signals};
use reda_engine::model::{Severity, Uuid, Violation};
use serde_json::{Value, json};

use crate::messages::{self, Note};
use crate::settings::output_job;
use crate::shell::Shell;
use crate::tr;

mod draw;

pub use draw::{ReportAction, show_progress, show_report};

/// The Messages panel source of a run's lines.
pub const SOURCE: &str = "output_job";

/// Called whenever a run reports progress or ends, to wake the window.
pub type Waker = Arc<dyn Fn() + Send + Sync>;

/// The last progress a run reported.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Step {
    /// From 0 to 1.
    pub fraction: f32,
    pub message: String,
}

struct Reporter {
    step: Mutex<Step>,
    waker: Option<Waker>,
}

impl Progress for Reporter {
    fn report(&self, progress: f64, total: Option<f64>, message: &str) {
        let fraction = match total {
            Some(total) if total > 0.0 => (progress / total).clamp(0.0, 1.0) as f32,
            _ => 0.0,
        };
        if let Ok(mut step) = self.step.lock() {
            *step = Step {
                fraction,
                message: message.to_owned(),
            };
        }
        if let Some(wake) = &self.waker {
            wake();
        }
    }
}

/// A job, or every job, running on its own thread.
pub struct JobRunner {
    /// The job run; `None` for Run all.
    job: Option<Uuid>,
    name: String,
    /// The PCB Document the Messages panel files the run's lines under.
    pcb: Option<Uuid>,
    cancel: CancelToken,
    reporter: Arc<Reporter>,
    thread: JoinHandle<Finished>,
}

/// One file a run wrote.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WrittenFile {
    /// Its path in the destination, such as `fab/Demo.gtl`.
    pub name: String,
    /// What it holds, such as `copper_top` or `bom`.
    pub function: String,
}

/// How a run ended.
#[derive(Clone, Debug, PartialEq)]
pub enum RunOutcome {
    /// The files and their zip were written.
    Written {
        destination: PathBuf,
        zip: PathBuf,
        files: Vec<WrittenFile>,
    },
    /// Blockers stopped it; nothing was written.
    Blocked,
    /// The user cancelled it; nothing was written.
    Cancelled,
    /// It failed, with the error's message.
    Failed(String),
    /// Run all: how each job went, in Projects panel order, and whether a blocked job
    /// stopped the run.
    All { jobs: Vec<JobLine>, stopped: bool },
}

/// One job of a Run all.
#[derive(Clone, Debug, PartialEq)]
pub struct JobLine {
    pub job: Uuid,
    pub name: String,
    pub status: JobStatus,
}

/// How one job of a Run all went.
#[derive(Clone, Debug, PartialEq)]
pub enum JobStatus {
    /// It wrote its files into `destination`.
    Written { destination: PathBuf },
    /// A blocking problem stopped it.
    Blocked,
    /// It could not run, with why.
    Failed(String),
    /// An earlier job was blocked, so it did not run.
    NotRun,
}

/// The run report: how it ended, and what blocked it or what it warns about.
#[derive(Clone, Debug, PartialEq)]
pub struct RunReport {
    /// The job run; `None` for Run all.
    pub job: Option<Uuid>,
    pub job_name: String,
    pub outcome: RunOutcome,
    pub blockers: Vec<String>,
    pub warnings: Vec<String>,
}

impl JobRunner {
    /// Starts `job` on its own thread.
    pub fn start(shell: &Shell, job: Uuid) -> Result<JobRunner, CommandError> {
        JobRunner::start_with(shell, job, CancelToken::new(), None)
    }

    /// Starts `job` with `cancel` as its token, waking the window through `waker` as it
    /// goes. A broken job (its PCB Document is gone) is refused.
    pub fn start_with(
        shell: &Shell,
        job: Uuid,
        cancel: CancelToken,
        waker: Option<Waker>,
    ) -> Result<JobRunner, CommandError> {
        let project = shell
            .engine()
            .project()
            .ok_or_else(|| CommandError::unavailable("no Project is open"))?;
        let entry = project
            .output_job(job)
            .ok_or_else(|| CommandError::not_found(format!("no Output Job {job}")))?;
        if output_job::broken(project, job) {
            return Err(CommandError::rejected(tr!(
                "output-job-broken-run",
                job = entry.name.as_str()
            )));
        }
        let (name, pcb) = (entry.name.clone(), entry.pcb);
        let detached = shell
            .engine()
            .detach("output.job.run", json!({ "job": job }))?;
        JobRunner::spawn(detached, Some(job), name, Some(pcb), cancel, waker)
    }

    /// Starts every job of the Project (`output.job.run_all`, stopping at the first
    /// blocked one) with `cancel` as its token, waking the window through `waker`.
    pub fn start_all(
        shell: &Shell,
        cancel: CancelToken,
        waker: Option<Waker>,
    ) -> Result<JobRunner, CommandError> {
        let project = shell
            .engine()
            .project()
            .ok_or_else(|| CommandError::unavailable("no Project is open"))?;
        let pcb = project.pcb().map(|p| p.uuid);
        let detached = shell
            .engine()
            .detach("output.job.run_all", json!({ "stop_on_block": true }))?;
        JobRunner::spawn(detached, None, tr!("output-job-all"), pcb, cancel, waker)
    }

    fn spawn(
        detached: reda_engine::Detached,
        job: Option<Uuid>,
        name: String,
        pcb: Option<Uuid>,
        cancel: CancelToken,
        waker: Option<Waker>,
    ) -> Result<JobRunner, CommandError> {
        let reporter = Arc::new(Reporter {
            step: Mutex::new(Step {
                fraction: 0.0,
                message: tr!("output-job-starting"),
            }),
            waker: waker.clone(),
        });
        let signals = Signals::new(cancel.clone()).with_progress(reporter.clone());
        let thread = std::thread::Builder::new()
            .name("output-job".into())
            .spawn(move || {
                let done = detached.run(&signals);
                if let Some(wake) = waker {
                    wake();
                }
                done
            })
            .map_err(|e| CommandError::internal(format!("could not start the run: {e}")))?;
        Ok(JobRunner {
            job,
            name,
            pcb,
            cancel,
            reporter,
            thread,
        })
    }

    /// The job run; `None` for Run all.
    pub fn job(&self) -> Option<Uuid> {
        self.job
    }

    pub fn job_name(&self) -> &str {
        &self.name
    }

    /// The last progress reported.
    pub fn step(&self) -> Step {
        self.reporter
            .step
            .lock()
            .map(|s| s.clone())
            .unwrap_or_default()
    }

    /// Asks the run to stop. It stops at its next step, up to when it starts writing.
    pub fn cancel(&self) {
        self.cancel.cancel();
    }

    /// Whether Cancel was asked for.
    pub fn cancelling(&self) -> bool {
        self.cancel.is_cancelled()
    }

    /// Whether the run has ended, so [`JobRunner::wait`] will not block.
    pub fn is_finished(&self) -> bool {
        self.thread.is_finished()
    }

    /// Waits for the run to end, journals it and puts its blockers and warnings in the
    /// Messages panel, and returns the run report.
    pub fn wait(self, shell: &mut Shell) -> RunReport {
        let outcome = match self.thread.join() {
            Ok(done) => shell.finish_detached(done),
            Err(_) => Err(CommandError::internal("the run stopped unexpectedly")),
        };
        let (outcome, blockers, warnings) = match outcome {
            Ok(result) if self.job.is_none() => all(&result),
            Ok(result) => {
                let warnings = untagged(violations(&result["warnings"]));
                (written(&result), Vec::new(), warnings)
            }
            Err(e) if e.is_cancelled() => (RunOutcome::Cancelled, Vec::new(), Vec::new()),
            Err(e) => match &e.details {
                Some(details) if details.get("blockers").is_some() => (
                    RunOutcome::Blocked,
                    untagged(violations(&details["blockers"])),
                    untagged(violations(&details["warnings"])),
                ),
                _ => (RunOutcome::Failed(e.message), Vec::new(), Vec::new()),
            },
        };
        if outcome != RunOutcome::Cancelled {
            let document = shell
                .engine()
                .project()
                .zip(self.pcb)
                .and_then(|(p, pcb)| p.document(pcb))
                .map(|d| d.uuid);
            let notes = blockers
                .iter()
                .chain(&warnings)
                .map(|(job, v)| Note {
                    source: SOURCE.to_owned(),
                    document,
                    severity: v.severity,
                    text: tr!(
                        "message-output-job",
                        job = job.as_deref().unwrap_or(self.name.as_str()),
                        text = messages::violation_text(v)
                    ),
                })
                .collect();
            shell.set_notes(SOURCE, notes);
        }
        let text = |list: Vec<(Option<String>, Violation)>| {
            list.iter()
                .map(|(job, v)| match job {
                    Some(job) => tr!(
                        "message-output-job",
                        job = job.as_str(),
                        text = messages::violation_text(v)
                    ),
                    None => messages::violation_text(v),
                })
                .collect()
        };
        RunReport {
            job: self.job,
            job_name: self.name,
            outcome,
            blockers: text(blockers),
            warnings: text(warnings),
        }
    }
}

/// The Violations in a JSON list; anything that does not read as one is skipped.
fn violations(list: &Value) -> Vec<Violation> {
    list.as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| serde_json::from_value::<Violation>(v.clone()).ok())
        .filter(|v| v.severity != Severity::Off)
        .collect()
}

/// Violations of the one job run, not tagged with a job name.
fn untagged(list: Vec<Violation>) -> Vec<(Option<String>, Violation)> {
    list.into_iter().map(|v| (None, v)).collect()
}

type Tagged = Vec<(Option<String>, Violation)>;

/// A Run all's report: each job's line, and the blockers and warnings of every job,
/// each tagged with its job's name.
fn all(result: &Value) -> (RunOutcome, Tagged, Tagged) {
    let (mut blockers, mut warnings) = (Vec::new(), Vec::new());
    let jobs = result["jobs"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|j| {
            let name = j["name"].as_str().unwrap_or_default().to_owned();
            let tag = |list: Vec<Violation>| list.into_iter().map(|v| (Some(name.clone()), v));
            blockers.extend(tag(violations(&j["blockers"])));
            warnings.extend(tag(violations(&j["result"]["warnings"])));
            let status = match j["status"].as_str() {
                Some("written") => JobStatus::Written {
                    destination: PathBuf::from(
                        j["result"]["destination"].as_str().unwrap_or_default(),
                    ),
                },
                Some("blocked") => JobStatus::Blocked,
                Some("failed") => {
                    JobStatus::Failed(j["error"].as_str().unwrap_or_default().to_owned())
                }
                _ => JobStatus::NotRun,
            };
            JobLine {
                job: j["job"]
                    .as_str()
                    .and_then(|u| u.parse().ok())
                    .unwrap_or_default(),
                name,
                status,
            }
        })
        .collect();
    let stopped = result["stopped"].as_bool().unwrap_or_default();
    (RunOutcome::All { jobs, stopped }, blockers, warnings)
}

fn written(result: &Value) -> RunOutcome {
    let path = |key: &str| PathBuf::from(result[key].as_str().unwrap_or_default());
    let files = result["files"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|f| WrittenFile {
            name: f["path"].as_str().unwrap_or_default().to_owned(),
            function: f["function"].as_str().unwrap_or_default().to_owned(),
        })
        .collect();
    RunOutcome::Written {
        destination: path("destination"),
        zip: path("zip"),
        files,
    }
}

/// The window's job run: the one running, or the report of the last until it is closed.
#[derive(Default)]
pub struct JobRuns {
    running: Option<JobRunner>,
    report: Option<RunReport>,
}

impl JobRuns {
    /// Whether a job is running.
    pub fn busy(&self) -> bool {
        self.running.is_some()
    }

    /// Starts `job`, waking the window as it goes. A problem goes to the status bar.
    pub fn start(&mut self, ctx: &egui::Context, shell: &mut Shell, job: Uuid) {
        if self.busy() {
            shell.report(tr!("output-job-busy"));
            return;
        }
        let ctx = ctx.clone();
        let waker: Waker = Arc::new(move || ctx.request_repaint());
        match JobRunner::start_with(shell, job, CancelToken::new(), Some(waker)) {
            Ok(runner) => {
                self.report = None;
                self.running = Some(runner);
            }
            Err(e) => shell.report(tr!("error-command", error = e.message)),
        }
    }

    /// Starts every job of the Project (Run all), waking the window as it goes.
    pub fn start_all(&mut self, ctx: &egui::Context, shell: &mut Shell) {
        if self.busy() {
            shell.report(tr!("output-job-busy"));
            return;
        }
        let ctx = ctx.clone();
        let waker: Waker = Arc::new(move || ctx.request_repaint());
        match JobRunner::start_all(shell, CancelToken::new(), Some(waker)) {
            Ok(runner) => {
                self.report = None;
                self.running = Some(runner);
            }
            Err(e) => shell.report(tr!("error-command", error = e.message)),
        }
    }

    /// Runs the job the Output Job dialog asked to run, after applying the dialog.
    pub fn run_from_dialog(
        &mut self,
        ctx: &egui::Context,
        shell: &mut Shell,
        dialog: &mut crate::settings::SettingsDialog,
    ) {
        if !dialog.take_run() {
            return;
        }
        if let Some(job) = output_job::job_of(dialog)
            && dialog.apply(shell).is_ok()
        {
            self.start(ctx, shell, job);
        }
    }

    /// Draws the progress modal while a job runs, then its report.
    pub fn show(&mut self, ctx: &egui::Context, shell: &mut Shell) {
        if let Some(runner) = &self.running {
            if !runner.is_finished() {
                if show_progress(ctx, runner) {
                    runner.cancel();
                }
                // The waker repaints on progress; this catches the thread's end.
                ctx.request_repaint_after(std::time::Duration::from_millis(100));
                return;
            }
            if let Some(runner) = self.running.take() {
                self.report = Some(runner.wait(shell));
            }
        }
        let Some(report) = &self.report else {
            return;
        };
        match show_report(ctx, report) {
            Some(ReportAction::ShowFolder(folder)) => {
                if let Err(e) = crate::os::show_in_folder(&folder) {
                    shell.report(tr!("error-show-in-folder", error = e.to_string()));
                }
            }
            Some(ReportAction::Close) => self.report = None,
            None => {}
        }
    }
}
