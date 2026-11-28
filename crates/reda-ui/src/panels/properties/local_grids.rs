//! Local Grids in Properties (docs/spec/v0.3/grids-guides.md#local-grids): the one
//! selected Local Grid's fields, and in the board's Grids & Guides region a row per Local
//! Grid in priority order, whose grip drags it up or down the list. Every edit is a
//! `pcb.local_grid.*` Command; a shape field sends the whole shape.

use egui::{Role, Ui, WidgetInfo};
use reda_engine::model::geom::Nm;
use reda_engine::model::{AppliesTo, CartesianGrid, LocalGrid, LocalGridShape, Project};
use reda_engine::preferences::Units;
use serde_json::{Value, json};

use super::guides::{degrees, parse_degrees};
use super::{Out, parse_length, text_field};
use crate::pcb::view::format_length;
use crate::properties::Shared;
use crate::shell::Shell;
use crate::{tr, widgets};

/// The Applies to choices, in the order the menu lists them.
fn applies_to_choices() -> [(AppliesTo, String); 3] {
    [
        (AppliesTo::All, tr!("local-grid-applies-all")),
        (AppliesTo::Components, tr!("local-grid-applies-components")),
        (
            AppliesTo::NotComponents,
            tr!("local-grid-applies-not-components"),
        ),
    ]
}

/// What the menu calls `applies_to`.
pub fn applies_to_name(applies_to: AppliesTo) -> String {
    (applies_to_choices().into_iter())
        .find(|(a, _)| *a == applies_to)
        .map(|(_, name)| name)
        .unwrap_or_default()
}

/// An Applies to menu; the choice picked, if it changed.
pub fn applies_to_menu(
    ui: &mut Ui,
    salt: impl std::hash::Hash + std::fmt::Debug,
    label: &str,
    current: AppliesTo,
    enabled: bool,
) -> Option<AppliesTo> {
    let mut picked = None;
    let response = ui
        .add_enabled_ui(enabled, |ui| {
            widgets::dropdown(("local-grid-applies-to", salt), applies_to_name(current))
                .show(ui, |ui| {
                    for (choice, title) in applies_to_choices() {
                        let on = choice == current;
                        if ui.add(widgets::choice(on, title)).clicked() && !on {
                            picked = Some(choice);
                        }
                    }
                })
                .response
        })
        .inner;
    response.widget_info(|| WidgetInfo::labeled(Role::ComboBox, enabled, label));
    picked
}

/// The selected Local Grid's fields, when the selection is one Local Grid; whether it
/// was.
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
    let Some(grid) = project.pcb().and_then(|pcb| pcb.local_grid(uuid)) else {
        return false;
    };
    let set = |out: &mut Vec<Out>, fields: Value| {
        let mut params = json!({"uuid": grid.uuid});
        if let (Value::Object(p), Value::Object(f)) = (&mut params, fields) {
            p.extend(f);
        }
        out.push(Out::Run("pcb.local_grid.set", params));
    };
    let LocalGridShape::Cartesian(c) = &grid.shape;
    let title = tr!("properties-region-local-grid");
    super::region(ui, shell, "local-grid", title, |ui| {
        widgets::form(("local-grid-properties", grid.uuid)).show(ui, |ui| {
            let key = format!("local-grid-name-{}", grid.uuid);
            let label = tr!("properties-local-grid-name");
            let shown = Shared::Same(grid.name.clone());
            if let Some(text) = text_field(ui, &key, &label, &shown, true) {
                set(out, json!({"name": text.trim()}));
            }
            ui.end_row();
            let label = tr!("properties-local-grid-applies-to");
            widgets::form_row(ui, &label, |ui| {
                if let Some(a) = applies_to_menu(ui, grid.uuid, &label, grid.applies_to, true) {
                    set(out, json!({ "applies_to": a }));
                }
            });
            ui.end_row();
            shape_fields(ui, grid, c, units, &mut |shape| {
                set(out, json!({ "shape": shape }));
            });
        });
        for (field, label, mut on) in [
            ("shown", tr!("properties-guide-shown"), grid.shown),
            ("enabled", tr!("properties-guide-enabled"), grid.enabled),
            ("locked", tr!("properties-guide-locked"), grid.locked),
        ] {
            if ui.add(widgets::checkbox(&mut on, label)).changed() {
                set(out, json!({ field: on }));
            }
        }
    });
    true
}

