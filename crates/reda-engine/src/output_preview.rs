//! What a run of an Output Job would write, worked out without writing anything: the
//! Output Job window's previews. It plots with the same writers as `output.job.run`, so
//! the Gerber and drill names, which files are left out as empty, the zip's contents and
//! the BOM and CPL rows are the run's own.
//!
//! The job is passed in, not looked up, so the window can preview its unapplied drafts.

use reda_model::OutputJob;
use reda_output::{BoardInstance, Plot, Table};

use crate::Engine;

/// Whether a planned file is written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileState {
    /// It is written.
    Plotted,
    /// It would hold nothing, so it is left out.
    Empty,
    /// Its output is turned off in the job.
    Off,
}

/// One Gerber or drill file a run plans.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlannedFile {
    /// The Layer it plots, such as "Top Copper"; empty for a drill file.
    pub layer: String,
    /// Its name in the zip, such as `Demo.gtl`.
    pub name: String,
    /// What it holds, such as `copper_top` or `drill_plated`.
    pub function: &'static str,
    pub state: FileState,
}

/// The preview of one job.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JobPreview {
    /// The name template resolved: the zip's stem, or why it can't be used.
    pub stem: Result<String, String>,
    /// The destination resolved, relative to the Project folder, or why it can't be used.
    pub destination: Result<String, String>,
    /// Every Gerber file, in plot order.
    pub gerber: Vec<PlannedFile>,
    /// Every drill file, in file order.
    pub drill: Vec<PlannedFile>,
    /// The zip's entries, in zip order.
    pub zip: Vec<String>,
    /// The files beside the zip in the destination, the zip first: `<stem>.zip`, the BOM
    /// and CPL (unless the preset zips them) and the schematic PDF.
    pub beside: Vec<String>,
    /// The BOM's rows as the writer makes them.
    pub bom: Table,
    /// The CPL's rows as the writer makes them.
    pub cpl: Table,
    /// A coordinate a Gerber file can't hold, which would block the run: the file.
    pub out_of_range: Option<String>,
}

/// The preview of `job` on the engine's open Project, dated by its Clock; `None` without
/// a Project, or when the job's PCB Document or Fab Preset copy is gone.
pub fn job_preview(engine: &Engine, job: &OutputJob) -> Option<JobPreview> {
    let project = engine.project()?;
    let pcb = project.pcb_document(job.pcb)?;
    let preset = project.fab_preset(job.fab_preset)?.preset();
    let name = project.props().name.clone();
    let utc = engine.now().utc();
    let date = utc.get(..10).unwrap_or_default().to_owned();
    let values = reda_output::name_values(project, job, &preset.name, &date);
    let stem = job
        .name_template
        .resolve(&values)
        .map_err(|e| e.to_string());
    let destination = reda_output::resolve_destination(job, &values);

    let fills = reda_pcb::zone::current(pcb, project.zone_fills(pcb.uuid));
    let instances = [BoardInstance::single(
        pcb,
        &fills,
        preset.gerber.file_origin,
    )];
    let plot = Plot {
        project,
        instances: &instances,
        gerber: &preset.gerber,
        fonts: engine.fonts(),
        version: engine.version(),
        part: reda_output::Part::Single,
        not_fitted_paste: job.not_fitted_paste,
    };
    let state = |on: bool, written: bool| match (on, written) {
        (false, _) => FileState::Off,
        (true, true) => FileState::Plotted,
        (true, false) => FileState::Empty,
    };
    let mut zip = Vec::new();
    let mut out_of_range = None;
    let gerber = match reda_output::gerber_files(&plot) {
        Ok(gerbers) => gerbers
            .layers
            .iter()
            .map(|(file, layer)| {
                let written = gerbers.files.iter().find(|f| &f.name == file);
                let function = written.map(|f| f.function).or_else(|| {
                    let skipped = gerbers.skipped.iter().find(|f| &f.name == file);
                    skipped.map(|f| f.function)
                });
                if job.outputs.gerber && written.is_some() {
                    zip.push(file.clone());
                }
                PlannedFile {
                    layer: layer.clone(),
                    name: file.clone(),
                    function: function.unwrap_or_default(),
                    state: state(job.outputs.gerber, written.is_some()),
                }
            })
            .collect(),
        Err(out) => {
            out_of_range = Some(out.file);
            Vec::new()
        }
    };
    let drills = reda_output::drill_files(&plot);
    let mut drill: Vec<PlannedFile> = drills
        .files
        .iter()
        .map(|f| (f.name.clone(), f.function, true))
        .chain(
            drills
                .skipped
                .iter()
                .map(|f| (f.name.clone(), f.function, false)),
        )
        .map(|(name, function, written)| PlannedFile {
            layer: String::new(),
            name,
            function,
            state: state(job.outputs.drill, written),
        })
        .collect();
    // In file order: plated, then non-plated (or the one merged file).
    let order = |f: &PlannedFile| match f.function {
        "drill_plated" | "drill_mixed" => 0,
        _ => 1,
    };
    drill.sort_by_key(order);
    if job.outputs.drill {
        let written = drill.iter().filter(|f| f.state == FileState::Plotted);
        zip.extend(written.map(|f| f.name.clone()));
    }

    // The run's zip order: Gerber, drill, Fab Notes, drill report, job file, then the
    // BOM and CPL with `zip_assembly`.
    let outputs = job.outputs;
    for (on, file) in [
        (outputs.fab_notes, reda_output::fab_notes_name(&name)),
        (outputs.drill_report, reda_output::drill_report_name(&name)),
        (outputs.gerber_job, reda_output::gerber_job_name(&name)),
    ] {
        if on {
            zip.push(file);
        }
    }
    let bom = reda_output::bom_file(project, pcb, &preset.bom);
    let cpl = reda_output::cpl_table(project, &instances, &preset.cpl, &preset.bom.mount_values);
    let mut beside = Vec::new();
    let shown_stem = stem.clone().unwrap_or_else(|_| name.clone());
    beside.push(format!("{shown_stem}.zip"));
    for (on, suffix) in [(outputs.bom, "BOM"), (outputs.cpl, "CPL")] {
        if !on {
            continue;
        }
        if preset.zip_assembly {
            zip.push(format!("{name}-{suffix}.csv"));
        } else {
            beside.push(format!("{shown_stem}-{suffix}.csv"));
        }
    }
    if outputs.schematic_pdf {
        beside.push(format!("{shown_stem}-schematic.pdf"));
    }
    Some(JobPreview {
        stem,
        destination,
        gerber,
        drill,
        zip,
        beside,
        bom: bom.table,
        cpl,
        out_of_range,
    })
}
