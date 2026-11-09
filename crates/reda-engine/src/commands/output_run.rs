//! `output.job.run`: runs an Output Job end to end, and `output.job.run_all` runs every
//! job. A run reads the schematic (Sync, ERC) as well as the board, so it lives in the
//! engine; the writers are `reda-output`'s.
//!
//! A run checks what blocks it (and refuses), refills stale Zones, plots the files, zips
//! them, and writes the layout of outputs.md, "Output layout", to the job's destination:
//! the zip, its loose copies in `fab/`, the BOM and CPL beside it (or in it, with the
//! preset's `zip_assembly`), the schematic PDF, `reports/` and the Output Manifest. It
//! first removes what the last run wrote there. It is long-running: it reports progress
//! per step and can be cancelled up to the moment it starts writing.

mod checks;
mod manifest;
mod run_all;

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

use crate::commands::checks_report::{JobReport, job_report};
use crate::fs::write_atomic;

pub(crate) fn register_all(registry: &mut Registry) {
    registry.add(JobRun);
    registry.add(run_all::JobRunAll);
}

pub struct JobRun;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct JobRunParams {
    /// The Output Job to run.
    pub job: Uuid,
}

/// One file the run wrote.
#[derive(Debug, Serialize, JsonSchema)]
pub struct WrittenFile {
    /// Its path in the destination, `/`-separated: `fab/Demo.gtl` for a file in the zip
    /// (whose entry is `Demo.gtl`), `Demo-Generic.zip`, `Demo-Generic-BOM.csv` or
    /// `reports/Demo-drc.json`.
    pub path: String,
    /// What it holds: `copper_top`, `copper_inner`, `copper_bottom`, `solder_mask_top`,
    /// `solder_mask_bottom`, `paste_top`, `paste_bottom`, `silkscreen_top`,
    /// `silkscreen_bottom`, `profile`, `drill_plated`, `drill_non_plated`, `drill_mixed`
    /// (with `merge_drill`), `fab_notes`, `drill_report`, `gerber_job`, `bom`, `cpl`, `zip`,
    /// `schematic_pdf`, `erc_report`, `drc_report` or `change_list_report`.
    pub function: &'static str,
}

/// A file the run left out because it would have held nothing.
#[derive(Debug, Serialize, JsonSchema)]
pub struct SkippedFile {
    /// The name it would have had in the zip, such as `Demo.gbp`.
    pub name: String,
    /// What it would have held: `paste_top`, `paste_bottom`, `silkscreen_top`,
    /// `silkscreen_bottom`, `drill_plated`, `drill_non_plated` or `drill_mixed`. Copper,
    /// Solder Mask and the Profile are always written.
    pub function: &'static str,
}

/// The run report.
#[derive(Debug, Serialize, JsonSchema)]
pub struct JobRunResult {
    /// The folder the files went to.
    pub destination: PathBuf,
    /// The zip, `<name>.zip` in the destination, `<name>` being the job's resolved name
    /// template.
    pub zip: PathBuf,
    /// The Output Manifest, `manifest.json` in the destination.
    pub manifest: PathBuf,
    /// Every file written but the manifest, in a fixed order by kind: the zip's files
    /// (as their `fab/` copies, in zip order), the zip, the BOM and CPL beside it, the
    /// schematic PDF, then the reports.
    pub files: Vec<WrittenFile>,
    /// The files left out because they would have been empty: a Silkscreen or Paste with
    /// nothing on it, or a drill file with no holes.
    pub skipped: Vec<SkippedFile>,
    /// What the run noticed but did not stop for, as Violations with severity `warning`:
    /// `out_of_sync {changes}`, `unassigned_footprint {designator}` (a Component on a sheet
    /// with no Footprint, so not on the PCB), `erc_errors {count}`, `drc_errors {count}`,
    /// `rule_outside_preset {rule, value, board, preset_limit, limit, preset, net_class?}`
    /// (as DRC reports it), `missing_font {font}`. The manifest lists them too.
    pub warnings: Vec<Violation>,
}

