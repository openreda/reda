//! The Start page: the Document area's tab while no Project is open. It lists recent
//! Projects (missing ones greyed out, each removable) and offers New and Open.

use std::path::PathBuf;

use egui::{Align, Label, Layout, Role, Ui, WidgetInfo};

use crate::shell::RecentProject;
use crate::theme::TokensExt as _;
use crate::tr;
use crate::widgets::{self, Glyph, Tooltip as _};

/// What the user asked for on the Start page this frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StartAction {
    NewProject,
    OpenProject,
    OpenRecent(PathBuf),
    RemoveRecent(PathBuf),
}

/// Shows the Start page; returns what the user clicked, if anything.
pub fn show(ui: &mut Ui, recent: &[RecentProject]) -> Option<StartAction> {
    let mut action = None;
    let t = ui.tokens();
    ui.add_space(t.space.section);
    ui.vertical_centered(|ui| {
        // Narrower when Docks leave the centre less room, so nothing is clipped.
        ui.set_max_width(ui.available_width().min(520.0));
        ui.with_layout(Layout::top_down(Align::Min), |ui| {
            let heading = tr!("start-page-heading");
            ui.label(t.ty.heading.rich(heading).color(t.color.text.primary));
            ui.add_space(t.space.section);
            ui.horizontal(|ui| {
                if ui.add(widgets::button(tr!("start-new-project"))).clicked() {
                    action = Some(StartAction::NewProject);
                }
                if ui.add(widgets::button(tr!("start-open-project"))).clicked() {
                    action = Some(StartAction::OpenProject);
                }
            });
            ui.add_space(t.space.section);
            let title = tr!("start-recent-projects");
            ui.label(t.ty.title.rich(title).color(t.color.text.primary));
            ui.add_space(t.space.stack);
            if recent.is_empty() {
                let none = tr!("start-no-recent-projects");
                ui.label(t.ty.body.rich(none).color(t.color.text.muted));
            }
            for project in recent {
                if let Some(a) = recent_row(ui, project) {
                    action = Some(a);
                }
            }
        });
    });
    action
}

fn recent_row(ui: &mut Ui, project: &RecentProject) -> Option<StartAction> {
    let mut action = None;
    let t = ui.tokens();
    ui.horizontal(|ui| {
        let open = ui
            .add_enabled(!project.missing, widgets::button(&project.name).flat())
            .tooltip(reda_engine::fs::display_path(&project.path))
            .disabled_tooltip(tr!("start-project-missing"));
        if open.clicked() {
            action = Some(StartAction::OpenRecent(project.path.clone()));
        }
        let location = project
            .path
            .parent()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        // The remove button is laid out first so a long location truncates instead of
        // pushing it out of the row.
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let name = project.name.clone();
            let remove = ui
                .add(widgets::glyph_button(Glyph::Close))
                .tooltip(tr!("start-remove-recent", name = name.as_str()));
            remove.widget_info(|| {
                WidgetInfo::labeled(
                    Role::Button,
                    true,
                    tr!("start-remove-recent", name = name.as_str()),
                )
            });
            if remove.clicked() {
                action = Some(StartAction::RemoveRecent(project.path.clone()));
            }
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                ui.add_enabled(
                    !project.missing,
                    Label::new(t.ty.body.rich(location).color(t.color.text.muted)).truncate(),
                );
            });
        });
    });
    action
}
