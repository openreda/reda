//! The Output Job dialog: one job's name, PCB Document, Fab Preset, outputs and
//! destination, applied with `output.job.set`, plus Run. A job whose PCB Document is gone
//! is broken: the PCB field is flagged until the user points it at another.

use egui::{Button, ComboBox, Role, Ui, WidgetInfo};
use reda_engine::model::{DocumentKind, PdfColour, Project, Uuid};
use reda_engine::output::job_destination;
use serde_json::{Map, Value, json};

use super::{Draft, Page, PageCx, Problem, SettingsDialog, Step, field, problem_label};
use crate::tr;

/// The Output Job dialog on `job`; `None` when the Project has no such job.
pub fn dialog(project: &Project, job: Uuid) -> Option<SettingsDialog> {
    let name = project.output_job(job)?.name.clone();
    Some(SettingsDialog::new(
        tr!("output-job-title", job = name.as_str()),
        vec![Box::new(JobPage::new(job))],
        project,
    ))
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

/// The dialog's one page: a draft of each of the job's settings.
#[derive(Clone, Debug)]
pub struct JobPage {
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
    /// The destination folder as typed; empty for the default.
    pub destination: Draft<String>,
    /// The job is no longer in the Project.
    gone: bool,
    /// The other jobs' names, which this one may not take.
    others: Vec<String>,
    /// The Project's PCB Documents, with their titles.
    pcbs: Vec<(Uuid, String)>,
    /// The Project's Fab Preset copies, with their names.
    presets: Vec<(Uuid, String)>,
    run: bool,
}

impl JobPage {
    pub fn new(job: Uuid) -> JobPage {
        JobPage {
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
            destination: Draft::default(),
            gone: false,
            others: Vec::new(),
            pcbs: Vec::new(),
            presets: Vec::new(),
            run: false,
        }
    }

    /// The job edited.
    pub fn job(&self) -> Uuid {
        self.job
    }

    /// The PCB Documents the job may plot.
    pub fn pcbs(&self) -> Vec<Uuid> {
        self.pcbs.iter().map(|(uuid, _)| *uuid).collect()
    }

    /// Whether the PCB Document picked is gone.
    pub fn broken(&self) -> bool {
        !self.pcbs.iter().any(|(uuid, _)| *uuid == *self.pcb.value())
    }

    /// Asks the window to apply the dialog and run the job.
    pub fn request_run(&mut self) {
        self.run = true;
    }

    /// The job, once, after the user asked to run it.
    pub fn take_run(&mut self) -> Option<Uuid> {
        std::mem::take(&mut self.run).then_some(self.job)
    }

    /// The default destination for the name typed.
    fn default_destination(&self) -> String {
        format!("outputs/{}", self.name.value().trim())
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
}

impl Page for JobPage {
    fn title(&self) -> String {
        tr!("output-job-page")
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
        let Some(job) = project.output_job(self.job) else {
            self.gone = true;
            return;
        };
        self.gone = false;
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
        self.destination
            .load(job.destination.clone().unwrap_or_default(), reset);
    }

    fn steps(&self) -> Vec<Step> {
        if self.gone {
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
        for (key, draft) in [
            ("gerber", &self.gerber),
            ("drill", &self.drill),
            ("bom", &self.bom),
            ("cpl", &self.cpl),
            ("fab_notes", &self.fab_notes),
            ("drill_report", &self.drill_report),
            ("gerber_job", &self.gerber_job),
            ("reports", &self.reports),
            ("schematic_pdf", &self.schematic_pdf),
        ] {
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
        if self.destination.changed() {
            params.insert("destination".into(), json!(self.destination.value().trim()));
        }
        if params.is_empty() {
            return Vec::new();
        }
        params.insert("job".into(), json!(self.job));
        vec![Step::new("output.job.set", Value::Object(params))]
    }

    fn problems(&self) -> Vec<Problem> {
        if self.gone {
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
        problems
    }

    fn show(&mut self, ui: &mut Ui, cx: &mut PageCx<'_>) {
        if let Some(problem) = cx.problem("job") {
            problem_label(ui, problem);
            return;
        }
        egui::Grid::new("output-job")
            .num_columns(2)
            .spacing([12.0, 8.0])
            .show(ui, |ui| {
                let label = tr!("output-job-name");
                ui.label(&label);
                field(ui, &label, self.name.value_mut(), 280.0, cx.problem("name"));
                ui.end_row();

                ui.label(tr!("output-job-pcb"));
                ui.vertical(|ui| {
                    let shown = self
                        .pcbs
                        .iter()
                        .find(|(uuid, _)| uuid == self.pcb.value())
                        .map_or_else(|| tr!("output-job-pcb-missing"), |(_, t)| t.clone());
                    let combo = ComboBox::from_id_salt("output-job-pcb")
                        .selected_text(shown)
                        .width(280.0)
                        .show_ui(ui, |ui| {
                            for (uuid, title) in &self.pcbs {
                                let mut picked = *self.pcb.value();
                                ui.selectable_value(&mut picked, *uuid, title);
                                self.pcb.set(picked);
                            }
                        });
                    labelled(&combo.response, tr!("output-job-pcb"));
                    if let Some(problem) = cx.problem("pcb") {
                        problem_label(ui, problem);
                    }
                });
                ui.end_row();

                ui.label(tr!("output-job-fab-preset"));
                let shown = self
                    .presets
                    .iter()
                    .find(|(uuid, _)| uuid == self.fab_preset.value())
                    .map(|(_, n)| n.clone())
                    .unwrap_or_default();
                let combo = ComboBox::from_id_salt("output-job-fab-preset")
                    .selected_text(shown)
                    .width(280.0)
                    .show_ui(ui, |ui| {
                        for (uuid, name) in &self.presets {
                            let mut picked = *self.fab_preset.value();
                            ui.selectable_value(&mut picked, *uuid, name);
                            self.fab_preset.set(picked);
                        }
                    });
                labelled(&combo.response, tr!("output-job-fab-preset"));
                ui.end_row();

                ui.label(tr!("output-job-outputs"));
                ui.vertical(|ui| {
                    for (label, draft) in [
                        (tr!("output-job-gerber"), &mut self.gerber),
                        (tr!("output-job-drill"), &mut self.drill),
                        (tr!("output-job-bom"), &mut self.bom),
                        (tr!("output-job-cpl"), &mut self.cpl),
                        (tr!("output-job-fab-notes"), &mut self.fab_notes),
                        (tr!("output-job-drill-report"), &mut self.drill_report),
                        (tr!("output-job-gerber-job"), &mut self.gerber_job),
                        (tr!("output-job-reports"), &mut self.reports),
                        (tr!("output-job-schematic-pdf"), &mut self.schematic_pdf),
                    ] {
                        ui.checkbox(draft.value_mut(), label);
                    }
                    ui.horizontal(|ui| {
                        ui.label(tr!("output-job-pdf-colour"));
                        let colour = self.schematic_pdf_colour.value_mut();
                        ui.radio_value(
                            colour,
                            PdfColour::Colour,
                            tr!("output-job-pdf-colour-colour"),
                        );
                        ui.radio_value(colour, PdfColour::Mono, tr!("output-job-pdf-colour-mono"));
                    });
                });
                ui.end_row();

                let label = tr!("output-job-destination");
                ui.label(&label);
                ui.vertical(|ui| {
                    let problem = cx.problem("destination");
                    field(ui, &label, self.destination.value_mut(), 280.0, problem);
                    if self.destination.value().trim().is_empty() {
                        let default = self.default_destination();
                        ui.weak(tr!("output-job-destination-default", folder = default));
                    }
                });
                ui.end_row();
            });
        ui.add_space(10.0);
        let can_run = self.problems().is_empty();
        let run = ui
            .add_enabled(can_run, Button::new(tr!("output-job-run")))
            .on_hover_text(tr!("output-job-run-hint"));
        if run.clicked() {
            self.request_run();
        }
    }
}

/// Announces a combo box as `label`.
fn labelled(response: &egui::Response, label: String) {
    response.widget_info(|| WidgetInfo::labeled(Role::ComboBox, true, label.clone()));
}