/// `c` as `pcb.local_grid.*` take a shape.
pub fn shape_param(c: &CartesianGrid) -> Value {
    json!({
        "kind": "cartesian",
        "origin": {"x": c.origin.x.0, "y": c.origin.y.0},
        "rotation": c.rotation.0,
        "step_x": c.step_x,
        "step_y": c.step_y,
        "width": c.width.0,
        "height": c.height.0,
    })
}

/// The shape's fields; `changed` gets the whole shape with the edited field as typed
/// (what isn't a length or angle goes as typed, for the Command to refuse).
fn shape_fields(
    ui: &mut Ui,
    grid: &LocalGrid,
    c: &CartesianGrid,
    units: Units,
    changed: &mut dyn FnMut(Value),
) {
    let moves = !grid.locked;
    let length = |text: &str| parse_length(text, units).map_or_else(|| json!(text), |v| json!(v.0));
    // A step keeps the unit it is typed in; a bare number is in the board's units.
    let step = |text: &str| match parse_length(text, units) {
        Some(Nm(v)) => json!(v),
        None => json!(text.trim()),
    };
    let lengths: [(&str, String, String); 4] = [
        (
            "x",
            tr!("properties-guide-x"),
            format_length(c.origin.x, units),
        ),
        (
            "y",
            tr!("properties-guide-y"),
            format_length(c.origin.y, units),
        ),
        (
            "width",
            tr!("properties-local-grid-width"),
            format_length(c.width, units),
        ),
        (
            "height",
            tr!("properties-local-grid-height"),
            format_length(c.height, units),
        ),
    ];
    for (field, label, shown) in lengths {
        let key = format!("local-grid-{field}-{}", grid.uuid);
        if let Some(text) = text_field(ui, &key, &label, &Shared::Same(shown), moves) {
            let mut shape = shape_param(c);
            match field {
                "x" | "y" => shape["origin"][field] = length(&text),
                _ => shape[field] = length(&text),
            }
            changed(shape);
        }
        ui.end_row();
    }
    let key = format!("local-grid-rotation-{}", grid.uuid);
    let label = tr!("properties-local-grid-rotation");
    let shown = Shared::Same(degrees(c.rotation));
    if let Some(text) = text_field(ui, &key, &label, &shown, moves) {
        let mut shape = shape_param(c);
        shape["rotation"] = parse_degrees(&text).map_or_else(|| json!(text), |a| json!(a.0));
        changed(shape);
    }
    ui.end_row();
    for (field, label, value) in [
        ("step_x", tr!("properties-local-grid-step-x"), c.step_x),
        ("step_y", tr!("properties-local-grid-step-y"), c.step_y),
    ] {
        let key = format!("local-grid-{field}-{}", grid.uuid);
        let shown = Shared::Same(value.to_string());
        if let Some(text) = text_field(ui, &key, &label, &shown, moves) {
            let mut shape = shape_param(c);
            shape[field] = step(&text);
            changed(shape);
        }
        ui.end_row();
    }
}

/// What a row says of a Local Grid: its name, step and Applies to.
pub fn row_text(grid: &LocalGrid) -> String {
    let LocalGridShape::Cartesian(c) = &grid.shape;
    let step = if c.step_x == c.step_y {
        c.step_x.to_string()
    } else {
        format!("{} × {}", c.step_x, c.step_y)
    };
    tr!(
        "properties-local-grids-row",
        name = grid.name.as_str(),
        step = step,
        applies = applies_to_name(grid.applies_to)
    )
}

/// The `pcb.local_grid.reorder` steps that turn `now` into `new` (both every Local Grid,
/// highest priority first): each grid from the last up put before the one after it.
pub fn reorder_steps(
    now: &[reda_engine::model::Uuid],
    new: &[reda_engine::model::Uuid],
) -> Vec<Value> {
    if now == new {
        return Vec::new();
    }
    (0..new.len())
        .rev()
        .map(|i| match new.get(i + 1) {
            Some(before) => json!({"local_grid": new[i], "before": before}),
            None => json!({"local_grid": new[i]}),
        })
        .collect()
}

