//! `output.job.run_all`: every Output Job of the Project, in Projects panel order, with one
//! combined report.

use reda_command::{
    AvailabilityContext, Command, CommandError, Context, Kind, Meta, NO_PROJECT, Unavailable,
};
use reda_model::{Uuid, Violation};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{JobRunResult, run_job};

pub struct JobRunAll;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct JobRunAllParams {
    /// Stop at the first job a blocking problem stops, leaving the rest `not_run`; on by
    /// default. Off runs every job anyway.
    #[serde(default = "yes")]
    pub stop_on_block: bool,
}

fn yes() -> bool {
    true
}

/// How one job of the run went.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    /// It ran and wrote its files (`result`).
    Written,
    /// A blocking problem stopped it (`blockers`); it wrote nothing.
    Blocked,
    /// It could not run for another reason (`error`), such as a PCB Document that is gone.
    Failed,
    /// An earlier job blocked and `stop_on_block` is on.
    NotRun,
}

/// One job's line of the combined report.
#[derive(Debug, Serialize, JsonSchema)]
pub struct JobOutcome {
    pub job: Uuid,
    pub name: String,
    pub status: JobStatus,
    /// The run report of a `written` job.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<JobRunResult>,
    /// What blocked a `blocked` job, as `output.job.run` gives them.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub blockers: Vec<Violation>,
    /// Why a `failed` job could not run.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// The combined report.
#[derive(Debug, Serialize, JsonSchema)]
pub struct JobRunAllResult {
    /// Every job, in Projects panel order.
    pub jobs: Vec<JobOutcome>,
    /// Whether a block stopped the run before its last job.
    pub stopped: bool,
}

impl Command for JobRunAll {
    const META: Meta = Meta {
        id: "output.job.run_all",
        title: "Run All Output Jobs",
        description: "Runs every Output Job of the Project as `output.job.run` does, in \
                      Projects panel order, and gives a combined report: each job's \
                      `status` (`written` with its run report as `result`, `blocked` with \
                      its `blockers`, `failed` with an `error`, or `not_run`). With \
                      `stop_on_block` (on by default) the first blocked job stops the run \
                      and the jobs after it are `not_run`. Long-running: reports progress \
                      and can be cancelled.",
        category: "output",
        icon: "output_run",
        kind: Kind::File,
    };
    type Params = JobRunAllParams;
    type Output = JobRunAllResult;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        match cx.project {
            None => Err(Unavailable {
                reason: NO_PROJECT.into(),
            }),
            Some(_) => Ok(()),
        }
    }

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: JobRunAllParams,
    ) -> Result<JobRunAllResult, CommandError> {
        // The Projects panel lists the jobs in this same order.
        let jobs: Vec<(Uuid, String)> = cx
            .project()?
            .output_jobs()
            .map(|j| (j.uuid, j.name.clone()))
            .collect();
        let mut out = JobRunAllResult {
            jobs: Vec::new(),
            stopped: false,
        };
        for (job, name) in jobs {
            let mut outcome = JobOutcome {
                job,
                name,
                status: JobStatus::NotRun,
                result: None,
                blockers: Vec::new(),
                error: None,
            };
            if !out.stopped {
                match run_job(cx, job) {
                    Ok(result) => {
                        outcome.status = JobStatus::Written;
                        outcome.result = Some(result);
                    }
                    Err(e) if e.is_cancelled() => return Err(e),
                    Err(e) => match blockers(&e) {
                        Some(blockers) => {
                            outcome.status = JobStatus::Blocked;
                            outcome.blockers = blockers;
                            out.stopped = p.stop_on_block;
                        }
                        None => {
                            outcome.status = JobStatus::Failed;
                            outcome.error = Some(e.message);
                        }
                    },
                }
            }
            out.jobs.push(outcome);
        }
        // Stopped only counts when a job was left out.
        out.stopped = out.stopped
            && out
                .jobs
                .last()
                .is_some_and(|j| j.status == JobStatus::NotRun);
        Ok(out)
    }
}

/// The blockers of a blocked run's error, or `None` for any other error.
fn blockers(e: &CommandError) -> Option<Vec<Violation>> {
    let list = e.details.as_ref()?.get("blockers")?;
    serde_json::from_value(list.clone()).ok()
}
