//! Output Jobs on the Project stack: add, edit, duplicate and delete.

use reda_command::{Command, CommandError, Context, Kind, Meta, Registry};
use reda_model::{
    DocumentKind, NAME_TEMPLATE_TOKENS, NameTemplate, NotFittedPaste, Object, OutputJob, Outputs,
    PdfColour, Project, Uuid,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Adds every Output Job Command.
pub fn register_all(registry: &mut Registry) {
    registry.add(JobAdd);
    registry.add(JobSet);
    registry.add(JobDuplicate);
    registry.add(JobDelete);
}

/// A new Output Job that writes every output to the default destination. `project.new`
/// and `output.job.add` both build their job with it.
pub fn new_job(uuid: Uuid, name: String, pcb: Uuid, fab_preset: Uuid) -> OutputJob {
    OutputJob {
        uuid,
        name,
        pcb,
        fab_preset,
        outputs: Outputs::ALL,
        destination: None,
        name_template: NameTemplate::default(),
        not_fitted_paste: NotFittedPaste::Keep,
        schematic_pdf_colour: PdfColour::Colour,
    }
}

/// What a Command that creates a job returns.
#[derive(Debug, Serialize, JsonSchema)]
pub struct JobCreated {
    /// The new job's UUID.
    pub uuid: Uuid,
    /// Its name.
    pub name: String,
}

/// Which outputs to write; each one left out keeps its current value (on for a new job).
#[derive(Clone, Copy, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OutputsParam {
    #[serde(default)]
    pub gerber: Option<bool>,
    #[serde(default)]
    pub drill: Option<bool>,
    #[serde(default)]
    pub bom: Option<bool>,
    #[serde(default)]
    pub cpl: Option<bool>,
    #[serde(default)]
    pub fab_notes: Option<bool>,
    #[serde(default)]
    pub drill_report: Option<bool>,
    #[serde(default)]
    pub gerber_job: Option<bool>,
    #[serde(default)]
    pub reports: Option<bool>,
    #[serde(default)]
    pub schematic_pdf: Option<bool>,
}

impl OutputsParam {
    fn apply(self, outputs: &mut Outputs) {
        let fields = [
            (self.gerber, &mut outputs.gerber),
            (self.drill, &mut outputs.drill),
            (self.bom, &mut outputs.bom),
            (self.cpl, &mut outputs.cpl),
            (self.fab_notes, &mut outputs.fab_notes),
            (self.drill_report, &mut outputs.drill_report),
            (self.gerber_job, &mut outputs.gerber_job),
            (self.reports, &mut outputs.reports),
            (self.schematic_pdf, &mut outputs.schematic_pdf),
        ];
        for (value, field) in fields {
            if let Some(value) = value {
                *field = value;
            }
        }
    }
}

fn job(project: &Project, uuid: Uuid) -> Result<OutputJob, CommandError> {
    project
        .output_job(uuid)
        .cloned()
        .ok_or_else(|| CommandError::not_found(format!("no Output Job has UUID {uuid}")))
}

fn check_preset(project: &Project, uuid: Uuid) -> Result<(), CommandError> {
    match project.fab_preset(uuid) {
        Some(_) => Ok(()),
        None => Err(CommandError::invalid_params(
            "/fab_preset",
            format!("the Project has no Fab Preset copy with UUID {uuid}"),
        )),
    }
}

fn check_pcb(project: &Project, uuid: Uuid) -> Result<(), CommandError> {
    match project.document(uuid) {
        Some(d) if d.kind == DocumentKind::Pcb => Ok(()),
        _ => Err(CommandError::invalid_params(
            "/pcb",
            format!("{uuid} is not a PCB Document of this Project"),
        )),
    }
}

