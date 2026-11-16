//! The Help menu's windows: the About box (with the Licences page showing the notices built
//! into the binary) and the result of a Check for Updates.

use egui::{Id, Ui};
use reda_engine::Engine;

use super::{NOTICES, UpdateCheck, take_checks, version};
use crate::settings::layout;
use crate::theme::TokensExt as _;
use crate::tr;
use crate::widgets::{self, Tooltip as _};

/// The open Help windows. The App keeps one and calls [`Dialogs::show`] every frame.
#[derive(Default)]
pub(crate) struct Dialogs {
    about: Option<AboutPage>,
    update: Option<Result<UpdateCheck, String>>,
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
        super::updates::repaint_with(engine, ctx);
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
    let modal = crate::modal::Dialog::new(Id::NULL.with("about"), tr!("about-title"))
        .width(600.0)
        .show(ctx, |ui| {
            let t = ui.tokens();
            let pages = [
                (AboutPage::About, tr!("about-tab-about")),
                (AboutPage::Licences, tr!("about-tab-licences")),
            ];
            ui.add(widgets::segmented(page, &pages));
            ui.add_space(t.space.section);
            match page {
                AboutPage::About => {
                    layout::text(ui, tr!("about-version", version = version));
                    ui.add_space(t.space.stack);
                    layout::text(ui, tr!("about-summary"));
                    ui.add_space(t.space.stack);
                    layout::text(ui, tr!("about-licence"));
                }
                AboutPage::Licences => licences(ui),
            }
            close_button(ui)
        });
    modal.inner || modal.should_close()
}

/// The notices, read-only but selectable for copying.
fn licences(ui: &mut Ui) {
    let label = layout::text(ui, tr!("about-notices"));
    egui::ScrollArea::vertical()
        .max_height(380.0)
        .show(ui, |ui| {
            ui.add(widgets::text_view(NOTICES).mono())
                .labelled_by(label.id);
        });
}

/// What a Check for Updates found. Returns whether it closed.
fn update(ctx: &egui::Context, check: &Result<UpdateCheck, String>) -> bool {
    let modal = crate::modal::Dialog::new(Id::NULL.with("update-check"), tr!("update-title"))
        .width(380.0)
        .show(ctx, |ui| {
            let check = match check {
                Ok(check) => check,
                Err(why) => {
                    layout::text(ui, tr!("update-failed", why = why.as_str()));
                    return close_button(ui);
                }
            };
            let latest = check.latest.as_str();
            let current = check.current.as_str();
            match &check.url {
                Some(url) => {
                    layout::text(
                        ui,
                        tr!("update-available", latest = latest, current = current),
                    );
                    let link = ui.add(widgets::link(tr!("update-open-release")));
                    if link.tooltip(url).clicked() {
                        ui.ctx().open_url(egui::OpenUrl::new_tab(url));
                    }
                }
                None if super::is_newer(current, latest) => {
                    layout::text(
                        ui,
                        tr!("update-current-dev", latest = latest, current = current),
                    );
                }
                None => {
                    layout::text(ui, tr!("update-current", latest = latest));
                }
            }
            close_button(ui)
        });
    modal.inner || modal.should_close()
}

fn close_button(ui: &mut Ui) -> bool {
    crate::modal::buttons(ui, |ui| {
        ui.add(widgets::button(tr!("help-close"))).clicked()
    })
}
