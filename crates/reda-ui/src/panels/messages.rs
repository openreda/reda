//! The Messages panel: ERC and DRC results and load errors and warnings, grouped by
//! Document, check or severity, with sorting and severity chips; the selected line's
//! details pane under the list. Clicking a Violation goes to it, double-clicking opens
//! Violation Details, Mod+click picks more lines for Waive, and right-click offers
//! Details, Waive, Unwaive, Set severity, Run this check only and Show rule. Drawing
//! only: every click becomes an [`Action`] the window applies after the frame.

use egui::{Role, ScrollArea, Ui, WidgetInfo};
use reda_engine::model::{Severity, Uuid};

use crate::layout::{PanelId, SettingsWindow};
use crate::messages::{CheckKind, GroupBy, Line, LineKey, SortBy, check_title};
use crate::shell::Shell;
use crate::theme::TokensExt as _;
use crate::tr;
use crate::widgets::{self, Status};

/// What a click in the panel, Violation Details or the Waive window asks for.
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Run(CheckKind),
    ShowWaived(bool),
    GoTo(LineKey),
    /// Mod+click: pick the line too, or unpick it.
    Pick(LineKey),
    Details(LineKey),
    CloseDetails,
    /// Opens the Waive window on the line, with every picked line when it is one of them.
    Waive(LineKey),
    WaiveReason(String),
    /// Closes the Waive window, waiving when true.
    FinishWaive(bool),
    Unwaive(LineKey),
    SetSeverity(LineKey, Severity),
    RunOnly(LineKey),
    ShowRule(LineKey),
    GroupBy(GroupBy),
    Sort(SortBy),
    Chip(Severity, bool),
    /// Goes to one object, in a Document.
    GoToObject(Option<Uuid>, Uuid),
    Highlight(LineKey),
    Jump(LineKey),
    SetReason(LineKey, String),
    /// Waives the line's Violation with this reason, from Violation Details.
    WaiveWith(LineKey, String),
    /// Opens a settings window on the page with this title (a Rules row in Properties).
    ShowPage(SettingsWindow, String),
}

/// Applies `action` to the shell. Errors go to the status bar. Show rule returns the
/// settings window and the title of the page to open, which the window opens.
pub fn apply(shell: &mut Shell, action: Action) -> Option<(SettingsWindow, String)> {
    let _ = match action {
        Action::Run(kind) => shell.run_check(kind),
        Action::ShowWaived(show) => shell.set_show_waived(show).map(|_| ()),
        Action::GoTo(key) => {
            shell.go_to(key);
            Ok(())
        }
        Action::Pick(key) => {
            shell.toggle_picked(key);
            Ok(())
        }
        Action::Details(key) => {
            shell.open_details(key);
            Ok(())
        }
        Action::CloseDetails => {
            shell.close_details();
            Ok(())
        }
        Action::Waive(key) => {
            let picked = shell.messages().picked();
            let keys = if picked.contains(&key) {
                picked
            } else {
                vec![key]
            };
            shell.start_waive(keys);
            Ok(())
        }
        Action::WaiveReason(reason) => {
            shell.set_waive_reason(reason);
            Ok(())
        }
        Action::FinishWaive(confirm) => shell.finish_waive(confirm),
        Action::Unwaive(key) => shell.unwaive(key),
        Action::SetSeverity(key, severity) => shell.set_severity(key, severity),
        Action::RunOnly(key) => match shell.violation_of(key) {
            Some((kind, v)) => shell.run_check_only(kind, &v.check),
            None => Ok(()),
        },
        Action::ShowRule(key) => return rule_page(shell, key),
        Action::ShowPage(window, page) => return Some((window, page)),
        Action::GroupBy(g) => {
            shell.set_group_by(g);
            Ok(())
        }
        Action::Sort(s) => {
            shell.set_sort(s);
            Ok(())
        }
        Action::Chip(severity, shown) => {
            shell.set_severity_shown(severity, shown);
            Ok(())
        }
        Action::GoToObject(document, object) => {
            shell.go_to_object(document, object);
            Ok(())
        }
        Action::Highlight(key) => shell.highlight_violation(key),
        Action::Jump(key) => shell.jump_to_violation(key),
        Action::SetReason(key, reason) => shell.set_waiver_reason(key, &reason),
        Action::WaiveWith(key, reason) => shell.waive_many(&[key], reason.trim()),
    };
    None
}

/// Where Show rule goes for a line's check ([`crate::messages::rule_page`]).
fn rule_page(shell: &Shell, key: LineKey) -> Option<(SettingsWindow, String)> {
    let (kind, v) = shell.violation_of(key)?;
    Some(crate::messages::rule_page(kind, &v.check))
}

/// Draws the panel into `ui`.
pub fn show(ui: &mut Ui, shell: &Shell) -> Vec<Action> {
    contents(ui, shell)
}

/// The height the details pane under the list opens at.
const DETAILS_HEIGHT: f32 = 64.0;

