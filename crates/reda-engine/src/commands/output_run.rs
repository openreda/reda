//! `output.job.run`: runs an Output Job end to end. It reads the schematic (Sync, ERC) as
//! well as the board, so it lives in the engine; the writers are `reda-output`'s.
//!
//! A run checks what blocks it (and refuses), refills stale Zones, plots the files, zips
//! them, and writes the files and the zip to the job's destination, replacing what an
//! earlier run wrote there. It is long-running: it reports progress per step and can be
//! cancelled up to the moment it starts writing.

mod checks;

use std::path::{Path, PathBuf};

use reda_command::{
    AvailabilityContext, Command, CommandError, Context, Kind, Meta, NO_PROJECT, Registry,
    Unavailable,
};
use reda_model::{Uuid, Violation};
use reda_output::{OutputFile, Plot};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::fs::write_atomic;

pub(crate) fn register_all(registry: &mut Registry) {
    registry.add(JobRun);
}

pub struct JobRun;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct JobRunParams {
    /// The Output Job to run.
    pub job: Uuid,
}

/// One file the run wrote, also in the zip.
#[derive(Debug, Serialize, JsonSchema)]
pub struct WrittenFile {
    /// The file name, such as `Demo.GTL`.
    pub name: String,
    /// What it holds: `copper_top`, `copper_inner`, `copper_bottom`, `solder_mask_top`,
    /// `solder_mask_bottom`, `paste_top`, `paste_bottom`, `silkscreen_top`, `silkscreen_bottom`, `profile`,
    /// `drill_plated`, `drill_non_plated`, `bom` or `cpl`.
    pub function: &'static str,
}

/// The run report.
#[derive(Debug, Serialize, JsonSchema)]
pub struct JobRunResult {
    /// The folder the files went to.
    pub destination: PathBuf,
    /// The zip of every file, `<project>-<job>.zip` in the destination.
    pub zip: PathBuf,
    /// Every file written, in zip order.
    pub files: Vec<WrittenFile>,
    /// What the run noticed but did not stop for, as Violations with severity `warning`:
    /// `out_of_sync {changes}`, `unassigned_footprint {designator}` (a Component on a sheet
    /// with no Footprint, so not on the PCB), `erc_errors {count}`, `drc_errors {count}`,
    /// `rule_below_preset {rule, board, minimum, preset, net_class?}`, `missing_font {font}`.
    pub warnings: Vec<Violation>,
}