/// A destination param: a `/`-separated folder inside the Project folder, or empty for the
/// default.
pub fn job_destination(value: String) -> Result<Option<String>, CommandError> {
    let value = value.trim().trim_end_matches('/').to_owned();
    if value.is_empty() {
        return Ok(None);
    }
    let escapes = value.starts_with('/')
        || value.contains('\\')
        || value.contains(':')
        || value.split('/').any(|part| part.is_empty() || part == "..");
    if escapes {
        return Err(CommandError::invalid_params(
            "/destination",
            "the destination must be a /-separated folder inside the Project folder",
        ));
    }
    // The same tokens as the name template; the run resolves them.
    let known = |name: &str| NAME_TEMPLATE_TOKENS.contains(&name).then(String::new);
    reda_model::text::resolve(&value, known).map_err(|e| {
        CommandError::invalid_params("/destination", format!("the destination: {e}"))
    })?;
    Ok(Some(value))
}

/// The value of each name template token for a run of `job` on `date` (`YYYY-MM-DD`),
/// with the Fab Preset copy named `preset`.
pub fn name_values<'a>(
    project: &'a Project,
    job: &'a OutputJob,
    preset: &'a str,
    date: &'a str,
) -> impl Fn(&str) -> Option<String> + 'a {
    move |token: &str| {
        Some(match token {
            "project" => project.props().name.clone(),
            "job" => job.name.clone(),
            "revision" => project.props().properties.revision.clone(),
            "date" => date.to_owned(),
            "preset" => preset.to_owned(),
            _ => return None,
        })
    }
}

/// The job's destination relative to the Project folder, `/`-separated, with the name
/// template's tokens resolved by `values`. An empty part (an unset revision) is dropped;
/// a value that would leave the Project folder is refused, with why.
pub fn resolve_destination(
    job: &OutputJob,
    values: impl Fn(&str) -> Option<String>,
) -> Result<String, String> {
    let raw = job.destination();
    let resolved = match &job.destination {
        Some(raw) => {
            reda_model::text::resolve(raw, values).map_err(|e| format!("the destination: {e}"))?
        }
        None => raw,
    };
    let parts: Vec<&str> = resolved.split('/').filter(|p| !p.is_empty()).collect();
    let escapes = parts.is_empty()
        || resolved.contains('\\')
        || resolved.contains(':')
        || parts.iter().any(|p| *p == ".." || *p == ".");
    if escapes {
        return Err(format!(
            "Output Job \"{}\": the destination \"{resolved}\" is not a folder inside the \
             Project folder",
            job.name
        ));
    }
    Ok(parts.join("/"))
}

/// The settings `output.job.add` and `output.job.set` share; each one left out keeps the
/// job's value.
struct JobSettings {
    outputs: Option<OutputsParam>,
    destination: Option<String>,
    name_template: Option<String>,
    not_fitted_paste: Option<NotFittedPaste>,
    schematic_pdf_colour: Option<PdfColour>,
}

impl JobSettings {
    fn apply(self, job: &mut OutputJob) -> Result<(), CommandError> {
        if let Some(outputs) = self.outputs {
            outputs.apply(&mut job.outputs);
        }
        if let Some(value) = self.destination {
            job.destination = job_destination(value)?;
        }
        if let Some(value) = self.name_template {
            job.name_template = NameTemplate::new(value).map_err(|reason| {
                CommandError::invalid_params("/name_template", reason.to_string())
            })?;
        }
        if let Some(value) = self.not_fitted_paste {
            job.not_fitted_paste = value;
        }
        if let Some(value) = self.schematic_pdf_colour {
            job.schematic_pdf_colour = value;
        }
        Ok(())
    }
}

/// A trimmed job name no other job (but `except`) has, compared case-insensitively.
fn free_name(project: &Project, name: &str, except: Option<Uuid>) -> Result<String, CommandError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(CommandError::invalid_params("/name", "the name is empty"));
    }
    if let Some(other) = project
        .output_jobs()
        .find(|j| j.name.eq_ignore_ascii_case(name) && Some(j.uuid) != except)
    {
        return Err(CommandError::invalid_params(
            "/name",
            format!("an Output Job named {:?} already exists", other.name),
        ));
    }
    Ok(name.to_owned())
}

