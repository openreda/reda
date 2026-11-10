//! The modals of a job run: the progress bar with Cancel while it runs, and the run
//! report (files written, blockers, warnings) when it ends, with Show in Folder.

use std::path::PathBuf;

use egui::{Button, Id, ProgressBar, RichText, ScrollArea, Ui};

use super::{JobRunner, JobStatus, RunOutcome, RunReport};
use crate::tr;

/// What the user did in the report.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReportAction {
    /// Show a job's destination folder in the OS file manager.
    ShowFolder(PathBuf),
    Close,
}

/// Draws the progress modal. Returns true when the user pressed Cancel.
pub fn show_progress(ctx: &egui::Context, run: &JobRunner) -> bool {
    let mut cancel = false;
    let step = run.step();
    crate::modal::Dialog::new(
        Id::NULL.with("job-run-progress"),
        tr!("output-job-running", job = run.job_name()),
    )
    .width(380.0)
    .show(ctx, |ui| {
        ui.add_space(6.0);
        ui.add(ProgressBar::new(step.fraction).show_percentage());
        ui.weak(&step.message);
        ui.add_space(6.0);
        // A row as tall as its buttons: a right-to-left layout of its own would centre
        // them in the whole height the dialog offers.
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let cancelling = run.cancelling();
                let label = if cancelling {
                    tr!("output-job-cancelling")
                } else {
                    tr!("dialog-cancel")
                };
                cancel = ui.add_enabled(!cancelling, Button::new(label)).clicked();
            })
        });
    });
    cancel
}

/// Draws the run report.
pub fn show_report(ctx: &egui::Context, report: &RunReport) -> Option<ReportAction> {
    let mut action = None;
    let job = report.job_name.as_str();
    let heading = match &report.outcome {
        RunOutcome::Written { .. } => tr!("output-job-done", job = job),
        RunOutcome::Blocked => tr!("output-job-blocked", job = job),
        RunOutcome::Cancelled => tr!("output-job-cancelled", job = job),
        RunOutcome::Failed(_) => tr!("output-job-failed", job = job),
        RunOutcome::All { .. } => tr!("output-job-all-done"),
    };
    let dialog = crate::modal::Dialog::new(Id::NULL.with("job-run-report"), heading).width(460.0);
    let modal = dialog.show(ctx, |ui| {
        ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
            match &report.outcome {
                RunOutcome::Written {
                    destination, files, ..
                } => {
                    ui.label(tr!(
                        "output-job-written",
                        count = files.len(),
                        folder = destination.display().to_string()
                    ));
                    for file in files {
                        ui.monospace(&file.name);
                    }
                }
                RunOutcome::Blocked => {
                    ui.label(tr!("output-job-nothing-written"));
                }
                RunOutcome::Cancelled => {
                    ui.label(tr!("output-job-nothing-written"));
                }
                RunOutcome::Failed(message) => {
                    ui.colored_label(ui.visuals().error_fg_color, message);
                }
                RunOutcome::All { jobs, stopped } => {
                    for line in jobs {
                        ui.horizontal(|ui| {
                            let name = line.name.as_str();
                            match &line.status {
                                JobStatus::Written { destination } => {
                                    ui.label(tr!("output-job-all-written", job = name));
                                    let button = ui.button(tr!("output-job-show-zip"));
                                    let label = tr!("output-job-all-show", job = name);
                                    button.widget_info(|| {
                                        egui::WidgetInfo::labeled(
                                            egui::Role::Button,
                                            true,
                                            label.clone(),
                                        )
                                    });
                                    if button.clicked() {
                                        action =
                                            Some(ReportAction::ShowFolder(destination.clone()));
                                    }
                                }
                                JobStatus::Blocked => {
                                    ui.colored_label(
                                        ui.visuals().error_fg_color,
                                        tr!("output-job-all-blocked", job = name),
                                    );
                                }
                                JobStatus::Failed(why) => {
                                    ui.colored_label(
                                        ui.visuals().error_fg_color,
                                        tr!(
                                            "output-job-all-failed",
                                            job = name,
                                            error = why.as_str()
                                        ),
                                    );
                                }
                                JobStatus::NotRun => {
                                    ui.weak(tr!("output-job-all-not-run", job = name));
                                }
                            }
                        });
                    }
                    if *stopped {
                        ui.weak(tr!("output-job-all-stopped"));
                    }
                }
            }
            list(ui, tr!("output-job-blockers"), &report.blockers, true);
            list(ui, tr!("output-job-warnings"), &report.warnings, false);
        });
        ui.add_space(8.0);
        // A row as tall as its buttons: a right-to-left layout of its own would centre
        // them in the whole height the dialog offers.
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(tr!("output-job-close")).clicked() {
                    action = Some(ReportAction::Close);
                }
                if let RunOutcome::Written { destination, .. } = &report.outcome
                    && ui.button(tr!("output-job-show-zip")).clicked()
                {
                    action = Some(ReportAction::ShowFolder(destination.clone()));
                }
            })
        });
    });
    if action.is_none() && modal.should_close() {
        action = Some(ReportAction::Close);
    }
    action
}

/// A titled list of lines, in the error or warning colour; nothing when empty.
fn list(ui: &mut Ui, title: String, lines: &[String], error: bool) {
    if lines.is_empty() {
        return;
    }
    ui.add_space(6.0);
    ui.strong(title);
    let colour = if error {
        ui.visuals().error_fg_color
    } else {
        ui.visuals().warn_fg_color
    };
    for line in lines {
        ui.label(RichText::new(line).color(colour));
    }
}
