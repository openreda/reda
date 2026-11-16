//! The modals of a job run: the progress bar with Cancel while it runs, and the run
//! report (files written, blockers, warnings) when it ends, with Show in Folder.

use std::path::PathBuf;

use egui::{Align, Id, Layout, ScrollArea, Ui};

use super::{JobRunner, JobStatus, RunOutcome, RunReport};
use crate::settings::{self, layout};
use crate::theme::TokensExt as _;
use crate::tr;
use crate::widgets;

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
        let t = ui.tokens();
        ui.add(widgets::progress(step.fraction).named(run.job_name()));
        ui.add_space(t.space.stack);
        ui.horizontal(|ui| {
            // The percentage first, so a long message is cut rather than pushing it out.
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let percent = format!("{:.0}%", step.fraction.clamp(0.0, 1.0) * 100.0);
                ui.label(t.ty.numeric_small.rich(percent).color(t.color.text.muted));
                ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                    layout::muted(ui, &step.message);
                });
            });
        });
        crate::modal::buttons(ui, |ui| {
            let cancelling = run.cancelling();
            let label = if cancelling {
                tr!("output-job-cancelling")
            } else {
                tr!("dialog-cancel")
            };
            cancel = ui
                .add_enabled(!cancelling, widgets::button(label))
                .clicked();
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
                    layout::text(
                        ui,
                        tr!(
                            "output-job-written",
                            count = files.len(),
                            folder = destination.display().to_string()
                        ),
                    );
                    for file in files {
                        settings::mono(ui, &file.name);
                    }
                }
                RunOutcome::Blocked => {
                    layout::text(ui, tr!("output-job-nothing-written"));
                }
                RunOutcome::Cancelled => {
                    layout::text(ui, tr!("output-job-nothing-written"));
                }
                RunOutcome::Failed(message) => {
                    layout::error(ui, message);
                }
                RunOutcome::All { jobs, stopped } => {
                    for line in jobs {
                        ui.horizontal(|ui| {
                            let name = line.name.as_str();
                            match &line.status {
                                JobStatus::Written { destination } => {
                                    layout::text(ui, tr!("output-job-all-written", job = name));
                                    let show = widgets::button(tr!("output-job-show-zip"));
                                    let button = ui.add(show);
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
                                    layout::error(ui, tr!("output-job-all-blocked", job = name));
                                }
                                JobStatus::Failed(why) => {
                                    layout::error(
                                        ui,
                                        tr!(
                                            "output-job-all-failed",
                                            job = name,
                                            error = why.as_str()
                                        ),
                                    );
                                }
                                JobStatus::NotRun => {
                                    layout::muted(ui, tr!("output-job-all-not-run", job = name));
                                }
                            }
                        });
                    }
                    if *stopped {
                        layout::muted(ui, tr!("output-job-all-stopped"));
                    }
                }
            }
            list(ui, tr!("output-job-blockers"), &report.blockers, true);
            list(ui, tr!("output-job-warnings"), &report.warnings, false);
        });
        crate::modal::buttons(ui, |ui| {
            if ui.add(widgets::button(tr!("output-job-close"))).clicked() {
                action = Some(ReportAction::Close);
            }
            if let RunOutcome::Written { destination, .. } = &report.outcome
                && ui
                    .add(widgets::button(tr!("output-job-show-zip")))
                    .clicked()
            {
                action = Some(ReportAction::ShowFolder(destination.clone()));
            }
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
    let t = ui.tokens();
    ui.add_space(t.space.stack);
    settings::strong(ui, title);
    for line in lines {
        if error {
            layout::error(ui, line);
        } else {
            layout::warning(ui, line);
        }
    }
}
