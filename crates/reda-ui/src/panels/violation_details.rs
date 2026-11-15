//! Violation Details, a window that leaves the canvas usable while it is open
//! (docs/spec/v0.1/checks.md#violation-details): the check, measured against required,
//! the rule with a link to its Board Setup page, the objects with their Layers and
//! locations, Highlight and Jump, and the Waiver's author, date and reason, edited there,
//! with Waive and Unwaive. Also the small Waive window that asks for one reason for every
//! Violation picked in Messages.

use egui::{Context, RichText, Ui};
use reda_engine::model::geom::Point;
use reda_engine::model::{Object, Uuid, Violation};

use super::messages::{Action, object_links};
use crate::messages::{LineKey, check_title, measurement, mm};
use crate::shell::Shell;
use crate::tr;
use crate::widgets;

/// Draws Violation Details and the Waive window, while they are open.
pub fn show(ctx: &Context, shell: &Shell) -> Vec<Action> {
    let mut actions = Vec::new();
    if shell.messages().details().is_some() {
        details_window(ctx, shell, &mut actions);
    }
    if shell.messages().waive_prompt().is_some() {
        waive_window(ctx, shell, &mut actions);
    }
    actions
}

fn details_window(ctx: &Context, shell: &Shell, actions: &mut Vec<Action>) {
    let mut open = true;
    widgets::window(
        egui::Id::NULL.with("violation-details"),
        tr!("details-title"),
    )
    .open(&mut open)
    .resizable(true)
    .width(380.0)
    .show(ctx, |ui| match shell.details_line() {
        Some(key) => contents(ui, shell, key, actions),
        None => {
            ui.weak(tr!("details-gone"));
        }
    });
    if !open {
        actions.push(Action::CloseDetails);
    }
}

fn contents(ui: &mut Ui, shell: &Shell, key: LineKey, actions: &mut Vec<Action>) {
    let Some(entry) = shell.messages().entry(key) else {
        return;
    };
    let v = &entry.violation;
    ui.label(RichText::new(check_title(&v.check)).strong());
    ui.add(egui::Label::new(entry.text.as_str()).wrap());
    if let (Some(measured), Some(required)) = (&v.measured, &v.required) {
        ui.label(tr!(
            "details-measured",
            measured = measurement(measured),
            required = measurement(required)
        ));
    }
    if let Some(rule) = rule_text(shell, v) {
        ui.horizontal(|ui| {
            ui.label(tr!("details-rule", rule = rule));
            if ui.add(widgets::link(tr!("details-open-rule"))).clicked() {
                actions.push(Action::ShowRule(key));
            }
        });
    }
    ui.separator();
    ui.label(RichText::new(tr!("details-objects")).strong());
    let layers = layer_names(shell, v);
    let columns = [
        widgets::Column::auto(),
        widgets::Column::auto(),
        widgets::Column::auto().number(),
    ];
    widgets::table("violation-details-objects")
        .columns(columns)
        .show(ui, |table| {
            for (object, name) in object_links(shell, entry.document, &v.objects) {
                table.row(|row| {
                    let link = row.ui(|ui| ui.add(widgets::link(name.as_str())));
                    if link.clicked() {
                        actions.push(Action::GoToObject(entry.document, object));
                    }
                    row.text(layers.as_str());
                    let at = object_location(shell, entry.document, object).or(v.at);
                    row.text(at.map_or_else(|| "—".to_owned(), point_text));
                });
            }
        });
    ui.horizontal(|ui| {
        if ui.add(widgets::button(tr!("details-highlight"))).clicked() {
            actions.push(Action::Highlight(key));
        }
        if ui.add(widgets::button(tr!("details-jump"))).clicked() {
            actions.push(Action::Jump(key));
        }
    });
    ui.separator();
    waiver(ui, shell, key, v, actions);
}

