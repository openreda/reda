//! Zones in Properties: a selected Zone's outline and filled areas (read-only), how it
//! pours over its own Net's copper and its islands; and, while the Zone, Zone Cutout or
//! Keepout Tool is active, that Tool's options, which its Preference keeps for the next
//! shape (docs/spec/v0.1/pcb.md#creating-zones).

use egui::{Checkbox, ComboBox, Grid, Role, Ui, WidgetInfo};
use reda_engine::model::{Object, PourOver, Restriction};
use reda_engine::preferences::{KeepoutToolShape, Units};
use serde_json::{Value, json};

use super::text_field;
use crate::properties::Shared;
use crate::shell::Shell;
use crate::tools::Tool;
use crate::tr;

/// Draws the active board Tool's options, if it has any; returns the Commands its edits
/// ask for, or `None` when the Tool has no options here.
pub(super) fn tool_options(ui: &mut Ui, shell: &Shell) -> Option<Vec<(&'static str, Value)>> {
    let tool = shell.tool();
    if !matches!(tool, Tool::Zone | Tool::Keepout | Tool::ZoneCutout) || !shell.board_is_active() {
        return None;
    }
    let mut out = Vec::new();
    ui.label(tr!("properties-tool-options", tool = tool_title(tool)));
    ui.separator();
    let set = |key: &str, value: Value| {
        (
            "app.preferences.set",
            json!({"key": format!("pcb.{key}"), "value": value}),
        )
    };
    match tool {
        Tool::Zone => {
            let values = shell.preferences().pcb.zone.tool.clone();
            let units = shell.board_view().units;
            Grid::new("properties-zone-tool")
                .num_columns(2)
                .show(ui, |ui| {
                    for (name, value) in [
                        ("clearance", values.clearance),
                        ("min_width", values.min_width),
                    ] {
                        let shown = value
                            .map(|v| crate::pcb::view::format_length(v.nm(), units))
                            .unwrap_or_default();
                        let label = match name {
                            "clearance" => tr!("properties-clearance"),
                            _ => tr!("properties-min-width"),
                        };
                        let key = format!("zone-tool-{name}");
                        if let Some(text) = text_field(ui, &key, &label, &Shared::Same(shown), true)
                        {
                            let value = if text.trim().is_empty() {
                                Value::Null
                            } else {
                                json!(with_unit(&text, units, false))
                            };
                            out.push(set(&format!("zone.tool.{name}"), value));
                        }
                        ui.end_row();
                    }
                    let pour = pour_over(ui, "zone-tool", &Shared::Same(values.pour_over));
                    if let Some(p) = pour {
                        out.push(set("zone.tool.pour_over", json!(p)));
                    }
                    ui.end_row();
                    let area =
                        crate::panels::navigator::format_area(values.min_island_area.0, units);
                    let label = tr!("properties-min-island-area");
                    if let Some(text) =
                        text_field(ui, "zone-tool-island", &label, &Shared::Same(area), true)
                    {
                        let text = if text.trim().is_empty() {
                            "0"
                        } else {
                            text.trim()
                        };
                        out.push(set(
                            "zone.tool.min_island_area",
                            json!(with_unit(text, units, true)),
                        ));
                    }
                    ui.end_row();
                });
            let mut on = values.remove_unconnected;
            if ui
                .add(Checkbox::new(&mut on, tr!("properties-remove-unconnected")))
                .clicked()
            {
                out.push(set("zone.tool.remove_unconnected", json!(on)));
            }
        }
        Tool::Keepout => {
            let values = shell.preferences().pcb.keepout.clone();
            let shapes = [
                (KeepoutToolShape::Polygon, tr!("properties-keepout-polygon")),
                (
                    KeepoutToolShape::Rectangle,
                    tr!("properties-keepout-rectangle"),
                ),
                (KeepoutToolShape::Circle, tr!("properties-keepout-circle")),
            ];
            ui.horizontal(|ui| {
                let label = tr!("properties-keepout-shape");
                ui.label(&label);
                let shown = shapes
                    .iter()
                    .find(|(s, _)| *s == values.shape)
                    .map(|(_, t)| t.clone())
                    .unwrap_or_default();
                let response = ComboBox::from_id_salt("properties-keepout-shape")
                    .selected_text(shown)
                    .show_ui(ui, |ui| {
                        for (shape, title) in &shapes {
                            let current = *shape == values.shape;
                            if ui.selectable_label(current, title).clicked() && !current {
                                out.push(set("keepout.shape", json!(shape)));
                            }
                        }
                    })
                    .response;
                response.widget_info(|| WidgetInfo::labeled(Role::ComboBox, true, label.clone()));
            });
            let mut all = values.all_layers;
            if ui
                .add(Checkbox::new(
                    &mut all,
                    tr!("properties-keepout-all-layers"),
                ))
                .clicked()
            {
                out.push(set("keepout.all_layers", json!(all)));
            }
            ui.label(tr!("properties-keepout-restrict"));
            for r in Restriction::ALL {
                let mut on = values.restrict.contains(&r);
                if ui
                    .add(Checkbox::new(&mut on, restriction_title(r)))
                    .clicked()
                {
                    let mut restrict = values.restrict.clone();
                    if on {
                        restrict.insert(r);
                    } else {
                        restrict.remove(&r);
                    }
                    // A Keepout must keep something out.
                    if !restrict.is_empty() {
                        out.push(set("keepout.restrict", json!(restrict)));
                    }
                }
            }
        }
        _ => {
            ui.weak(tr!("properties-zone-cutout-tool"));
        }
    }
    Some(out)
}

