//! The two crash dialogs: the recovery prompt (leftover Recovery Snapshots offered when a
//! Project opens) and the Crash Report offer on the next start. Drawing only: they return
//! what was clicked, and the window acts on it.

use std::path::PathBuf;

use egui::{Id, RichText};

use crate::recovery::{Choice, Prompt};
use crate::tr;

/// What the recovery prompt asked for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum RecoveryAction {
    Restore(String),
    Discard(String),
    ShowInFolder(PathBuf),
    RestoreAll,
    DiscardAll,
}

/// Draws the recovery prompt.
pub(crate) fn recovery(ctx: &egui::Context, prompt: &Prompt) -> Option<RecoveryAction> {
    let mut action = None;
    crate::modal::Dialog::new(Id::NULL.with("recovery-prompt"), tr!("recovery-title"))
        .width(560.0)
        .show(ctx, |ui| {
            ui.label(tr!("recovery-intro"));
            ui.separator();
            for row in &prompt.rows {
                let file = row.found.snapshot.file.clone();
                ui.group(|ui| {
                    ui.add_enabled_ui(row.can_restore(), |ui| {
                        ui.strong(row.title());
                        if row.found.snapshot.taken > 0 {
                            ui.weak(tr!("recovery-taken", time = utc(row.found.snapshot.taken)));
                        }
                    });
                    if let Some(error) = row.error() {
                        ui.colored_label(
                            ui.visuals().error_fg_color,
                            tr!("recovery-unreadable", error = error),
                        );
                    } else if row.found.disk_changed {
                        ui.colored_label(ui.visuals().warn_fg_color, tr!("recovery-disk-changed"));
                    } else if row.found.missing {
                        ui.label(tr!("recovery-missing"));
                    }
                    ui.horizontal(|ui| {
                        let emphasis = |text: String, on: bool| match on {
                            true => RichText::new(text).strong(),
                            false => RichText::new(text),
                        };
                        if row.can_restore() {
                            let restore =
                                emphasis(tr!("recovery-restore"), row.choice == Choice::Restore);
                            if ui.button(restore).clicked() {
                                action = Some(RecoveryAction::Restore(file.clone()));
                            }
                        }
                        let discard =
                            emphasis(tr!("recovery-discard"), row.choice == Choice::Discard);
                        if ui.button(discard).clicked() {
                            action = Some(RecoveryAction::Discard(file.clone()));
                        }
                        if !row.can_restore() && ui.button(tr!("recovery-show-in-folder")).clicked()
                        {
                            action = Some(RecoveryAction::ShowInFolder(prompt.dir.clone()));
                        }
                    });
                });
            }
            ui.separator();
            ui.horizontal(|ui| {
                let any = prompt.rows.iter().any(|r| r.choice == Choice::Restore);
                if ui
                    .add_enabled(any, egui::Button::new(tr!("recovery-restore-all")))
                    .clicked()
                {
                    action = Some(RecoveryAction::RestoreAll);
                }
                if ui.button(tr!("recovery-discard-all")).clicked() {
                    action = Some(RecoveryAction::DiscardAll);
                }
            });
        });
    action
}

/// What the Crash Report offer asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CrashAction {
    OpenFolder,
    ReportOnGitHub,
    Dismiss,
}

/// Draws the offer of the Crash Report at `report`.
pub(crate) fn crash(ctx: &egui::Context, report: &std::path::Path) -> Option<CrashAction> {
    let mut action = None;
    crate::modal::Dialog::new(Id::NULL.with("crash-report"), tr!("crash-title"))
        .width(480.0)
        .show(ctx, |ui| {
            ui.label(tr!("crash-intro"));
            ui.monospace(report.display().to_string());
            ui.horizontal(|ui| {
                if ui.button(tr!("crash-open-folder")).clicked() {
                    action = Some(CrashAction::OpenFolder);
                }
                if ui
                    .button(tr!("crash-report-github"))
                    .on_hover_text(tr!("crash-report-github-hint"))
                    .clicked()
                {
                    action = Some(CrashAction::ReportOnGitHub);
                }
                if ui.button(tr!("crash-dismiss")).clicked() {
                    action = Some(CrashAction::Dismiss);
                }
            });
        });
    action
}

/// `ms` since the Unix epoch as `YYYY-MM-DD HH:MM UTC`.
fn utc(ms: i64) -> String {
    let secs = ms.div_euclid(1000);
    let (days, rest) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // Howard Hinnant's days-to-civil algorithm.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02} UTC",
        rest / 3600,
        rest % 3600 / 60
    )
}
