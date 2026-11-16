//! The two crash dialogs: the recovery prompt (leftover Recovery Snapshots offered when a
//! Project opens) and the Crash Report offer on the next start. Drawing only: they return
//! what was clicked, and the window acts on it.

use std::path::PathBuf;

use egui::Id;

use crate::recovery::{Choice, Prompt};
use crate::settings::{self, layout};
use crate::theme::TokensExt as _;
use crate::tr;
use crate::widgets::{self, Tooltip as _};

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
            let t = ui.tokens();
            layout::text(ui, tr!("recovery-intro"));
            for row in &prompt.rows {
                let file = row.found.snapshot.file.clone();
                widgets::card_frame(&t).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.add_enabled_ui(row.can_restore(), |ui| {
                        settings::strong(ui, row.title());
                        if row.found.snapshot.taken > 0 {
                            let taken = utc(row.found.snapshot.taken);
                            layout::muted(ui, tr!("recovery-taken", time = taken));
                        }
                    });
                    if let Some(error) = row.error() {
                        layout::error(ui, tr!("recovery-unreadable", error = error));
                    } else if row.found.disk_changed {
                        layout::warning(ui, tr!("recovery-disk-changed"));
                    } else if row.found.missing {
                        layout::text(ui, tr!("recovery-missing"));
                    }
                    ui.horizontal(|ui| {
                        // The row's choice is the toggle that is on.
                        if row.can_restore() {
                            let restore = widgets::button(tr!("recovery-restore"))
                                .on(row.choice == Choice::Restore);
                            if ui.add(restore).clicked() {
                                action = Some(RecoveryAction::Restore(file.clone()));
                            }
                        }
                        let discard = widgets::button(tr!("recovery-discard"))
                            .on(row.choice == Choice::Discard);
                        if ui.add(discard).clicked() {
                            action = Some(RecoveryAction::Discard(file.clone()));
                        }
                        if !row.can_restore()
                            && ui
                                .add(widgets::button(tr!("recovery-show-in-folder")))
                                .clicked()
                        {
                            action = Some(RecoveryAction::ShowInFolder(prompt.dir.clone()));
                        }
                    });
                });
            }
            crate::modal::buttons(ui, |ui| {
                let any = prompt.rows.iter().any(|r| r.choice == Choice::Restore);
                if ui
                    .add_enabled(any, widgets::button(tr!("recovery-restore-all")).primary())
                    .clicked()
                {
                    action = Some(RecoveryAction::RestoreAll);
                }
                if ui
                    .add(widgets::button(tr!("recovery-discard-all")))
                    .clicked()
                {
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
            layout::text(ui, tr!("crash-intro"));
            settings::mono(ui, report.display().to_string());
            crate::modal::buttons(ui, |ui| {
                if ui
                    .add(widgets::button(tr!("crash-report-github")).primary())
                    .tooltip(tr!("crash-report-github-hint"))
                    .clicked()
                {
                    action = Some(CrashAction::ReportOnGitHub);
                }
                if ui.add(widgets::button(tr!("crash-dismiss"))).clicked() {
                    action = Some(CrashAction::Dismiss);
                }
                if ui.add(widgets::button(tr!("crash-open-folder"))).clicked() {
                    action = Some(CrashAction::OpenFolder);
                }
            });
        });
    action
}

/// `ms` since the Unix epoch as `YYYY-MM-DD HH:MM UTC`.
pub(crate) fn utc(ms: i64) -> String {
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
