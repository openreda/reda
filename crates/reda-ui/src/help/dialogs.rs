//! The Help menu's windows: the About box (with the Licences page showing the notices built
//! into the binary) and the result of a Check for Updates.

use egui::{Id, Modal, TextEdit, TextStyle, Ui};
use reda_engine::Engine;

use super::{NOTICES, UpdateCheck, take_checks, version};
use crate::tr;

/// The open Help windows. The App keeps one and calls [`Dialogs::show`] every frame.
#[derive(Default)]
pub(crate) struct Dialogs {
    about: Option<AboutPage>,
    update: Option<UpdateCheck>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AboutPage {
    About,
    Licences,
}

impl Dialogs {
    pub(crate) fn open_about(&mut self) {
        self.about = Some(AboutPage::About);
    }

    /// Draws the open windows; a check `app.update.check` ran since the last frame opens
    /// its result.
    pub(crate) fn show(&mut self, ctx: &egui::Context, engine: &Engine) {
        if let Some(check) = take_checks(engine).pop() {
            self.update = Some(check);
        }
        if let Some(page) = &mut self.about
            && about(ctx, page, &version(engine))
        {
            self.about = None;
        }
        if let Some(check) = &self.update
            && update(ctx, check)
        {
            self.update = None;
        }
    }
}

/// The About box. Returns whether it closed.
fn about(ctx: &egui::Context, page: &mut AboutPage, version: &str) -> bool {
    let modal = Modal::new(Id::NULL.with("about")).show(ctx, |ui| {
        ui.set_width(600.0);
        ui.heading(tr!("about-title"));
        ui.horizontal(|ui| {
            ui.selectable_value(page, AboutPage::About, tr!("about-tab-about"));
            ui.selectable_value(page, AboutPage::Licences, tr!("about-tab-licences"));
        });
        ui.separator();
        match page {
            AboutPage::About => {
                ui.label(tr!("about-version", version = version));
                ui.add_space(6.0);
                ui.label(tr!("about-summary"));
                ui.add_space(6.0);
                ui.label(tr!("about-licence"));
            }
            AboutPage::Licences => licences(ui),
        }
        ui.add_space(8.0);
        close_button(ui)
    });
    modal.inner || modal.should_close()
}

/// The notices, read-only but selectable for copying.
fn licences(ui: &mut Ui) {
    let label = ui.label(tr!("about-notices"));
    egui::ScrollArea::vertical()
        .max_height(380.0)
        .show(ui, |ui| {
            let mut text = NOTICES;
            ui.add(
                TextEdit::multiline(&mut text)
                    .font(TextStyle::Monospace)
                    .desired_width(f32::INFINITY),
            )
            .labelled_by(label.id);
        });
}

/// What a Check for Updates found. Returns whether it closed.
fn update(ctx: &egui::Context, check: &UpdateCheck) -> bool {
    let modal = Modal::new(Id::NULL.with("update-check")).show(ctx, |ui| {
        ui.set_width(380.0);
        ui.heading(tr!("update-title"));
        ui.add_space(6.0);
        let latest = check.latest.as_str();
        let current = check.current.as_str();
        match &check.url {
            Some(url) => {
                ui.label(tr!("update-available", latest = latest, current = current));
                ui.hyperlink_to(tr!("update-open-release"), url);
            }
            None if super::is_newer(current, latest) => {
                ui.label(tr!(
                    "update-current-dev",
                    latest = latest,
                    current = current
                ));
            }
            None => {
                ui.label(tr!("update-current", latest = latest));
            }
        }
        ui.add_space(8.0);
        close_button(ui)
    });
    modal.inner || modal.should_close()
}

fn close_button(ui: &mut Ui) -> bool {
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
        ui.button(tr!("help-close")).clicked()
    })
    .inner
}