/// The Local Grid rows of the board's Grids & Guides region, in priority order, each
/// with its grip, name (a click selects it), shown and enabled switches and Remove, then
/// Add Local Grid.
pub(super) fn rows(ui: &mut Ui, shell: &Shell, project: &Project, out: &mut Vec<Out>) {
    let grids: Vec<&LocalGrid> = project
        .pcb()
        .map(|pcb| pcb.local_grids().collect())
        .unwrap_or_default();
    ui.add_space(4.0);
    ui.strong(tr!("properties-local-grids"));
    if grids.is_empty() {
        ui.weak(tr!("properties-local-grids-none"));
    }
    let grip = |i: usize| tr!("local-grid-drag", name = grids[i].name.as_str());
    let order = widgets::reorder_rows(ui, "properties-local-grids", grids.len(), grip, |ui, i| {
        let grid = grids[i];
        let selected = shell.selection().objects == [grid.uuid];
        if ui.add(widgets::link(row_text(grid))).clicked() && !selected {
            out.push(Out::Run("selection.set", json!({"objects": [grid.uuid]})));
        }
        for (field, label, mut on) in [
            ("shown", tr!("properties-guide-shown"), grid.shown),
            ("enabled", tr!("properties-guide-enabled"), grid.enabled),
        ] {
            let check = ui.add(widgets::checkbox(&mut on, ""));
            let name = tr!(
                "local-grid-switch",
                what = label.as_str(),
                name = grid.name.as_str()
            );
            check.widget_info(|| WidgetInfo::selected(Role::CheckBox, true, on, &name));
            if check.changed() {
                let params = json!({"uuid": grid.uuid, field: on});
                out.push(Out::Run("pcb.local_grid.set", params));
            }
        }
        let remove = ui.add_enabled(
            !grid.locked,
            widgets::button(tr!("properties-guides-remove")),
        );
        let name = tr!("local-grid-remove", name = grid.name.as_str());
        remove.widget_info(|| WidgetInfo::labeled(Role::Button, !grid.locked, &name));
        if remove.clicked() {
            out.push(Out::Run(
                "pcb.local_grid.delete",
                json!({"uuid": grid.uuid}),
            ));
        }
    });
    if let Some(order) = order {
        // One drop moves one grid: put it before the grid now after it.
        let now: Vec<_> = grids.iter().map(|g| g.uuid).collect();
        let new: Vec<_> = order.iter().map(|&i| now[i]).collect();
        if let Some(moved) = moved_one(&now, &new) {
            let at = new.iter().position(|u| *u == moved).expect("in the order");
            let mut params = json!({ "local_grid": moved });
            if let Some(before) = new.get(at + 1) {
                params["before"] = json!(before);
            }
            out.push(Out::Run("pcb.local_grid.reorder", params));
        }
    }
    if ui
        .add(widgets::button(tr!("properties-local-grids-add")))
        .clicked()
    {
        out.push(Out::Run("tool.activate", json!({ "tool": "local_grid" })));
    }
}

/// The one grid a drop moved from `now` to get `new`.
fn moved_one<T: Copy + PartialEq>(now: &[T], new: &[T]) -> Option<T> {
    if now == new {
        return None;
    }
    // `x` is the moved one when taking it out of both leaves the same order.
    let without = |v: &[T], x: T| v.iter().copied().filter(|y| *y != x).collect::<Vec<_>>();
    now.iter()
        .copied()
        .find(|&x| without(now, x) == without(new, x))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_moved_grid_is_the_one_whose_removal_leaves_the_same_order() {
        assert_eq!(moved_one(&[1, 2, 3], &[3, 1, 2]), Some(3));
        assert_eq!(moved_one(&[1, 2, 3], &[2, 1, 3]), Some(1));
        assert_eq!(moved_one(&[1, 2, 3], &[1, 2, 3]), None);
    }
}
