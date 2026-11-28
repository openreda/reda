//! Guides in Properties (docs/spec/v0.3/grids-guides.md#editing): the one selected Guide's
//! fields, and with nothing selected the board's **Grids & Guides** region, a row per
//! Guide with its shown and enabled switches and Remove, the Show Grids & Guides toggle,
//! and Add for each kind. Clicking a row's name selects that Guide. Every edit is a
//! `pcb.guide.*` Command. The board's Local Grids follow (see [`super::local_grids`]).

use egui::Ui;
use reda_engine::model::geom::Millideg;
use reda_engine::model::{Guide, GuideKind, Project};
use reda_engine::preferences::Units;
use serde_json::{Value, json};

use super::{Out, parse_length, text_field};
use crate::pcb::view::format_length;
use crate::properties::Shared;
use crate::shell::Shell;
use crate::{tr, widgets};

/// The selected Guide's fields, when the selection is one Guide; whether it was.
pub(super) fn selected(
    ui: &mut Ui,
    shell: &Shell,
    project: &Project,
    units: Units,
    out: &mut Vec<Out>,
) -> bool {
    let [uuid] = shell.selection().objects[..] else {
        return false;
    };
    let Some(guide) = project.pcb().and_then(|pcb| pcb.guide(uuid)) else {
        return false;
    };
    let set = |out: &mut Vec<Out>, fields: Value| {
        let mut params = json!({"uuid": guide.uuid});
        if let (Value::Object(p), Value::Object(f)) = (&mut params, fields) {
            p.extend(f);
        }
        out.push(Out::Run("pcb.guide.set", params));
    };
    super::region(ui, shell, "guide", tr!("properties-region-guide"), |ui| {
        widgets::form(("guide-properties", guide.uuid)).show(ui, |ui| {
            widgets::form_row(ui, tr!("properties-guide-kind"), |ui| {
                ui.label(kind_name(guide.kind))
            });
            ui.end_row();
            let moves = !guide.locked;
            for (axis, label, value) in [
                ("x", tr!("properties-guide-x"), guide.at.x),
                ("y", tr!("properties-guide-y"), guide.at.y),
            ] {
                let key = format!("guide-{axis}-{}", guide.uuid);
                let shown = Shared::Same(format_length(value, units));
                if let Some(text) = text_field(ui, &key, &label, &shown, moves) {
                    // What isn't a length goes as typed, for the Command to refuse.
                    let v = parse_length(&text, units).map_or_else(|| json!(text), |v| json!(v));
                    let at = match axis {
                        "x" => json!({"x": v, "y": guide.at.y}),
                        _ => json!({"x": guide.at.x, "y": v}),
                    };
                    set(out, json!({ "at": at }));
                }
                ui.end_row();
            }
            if let Some(angle) = guide.angle {
                let key = format!("guide-angle-{}", guide.uuid);
                let shown = Shared::Same(degrees(angle));
                let label = tr!("properties-guide-angle");
                if let Some(text) = text_field(ui, &key, &label, &shown, moves) {
                    let a = parse_degrees(&text).map_or_else(|| json!(text), |a| json!(a.0));
                    set(out, json!({ "angle": a }));
                }
                ui.end_row();
            }
            let key = format!("guide-name-{}", guide.uuid);
            let shown = Shared::Same(guide.name.clone().unwrap_or_default());
            let label = tr!("properties-guide-name");
            if let Some(text) = text_field(ui, &key, &label, &shown, true) {
                let text = text.trim();
                set(
                    out,
                    json!({"name": (!text.is_empty()).then(|| text.to_owned())}),
                );
            }
            ui.end_row();
        });
        for (field, label, mut on) in [
            ("shown", tr!("properties-guide-shown"), guide.shown),
            ("enabled", tr!("properties-guide-enabled"), guide.enabled),
            ("locked", tr!("properties-guide-locked"), guide.locked),
        ] {
            if ui.add(widgets::checkbox(&mut on, label)).changed() {
                set(out, json!({ field: on }));
            }
        }
    });
    true
}

