//! The Output Job window: one job's pages General, Outputs, Gerber, Drill, BOM, CPL and
//! Last run, applied with `output.job.set`, plus Run, which applies and then runs the job
//! (`job_run`). A job whose PCB Document is gone is broken: the PCB field is flagged until
//! the user points it at another.
//!
//! The pages share one draft of the job ([`JobDraft`]); General owns loading it, its
//! Commands and its problems. The read-only pages preview what a run of the drafted job
//! would write, from the writers themselves (`reda_engine::output_preview`), and Last run
//! reads the destination's `manifest.json` ([`last_run`]).

use std::cell::{RefCell, RefMut};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::SystemTime;

use reda_engine::Engine;
use reda_engine::model::{
    DocumentKind, NameTemplate, NotFittedPaste, OutputJob, PdfColour, Project, Uuid,
};
use reda_engine::output::job_destination;
use reda_engine::output_preview::{JobPreview, job_preview};
use serde_json::{Map, Value, json};

use super::{Draft, Page, PageCx, Problem, SettingsDialog, Step};
use crate::settings::fab_preset::PresetPage;
use crate::tr;

mod draw;
pub mod last_run;

pub use draw::empty_columns;
pub use last_run::{FileCheck, LastRun, Manifest, ManifestFile};

/// The Output Job window on `job`; `None` when the Project has no such job.
pub fn dialog(project: &Project, job: Uuid) -> Option<SettingsDialog> {
    let name = project.output_job(job)?.name.clone();
    let draft = Rc::new(RefCell::new(JobDraft::new(job)));
    let pages: Vec<Box<dyn Page>> = Section::ALL
        .into_iter()
        .map(|section| {
            Box::new(JobPage {
                section,
                draft: draft.clone(),
            }) as Box<dyn Page>
        })
        .collect();
    let title = tr!("output-job-title", job = name.as_str());
    Some(SettingsDialog::new(title, pages, project).with_run())
}

/// The job an Output Job window edits.
pub fn job_of(dialog: &SettingsDialog) -> Option<Uuid> {
    dialog.page::<JobPage>().map(JobPage::job)
}

/// Whether `job` names a PCB Document the Project no longer has.
pub fn broken(project: &Project, job: Uuid) -> bool {
    project
        .output_job(job)
        .is_some_and(|j| !is_pcb(project, j.pcb))
}

fn is_pcb(project: &Project, document: Uuid) -> bool {
    project
        .document(document)
        .is_some_and(|d| d.kind == DocumentKind::Pcb)
}

/// The window's pages, in tree order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Section {
    General,
    Outputs,
    Gerber,
    Drill,
    Bom,
    Cpl,
    LastRun,
}

impl Section {
    pub const ALL: [Section; 7] = [
        Section::General,
        Section::Outputs,
        Section::Gerber,
        Section::Drill,
        Section::Bom,
        Section::Cpl,
        Section::LastRun,
    ];

    pub fn title(self) -> String {
        match self {
            Section::General => tr!("output-job-page-general"),
            Section::Outputs => tr!("output-job-page-outputs"),
            Section::Gerber => tr!("output-job-page-gerber"),
            Section::Drill => tr!("output-job-page-drill"),
            Section::Bom => tr!("output-job-page-bom"),
            Section::Cpl => tr!("output-job-page-cpl"),
            Section::LastRun => tr!("output-job-page-last-run"),
        }
    }

    /// The Fab Preset page its read-only values come from, for **Edit preset copy…**.
    pub fn preset_page(self) -> Option<PresetPage> {
        match self {
            Section::General => Some(PresetPage::Packaging),
            Section::Gerber => Some(PresetPage::Gerber),
            Section::Drill => Some(PresetPage::Drill),
            Section::Bom => Some(PresetPage::Bom),
            Section::Cpl => Some(PresetPage::Cpl),
            Section::Outputs | Section::LastRun => None,
        }
    }
}

/// One page of the window, on the job's shared draft.
pub struct JobPage {
    section: Section,
    draft: Rc<RefCell<JobDraft>>,
}

impl JobPage {
    pub fn section(&self) -> Section {
        self.section
    }