fn contents(ui: &mut Ui, shell: &Shell) -> Vec<Action> {
    let mut actions = Vec::new();
    toolbar(ui, shell, &mut actions);
    for kind in CheckKind::ALL {
        if shell.messages().run(kind).is_some_and(|r| r.stale) {
            let text = match kind {
                CheckKind::Erc => tr!("messages-erc-stale"),
                CheckKind::Drc => tr!("messages-drc-stale"),
            };
            ui.colored_label(ui.tokens().color.warning.text, text);
        }
    }
    ui.separator();
    // The details pane keeps its strip at the bottom; the list fills the rest.
    egui::Panel::bottom(egui::Id::NULL.with("messages-details-pane"))
        .resizable(true)
        .default_size(DETAILS_HEIGHT)
        .show(ui, |ui| details_pane(ui, shell, &mut actions));
    let groups = shell.message_groups();
    if groups.is_empty() {
        ui.weak(tr!("messages-none"));
        return actions;
    }
    let picked = shell.messages().picked();
    let focused = shell.panel_focused(PanelId::Messages);
    ScrollArea::vertical()
        .id_salt("messages-list")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            widgets::rows(ui, |ui| {
                for group in &groups {
                    let title = tr!(
                        "messages-group",
                        document = group.title.as_str(),
                        count = group.lines.len()
                    );
                    widgets::section(title)
                        .id_salt(("messages-group", &group.kind))
                        .default_open(!group.collapsed)
                        .show(ui, |ui| {
                            for line in &group.lines {
                                let on = picked.contains(&line.key);
                                let look = Look {
                                    picked: on,
                                    focused,
                                };
                                row(ui, shell, line, look, &mut actions);
                            }
                        });
                }
            });
        });
    actions
}

fn toolbar(ui: &mut Ui, shell: &Shell, actions: &mut Vec<Action>) {
    let open = shell.engine().project();
    ui.horizontal_wrapped(|ui| {
        let erc = open.is_some();
        if ui
            .add_enabled(erc, widgets::button(tr!("messages-run-erc")))
            .clicked()
        {
            actions.push(Action::Run(CheckKind::Erc));
        }
        let drc = open.and_then(|p| p.pcb()).is_some()
            && shell.engine().registry().get("drc.run").is_some();
        if ui
            .add_enabled(drc, widgets::button(tr!("messages-run-drc")))
            .clicked()
        {
            actions.push(Action::Run(CheckKind::Drc));
        }
        let messages = shell.messages();
        let mut show_waived = messages.show_waived();
        let waived = widgets::checkbox(&mut show_waived, tr!("messages-show-waived"));
        if ui.add(waived).changed() {
            actions.push(Action::ShowWaived(show_waived));
        }
        ui.separator();
        // The severity chips: each counts its lines, and shows or hides them.
        let (errors, warnings) = messages.counts();
        for (severity, status, text) in [
            (
                Severity::Error,
                Status::Error,
                tr!("messages-chip-errors", count = errors),
            ),
            (
                Severity::Warning,
                Status::Warning,
                tr!("messages-chip-warnings", count = warnings),
            ),
        ] {
            let on = messages.severity_shown(severity);
            let chip = widgets::badge(text).status(status).filter(on);
            if ui.add(chip).clicked() {
                actions.push(Action::Chip(severity, !on));
            }
        }
        ui.separator();
        let group_by = messages.group_by();
        let text = tr!("messages-group-by", by = group_by.label());
        let response = widgets::dropdown("messages-group-by", text)
            .show(ui, |ui| {
                for g in GroupBy::ALL {
                    if ui.add(widgets::choice(g == group_by, g.label())).clicked() {
                        actions.push(Action::GroupBy(g));
                    }
                }
            })
            .response;
        let label = tr!("messages-group-by-label");
        response.widget_info(|| WidgetInfo::labeled(Role::ComboBox, true, label.clone()));
        let sort = messages.sort();
        let text = tr!("messages-sort", by = sort.label());
        let response = widgets::dropdown("messages-sort", text)
            .show(ui, |ui| {
                for s in SortBy::ALL {
                    if ui.add(widgets::choice(s == sort, s.label())).clicked() {
                        actions.push(Action::Sort(s));
                    }
                }
            })
            .response;
        let label = tr!("messages-sort-label");
        response.widget_info(|| WidgetInfo::labeled(Role::ComboBox, true, label.clone()));
    });
}

/// How a line's row shows: picked, and whether the panel holds keyboard focus.
#[derive(Clone, Copy)]
struct Look {
    picked: bool,
    focused: bool,
}