/// `base`, or `base 2`, `base 3`… : the first name no job has.
fn unique_name(project: &Project, base: &str) -> String {
    let taken = |name: &str| {
        project
            .output_jobs()
            .any(|j| j.name.eq_ignore_ascii_case(name))
    };
    let mut name = base.to_owned();
    let mut n = 2;
    while taken(&name) {
        name = format!("{base} {n}");
        n += 1;
    }
    name
}

pub struct JobAdd;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct JobAddParams {
    /// The Project's copy of the Fab Preset the job uses (`fab_preset.add` makes one).
    pub fab_preset: Uuid,
    /// The job name; defaults to the preset's name, numbered if taken.
    #[serde(default)]
    pub name: Option<String>,
    /// The PCB Document to plot; defaults to the Project's PCB Document.
    #[serde(default)]
    pub pcb: Option<Uuid>,
    /// Which outputs to write; every output by default.
    #[serde(default)]
    pub outputs: Option<OutputsParam>,
    /// The output folder inside the Project folder; defaults to `outputs/<job name>`.
    #[serde(default)]
    pub destination: Option<String>,
    /// Names the zip and the BOM and CPL files, from `${project}`, `${job}`,
    /// `${revision}`, `${date}` and `${preset}`; defaults to `${project}-${job}`.
    #[serde(default)]
    pub name_template: Option<String>,
    /// Keep or remove Not Fitted Components' paste openings.
    #[serde(default)]
    pub not_fitted_paste: Option<NotFittedPaste>,
    /// The schematic PDF in colour or mono.
    #[serde(default)]
    pub schematic_pdf_colour: Option<PdfColour>,
}

impl Command for JobAdd {
    const META: Meta = Meta {
        id: "output.job.add",
        title: "New Output Job",
        description: "Adds an Output Job that plots a PCB Document with one of the Project's \
                      Fab Preset copies. It writes every output to `outputs/<job name>` \
                      unless told otherwise.",
        category: "output",
        icon: "output_job_add",
        kind: Kind::Edit,
    };
    type Params = JobAddParams;
    type Output = JobCreated;

    fn run(&self, cx: &mut Context<'_>, p: JobAddParams) -> Result<JobCreated, CommandError> {
        let project = cx.project()?;
        check_preset(project, p.fab_preset)?;
        let pcb = match p.pcb {
            Some(pcb) => {
                check_pcb(project, pcb)?;
                pcb
            }
            None => project.pcb().map(|pcb| pcb.uuid).ok_or_else(|| {
                CommandError::unavailable(
                    "the Project has no PCB Document; add one with `document.add {kind: pcb}`",
                )
            })?,
        };
        let name = match p.name {
            Some(name) => free_name(project, &name, None)?,
            None => {
                let preset = project.fab_preset(p.fab_preset).expect("checked above");
                unique_name(project, &preset.name)
            }
        };
        let mut job = new_job(cx.new_id(), name, pcb, p.fab_preset);
        JobSettings {
            outputs: p.outputs,
            destination: p.destination,
            name_template: p.name_template,
            not_fitted_paste: p.not_fitted_paste,
            schematic_pdf_colour: p.schematic_pdf_colour,
        }
        .apply(&mut job)?;
        let created = JobCreated {
            uuid: job.uuid,
            name: job.name.clone(),
        };
        cx.tx()?.insert(Object::OutputJob(job))?;
        Ok(created)
    }
}