    /// The job's draft, shared by every page.
    pub fn draft(&self) -> RefMut<'_, JobDraft> {
        self.draft.borrow_mut()
    }

    /// The job edited.
    pub fn job(&self) -> Uuid {
        self.draft.borrow().job
    }

    /// The PCB Documents the job may plot.
    pub fn pcbs(&self) -> Vec<Uuid> {
        self.draft
            .borrow()
            .pcbs
            .iter()
            .map(|(uuid, _)| *uuid)
            .collect()
    }

    /// Whether the PCB Document picked is gone.
    pub fn broken(&self) -> bool {
        self.draft.borrow().broken()
    }

    /// Whether this page loads the draft and reports its Commands and problems.
    fn owns(&self) -> bool {
        self.section == Section::General
    }
}

impl Page for JobPage {
    fn title(&self) -> String {
        self.section.title()
    }

    fn load(&mut self, project: &Project, reset: bool) {
        if self.owns() {
            self.draft.borrow_mut().load(project, reset);
        }
    }

    fn steps(&self) -> Vec<Step> {
        if self.owns() {
            self.draft.borrow().steps()
        } else {
            Vec::new()
        }
    }

    fn problems(&self) -> Vec<Problem> {
        if self.owns() {
            self.draft.borrow().problems()
        } else {
            Vec::new()
        }
    }

    fn show(&mut self, ui: &mut egui::Ui, cx: &mut PageCx<'_>) {
        let section = self.section;
        draw::page(ui, cx, section, &mut self.draft.borrow_mut());
    }
}

/// The job's settings as edited, what the pages need from the Project, and the previews
/// worked out from them.
pub struct JobDraft {
    job: Uuid,
    pub name: Draft<String>,
    pub pcb: Draft<Uuid>,
    pub fab_preset: Draft<Uuid>,
    pub gerber: Draft<bool>,
    pub drill: Draft<bool>,
    pub bom: Draft<bool>,
    pub cpl: Draft<bool>,
    pub fab_notes: Draft<bool>,
    pub drill_report: Draft<bool>,
    pub gerber_job: Draft<bool>,
    pub reports: Draft<bool>,
    pub schematic_pdf: Draft<bool>,
    pub schematic_pdf_colour: Draft<PdfColour>,
    pub not_fitted_paste: Draft<NotFittedPaste>,
    /// The destination folder as typed; empty for the default.
    pub destination: Draft<String>,
    /// The name template as typed.
    pub name_template: Draft<String>,
    /// The job as the Project has it; `None` once it is deleted.
    saved: Option<OutputJob>,
    /// The other jobs' names, which this one may not take.
    others: Vec<String>,
    /// The Project's PCB Documents, with their titles.
    pcbs: Vec<(Uuid, String)>,
    /// The Project's Fab Preset copies, with their names.
    presets: Vec<(Uuid, String)>,
    /// The last preview, with what it was worked out from (the Journal's length and the
    /// drafted job).
    preview: Option<(PreviewKey, Option<JobPreview>)>,
    /// The last manifest read, with its path and modification time.
    last_run: Option<(PathBuf, Option<SystemTime>, LastRun)>,
}

type PreviewKey = (usize, OutputJob);

impl JobDraft {
    fn new(job: Uuid) -> JobDraft {
        JobDraft {
            job,
            name: Draft::default(),
            pcb: Draft::new(job),
            fab_preset: Draft::new(job),
            gerber: Draft::default(),
            drill: Draft::default(),
            bom: Draft::default(),
            cpl: Draft::default(),
            fab_notes: Draft::default(),
            drill_report: Draft::default(),
            gerber_job: Draft::default(),
            reports: Draft::default(),
            schematic_pdf: Draft::default(),
            schematic_pdf_colour: Draft::default(),
            not_fitted_paste: Draft::default(),
            destination: Draft::default(),
            name_template: Draft::default(),
            saved: None,
            others: Vec::new(),
            pcbs: Vec::new(),
            presets: Vec::new(),
            preview: None,
            last_run: None,
        }
    }

    pub fn job(&self) -> Uuid {
        self.job
    }

    /// Whether the PCB Document picked is gone.
    pub fn broken(&self) -> bool {
        !self.pcbs.iter().any(|(uuid, _)| *uuid == *self.pcb.value())
    }

    /// The default destination for the name typed.
    fn default_destination(&self) -> String {
        format!("outputs/{}", self.name.value().trim())
    }

