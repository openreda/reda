//! The Messages panel: ERC and DRC results and load errors and warnings, grouped by
//! Document, check or severity, with sorting and severity chips; the selected line's
//! details pane under the list. Clicking a Violation goes to it, double-clicking opens
//! Violation Details, Mod+click picks more lines for Waive, and right-click offers
//! Details, Waive, Unwaive, Set severity, Run this check only and Show rule. Drawing
//! only: every click becomes an [`Action`] the window applies after the frame.

use egui::{Button, ComboBox, RichText, ScrollArea, Ui};
use reda_engine::model::{Severity, Uuid};

use crate::layout::SettingsWindow;
use crate::messages::{CheckKind, GroupBy, Line, LineKey, SortBy, check_title};
use crate::shell::Shell;
use crate::tr;

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
    egui::Frame::new()
        .inner_margin(egui::Margin::symmetric(6, 2))
        .show(ui, |ui| contents(ui, shell))
        .inner
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
            ui.colored_label(ui.visuals().warn_fg_color, text);
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
    ScrollArea::vertical()
        .id_salt("messages-list")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for group in &groups {
                let title = tr!(
                    "messages-group",
                    document = group.title.as_str(),
                    count = group.lines.len()
                );
                egui::CollapsingHeader::new(title)
                    .id_salt(("messages-group", &group.kind))
                    .default_open(!group.collapsed)
                    .show(ui, |ui| {
                        for line in &group.lines {
                            let on = picked.contains(&line.key);
                            row(ui, shell, line, on, &mut actions);
                        }
                    });
            }
        });
    actions
}

fn toolbar(ui: &mut Ui, shell: &Shell, actions: &mut Vec<Action>) {
    let open = shell.engine().project();
    ui.horizontal_wrapped(|ui| {
        let erc = open.is_some();
        if ui
            .add_enabled(erc, Button::new(tr!("messages-run-erc")))
            .clicked()
        {
            actions.push(Action::Run(CheckKind::Erc));
        }
        let drc = open.and_then(|p| p.pcb()).is_some()
            && shell.engine().registry().get("drc.run").is_some();
        if ui
            .add_enabled(drc, Button::new(tr!("messages-run-drc")))
            .clicked()
        {
            actions.push(Action::Run(CheckKind::Drc));
        }
        let messages = shell.messages();
        let mut show_waived = messages.show_waived();
        if ui
            .checkbox(&mut show_waived, tr!("messages-show-waived"))
            .changed()
        {
            actions.push(Action::ShowWaived(show_waived));
        }
        ui.separator();
        let (errors, warnings) = messages.counts();
        for (severity, text) in [
            (Severity::Error, tr!("messages-chip-errors", count = errors)),
            (
                Severity::Warning,
                tr!("messages-chip-warnings", count = warnings),
            ),
        ] {
            let on = messages.severity_shown(severity);
            if ui.selectable_label(on, text).clicked() {
                actions.push(Action::Chip(severity, !on));
            }
        }
        ui.separator();
        let group_by = messages.group_by();
        let response = ComboBox::from_id_salt("messages-group-by")
            .selected_text(tr!("messages-group-by", by = group_by.label()))
            .show_ui(ui, |ui| {
                for g in GroupBy::ALL {
                    if ui.selectable_label(g == group_by, g.label()).clicked() {
                        actions.push(Action::GroupBy(g));
                    }
                }
            })
            .response;
        let label = tr!("messages-group-by-label");
        response
            .widget_info(|| egui::WidgetInfo::labeled(egui::Role::ComboBox, true, label.clone()));
        let sort = messages.sort();
        let response = ComboBox::from_id_salt("messages-sort")
            .selected_text(tr!("messages-sort", by = sort.label()))
            .show_ui(ui, |ui| {
                for s in SortBy::ALL {
                    if ui.selectable_label(s == sort, s.label()).clicked() {
                        actions.push(Action::Sort(s));
                    }
                }
            })
            .response;
        let label = tr!("messages-sort-label");
        response
            .widget_info(|| egui::WidgetInfo::labeled(egui::Role::ComboBox, true, label.clone()));
    });
}

fn row(ui: &mut Ui, shell: &Shell, line: &Line, picked: bool, actions: &mut Vec<Action>) {
    ui.horizontal(|ui| {
        let (label, color) = match line.severity {
            Severity::Error => (tr!("severity-error"), ui.visuals().error_fg_color),
            _ => (tr!("severity-warning"), ui.visuals().warn_fg_color),
        };
        let color = if line.waived || line.stale {
            color.gamma_multiply(0.5)
        } else {
            color
        };
        ui.label(RichText::new(label).small().color(color));
        let mut text = RichText::new(line.text.as_str());
        if line.stale || line.waived {
            text = text.color(ui.visuals().weak_text_color());
        }
        if line.waived {
            text = text.italics();
        }
        let response = ui.selectable_label(picked, text);
        if line.waived {
            response.widget_info(|| {
                egui::WidgetInfo::labeled(
                    egui::Role::Button,
                    true,
                    tr!("messages-waived-line", text = line.text.as_str()),
                )
            });
        }
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
                response.context_menu(|ui| violation_menu(ui, shell, line.key, actions));
            }
            LineKey::StaleWaiver(..) => {
                response.context_menu(|ui| {
                    if ui.button(tr!("messages-delete-waiver")).clicked() {
                        actions.push(Action::Unwaive(line.key));
                        ui.close();
                    }
                });
            }
            LineKey::Note(_) => {}
        }
    });
}

/// One item of a right-click menu.
fn item(ui: &mut Ui, label: String, action: Action, actions: &mut Vec<Action>) {
    if ui.button(label).clicked() {
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
    ui.menu_button(tr!("messages-set-severity", check = check.as_str()), |ui| {
        for (severity, label) in [
            (Severity::Error, tr!("severity-error")),
            (Severity::Warning, tr!("severity-warning")),
            (Severity::Off, tr!("severity-off")),
        ] {
            if ui
                .radio(entry.violation.severity == severity, label)
                .clicked()
            {
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
                ui.add(egui::Label::new(RichText::new(entry.text.as_str()).strong()).wrap());
                ui.horizontal_wrapped(|ui| {
                    ui.label(tr!("messages-details-objects"));
                    for (object, name) in
                        object_links(shell, entry.document, &entry.violation.objects)
                    {
                        if ui.link(name.as_str()).clicked() {
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