impl Command for JobRun {
    const META: Meta = Meta {
        id: "output.job.run",
        title: "Run Output Job",
        description: "Runs an Output Job: refills stale Zones, plots the Gerber X2 files \
                      (copper, Solder Mask, Paste and Silkscreen, the Board Outline as the \
                      Profile; an empty Silkscreen or Paste is left out and listed in \
                      `skipped`), named by the preset's `naming` (`classic`: `<project>.gtl` \
                      … `.gm1`; `descriptive`: `<project>-<Layer>.gbr`) and `file_names`, \
                      and the Excellon drill files (plated and non-plated, or one file with \
                      the preset's `merge_drill`; a drill file with no holes is left out and \
                      listed in `skipped`), then the files for the fab: \
                      `<project>-fab-notes.txt` (board, Stackup, minimum features, hole \
                      tolerances, Net Ties as deliberate shorts, the files in the zip and \
                      the board's `fab_notes` text), `<project>-drill-report.txt` (each \
                      drill file's tools and counts) and `<project>-job.gbrjob` (a minimal \
                      Gerber job file), each with its `outputs` toggle. They go into \
                      `<name>.zip`, every entry dated 1980-01-01 so the same design gives \
                      the same zip, with loose copies in `fab/`; `<name>` is the job's name \
                      template (default `${project}-${job}`), and files in the zip stay \
                      named from the project. The CSV BOM and CPL (columns from the job's \
                      Fab Preset; Not Fitted and Mechanical Parts left out) sit beside the \
                      zip as `<name>-BOM.csv` and `<name>-CPL.csv`, or go into it as \
                      `<project>-BOM.csv` and `<project>-CPL.csv` with the preset's \
                      `zip_assembly`. `<name>-schematic.pdf`, beside the zip and never in \
                      it, holds every Schematic Sheet as `schematic.export_pdf` writes \
                      them, in the job's `schematic_pdf_colour` (`outputs.schematic_pdf` \
                      toggle). `reports/` gets the ERC, DRC and Change List reports \
                      as `checks.report` writes them (JSON; `outputs.reports` toggle), and \
                      `manifest.json` lists every file with its size, SHA-256 and kind, the \
                      zip's SHA-256, the REDA version, job, Fab Preset and its hash, PCB \
                      Document, revision, the Clock date and the warnings. All of it goes \
                      to the job's destination (default `outputs/<job>/`; the same tokens \
                      as the name template), first removing what the last run's manifest \
                      lists. The job's `not_fitted_paste: remove` leaves Not Fitted Pads \
                      out of the Paste. Gerber, \
                      drill and CPL share one origin: the board's own zero for an `absolute` \
                      preset, and for a `relative` one the Output Origin, or the lower-left \
                      of the Board Outline while none is placed. The checks \
                      that block read only the job's PCB: duplicate Designators, an open \
                      Board Outline, a Footprint the PCB cannot resolve and a coordinate \
                      beyond ±2147.483647 mm (`coordinate_out_of_range`) block the run \
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
        run_job(cx, p.job)
    }
}