    fn load(&mut self, project: &Project, reset: bool) {
        self.pcbs = project
            .documents()
            .filter(|d| d.kind == DocumentKind::Pcb)
            .map(|d| (d.uuid, d.title.clone()))
            .collect();
        self.presets = project
            .fab_presets()
            .map(|p| (p.uuid, p.name.clone()))
            .collect();
        self.others = project
            .output_jobs()
            .filter(|j| j.uuid != self.job)
            .map(|j| j.name.clone())
            .collect();
        self.saved = project.output_job(self.job).cloned();
        let Some(job) = &self.saved else {
            return;
        };
        self.name.load(job.name.clone(), reset);
        self.pcb.load(job.pcb, reset);
        self.fab_preset.load(job.fab_preset, reset);
        self.gerber.load(job.outputs.gerber, reset);
        self.drill.load(job.outputs.drill, reset);
        self.bom.load(job.outputs.bom, reset);
        self.cpl.load(job.outputs.cpl, reset);
        self.fab_notes.load(job.outputs.fab_notes, reset);
        self.drill_report.load(job.outputs.drill_report, reset);
        self.gerber_job.load(job.outputs.gerber_job, reset);
        self.reports.load(job.outputs.reports, reset);
        self.schematic_pdf.load(job.outputs.schematic_pdf, reset);
        self.schematic_pdf_colour
            .load(job.schematic_pdf_colour, reset);
        self.not_fitted_paste.load(job.not_fitted_paste, reset);
        self.destination
            .load(job.destination.clone().unwrap_or_default(), reset);
        self.name_template
            .load(job.name_template.as_str().to_owned(), reset);
    }

