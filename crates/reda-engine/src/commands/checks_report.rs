//! `checks.report`: runs ERC or DRC with its run Command's parameters, or takes the Sync
//! Change List, and writes it to a JSON or HTML file. It spans the schematic, the board
//! and Sync, so it lives in the engine. `erc.run` and `drc.run` stay pure queries.
//!
//! A report depends only on the design and the Clock, so it is the same file byte for
//! byte under a fixed Clock.

mod html;

use std::path::PathBuf;

use reda_command::{Command, CommandError, Context, Kind, Meta, Registry};
use reda_model::{DrcCheck, Project, Severity, Uuid, Violation, Waiver};
use reda_pcb::DrcRunParams;
use reda_schematic::erc::ErcRunParams;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::fs::write_atomic;
use crate::sync::{self, NO_PCB};

pub(crate) fn register_all(registry: &mut Registry) {
    registry.add(ChecksReport);
}

pub struct ChecksReport;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ChecksReportParams {
    /// Where to write the report. A relative path is taken from the Project folder.
    pub path: PathBuf,
    pub format: ReportFormat,
    #[serde(flatten)]
    pub of: ReportOf,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReportFormat {
    /// The run's result, with a per-check summary and the stale Waivers.
    Json,
    /// A page with a per-check summary and sections for the Violations, the waived
    /// Violations, the stale Waivers and (for DRC) the health checks.
    Html,
}

/// What the report is of, with the parameters of its run Command.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ReportOf {
    /// ERC, with `erc.run`'s parameters.
    Erc(ErcRunParams),
    /// DRC, with `drc.run`'s parameters.
    Drc(DrcRunParams),
    /// The Change List `sync.preview` returns.
    Sync(SyncReportParams),
}

/// A Sync report takes no further parameters.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SyncReportParams {}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ChecksReportResult {
    /// The file written.
    pub path: PathBuf,
    /// ERC and DRC: the Errors and Warnings listed, waived ones left out. Sync: the
    /// out-of-sync count.
    pub count: usize,
}

impl Command for ChecksReport {
    const META: Meta = Meta {
        id: "checks.report",
        title: "Write Check Report",
        description: "Runs ERC (`kind: erc`) or DRC (`kind: drc`) with the same parameters \
                      as `erc.run` or `drc.run`, or takes the Change List of \
                      `sync.preview` (`kind: sync`), and writes it to `path` as JSON or \
                      HTML. Waived Violations are always listed, each naming its Waiver. \
                      The JSON is the run's result with `kind`, `project`, `generated` \
                      (the Clock, ms), a per-check `summary` and the `stale_waivers`; the \
                      HTML has a per-check summary and Violations, waived, stale-Waiver \
                      and (DRC) health sections. Nothing in the design changes.",
        category: "checks",
        icon: "drc",
        kind: Kind::File,
    };
    type Params = ChecksReportParams;
    type Output = ChecksReportResult;

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: ChecksReportParams,
    ) -> Result<ChecksReportResult, CommandError> {
        let generated = cx.now();
        let location = cx.session()?.location.clone();
        let path = match location {
            Some(folder) if p.path.is_relative() => folder.join(&p.path),
            None if p.path.is_relative() => {
                return Err(CommandError::rejected(
                    "the Project has no folder yet: give an absolute path",
                ));
            }
            _ => p.path.clone(),
        };
        let project = cx.project()?;
        let header = Header {
            project: project.props().name.clone(),
            generated: generated.0,
            utc: generated.utc(),
        };
        let (report, count) = match p.of {
            ReportOf::Erc(mut params) => {
                params.include_waived = true;
                let library = |uuids: &[Uuid]| cx.library_parts(uuids);
                let run = reda_schematic::erc::run(project, &library, &params)?;
                let count = unwaived(&run.violations);
                (Report::checks("erc", project, to_value(&run)?), count)
            }
            ReportOf::Drc(mut params) => {
                params.include_waived = true;
                let run = reda_pcb::drc_run(project, cx.fonts(), sync::drc_state, &params)?;
                let count = unwaived(&run.violations);
                (Report::checks("drc", project, to_value(&run)?), count)
            }
            ReportOf::Sync(SyncReportParams {}) => {
                let list = sync::plan(project).ok_or_else(|| CommandError::unavailable(NO_PCB))?;
                let count = list.count;
                (Report::Sync(to_value(&list)?), count)
            }
        };
        let bytes = match p.format {
            ReportFormat::Json => {
                let mut text = serde_json::to_string_pretty(&report.json(&header))
                    .map_err(|e| CommandError::internal(e.to_string()))?;
                text.push('\n');
                text.into_bytes()
            }
            ReportFormat::Html => html::page(&report, &header).into_bytes(),
        };
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| write_error(&path, &e))?;
        }
        write_atomic(&path, &bytes).map_err(|e| write_error(&path, &e))?;
        Ok(ChecksReportResult { path, count })
    }
}