/// Runs the Output Job `job`; a blocked run is `rejected` with `details.blockers`.
pub(crate) fn run_job(cx: &mut Context<'_>, job_id: Uuid) -> Result<JobRunResult, CommandError> {
    const STEPS: f64 = 6.0;
    let created = cx.now();
    let fonts = cx.fonts().clone();
    let version = cx.version().to_owned();
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
        .output_job(job_id)
        .ok_or_else(|| CommandError::not_found(format!("no Output Job {job_id}")))?
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
    let instances = [reda_output::BoardInstance::single(
        &pcb,
        &fills,
        preset.gerber.file_origin,
    )];
    let plot = Plot {
        project,
        instances: &instances,
        gerber: &preset.gerber,
        fonts: &fonts,
        version: &version,
        part: reda_output::Part::Single,
        not_fitted_paste: job.not_fitted_paste,
    };
    let gerbers = reda_output::gerber_files(&plot).map_err(|out| {
        let blocker = checks::out_of_range(&out);
        let mut err = CommandError::rejected(format!(
            "Output Job \"{}\" cannot run: {} holds a coordinate beyond ±2147.483647 mm, \
                 which a Gerber file can't hold",
            job.name, out.file
        ));
        err.details = Some(json!({"blockers": [blocker], "warnings": warnings}));
        err
    })?;
    let bom = reda_output::bom_file(project, &pcb, &preset.bom.columns, &preset.bom.mount_values);
    let cpl = reda_output::cpl_file(project, &instances, &preset.cpl, &preset.bom.mount_values);
    let drills = reda_output::drill_files(&plot);
    let values = |token: &str| {
        Some(match token {
            "project" => name.clone(),
            "job" => job.name.clone(),
            "revision" => project.props().properties.revision.clone(),
            "date" => manifest::iso_date(created),
            "preset" => preset.name.clone(),
            _ => return None,
        })
    };
    let stem = job.name_template.resolve(values).map_err(|e| {
        CommandError::rejected(format!(
            "Output Job \"{}\": the name template can't be used: {e}",
            job.name
        ))
    })?;
    let destination = resolve_destination(&job, values)?;
    let revision = project.props().properties.revision.clone();

    let mut skipped = Vec::new();
    for (on, left_out) in [
        (job.outputs.gerber, &gerbers.skipped),
        (job.outputs.drill, &drills.skipped),
    ] {
        if on {
            skipped.extend(left_out.iter().map(|f| SkippedFile {
                name: f.name.clone(),
                function: f.function,
            }));
        }
    }
    // Every name a run of this job could give a file, so turning an output off drops
    // its old file.
    let mut known: Vec<String> = Vec::new();
    known.extend(gerbers.skipped.iter().map(|f| f.name.clone()));
    // Every drill name, so switching `merge_drill` drops the other form's old files.
    known.extend(reda_output::drill_file_names(&name));
    let fab_notes_name = reda_output::fab_notes_name(&name);
    known.extend([
        fab_notes_name.clone(),
        reda_output::drill_report_name(&name),
        reda_output::gerber_job_name(&name),
        bom.name.clone(),
        cpl.name.clone(),
    ]);
    let pdf_name = format!("{stem}-schematic.pdf");
    known.push(pdf_name.clone());
    // In zip order: Gerber, drill, the files written for the fab, then the BOM and CPL
    // with `zip_assembly`.
    let mut zipped: Vec<OutputFile> = Vec::new();
    for (on, every) in [
        (job.outputs.gerber, gerbers.files),
        (job.outputs.drill, drills.files),
    ] {
        known.extend(every.iter().map(|f| f.name.clone()));
        if on {
            zipped.extend(every);
        }
    }
    let job_file = job
        .outputs
        .gerber_job
        .then(|| reda_output::gerber_job_file(&plot, &zipped));
    let report = job
        .outputs
        .drill_report
        .then(|| reda_output::drill_report_file(&plot));
    let mut after: Vec<OutputFile> = report.into_iter().chain(job_file).collect();
    let mut beside: Vec<OutputFile> = Vec::new();
    for (on, mut file, suffix) in [(job.outputs.bom, bom, "BOM"), (job.outputs.cpl, cpl, "CPL")] {
        if !on {
            continue;
        }
        if preset.zip_assembly {
            after.push(file);
        } else {
            file.name = format!("{stem}-{suffix}.csv");
            beside.push(file);
        }
    }
    if job.outputs.fab_notes {
        // The Fab Notes list every file in the zip, themselves included.
        let zip_names: Vec<String> = zipped
            .iter()
            .map(|f| f.name.clone())
            .chain([fab_notes_name])
            .chain(after.iter().map(|f| f.name.clone()))
            .collect();
        zipped.push(reda_output::fab_notes_file(&plot, &zip_names));
    }
    zipped.extend(after);

    cx.progress(3.0, Some(STEPS), "Zipping");
    cx.check_cancelled()?;
    let zip_name = format!("{stem}.zip");
    let zip = reda_output::zip(&zipped);

    // Every file but the manifest, in run order: path, function and bytes.
    let mut written: Vec<(String, &'static str, Vec<u8>)> = Vec::new();
    written.extend(
        zipped
            .into_iter()
            .map(|f| (format!("{FAB}/{}", f.name), f.function, f.bytes)),
    );
    written.push((zip_name.clone(), "zip", zip.clone()));
    written.extend(beside.into_iter().map(|f| (f.name, f.function, f.bytes)));
    if job.outputs.schematic_pdf {
        // Every sheet, drawn as this user draws an output; headless, with the defaults.
        cx.progress(3.5, Some(STEPS), "Writing the schematic PDF");
        let session = cx.session()?;
        let preferences = crate::preferences::current(session.user_dirs.as_ref());
        let project = session
            .project
            .as_ref()
            .ok_or_else(|| CommandError::unavailable(NO_PROJECT))?;
        let sheets = crate::schematic_pdf::sheets(project);
        let (pdf, _) = crate::schematic_pdf::schematic_pdf(
            project,
            &preferences,
            &sheets,
            job.schematic_pdf_colour,
        );
        written.push((pdf_name, "schematic_pdf", pdf));
    }
    if job.outputs.reports {
        cx.progress(4.0, Some(STEPS), "Writing the check reports");
        for which in JobReport::ALL {
            let bytes = job_report(cx, which, created)?;
            written.push((
                format!("{REPORTS}/{}", which.file_name(&name)),
                which.function(),
                bytes,
            ));
        }
    }
    let manifest_bytes = manifest::manifest(&manifest::Run {
        version: &version,
        job: &job,
        preset: &preset,
        pcb: pcb.uuid,
        revision: &revision,
        created,
        zip: (&zip_name, &zip),
        files: &written,
        warnings: &warnings,
    })?;

    cx.progress(5.0, Some(STEPS), "Writing files");
    cx.check_cancelled()?;
    let destination: PathBuf = folder.join(destination).components().collect();
    let mut stale: Vec<String> = Vec::new();
    for name in &known {
        // The top level too: runs before `fab/` wrote the files there.
        stale.push(name.clone());
        stale.push(format!("{FAB}/{name}"));
    }
    stale.extend(
        JobReport::ALL
            .iter()
            .map(|r| format!("{REPORTS}/{}", r.file_name(&name))),
    );
    stale.extend(last_run(&destination));
    write_all(&destination, &stale, &written, &manifest_bytes)?;
    cx.progress(STEPS, Some(STEPS), "Done");

    Ok(JobRunResult {
        zip: destination.join(&zip_name),
        manifest: destination.join(manifest::MANIFEST),
        destination,
        files: written
            .iter()
            .map(|(path, function, _)| WrittenFile {
                path: path.clone(),
                function,
            })
            .collect(),
        skipped,
        warnings,
    })
}

/// The folder in the destination that holds loose copies of the zip's files.
const FAB: &str = "fab";
/// The folder in the destination that holds the check reports.
const REPORTS: &str = "reports";

/// The job's destination relative to the Project folder, `/`-separated, with the name
/// template's tokens resolved. An empty part (an unset revision) is dropped; a value that
/// would leave the Project folder refuses the run.
fn resolve_destination(
    job: &reda_model::OutputJob,
    values: impl Fn(&str) -> Option<String>,
) -> Result<String, CommandError> {
    let raw = job.destination();
    let resolved = match &job.destination {
        Some(raw) => reda_model::text::resolve(raw, values)
            .map_err(|e| CommandError::rejected(format!("the destination: {e}")))?,
        None => raw,
    };
    let parts: Vec<&str> = resolved.split('/').filter(|p| !p.is_empty()).collect();
    let escapes = parts.is_empty()
        || resolved.contains('\\')
        || resolved.contains(':')
        || parts.iter().any(|p| *p == ".." || *p == ".");
    if escapes {
        return Err(CommandError::rejected(format!(
            "Output Job \"{}\": the destination \"{resolved}\" is not a folder inside the \
             Project folder",
            job.name
        )));
    }
    Ok(parts.join("/"))
}

/// The paths the last run's manifest in `destination` lists, the zip among them; only
/// plain relative paths, so a hand-edited manifest can't reach outside the folder.
fn last_run(destination: &Path) -> Vec<String> {
    let Ok(bytes) = std::fs::read(destination.join(manifest::MANIFEST)) else {
        return Vec::new();
    };
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
        return Vec::new();
    };
    let files = value["files"].as_array().into_iter().flatten();
    files
        .chain([&value["zip"]])
        .filter_map(|f| f["path"].as_str())
        .filter(|path| {
            !path.is_empty()
                && !path.starts_with('/')
                && !path.contains('\\')
                && !path.contains(':')
                && path
                    .split('/')
                    .all(|p| !p.is_empty() && p != "." && p != "..")
        })
        .map(str::to_owned)
        .collect()
}