/// The Waiver part: its author, date and editable reason with Unwaive, or a reason and
/// Waive for a Violation not waived yet.
fn waiver(ui: &mut Ui, shell: &Shell, key: LineKey, v: &Violation, actions: &mut Vec<Action>) {
    let stored = v
        .waiver
        .and_then(|w| match shell.engine().project()?.object(w) {
            Some(Object::Waiver(w)) => Some(w.object.clone()),
            _ => None,
        });
    let id = egui::Id::NULL.with(("violation-details-reason", v.waiver, key));
    let initial = stored
        .as_ref()
        .map(|w| w.reason.clone())
        .unwrap_or_default();
    let mut reason: String = ui.data_mut(|d| d.get_temp(id)).unwrap_or(initial);
    match &stored {
        Some(w) => {
            ui.label(RichText::new(tr!("details-waived")).strong());
            ui.label(tr!(
                "details-waiver-by",
                author = w.author.as_str(),
                date = crate::recovery_dialog::utc(w.created)
            ));
        }
        None => {
            ui.label(RichText::new(tr!("details-not-waived")).strong());
        }
    }
    ui.horizontal(|ui| {
        let label = ui.label(tr!("details-reason"));
        ui.add(widgets::text_field(&mut reason).width(240.0))
            .labelled_by(label.id);
    });
    ui.data_mut(|d| d.insert_temp(id, reason.clone()));
    ui.horizontal(|ui| match &stored {
        Some(w) => {
            if ui
                .add_enabled(
                    reason != w.reason,
                    widgets::button(tr!("details-save-reason")),
                )
                .clicked()
            {
                actions.push(Action::SetReason(key, reason.clone()));
            }
            if ui.add(widgets::button(tr!("messages-unwaive"))).clicked() {
                actions.push(Action::Unwaive(key));
            }
        }
        None => {
            if ui.add(widgets::button(tr!("messages-waive"))).clicked() {
                actions.push(Action::WaiveWith(key, reason.clone()));
            }
        }
    });
}

/// The rule record that set the limit: board-wide or a Net Class's, with its kind.
fn rule_text(shell: &Shell, v: &Violation) -> Option<String> {
    let pcb = shell.engine().project()?.pcb()?;
    let rule = pcb.rule(v.rule?)?;
    let kind = crate::messages::check_title(rule.kind.id());
    Some(match rule.scope.net_class.and_then(|c| pcb.net_class(c)) {
        Some(class) => tr!(
            "details-rule-class",
            class = class.name.as_str(),
            kind = kind
        ),
        None => tr!("details-rule-board", kind = kind),
    })
}

/// The Layers involved, by name, joined; `—` on a sheet.
fn layer_names(shell: &Shell, v: &Violation) -> String {
    let names: Vec<String> = shell
        .engine()
        .project()
        .and_then(|p| p.pcb())
        .map(|pcb| {
            v.layers
                .iter()
                .filter_map(|l| pcb.layers().layer(*l))
                .map(|l| crate::panels::layers::layer_name(pcb, l))
                .collect()
        })
        .unwrap_or_default();
    if names.is_empty() {
        "—".to_owned()
    } else {
        names.join(", ")
    }
}

/// The centre of an object's drawing on its open Document's canvas.
fn object_location(shell: &Shell, document: Option<Uuid>, object: Uuid) -> Option<Point> {
    let tab = shell.document_tab(document?)?;
    (tab.scene.scene.object_bounds(object)).map(reda_engine::model::geom::Bounds::center)
}

fn point_text(p: Point) -> String {
    format!("({}, {})", mm(p.x.0), mm(p.y.0))
}

fn waive_window(ctx: &Context, shell: &Shell, actions: &mut Vec<Action>) {
    let Some(prompt) = shell.messages().waive_prompt() else {
        return;
    };
    let mut open = true;
    widgets::window(egui::Id::NULL.with("waive-window"), tr!("waive-title"))
        .open(&mut open)
        .show(ctx, |ui| {
            ui.label(tr!("waive-intro", count = prompt.violations.len()));
            let mut reason = prompt.reason.clone();
            ui.horizontal(|ui| {
                let label = ui.label(tr!("details-reason"));
                let response = ui
                    .add(widgets::text_field(&mut reason).width(240.0))
                    .labelled_by(label.id);
                if response.changed() {
                    actions.push(Action::WaiveReason(reason.clone()));
                }
            });
            widgets::button_row(ui, |ui| {
                let waive = widgets::button(tr!("messages-waive")).primary();
                if ui.add(waive).clicked() {
                    actions.push(Action::FinishWaive(true));
                }
                if ui.add(widgets::button(tr!("dialog-cancel"))).clicked() {
                    actions.push(Action::FinishWaive(false));
                }
            });
        });
    if !open {
        actions.push(Action::FinishWaive(false));
    }
}