impl Command for JobRun {
    const META: Meta = Meta {
        id: "output.job.run",
        title: "Run Output Job",
        description: "Runs an Output Job: refills stale Zones, plots the Gerber X2 files \
                      (top and bottom copper, Solder Mask, Paste and Silkscreen, the Board \
                      Outline as the Profile) and the Excellon drill files (plated and \
                      non-plated) and writes them with the CSV BOM and CPL (columns from the \
                      job's Fab Preset; Not Fitted and Mechanical Parts left out; CPL positions \
                      from the Placement Origin, else the Board Outline's lower-left) and their \
                      zip, `<project>-<job>.zip`, to the job's destination \
                      (default `outputs/<job>/`), replacing the last run's files. The checks \
                      that block read only the job's PCB: duplicate Designators, an open \
                      Board Outline and a Footprint the PCB cannot resolve block the run \
                      (`rejected`, `details.blockers`). Other problems, such as an \
                      out-of-sync PCB or a Component without a Footprint, are listed as \
                      `warnings`. \
                      Long-running: reports progress and can be cancelled.",
        category: "output",
        icon: "output_run",
        kind: Kind::File,
    };
    type Params = JobRunParams;
    type Output = JobRunResult;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        match cx.project {
            None => Err(Unavailable {
                reason: NO_PROJECT.into(),
            }),
            Some(_) => Ok(()),
        }
    }

    fn run(&self, cx: &mut Context<'_>, p: JobRunParams) -> Result<JobRunResult, CommandError> {
        const STEPS: f64 = 6.0;
        let created = cx.now();
        let fonts = cx.fonts().clone();
        cx.progress(0.0, Some(STEPS), "Checking the design");
        cx.check_cancelled()?;

        let session = cx.session()?;
        let folder = session
            .location
            .clone()
            .ok_or_else(|| CommandError::rejected("the Project has no folder yet: save it"))?;
        let project = session
            .project
            .as_ref()
            .ok_or_else(|| CommandError::unavailable(NO_PROJECT))?;
        let job = project
            .output_job(p.job)
            .ok_or_else(|| CommandError::not_found(format!("no Output Job {}", p.job)))?
            .clone();
        let pcb = project.pcb_document(job.pcb).cloned().ok_or_else(|| {
            CommandError::rejected(format!(
                "Output Job \"{}\" names a PCB Document that is gone: point it at another",
                job.name
            ))
        })?;
        let preset = project
            .fab_preset(job.fab_preset)
            .map(|p| p.preset())
            .ok_or_else(|| CommandError::internal("the job's Fab Preset copy is missing"))?;

        let blockers = checks::blockers(&pcb);
        let warnings = checks::warnings(project, &pcb, &preset, &fonts);
        if !blockers.is_empty() {
            let mut err = CommandError::rejected(format!(
                "Output Job \"{}\" cannot run: {} blocking problem(s)",
                job.name,
                blockers.len()
            ));
            err.details = Some(json!({"blockers": blockers, "warnings": warnings}));
            return Err(err);
        }

        let stored = project.zone_fills(pcb.uuid).cloned();
        cx.progress(1.0, Some(STEPS), "Refilling Zones");
        cx.check_cancelled()?;
        let fills = reda_pcb::zone::current(&pcb, stored.as_ref());
        if stored.as_ref() != Some(&fills)
            && let Some(project) = cx.session()?.project.as_mut()
        {
            project.set_zone_fills(pcb.uuid, Some(fills.clone()));
        }

        cx.progress(2.0, Some(STEPS), "Plotting Gerber and drill files");
        cx.check_cancelled()?;
        let project = cx
            .session()?
            .project
            .as_ref()
            .ok_or_else(|| CommandError::unavailable(NO_PROJECT))?;
        let name = project.props().name.clone();
        let plot = Plot {
            project,
            pcb: &pcb,
            fills: &fills,
            gerber: preset.gerber,
            fonts: &fonts,
            created,
        };
        let bom = reda_output::bom_file(project, &pcb, &preset.bom);
        let cpl = reda_output::cpl_file(project, &pcb, &preset.cpl);
        let mut known: Vec<String> = Vec::new();
        let mut files: Vec<OutputFile> = Vec::new();
        for (on, every) in [
            (job.outputs.gerber, reda_output::gerber_files(&plot)),
            (job.outputs.drill, reda_output::drill_files(&plot)),
            (job.outputs.bom, vec![bom]),
            (job.outputs.cpl, vec![cpl]),
        ] {
            known.extend(every.iter().map(|f| f.name.clone()));
            if on {
                files.extend(every);
            }
        }

        cx.progress(3.0, Some(STEPS), "Zipping");
        cx.check_cancelled()?;
        let zip_name = format!("{name}-{}.zip", job.name);
        let zip = reda_output::zip(&files, created);

        cx.progress(4.0, Some(STEPS), "Writing files");
        cx.check_cancelled()?;
        // The Job's `/`-separated destination, with the platform's separators.
        let destination: PathBuf = folder.join(job.destination()).components().collect();
        write_all(&destination, &known, &files, &zip_name, &zip)?;
        cx.progress(STEPS, Some(STEPS), "Done");

        Ok(JobRunResult {
            zip: destination.join(&zip_name),
            destination,
            files: files
                .iter()
                .map(|f| WrittenFile {
                    name: f.name.clone(),
                    function: f.function,
                })
                .collect(),
            warnings,
        })
    }
}

/// Writes `files` and the zip into `destination`, first removing every file a run of this
/// job could write (`known`) so turning an output off drops its old files. Other files in
/// the folder are left alone.
fn write_all(
    destination: &Path,
    known: &[String],
    files: &[OutputFile],
    zip_name: &str,
    zip: &[u8],
) -> Result<(), CommandError> {
    let failed = |path: &Path, e: std::io::Error| {
        CommandError::internal(format!("could not write {}: {e}", path.display()))
    };
    std::fs::create_dir_all(destination).map_err(|e| failed(destination, e))?;
    for name in known {
        let path = destination.join(name);
        match std::fs::remove_file(&path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(failed(&path, e)),
            _ => {}
        }
    }
    for file in files {
        let path = destination.join(&file.name);
        write_atomic(&path, &file.bytes).map_err(|e| failed(&path, e))?;
    }
    let path = destination.join(zip_name);
    write_atomic(&path, zip).map_err(|e| failed(&path, e))
}