/// The Grids & Guides region's body for the board `project` holds: the Show Grids &
/// Guides toggle, a row per Guide, and Add for each kind.
pub(super) fn region(ui: &mut Ui, shell: &Shell, project: &Project, out: &mut Vec<Out>) {
    let mut on = !shell.grids().aids_hidden;
    let toggle = widgets::checkbox(&mut on, tr!("properties-grids-guides-show"));
    if ui.add(toggle).changed() {
        out.push(Out::Run("view.grids_guides.show", json!({ "on": on })));
    }
    let units = shell.board_view().units;
    let guides: Vec<&Guide> = project
        .pcb()
        .map(|pcb| pcb.guides().collect())
        .unwrap_or_default();
    if guides.is_empty() {
        ui.weak(tr!("properties-guides-none"));
    }
    for guide in guides {
        ui.push_id(("grids-guides-row", guide.uuid), |ui| {
            row(ui, shell, guide, units, out);
        });
    }
    super::local_grids::rows(ui, shell, project, out);
    ui.horizontal_wrapped(|ui| {
        for (tool, label) in [
            ("guide", tr!("properties-guides-add-line")),
            ("point_guide", tr!("properties-guides-add-point")),
            ("local_grid", tr!("properties-local-grids-add")),
        ] {
            if ui.add(widgets::button(label)).clicked() {
                out.push(Out::Run("tool.activate", json!({ "tool": tool })));
            }
        }
    });
}

/// One Guide's row: its name (a click selects it), shown, enabled and Remove.
fn row(ui: &mut Ui, shell: &Shell, guide: &Guide, units: Units, out: &mut Vec<Out>) {
    let selected = shell.selection().objects == [guide.uuid];
    ui.horizontal_wrapped(|ui| {
        let name = widgets::link(row_name(guide, units));
        if ui.add(name).clicked() && !selected {
            out.push(Out::Run("selection.set", json!({"objects": [guide.uuid]})));
        }
        for (field, label, mut on) in [
            ("shown", tr!("properties-guide-shown"), guide.shown),
            ("enabled", tr!("properties-guide-enabled"), guide.enabled),
        ] {
            if ui.add(widgets::checkbox(&mut on, label)).changed() {
                out.push(Out::Run(
                    "pcb.guide.set",
                    json!({"uuid": guide.uuid, field: on}),
                ));
            }
        }
        let remove = widgets::button(tr!("properties-guides-remove"));
        if ui.add_enabled(!guide.locked, remove).clicked() {
            out.push(Out::Run("pcb.guide.delete", json!({"uuid": guide.uuid})));
        }
    });
}

/// What a row calls a Guide: its name, else its kind with its angle or position.
pub fn row_name(guide: &Guide, units: Units) -> String {
    if let Some(name) = &guide.name {
        return name.clone();
    }
    match guide.angle {
        Some(angle) => tr!("properties-guides-line", angle = degrees(angle)),
        None => tr!(
            "properties-guides-point",
            x = format_length(guide.at.x, units),
            y = format_length(guide.at.y, units)
        ),
    }
}

fn kind_name(kind: GuideKind) -> String {
    match kind {
        GuideKind::Line => tr!("properties-guide-kind-line"),
        GuideKind::Point => tr!("properties-guide-kind-point"),
    }
}

/// An angle as degrees, without trailing zeros: `45`, `22.5`.
pub(super) fn degrees(angle: Millideg) -> String {
    let text = format!("{:.3}", angle.0 as f64 / 1000.0);
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}

/// Degrees as typed, `45`, `-45°` or `22.5 deg`, in millidegrees.
pub(super) fn parse_degrees(text: &str) -> Option<Millideg> {
    let text = text.trim();
    let number = text
        .strip_suffix('°')
        .or_else(|| text.strip_suffix("deg"))
        .unwrap_or(text)
        .trim();
    let v: f64 = number.parse().ok()?;
    v.is_finite().then(|| Millideg((v * 1000.0).round() as i64))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn degrees_read_and_show_without_trailing_zeros() {
        assert_eq!(parse_degrees("45"), Some(Millideg(45_000)));
        assert_eq!(parse_degrees("-45°"), Some(Millideg(-45_000)));
        assert_eq!(parse_degrees("22.5 deg"), Some(Millideg(22_500)));
        assert_eq!(parse_degrees("north"), None);
        assert_eq!(degrees(Millideg(22_500)), "22.5");
        assert_eq!(degrees(Millideg(90_000)), "90");
    }
}