    fn output_drafts(&self) -> [(&'static str, &Draft<bool>); 9] {
        [
            ("gerber", &self.gerber),
            ("drill", &self.drill),
            ("bom", &self.bom),
            ("cpl", &self.cpl),
            ("fab_notes", &self.fab_notes),
            ("drill_report", &self.drill_report),
            ("gerber_job", &self.gerber_job),
            ("reports", &self.reports),
            ("schematic_pdf", &self.schematic_pdf),
        ]
    }

    fn steps(&self) -> Vec<Step> {
        if self.saved.is_none() {
            return Vec::new();
        }
        let mut params = Map::new();
        if self.name.changed() {
            params.insert("name".into(), json!(self.name.value().trim()));
        }
        if self.pcb.changed() {
            params.insert("pcb".into(), json!(self.pcb.value()));
        }
        if self.fab_preset.changed() {
            params.insert("fab_preset".into(), json!(self.fab_preset.value()));
        }
        let mut outputs = Map::new();
        for (key, draft) in self.output_drafts() {
            if draft.changed() {
                outputs.insert(key.into(), json!(draft.value()));
            }
        }
        if !outputs.is_empty() {
            params.insert("outputs".into(), Value::Object(outputs));
        }
        if self.schematic_pdf_colour.changed() {
            params.insert(
                "schematic_pdf_colour".into(),
                json!(self.schematic_pdf_colour.value()),
            );
        }
        if self.not_fitted_paste.changed() {
            params.insert(
                "not_fitted_paste".into(),
                json!(self.not_fitted_paste.value()),
            );
        }
        if self.destination.changed() {
            params.insert("destination".into(), json!(self.destination.value().trim()));
        }
        if self.name_template.changed() {
            params.insert("name_template".into(), json!(self.name_template.value()));
        }
        if params.is_empty() {
            return Vec::new();
        }
        params.insert("job".into(), json!(self.job));
        vec![Step::new("output.job.set", Value::Object(params))]
    }

    fn name_problem(&self) -> Option<Problem> {
        let name = self.name.value().trim();
        let message = if name.is_empty() {
            tr!("output-job-name-blank")
        } else if self.others.iter().any(|o| o.eq_ignore_ascii_case(name)) {
            tr!("output-job-name-taken", name = name)
        } else {
            return None;
        };
        Some(Problem::new("name", message))
    }

    fn problems(&self) -> Vec<Problem> {
        if self.saved.is_none() {
            return vec![Problem::new("job", tr!("output-job-gone"))];
        }
        let mut problems: Vec<Problem> = self.name_problem().into_iter().collect();
        if self.broken() {
            problems.push(Problem::new("pcb", tr!("output-job-broken")));
        }
        if job_destination(self.destination.value().clone()).is_err() {
            problems.push(Problem::new(
                "destination",
                tr!("output-job-destination-invalid"),
            ));
        }
        if let Err(e) = NameTemplate::new(self.name_template.value().clone()) {
            problems.push(Problem::new("name_template", e.to_string()));
        }
        problems
    }

    /// The job as drafted: the Project's job with every valid draft in it. A value that
    /// is invalid keeps the Project's.
    pub fn drafted(&self) -> Option<OutputJob> {
        let mut job = self.saved.clone()?;
        let name = self.name.value().trim();
        if !name.is_empty() {
            job.name = name.to_owned();
        }
        job.pcb = *self.pcb.value();
        job.fab_preset = *self.fab_preset.value();
        let o = &mut job.outputs;
        o.gerber = *self.gerber.value();
        o.drill = *self.drill.value();
        o.bom = *self.bom.value();
        o.cpl = *self.cpl.value();
        o.fab_notes = *self.fab_notes.value();
        o.drill_report = *self.drill_report.value();
        o.gerber_job = *self.gerber_job.value();
        o.reports = *self.reports.value();
        o.schematic_pdf = *self.schematic_pdf.value();
        job.schematic_pdf_colour = *self.schematic_pdf_colour.value();
        job.not_fitted_paste = *self.not_fitted_paste.value();
        if let Ok(destination) = job_destination(self.destination.value().clone()) {
            job.destination = destination;
        }
        if let Ok(template) = NameTemplate::new(self.name_template.value().clone()) {
            job.name_template = template;
        }
        Some(job)
    }

    /// What a run of the drafted job would write; worked out again only when the job or
    /// the Project changed.
    pub fn preview(&mut self, engine: &Engine) -> Option<&JobPreview> {
        let job = self.drafted()?;
        let key = (engine.journal().len(), job);
        if self.preview.as_ref().map(|(k, _)| k) != Some(&key) {
            let preview = job_preview(engine, &key.1);
            self.preview = Some((key, preview));
        }
        self.preview.as_ref().and_then(|(_, p)| p.as_ref())
    }

    /// A warning about the resolved name: one with non-ASCII characters, which some fabs
    /// refuse.
    pub fn name_warning(&mut self, engine: &Engine) -> Option<String> {
        let stem = self.preview(engine)?.stem.as_ref().ok()?;
        (!stem.is_ascii()).then(|| tr!("output-job-name-template-ascii"))
    }

    /// The drafted job's destination folder, resolved, in the Project folder.
    pub fn destination_folder(&mut self, engine: &Engine) -> Option<PathBuf> {
        let folder = engine.location()?.to_path_buf();
        let destination = self.preview(engine)?.destination.clone().ok()?;
        Some(
            destination
                .split('/')
                .fold(folder, |path, part| path.join(part)),
        )
    }

    /// The last run in the destination, read from its `manifest.json` again whenever the
    /// file changes.
    pub fn last_run(&mut self, engine: &Engine) -> LastRun {
        let Some(folder) = self.destination_folder(engine) else {
            return LastRun::NotRun;
        };
        let manifest = folder.join(last_run::MANIFEST);
        let modified = std::fs::metadata(&manifest).and_then(|m| m.modified()).ok();
        let fresh = matches!(&self.last_run,
            Some((path, time, _)) if *path == manifest && *time == modified);
        if !fresh {
            let read = last_run::read(&folder);
            self.last_run = Some((manifest, modified, read));
        }
        self.last_run
            .as_ref()
            .map_or(LastRun::NotRun, |(_, _, run)| run.clone())
    }

    /// The picked PCB Document's title.
    fn pcb_title(&self) -> String {
        self.pcbs
            .iter()
            .find(|(uuid, _)| uuid == self.pcb.value())
            .map_or_else(|| tr!("output-job-pcb-missing"), |(_, t)| t.clone())
    }

    /// The picked Fab Preset copy's name.
    fn preset_name(&self) -> String {
        self.presets
            .iter()
            .find(|(uuid, _)| uuid == self.fab_preset.value())
            .map(|(_, n)| n.clone())
            .unwrap_or_default()
    }
}
