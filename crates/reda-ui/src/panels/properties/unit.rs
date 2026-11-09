//! The Unit rows of one selected Unit: which Unit of its Component it is, the unplaced
//! Units each with a **Place** button (`tool.place_unit`), and each field moved by hand
//! with **Reset position** (`schematic.field.reset_position`), which puts it back on
//! Autoposition.

use egui::Ui;
use reda_engine::model::{Project, Uuid};
use serde_json::{Value, json};

use crate::tr;

/// What the Unit rows show for the one Unit selected.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitRows {
    pub unit: Uuid,
    pub component: Uuid,
    pub document: Uuid,
    /// The Unit's name in its Symbol, such as `A`.
    pub name: String,
    /// How many Units the Symbol has.
    pub count: usize,
    /// The Units of the Component placed on no sheet, by name, in Symbol order.
    pub unplaced: Vec<String>,
    /// The fields moved by hand, by name.
    pub moved: Vec<String>,
}

/// The rows for `selection` when it is one Unit.
pub fn rows(project: &Project, selection: &[Uuid]) -> Option<UnitRows> {
    let [uuid] = selection else {
        return None;
    };
    let sheets = || {
        project
            .documents()
            .filter_map(|d| project.schematic_sheet(d.uuid))
    };
    let (sheet, unit) = sheets().find_map(|s| s.unit(*uuid).map(|u| (s, u)))?;
    let symbol = &sheet.part(unit.part)?.symbol;
    let placed: Vec<Uuid> = sheets()
        .flat_map(|s| s.units())
        .filter(|u| u.component.uuid == unit.component.uuid)
        .map(|u| u.unit)
        .collect();
    Some(UnitRows {
        unit: unit.uuid,
        component: unit.component.uuid,
        document: sheet.uuid,
        name: symbol
            .units
            .iter()
            .find(|u| u.uuid == unit.unit)
            .map(|u| u.name.clone())
            .unwrap_or_default(),
        count: symbol.units.len(),
        unplaced: symbol
            .units
            .iter()
            .filter(|u| !placed.contains(&u.uuid))
            .map(|u| u.name.clone())
            .collect(),
        moved: unit.field_text.iter().map(|f| f.field.clone()).collect(),
    })
}

/// Draws the rows; returns the Commands their buttons ask for.
pub fn section(
    ui: &mut Ui,
    rows: &UnitRows,
    field_label: impl Fn(&str) -> String,
) -> Vec<(&'static str, Value)> {
    let mut out = Vec::new();
    if rows.count > 1 {
        ui.label(tr!(
            "properties-unit-of",
            unit = rows.name.clone(),
            count = rows.count
        ));
        for name in &rows.unplaced {
            ui.horizontal(|ui| {
                ui.weak(tr!("properties-unit-unplaced", unit = name.clone()));
                let button = ui.button(tr!("properties-unit-place"));
                let label = tr!("properties-unit-place-named", unit = name.clone());
                button.widget_info(|| {
                    egui::WidgetInfo::labeled(egui::Role::Button, true, label.clone())
                });
                if button.clicked() {
                    out.push((
                        "tool.place_unit",
                        json!({"component": rows.component, "unit": name,
                               "document": rows.document}),
                    ));
                }
            });
        }
    }
    for field in &rows.moved {
        ui.horizontal(|ui| {
            let label = field_label(field);
            ui.weak(tr!("properties-field-moved", field = label.clone()));
            let button = ui.button(tr!("properties-field-reset"));
            let name = tr!("properties-field-reset-named", field = label);
            button
                .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, name.clone()));
            if button.clicked() {
                out.push((
                    "schematic.field.reset_position",
                    json!({"unit": rows.unit, "field": field}),
                ));
            }
        });
    }
    out
}
