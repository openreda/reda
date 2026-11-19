//! Layer table actions and the confirmation shared by structural copper edits.
use super::{Edits, View};
use crate::shell::Shell;
use crate::{modal, tr, widgets};
use egui::Ui;
use reda_engine::model::{PcbDocument, Uuid};
use serde_json::{Value, json};

pub fn buttons(ui: &mut Ui, pcb: &PcbDocument, selected: Option<Uuid>, edits: &mut Edits) {
    let stack = pcb.stackup();
    let count = stack.copper().count();
    let split = selected.filter(|id| stack.dielectrics().any(|d| d.uuid == *id));
    let position = selected.and_then(|id| pcb.copper_number(id));
    let inner = position.is_some_and(|n| n > 1 && n < count);
    ui.horizontal_wrapped(|ui| {
        if ui
            .add_enabled(count < 32, widgets::button(tr!("stackup-add-layer")))
            .clicked()
        {
            let mut p = json!({"document":pcb.uuid});
            if let Some(id) = split {
                p["split"] = json!(id);
            }
            edits.commands.push((None, "pcb.layer.add", p));
        }
        if ui
            .add_enabled(inner, widgets::button(tr!("stackup-remove-layer")))
            .clicked()
        {
            edits
                .commands
                .push((None, "pcb.layer.remove", json!({"layer":selected})));
        }
        for (direction, label, available) in [
            ("up", tr!("via-move-up"), position.is_some_and(|n| n > 2)),
            (
                "down",
                tr!("via-move-down"),
                position.is_some_and(|n| n + 1 < count),
            ),
        ] {
            if ui
                .add_enabled(inner && available, widgets::button(label))
                .clicked()
            {
                edits.commands.push((
                    None,
                    "pcb.layer.move",
                    json!({"layer":selected,"direction":direction}),
                ));
            }
        }
    });
}

#[derive(Clone)]
pub(super) struct Pending {
    command: &'static str,
    params: Value,
    lines: Vec<String>,
}

pub fn prepare(shell: &mut Shell, view: &mut View, command: &'static str, params: &Value) -> bool {
    let Some(lines) = impact(shell, command, params) else {
        return false;
    };
    if lines.is_empty() {
        true
    } else {
        view.layer_change = Some(Pending {
            command,
            params: params.clone(),
            lines,
        });
        false
    }
}

fn impact(shell: &mut Shell, command: &str, params: &Value) -> Option<Vec<String>> {
    let out = shell
        .run_reporting(
            "pcb.stackup.change_preview",
            json!({"change":{"id":command,"params":params}}),
        )
        .ok()?;
    let mut lines = Vec::new();
    // An empty removal and a move that crosses no Via span are immediate.
    let mut line = |key: &str, format: fn(i64) -> String| {
        if let Some(n) = out[key].as_i64().filter(|n| *n > 0) {
            lines.push(format(n));
        }
    };
    line("vias_using_span", |n| tr!("layer-vias-crossed", count = n));
    line("vias_stretched", |n| tr!("layer-vias-stretched", count = n));
    line("vias_shortened", |n| tr!("layer-vias-shortened", count = n));
    line("vias_deleted", |n| tr!("layer-vias-deleted", count = n));
    if command == "pcb.layer.add" {
        line("via_types_changed", |n| {
            tr!("layer-types-stretched", count = n)
        });
    } else if command == "pcb.layer.remove" {
        line("via_types_changed", |n| {
            tr!("layer-types-shortened", count = n)
        });
        line("via_types_removed", |n| {
            tr!("layer-types-removed", count = n)
        });
        line("rules_changed", |n| tr!("layer-rules-changed", count = n));
        line("rules_removed", |n| tr!("layer-rules-removed", count = n));
    }
    if let Some(objects) = out["objects_deleted"].as_object() {
        for (kind, n) in objects {
            let n = n.as_i64().unwrap_or(0);
            let label = match kind.as_str() {
                "track" => tr!("layer-tracks-deleted", count = n),
                "zone" => tr!("layer-zones-deleted", count = n),
                "zone_cutout" => tr!("layer-cutouts-deleted", count = n),
                "keepout" => tr!("layer-keepouts-deleted", count = n),
                "text" => tr!("layer-texts-deleted", count = n),
                "graphic" => tr!("layer-graphics-deleted", count = n),
                "dimension" => tr!("layer-dimensions-deleted", count = n),
                _ => continue,
            };
            lines.push(label);
        }
    }
    if let Some(names) = out["classes_allowing_all"].as_array() {
        lines.extend(
            names
                .iter()
                .filter_map(Value::as_str)
                .map(|name| tr!("layer-class-all", name = name)),
        );
    }
    Some(lines)
}

pub fn confirm(ui: &Ui, view: &mut View, document: Uuid, shell: &mut Shell) {
    let Some(p) = &view.layer_change else { return };
    let dialog = modal::Dialog::new(
        egui::Id::NULL.with(("copper-change", document)),
        tr!("stackup-layer-change"),
    )
    .show(ui.ctx(), |ui| {
        for line in &p.lines {
            ui.label(line);
        }
        modal::buttons(ui, |ui| {
            if ui
                .add(widgets::button(tr!("settings-apply")).primary())
                .clicked()
            {
                return Some(true);
            }
            if ui.add(widgets::button(tr!("dialog-cancel"))).clicked() {
                return Some(false);
            }
            None
        })
    });
    if let Some(apply) = dialog
        .inner
        .or_else(|| dialog.should_close().then_some(false))
    {
        let p = view.layer_change.take().unwrap();
        if !apply {
            return;
        }
        let Some(current) = impact(shell, p.command, &p.params) else {
            return;
        };
        if !current.is_empty() && current != p.lines {
            view.layer_change = Some(Pending {
                lines: current,
                ..p
            });
            return;
        }
        if let Ok(out) = shell.run_reporting(p.command, p.params) {
            settle(view, p.command, &out);
        }
    }
}

pub fn settle(view: &mut View, command: &str, out: &Value) {
    if command == "pcb.layer.remove" {
        view.selected = None;
    } else if let Some(id) = out["result"]["uuid"].as_str().and_then(|s| s.parse().ok()) {
        view.selected = Some(id);
    }
}
