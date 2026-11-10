//! The chosen row's details: its previews (the Symbol with a tab per Unit, the Footprint
//! with a picker for the alternates), "Used in Project: N", and Where Used with what it
//! found.

use egui::{ComboBox, Role, Ui, WidgetInfo};
use reda_engine::library::{ElementKind, LibraryIndex};
use reda_engine::model::{Part, Project};
use serde_json::Value;

use super::rows::Entry;
use super::{Do, LibraryPanel, Placing, place, preview};
use crate::tr;

/// Draws the details of `entry`, placing into `target`.
pub(super) fn show(
    ui: &mut Ui,
    panel: &mut LibraryPanel,
    project: Option<&Project>,
    target: Placing,
    entry: &Entry,
) -> Vec<Do> {
    let mut out = Vec::new();
    let Some((_, index)) = &panel.index else {
        return out;
    };
    let index = index.clone();
    ui.strong(&entry.name);
    ui.weak(tr!("library-in", library = entry.library_name.clone()));
    match (&entry.part, entry.kind) {
        (Some(part), _) => out.extend(part_previews(ui, panel, &index, target, part)),
        (None, ElementKind::Symbol) => {
            if let Some(symbol) = index.symbol(entry.uuid) {
                unit_tabs(ui, panel, symbol);
                let unit = symbol.units.get(panel.unit).map(|u| u.uuid);
                preview::symbol(ui, symbol, unit);
            }
        }
        (None, _) => {
            if let Some(footprint) = index.footprint(entry.uuid) {
                preview::footprint(ui, footprint);
            }
        }
    }
    if let Some(project) = project {
        let used = reda_engine::library::components_using(project, entry.uuid).len();
        ui.label(tr!("library-used-in-project", count = used));
    }
    let label = tr!("library-where-used");
    if ui.button(&label).clicked() {
        out.push(Do::WhereUsed(entry.uuid));
    }
    if let Some((element, found)) = &panel.where_used
        && *element == entry.uuid
    {
        where_used(ui, found);
    }
    out
}

/// What Where Used found: the Parts that use the element and the Components placed from
/// it, or that nothing does.
fn where_used(ui: &mut Ui, found: &Value) {
    let names = |list: &Value, key: &str| -> Vec<String> {
        (list.as_array().into_iter().flatten())
            .filter_map(|v| v[key].as_str().map(str::to_owned))
            .collect()
    };
    let parts: Vec<String> = (found["parts"].as_array().into_iter().flatten())
        .filter_map(|p| {
            let name = p["name"].as_str()?;
            let library = p["library"]["name"].as_str().unwrap_or_default();
            Some(tr!(
                "library-where-used-part",
                name = name,
                library = library
            ))
        })
        .collect();
    let components = names(&found["components"], "designator");
    ui.group(|ui| {
        ui.strong(tr!("library-where-used-title"));
        if parts.is_empty() && components.is_empty() {
            ui.weak(tr!("library-where-used-nothing"));
        }
        for part in parts {
            ui.label(part);
        }
        if !components.is_empty() {
            ui.label(tr!(
                "library-where-used-components",
                designators = components.join(", ")
            ));
        }
    });
}

/// Unit tabs over a Symbol of more than one Unit; returns whether the Unit changed.
fn unit_tabs(ui: &mut Ui, panel: &mut LibraryPanel, symbol: &reda_engine::model::Symbol) -> bool {
    let mut changed = false;
    if symbol.units.len() > 1 {
        ui.horizontal(|ui| {
            for (i, unit) in symbol.units.iter().enumerate() {
                let label = tr!("library-unit", unit = unit.name.clone());
                let tab = ui.selectable_label(panel.unit == i, &unit.name);
                tab.widget_info(|| {
                    WidgetInfo::selected(Role::Tab, true, panel.unit == i, label.clone())
                });
                if tab.clicked() && panel.unit != i {
                    panel.unit = i;
                    changed = true;
                }
            }
        });
    }
    changed
}

/// A Part's previews: the Symbol with a tab per Unit, the Footprint with a picker for the
/// alternates. Choosing a Unit or Footprint while it is being placed places that one.
fn part_previews(
    ui: &mut Ui,
    panel: &mut LibraryPanel,
    index: &LibraryIndex,
    target: Placing,
    part: &Part,
) -> Vec<Do> {
    let mut out = Vec::new();
    if !part.fields.description.is_empty() {
        ui.weak(&part.fields.description);
    }
    let symbol = part.symbol.and_then(|s| index.symbol(s)).cloned();
    let footprints: Vec<_> = part
        .footprints
        .iter()
        .filter_map(|f| index.footprint(*f))
        .cloned()
        .collect();
    let mut changed = false;
    match &symbol {
        Some(symbol) => {
            changed |= unit_tabs(ui, panel, symbol);
            let unit = symbol.units.get(panel.unit).map(|u| u.uuid);
            preview::symbol(ui, symbol, unit);
        }
        None => {
            ui.weak(tr!("library-no-symbol"));
        }
    }
    if !footprints.is_empty() {
        panel.footprint = panel.footprint.min(footprints.len() - 1);
        let label = tr!("library-footprint");
        let response = ComboBox::from_id_salt("library-footprint")
            .selected_text(&footprints[panel.footprint].name)
            .show_ui(ui, |ui| {
                for (i, f) in footprints.iter().enumerate() {
                    if ui.selectable_label(panel.footprint == i, &f.name).clicked()
                        && panel.footprint != i
                    {
                        panel.footprint = i;
                        changed = true;
                    }
                }
            })
            .response;
        response.widget_info(|| WidgetInfo::labeled(Role::ComboBox, true, label.clone()));
        preview::footprint(ui, &footprints[panel.footprint]);
    }
    if changed {
        let unit = symbol
            .as_ref()
            .and_then(|s| s.units.get(panel.unit))
            .map(|u| u.name.clone());
        let footprint = part.footprints.get(panel.footprint).copied();
        out.extend(place(target, part, unit, footprint).map(Do::Out));
    }
    out
}
