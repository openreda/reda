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
    let line = |lines: &mut Vec<String>, key: &str, format: fn(i64) -> String| {
        if let Some(n) = out[key].as_i64().filter(|n| *n > 0) {
            lines.push(format(n));
        }
    };
    line(&mut lines, "vias_using_span", |n| {
        tr!("layer-vias-crossed", count = n)
    });
    line(&mut lines, "vias_stretched", |n| {
        tr!("layer-vias-stretched", count = n)
    });
    let count = |key: &str| out[key].as_i64().unwrap_or(0);
    match (count("vias_shortened"), count("vias_deleted")) {
        (0, 0) => {}
        (shortened, 0) => lines.push(tr!("layer-vias-shortened", count = shortened)),
        (0, deleted) => lines.push(tr!("layer-vias-deleted", count = deleted)),
        (shortened, deleted) => lines.push(tr!(
            "layer-vias-shortened-deleted",
            shortened = shortened,
            deleted = deleted
        )),
    }
    if matches!(
        command,
        "pcb.layer.add" | "pcb.layer.remove" | super::apply::COMMAND
    ) {
        let removed = command == "pcb.layer.remove"
            || params["remove"]
                .as_array()
                .is_some_and(|ids| !ids.is_empty());
        let describe: fn(i64) -> String = if params["via_types"].as_bool() == Some(true) {
            |n| tr!("construction-types-changed", count = n)
        } else if removed {
            |n| tr!("layer-types-shortened", count = n)
        } else {
            |n| tr!("layer-types-stretched", count = n)
        };
        line(&mut lines, "via_types_changed", describe);
        line(&mut lines, "via_types_removed", |n| {
            tr!("layer-types-removed", count = n)
        });
        line(&mut lines, "vias_without_type", |n| {
            tr!("construction-vias-unmatched", count = n)
        });
        line(&mut lines, "rules_changed", |n| {
            tr!("layer-rules-changed", count = n)
        });
        line(&mut lines, "rules_removed", |n| {
            tr!("layer-rules-removed", count = n)
        });
    }
    if let Some(objects) = out["objects_deleted"].as_object() {
        // In the spec's order, Tracks first, whatever order the preview lists them in.
        let counts: Vec<String> = [
            "track",
            "zone",
            "zone_cutout",
            "keepout",
            "text",
            "graphic",
            "dimension",
        ]
        .into_iter()
        .filter_map(|kind| {
            let n = objects.get(kind)?.as_i64().filter(|n| *n > 0)?;
            Some(match kind {
                "track" => tr!("layer-count-tracks", count = n),
                "zone" => tr!("layer-count-zones", count = n),
                "zone_cutout" => tr!("layer-count-cutouts", count = n),
                "keepout" => tr!("layer-count-keepouts", count = n),
                "text" => tr!("layer-count-texts", count = n),
                "graphic" => tr!("layer-count-graphics", count = n),
                _ => tr!("layer-count-dimensions", count = n),
            })
        })
        .collect();
        if !counts.is_empty() {
            lines.push(tr!(
                "layer-objects-deleted",
                objects = counts.join(", "),
                layer = removed_layers(shell, command, params)
            ));
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

/// The labels of the inner copper Layers `command` removes, such as `L3 GND`.
fn removed_layers(shell: &Shell, command: &str, params: &Value) -> String {
    let ids: Vec<Uuid> = if command == "pcb.layer.remove" {
        params["layer"]
            .as_str()
            .and_then(|s| s.parse().ok())
            .into_iter()
            .collect()
    } else {
        params["remove"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|id| id.as_str()?.parse().ok())
            .collect()
    };
    let Some(pcb) = shell.engine().project().and_then(|p| p.pcb()) else {
        return String::new();
    };
    let mut labels: Vec<(usize, String)> = ids
        .iter()
        .filter_map(|id| Some((pcb.copper_number(*id)?, pcb.inner_layer_label(*id)?)))
        .collect();
    labels.sort();
    labels
        .into_iter()
        .map(|(_, label)| label)
        .collect::<Vec<_>>()
        .join(", ")
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
            settle(shell, document, view.selected, p.command, &out);
        }
    }
}

pub fn settle(
    shell: &mut Shell,
    document: Uuid,
    selected: Option<Uuid>,
    command: &str,
    out: &Value,
) {
    let next = match command {
        super::apply::COMMAND => selected.filter(|id| {
            ["layers", "dielectrics", "via_types"].iter().any(|key| {
                out["result"][key]
                    .as_array()
                    .is_some_and(|ids| ids.contains(&json!(id)))
            })
        }),
        "pcb.layer.remove" | "pcb.stackup.dielectric.remove" | "pcb.via_type.remove" => None,
        "pcb.layer.add" | "pcb.stackup.dielectric.add" | "pcb.via_type.add" => {
            out["result"]["uuid"].as_str().and_then(|s| s.parse().ok())
        }
        _ => return,
    };
    if selected != next {
        let _ = shell.run_reporting(
            super::commands::SELECT,
            json!({"document":document,"entry":next}),
        );
    }
}
