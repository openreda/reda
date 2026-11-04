//! The Start page: the Document area's tab while no Project is open. It lists recent
//! Projects (missing ones greyed out, each removable) and offers New and Open.

use std::path::PathBuf;

use egui::{Align, Button, Label, Layout, RichText, Role, Ui, WidgetInfo};

use crate::shell::RecentProject;
use crate::tr;

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
    ui.add_space(24.0);
    ui.vertical_centered(|ui| {
        // Narrower when Docks leave the centre less room, so nothing is clipped.
        ui.set_max_width(ui.available_width().min(520.0));
        ui.with_layout(Layout::top_down(Align::Min), |ui| {
            ui.heading(tr!("start-page-heading"));
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui.button(tr!("start-new-project")).clicked() {
                    action = Some(StartAction::NewProject);
                }
                if ui.button(tr!("start-open-project")).clicked() {
                    action = Some(StartAction::OpenProject);
                }
            });
            ui.add_space(18.0);
            ui.label(RichText::new(tr!("start-recent-projects")).strong());
            ui.separator();
            if recent.is_empty() {
                ui.weak(tr!("start-no-recent-projects"));
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
    ui.horizontal(|ui| {
        let open = ui
            .add_enabled(!project.missing, Button::new(&project.name).frame(false))
            .on_hover_text(project.path.display().to_string())
            .on_disabled_hover_text(tr!("start-project-missing"));
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
                .add(Button::new("×").frame(false))
                .on_hover_text(tr!("start-remove-recent", name = name.as_str()));
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
                    Label::new(RichText::new(location).weak()).truncate(),
                );
            });
        });
    });
    action
}