/// One line: its severity as a mark and the message. A waived or stale line is muted,
/// with the muted mark.
fn row(ui: &mut Ui, shell: &Shell, line: &Line, look: Look, actions: &mut Vec<Action>) {
    let off = line.waived || line.stale;
    let (severity, status) = match line.severity {
        Severity::Error => (tr!("severity-error"), Status::Error),
        _ => (tr!("severity-warning"), Status::Warning),
    };
    let status = if off { Status::Off } else { status };
    let row = widgets::list_row(line.text.as_str())
        .status(status)
        .muted(off)
        .selected(look.picked)
        .panel_focused(look.focused);
    let response = ui.add(row);
    // The line reads as a button, as it always has; its severity, which the mark shows,
    // is its description.
    let name = if line.waived {
        tr!("messages-waived-line", text = line.text.as_str())
    } else {
        line.text.clone()
    };
    response.widget_info(|| WidgetInfo::selected(Role::Button, true, look.picked, &name));
    ui.ctx().accesskit_node_builder(response.id, |node| {
        node.set_description(severity.clone());
    });
    // A pass the frame discards counts the same press again, so a double-click can
    // read as a triple one.
    if response.double_clicked() || response.triple_clicked() {
        actions.push(Action::Details(line.key));
    } else if response.clicked() {
        if ui.input(|i| i.modifiers.command) {
            actions.push(Action::Pick(line.key));
        } else {
            actions.push(Action::GoTo(line.key));
        }
    }
    match line.key {
        LineKey::Violation(..) => {
            widgets::context_menu(&response, |ui| violation_menu(ui, shell, line.key, actions));
        }
        LineKey::StaleWaiver(..) => {
            widgets::context_menu(&response, |ui| {
                let delete = tr!("messages-delete-waiver");
                item(ui, delete, Action::Unwaive(line.key), actions);
            });
        }
        LineKey::Note(_) => {}
    }
}

/// One item of a right-click menu.
fn item(ui: &mut Ui, label: String, action: Action, actions: &mut Vec<Action>) {
    if ui.add(widgets::menu_item(label)).clicked() {
        actions.push(action);
        ui.close();
    }
}

/// The right-click menu: Details, Waive (or Unwaive), Set severity of the check, Run
/// this check only and Show rule.
fn violation_menu(ui: &mut Ui, shell: &Shell, key: LineKey, actions: &mut Vec<Action>) {
    let Some(entry) = shell.messages().entry(key) else {
        return;
    };
    item(ui, tr!("messages-details"), Action::Details(key), actions);
    if entry.violation.waiver.is_some() {
        item(ui, tr!("messages-unwaive"), Action::Unwaive(key), actions);
    } else {
        item(ui, tr!("messages-waive"), Action::Waive(key), actions);
    }
    let check = check_title(&entry.violation.check);
    let title = tr!("messages-set-severity", check = check.as_str());
    widgets::menu_button(title).show(ui, |ui| {
        for (severity, label) in [
            (Severity::Error, tr!("severity-error")),
            (Severity::Warning, tr!("severity-warning")),
            (Severity::Off, tr!("severity-off")),
        ] {
            let chosen = entry.violation.severity == severity;
            if ui.add(widgets::radio(chosen, label)).clicked() {
                actions.push(Action::SetSeverity(key, severity));
                ui.close();
            }
        }
    });
    item(ui, tr!("messages-run-only"), Action::RunOnly(key), actions);
    item(
        ui,
        tr!("messages-show-rule"),
        Action::ShowRule(key),
        actions,
    );
}

/// Under the list: the selected line's message and its Violation's objects, each a link
/// that goes to it.
fn details_pane(ui: &mut Ui, shell: &Shell, actions: &mut Vec<Action>) {
    let messages = shell.messages();
    let Some(key) = messages.current() else {
        ui.weak(tr!("messages-details-none"));
        return;
    };
    ScrollArea::vertical()
        .id_salt("messages-details")
        .auto_shrink([false, true])
        .show(ui, |ui| match key {
            LineKey::Violation(..) => {
                let Some(entry) = messages.entry(key) else {
                    return;
                };
                let text = ui.tokens().ty.body_strong.rich(entry.text.as_str());
                ui.add(egui::Label::new(text).wrap());
                ui.horizontal_wrapped(|ui| {
                    ui.label(tr!("messages-details-objects"));
                    for (object, name) in
                        object_links(shell, entry.document, &entry.violation.objects)
                    {
                        if ui.add(widgets::link(name.as_str())).clicked() {
                            actions.push(Action::GoToObject(entry.document, object));
                        }
                    }
                });
            }
            LineKey::Note(i) => {
                if let Some(note) = messages.notes().get(i) {
                    ui.add(egui::Label::new(note.text.as_str()).wrap());
                }
            }
            LineKey::StaleWaiver(..) => {}
        });
}

/// The objects of a Violation that still exist, once each, with their names (R12,
/// Track, Zone GND).
pub fn object_links(
    shell: &Shell,
    document: Option<Uuid>,
    objects: &[Uuid],
) -> Vec<(Uuid, String)> {
    let Some(project) = shell.engine().project() else {
        return Vec::new();
    };
    let mut seen: Vec<(Uuid, String)> = Vec::new();
    for o in objects {
        if project.object(*o).is_none() || seen.iter().any(|(u, _)| u == o) {
            continue;
        }
        let name = match document {
            Some(d) => crate::context_menu::object_name(shell, d, *o),
            None => tr!("context-object"),
        };
        seen.push((*o, name));
    }
    seen
}