pub struct JobSet;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct JobSetParams {
    /// The job's UUID.
    pub job: Uuid,
    #[serde(default)]
    pub name: Option<String>,
    /// The PCB Document to plot, for instance to repair a broken job.
    #[serde(default)]
    pub pcb: Option<Uuid>,
    /// Another of the Project's Fab Preset copies.
    #[serde(default)]
    pub fab_preset: Option<Uuid>,
    #[serde(default)]
    pub outputs: Option<OutputsParam>,
    /// The output folder inside the Project folder; empty for `outputs/<job name>`.
    #[serde(default)]
    pub destination: Option<String>,
    /// Names the zip and the BOM and CPL files, from `${project}`, `${job}`,
    /// `${revision}`, `${date}` and `${preset}`; defaults to `${project}-${job}`.
    #[serde(default)]
    pub name_template: Option<String>,
    /// Keep or remove Not Fitted Components' paste openings.
    #[serde(default)]
    pub not_fitted_paste: Option<NotFittedPaste>,
    /// The schematic PDF in colour or mono.
    #[serde(default)]
    pub schematic_pdf_colour: Option<PdfColour>,
}

impl Command for JobSet {
    const META: Meta = Meta {
        id: "output.job.set",
        title: "Edit Output Job",
        description: "Changes an Output Job's name, PCB Document, Fab Preset copy, outputs, \
                      destination, name template, Not Fitted paste or schematic PDF colour. \
                      Values left out are kept; an empty `destination` restores \
                      the default. Returns the job.",
        category: "output",
        icon: "output_job_edit",
        kind: Kind::Edit,
    };
    type Params = JobSetParams;
    type Output = OutputJob;

    fn run(&self, cx: &mut Context<'_>, p: JobSetParams) -> Result<OutputJob, CommandError> {
        let project = cx.project()?;
        let mut job = job(project, p.job)?;
        if let Some(name) = p.name {
            job.name = free_name(project, &name, Some(job.uuid))?;
        }
        if let Some(pcb) = p.pcb {
            check_pcb(project, pcb)?;
            job.pcb = pcb;
        }
        if let Some(preset) = p.fab_preset {
            check_preset(project, preset)?;
            job.fab_preset = preset;
        }
        JobSettings {
            outputs: p.outputs,
            destination: p.destination,
            name_template: p.name_template,
            not_fitted_paste: p.not_fitted_paste,
            schematic_pdf_colour: p.schematic_pdf_colour,
        }
        .apply(&mut job)?;
        cx.tx()?.replace(Object::OutputJob(job.clone()))?;
        Ok(job)
    }
}

pub struct JobDuplicate;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct JobRef {
    /// The job's UUID.
    pub job: Uuid,
}

impl Command for JobDuplicate {
    const META: Meta = Meta {
        id: "output.job.duplicate",
        title: "Duplicate Output Job",
        description: "Adds a copy of an Output Job named \"<name> copy\" (numbered if taken). \
                      A custom destination is not copied, so the two jobs write to different \
                      folders.",
        category: "output",
        icon: "duplicate",
        kind: Kind::Edit,
    };
    type Params = JobRef;
    type Output = JobCreated;

    fn run(&self, cx: &mut Context<'_>, p: JobRef) -> Result<JobCreated, CommandError> {
        let project = cx.project()?;
        let mut copy = job(project, p.job)?;
        copy.name = unique_name(project, &format!("{} copy", copy.name));
        copy.destination = None;
        copy.uuid = cx.new_id();
        let created = JobCreated {
            uuid: copy.uuid,
            name: copy.name.clone(),
        };
        cx.tx()?.insert(Object::OutputJob(copy))?;
        Ok(created)
    }
}

pub struct JobDelete;

impl Command for JobDelete {
    const META: Meta = Meta {
        id: "output.job.delete",
        title: "Delete Output Job",
        description: "Deletes an Output Job. Its Fab Preset copy and any files it wrote stay.",
        category: "output",
        icon: "delete",
        kind: Kind::Edit,
    };
    type Params = JobRef;
    type Output = ();

    fn run(&self, cx: &mut Context<'_>, p: JobRef) -> Result<(), CommandError> {
        job(cx.project()?, p.job)?;
        cx.tx()?.remove(p.job)?;
        Ok(())
    }
}
