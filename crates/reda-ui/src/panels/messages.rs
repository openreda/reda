//! The Messages panel: ERC and DRC results and load errors and warnings, grouped by
//! Document. Clicking a Violation goes to it; right-clicking offers Waive and Set
//! severity. Drawing only: every click becomes an [`Action`] the window applies after the
//! frame.

use egui::{Button, RichText, ScrollArea, Ui};
use reda_engine::model::Severity;

use crate::messages::{CheckKind, Line, LineKey, check_title};
use crate::shell::Shell;
use crate::tr;

/// What a click in the panel asks for.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Action {
    Run(CheckKind),
    ShowWaived(bool),
    GoTo(LineKey),
    Waive(LineKey),
    Unwaive(LineKey),
    SetSeverity(LineKey, Severity),
}

/// Applies `action` to the shell. Errors go to the status bar.
pub fn apply(shell: &mut Shell, action: Action) {
    let _ = match action {
        Action::Run(kind) => shell.run_check(kind),
        Action::ShowWaived(show) => {
            shell.set_show_waived(show);
            Ok(())
        }
        Action::GoTo(key) => {
            shell.go_to(key);
            Ok(())
        }
        Action::Waive(key) => shell.waive(key),
        Action::Unwaive(key) => shell.unwaive(key),
        Action::SetSeverity(key, severity) => shell.set_severity(key, severity),
    };
}

/// Draws the panel into `ui`.
pub fn show(ui: &mut Ui, shell: &Shell) -> Vec<Action> {
    egui::Frame::new()
        .inner_margin(egui::Margin::symmetric(6, 2))
        .show(ui, |ui| contents(ui, shell))
        .inner
}

fn contents(ui: &mut Ui, shell: &Shell) -> Vec<Action> {
    let mut actions = Vec::new();
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
        let mut show_waived = shell.messages().show_waived();
        if ui
            .checkbox(&mut show_waived, tr!("messages-show-waived"))
            .changed()
        {
            actions.push(Action::ShowWaived(show_waived));
        }
    });
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
    let groups = shell.message_groups();
    if groups.is_empty() {
        ui.weak(tr!("messages-none"));
        return actions;
    }
    let current = shell.messages().current();
    ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for group in &groups {
                let title = tr!(
                    "messages-group",
                    document = group.title.as_str(),
                    count = group.lines.len()
                );
                egui::CollapsingHeader::new(title)
                    .id_salt(("messages-group", group.document))
                    .default_open(true)
                    .show(ui, |ui| {
                        for line in &group.lines {
                            row(ui, shell, line, current == Some(line.key), &mut actions);
                        }
                    });
            }
        });
    actions
}

fn row(ui: &mut Ui, shell: &Shell, line: &Line, current: bool, actions: &mut Vec<Action>) {
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
        let response = ui.selectable_label(current, text);
        if line.waived {
            response.widget_info(|| {
                egui::WidgetInfo::labeled(
                    egui::Role::Button,
                    true,
                    tr!("messages-waived-line", text = line.text.as_str()),
                )
            });
        }
        if response.clicked() {
            actions.push(Action::GoTo(line.key));
        }
        if let LineKey::Violation(..) = line.key {
            response.context_menu(|ui| violation_menu(ui, shell, line, actions));
        }
    });
}

/// The right-click menu: Waive (or Unwaive) and Set severity of the check.
fn violation_menu(ui: &mut Ui, shell: &Shell, line: &Line, actions: &mut Vec<Action>) {
    let Some(entry) = shell.messages().entry(line.key) else {
        return;
    };
    let waived = entry.violation.waiver.is_some();
    let (label, action) = if waived {
        (tr!("messages-unwaive"), Action::Unwaive(line.key))
    } else {
        (tr!("messages-waive"), Action::Waive(line.key))
    };
    if ui.button(label).clicked() {
        actions.push(action);
        ui.close();
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
                actions.push(Action::SetSeverity(line.key, severity));
                ui.close();
            }
        }
    });
}