fn tool_title(tool: Tool) -> String {
    match tool {
        Tool::Zone => tr!("properties-tool-zone"),
        Tool::Keepout => tr!("properties-tool-keepout"),
        _ => tr!("properties-tool-zone-cutout"),
    }
}

fn restriction_title(r: Restriction) -> String {
    match r {
        Restriction::Tracks => tr!("properties-restrict-tracks"),
        Restriction::Vias => tr!("properties-restrict-vias"),
        Restriction::Zones => tr!("properties-restrict-zones"),
        Restriction::SmdPads => tr!("properties-restrict-smd-pads"),
        Restriction::ThtPads => tr!("properties-restrict-tht-pads"),
        Restriction::Footprints => tr!("properties-restrict-footprints"),
    }
}

/// `text` with the board's unit added when it is a bare number: a length, or with
/// `area` an area.
fn with_unit(text: &str, units: Units, area: bool) -> String {
    let t = text.trim();
    if t.parse::<f64>().is_err() {
        return t.replace("mm²", "mm2").replace("mil²", "mil2");
    }
    let unit = match (units, area) {
        (Units::Mm, false) => "mm",
        (Units::Mil, false) => "mil",
        (Units::Mm, true) => "mm2",
        (Units::Mil, true) => "mil2",
    };
    format!("{t}{unit}")
}

/// The Pour Over choice; returns a new choice.
fn pour_over(ui: &mut Ui, salt: &str, value: &Shared<PourOver>) -> Option<PourOver> {
    let choices = [
        (PourOver::ZonesOnly, tr!("properties-pour-over-zones-only")),
        (PourOver::All, tr!("properties-pour-over-all")),
        (PourOver::None, tr!("properties-pour-over-none")),
    ];
    let label = tr!("properties-pour-over");
    ui.label(&label);
    let shown = match value {
        Shared::Mixed => crate::properties::MIXED.to_owned(),
        Shared::Same(v) => choices
            .iter()
            .find(|(c, _)| c == v)
            .map(|(_, t)| t.clone())
            .unwrap_or_default(),
    };
    let mut picked = None;
    let response = ComboBox::from_id_salt(("properties-pour-over", salt))
        .selected_text(shown)
        .show_ui(ui, |ui| {
            for (choice, title) in &choices {
                let current = value.same() == Some(choice);
                if ui.selectable_label(current, title).clicked() && !current {
                    picked = Some(*choice);
                }
            }
        })
        .response;
    response.widget_info(|| WidgetInfo::labeled(Role::ComboBox, true, label.clone()));
    picked
}

/// The selected Zones' own section, when every selected object is a Zone: each one's
/// outline and filled areas (read-only, for one Zone), Detach for one that follows the
/// Board Outline, Pour Over and Remove Unconnected.
pub(super) fn zones_section(ui: &mut Ui, shell: &Shell, out: &mut Vec<(&'static str, Value)>) {
    let Some(project) = shell.engine().project() else {
        return;
    };
    let Some(pcb) = project.pcb() else {
        return;
    };
    let zones: Vec<_> = shell
        .selection()
        .objects
        .iter()
        .map(|o| match project.object(*o) {
            Some(Object::Zone(z)) => Some(z.object),
            _ => None,
        })
        .collect::<Option<_>>()
        .unwrap_or_default();
    if zones.is_empty() {
        return;
    }
    let units = shell.board_view().units;
    ui.separator();
    if let [zone] = &zones[..] {
        let listing = reda_engine::pcb::zone_listing(pcb, project.zone_fills(pcb.uuid), None);
        if let Some(row) = listing
            .layers
            .iter()
            .flat_map(|l| &l.zones)
            .find(|r| r.uuid == zone.uuid)
        {
            Grid::new("properties-zone-areas")
                .num_columns(2)
                .show(ui, |ui| {
                    let format = crate::panels::navigator::format_area;
                    ui.weak(tr!("properties-outline-area"));
                    ui.label(format(row.outline_area, units));
                    ui.end_row();
                    ui.weak(tr!("properties-filled-area"));
                    ui.label(format(row.filled_area, units));
                    ui.end_row();
                });
        }
    }
    if let [zone] = &zones[..]
        && zone.follows_outline
    {
        ui.weak(tr!("properties-zone-follows-outline"));
        if ui.button(tr!("properties-zone-detach")).clicked() {
            out.push(("pcb.zone.detach_outline", json!({"zone": zone.uuid})));
        }
    }
    let each = |name: &str, value: Value| -> Vec<(&'static str, Value)> {
        zones
            .iter()
            .map(|z| ("pcb.zone.set", json!({"zone": z.uuid, name: value.clone()})))
            .collect()
    };
    let mut commands = Vec::new();
    Grid::new("properties-zone-pour")
        .num_columns(2)
        .show(ui, |ui| {
            let pour = Shared::of(zones.iter().map(|z| z.pour_over)).unwrap_or(Shared::Mixed);
            if let Some(p) = pour_over(ui, "zones", &pour) {
                commands = each("pour_over", json!(p));
            }
            ui.end_row();
        });
    let remove = Shared::of(zones.iter().map(|z| z.remove_unconnected)).unwrap_or(Shared::Mixed);
    let mut on = remove.same().copied().unwrap_or(false);
    if ui
        .add(
            Checkbox::new(&mut on, tr!("properties-remove-unconnected"))
                .indeterminate(remove == Shared::Mixed),
        )
        .clicked()
    {
        commands = each("remove_unconnected", json!(on));
    }
    match commands.len() {
        0 => {}
        1 => out.extend(commands),
        _ => {
            let steps: Vec<Value> = commands
                .into_iter()
                .map(|(id, params)| json!({"id": id, "params": params}))
                .collect();
            out.push(("batch", json!({ "commands": steps })));
        }
    }
}