fn write_error(path: &std::path::Path, e: &std::io::Error) -> CommandError {
    CommandError::rejected(format!("could not write {}: {e}", path.display()))
}

fn to_value(value: &impl Serialize) -> Result<Value, CommandError> {
    serde_json::to_value(value).map_err(|e| CommandError::internal(e.to_string()))
}

fn unwaived(violations: &[Violation]) -> usize {
    violations.iter().filter(|v| v.waiver.is_none()).count()
}

/// What every report starts with.
pub(super) struct Header {
    project: String,
    /// Milliseconds since the Unix epoch, from the Clock.
    generated: i64,
    utc: String,
}

/// One check's line of the summary.
#[derive(Debug, Serialize)]
struct Summary {
    check: String,
    severity: Severity,
    /// The Violations listed, waived ones left out.
    count: usize,
    waived: usize,
}

/// A report's content, before it is written out.
enum Report {
    Checks {
        kind: &'static str,
        /// The run Command's result.
        run: Value,
        violations: Vec<Violation>,
        stale_waivers: Vec<Waiver>,
        /// Waivers by UUID, for the waived Violations' reasons.
        waivers: Vec<Waiver>,
    },
    /// The Change List.
    Sync(Value),
}

impl Report {
    fn checks(kind: &'static str, project: &Project, run: Value) -> Report {
        let violations: Vec<Violation> =
            serde_json::from_value(run["violations"].clone()).unwrap_or_default();
        let waivers: Vec<Waiver> = project
            .documents()
            .flat_map(|d| {
                let sheet = project.schematic_sheet(d.uuid).into_iter();
                let board = project.pcb_document(d.uuid).into_iter();
                sheet
                    .flat_map(|s| s.waivers())
                    .chain(board.flat_map(|b| b.waivers()))
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .collect();
        let unmatched: Vec<Uuid> =
            serde_json::from_value(run["unmatched_waivers"].clone()).unwrap_or_default();
        let stale_waivers = (unmatched.iter())
            .filter_map(|u| waivers.iter().find(|w| w.uuid == *u).cloned())
            .collect();
        Report::Checks {
            kind,
            run,
            violations,
            stale_waivers,
            waivers,
        }
    }

    /// The per-check summary, in the order the checks first appear in the run.
    fn summary(violations: &[Violation]) -> Vec<Summary> {
        let mut out: Vec<Summary> = Vec::new();
        for v in violations {
            let line = match out.iter_mut().position(|s| s.check == v.check) {
                Some(i) => &mut out[i],
                None => {
                    out.push(Summary {
                        check: v.check.clone(),
                        severity: v.severity,
                        count: 0,
                        waived: 0,
                    });
                    out.last_mut().expect("just pushed")
                }
            };
            if v.waiver.is_some() {
                line.waived += 1;
            } else {
                line.count += 1;
            }
        }
        out
    }

    fn json(&self, header: &Header) -> Value {
        let mut out = match self {
            Report::Checks {
                kind,
                run,
                violations,
                stale_waivers,
                ..
            } => {
                let mut out = run.clone();
                out["kind"] = json!(kind);
                out["summary"] = json!(Report::summary(violations));
                out["stale_waivers"] = json!(stale_waivers);
                out
            }
            Report::Sync(list) => json!({"kind": "sync", "change_list": list}),
        };
        out["project"] = json!(header.project);
        out["generated"] = json!(header.generated);
        out
    }
}

/// Whether `check` is one of DRC's health checks.
fn is_health(check: &str) -> bool {
    DrcCheck::HEALTH.iter().any(|c| c.id() == check)
}