/// Writes `files` and the manifest into `destination`, first removing every file in
/// `stale` (what a run of this job could have written) so turning an output off or
/// renaming the zip drops the old files. Other files in the folder are left alone.
fn write_all(
    destination: &Path,
    stale: &[String],
    files: &[(String, &'static str, Vec<u8>)],
    manifest: &[u8],
) -> Result<(), CommandError> {
    let failed = |path: &Path, e: std::io::Error| {
        CommandError::internal(format!("could not write {}: {e}", path.display()))
    };
    let at = |rel: &str| -> PathBuf {
        rel.split('/')
            .fold(destination.to_path_buf(), |p, s| p.join(s))
    };
    std::fs::create_dir_all(destination).map_err(|e| failed(destination, e))?;
    for rel in stale {
        let path = at(rel);
        match std::fs::remove_file(&path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(failed(&path, e)),
            _ => {}
        }
    }
    for folder in [FAB, REPORTS] {
        // Gone when it is left empty, as when the reports are turned off.
        let _ = std::fs::remove_dir(destination.join(folder));
    }
    for (rel, _, bytes) in files {
        let path = at(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| failed(parent, e))?;
        }
        write_atomic(&path, bytes).map_err(|e| failed(&path, e))?;
    }
    let path = destination.join(manifest::MANIFEST);
    write_atomic(&path, manifest).map_err(|e| failed(&path, e))
}
