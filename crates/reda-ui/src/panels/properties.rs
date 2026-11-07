//! The Properties panel: the selection's shared properties ([`Properties`]), with
//! `<mixed>` where the selected objects differ. Every edit becomes one Command that
//! applies to everything selected; nothing here changes the Project directly.

use egui::{Checkbox, ComboBox, Grid, Id, Key, Role, TextEdit, Ui, WidgetInfo};
use reda_engine::model::Uuid;
use serde_json::{Value, json};

use crate::placement::Placing;
use crate::properties::{ComponentProperties, Edit, MIXED, NetClassProperty, Properties, Shared};
use crate::shell::Shell;
use crate::tr;

/// Draws the panel for the Shell's selection; returns the Commands its edits ask for.
pub fn show(ui: &mut Ui, shell: &Shell) -> Vec<(&'static str, Value)> {
    if let Some(placing) = crate::placement::placing(shell) {
        return placing_section(ui, placing);
    }
    if shell.selection().is_empty() {
        return crate::selection::filter_ui::properties(ui, shell);
    }
    let Some(project) = shell.engine().project() else {
        ui.weak(tr!("properties-nothing-selected"));
        return Vec::new();
    };
    let properties = Properties::of(project, shell.selection());
    if properties.count == 0 {
        ui.weak(tr!("properties-nothing-selected"));
        return Vec::new();
    }
    let mut edits = Vec::new();
    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.label(tr!("properties-selected", count = properties.count));
        for (kind, n) in &properties.kinds {
            ui.weak(format!("{kind} × {n}"));
        }
        if let Some(component) = &properties.component {
            ui.separator();
            component_section(ui, component, &mut edits);
        }
        if let Some(class) = &properties.net_class {
            ui.separator();
            net_class_row(ui, class, &mut edits);
        }
        if properties.component.is_none() && !properties.details.is_empty() {
            ui.separator();
            Grid::new("properties-details")
                .num_columns(2)
                .show(ui, |ui| {
                    for (name, value) in &properties.details {
                        ui.weak(name);
                        ui.label(value);
                        ui.end_row();
                    }
                });
        }
    });
    edits
        .iter()
        .filter_map(|edit| properties.command(edit))
        .collect()
}

/// While a Part is being placed (Tab opens the panel then): its Designator, value and
/// Footprint, which `tool.place.set` changes for this and later placements.
fn placing_section(ui: &mut Ui, placing: &Placing) -> Vec<(&'static str, Value)> {
    let mut out = Vec::new();
    ui.label(tr!(
        "properties-placing",
        part = placing.copy.part.name.clone()
    ));
    ui.separator();
    Grid::new("properties-placing")
        .num_columns(2)
        .show(ui, |ui| {
            let designator = Shared::Same(placing.designator.clone().unwrap_or_default());
            let label = field_label("designator");
            if let Some(text) = text_field(ui, "placing-designator", &label, &designator, true) {
                out.push(("tool.place.set", json!({ "designator": text })));
            }
            ui.end_row();
            let value = placing
                .fields
                .get("value")
                .cloned()
                .unwrap_or_else(|| placing.copy.part.fields.value.clone());
            let label = field_label("value");
            if let Some(text) = text_field(ui, "placing-value", &label, &Shared::Same(value), true)
            {
                out.push(("tool.place.set", json!({ "fields": {"value": text} })));
            }
            ui.end_row();
            ui.label(tr!("properties-footprint"));
            let current = placing
                .footprint
                .or_else(|| placing.copy.part.footprints.first().copied());
            let name = |uuid: Uuid| {
                placing
                    .copy
                    .footprints
                    .iter()
                    .find(|f| f.footprint.uuid == uuid)
                    .map_or_else(|| uuid.to_string(), |f| f.footprint.name.clone())
            };
            let selected = current.map_or_else(|| tr!("properties-no-footprint"), name);
            let response = ComboBox::from_id_salt("properties-placing-footprint")
                .selected_text(selected)
                .show_ui(ui, |ui| {
                    for f in &placing.copy.footprints {
                        let uuid = f.footprint.uuid;
                        let chosen = current == Some(uuid);
                        if ui.selectable_label(chosen, &f.footprint.name).clicked() && !chosen {
                            out.push(("tool.place.set", json!({ "footprint": uuid })));
                        }
                    }
                })
                .response;
            let label = tr!("properties-footprint");
            response.widget_info(|| WidgetInfo::labeled(Role::ComboBox, true, label.clone()));
            ui.end_row();
        });
    out
}

/// A field's label: the built-in ones translated, free-form ones as named.
fn field_label(name: &str) -> String {
    match name {
        "designator" => tr!("properties-designator"),
        "value" => tr!("properties-value"),
        "description" => tr!("properties-description"),
        "datasheet" => tr!("properties-datasheet"),
        "manufacturer" => tr!("properties-manufacturer"),
        "mpn" => tr!("properties-mpn"),
        "keywords" => tr!("properties-keywords"),
        other => other.to_owned(),
    }
}

fn component_section(ui: &mut Ui, component: &ComponentProperties, edits: &mut Vec<Edit>) {
    Grid::new("properties-component")
        .num_columns(3)
        .show(ui, |ui| {
            for row in &component.fields {
                let label = field_label(&row.name);
                if let Some(text) = text_field(ui, &row.name, &label, &row.value, row.editable) {
                    edits.push(Edit::Field(row.name.clone(), text));
                }
                // Edited from the board, which fields show on the sheet is not set here.
                if !component.footprints.is_empty() {
                    ui.end_row();
                    continue;
                }
                let mixed = row.visible == Shared::Mixed;
                let mut shown = row.visible.same().copied().unwrap_or(false);
                let response = ui.add(Checkbox::without_text(&mut shown).indeterminate(mixed));
                let name = tr!("properties-show-field", field = label.clone());
                response.widget_info(|| WidgetInfo::labeled(Role::CheckBox, true, name.clone()));
                if response
                    .on_hover_text(tr!("properties-show-field-hint"))
                    .clicked()
                {
                    edits.push(Edit::Visible(row.name.clone(), shown));
                }
                ui.end_row();
            }

            ui.label(tr!("properties-footprint"));
            footprint_combo(ui, component, edits);
            ui.end_row();
        });

    flag(
        ui,
        &component.in_bom,
        tr!("properties-in-bom"),
        Edit::InBom,
        edits,
    );
    flag(
        ui,
        &component.in_cpl,
        tr!("properties-in-cpl"),
        Edit::InCpl,
        edits,
    );
    flag(
        ui,
        &component.not_fitted,
        tr!("properties-not-fitted"),
        Edit::NotFitted,
        edits,
    );

    new_field(ui, edits);
}

/// A Component flag's tick box, indeterminate while the selection is mixed.
fn flag(
    ui: &mut Ui,
    value: &Shared<bool>,
    label: String,
    edit: fn(bool) -> Edit,
    edits: &mut Vec<Edit>,
) {
    let mixed = *value == Shared::Mixed;
    let mut on = value.same().copied().unwrap_or(false);
    if ui
        .add(Checkbox::new(&mut on, label).indeterminate(mixed))
        .clicked()
    {
        edits.push(edit(on));
    }
}

fn footprint_combo(ui: &mut Ui, component: &ComponentProperties, edits: &mut Vec<Edit>) {
    let selected = match &component.footprint {
        Shared::Mixed => MIXED.to_owned(),
        Shared::Same(None) => tr!("properties-no-footprint"),
        Shared::Same(Some(uuid)) => component
            .alternates
            .iter()
            .find(|a| a.footprint == *uuid)
            .map_or_else(|| uuid.to_string(), |a| a.name.clone()),
    };
    let enabled = !component.alternates.is_empty();
    let response = ui
        .add_enabled_ui(enabled, |ui| {
            ComboBox::from_id_salt("properties-footprint")
                .selected_text(selected)
                .show_ui(ui, |ui| {
                    for alternate in &component.alternates {
                        let current =
                            component.footprint.same() == Some(&Some(alternate.footprint));
                        if ui.selectable_label(current, &alternate.name).clicked() && !current {
                            edits.push(Edit::Footprint(alternate.footprint));
                        }
                    }
                })
                .response
        })
        .inner;
    let name = tr!("properties-footprint");
    response.widget_info(|| WidgetInfo::labeled(Role::ComboBox, enabled, name.clone()));
    if !enabled {
        response.on_disabled_hover_text(tr!("properties-footprint-different-parts"));
    }
}

fn net_class_row(ui: &mut Ui, class: &NetClassProperty, edits: &mut Vec<Edit>) {
    let name_of = |uuid: &Uuid| {
        class
            .classes
            .iter()
            .find(|(u, _)| u == uuid)
            .map_or_else(|| uuid.to_string(), |(_, name)| name.clone())
    };
    let selected = match &class.class {
        Shared::Mixed => MIXED.to_owned(),
        Shared::Same(list) if list.is_empty() => tr!("properties-default-net-class"),
        Shared::Same(list) => list.iter().map(name_of).collect::<Vec<_>>().join(", "),
    };
    Grid::new("properties-net-class")
        .num_columns(2)
        .show(ui, |ui| {
            ui.label(tr!("properties-net-class"));
            let response = ComboBox::from_id_salt("properties-net-class")
                .selected_text(selected)
                .show_ui(ui, |ui| {
                    for (uuid, name) in &class.classes {
                        let member = class.class.same().is_some_and(|c| c.contains(uuid));
                        if ui.selectable_label(member, name).clicked() {
                            edits.push(Edit::NetClass(*uuid));
                        }
                    }
                })
                .response;
            let name = tr!("properties-net-class");
            response.widget_info(|| WidgetInfo::labeled(Role::ComboBox, true, name.clone()));
            ui.end_row();
        });
}

/// What a text field holds between frames: the value it was showing and what the user has
/// typed since.
#[derive(Clone, Default)]
struct Draft {
    shown: Option<String>,
    text: String,
}

/// A labelled one-line field for a value that may be mixed. It returns the new text when
/// the user commits an edit (Enter or leaving the field); Escape puts the value back.
fn text_field(
    ui: &mut Ui,
    key: &str,
    label: &str,
    value: &Shared<String>,
    enabled: bool,
) -> Option<String> {
    let id = Id::NULL.with(("properties-field", key));
    let shown = value.same().cloned();
    let mut draft: Draft = ui.data(|d| d.get_temp(id)).unwrap_or_default();
    let focused = ui.memory(|m| m.has_focus(id));
    if draft.shown != shown && !focused {
        draft = Draft {
            text: shown.clone().unwrap_or_default(),
            shown: shown.clone(),
        };
    }
    let label = ui.label(label);
    let mut edit = TextEdit::singleline(&mut draft.text)
        .id(id)
        .desired_width(160.0);
    if shown.is_none() {
        edit = edit.hint_text(MIXED);
    }
    let response = ui.add_enabled(enabled, edit).labelled_by(label.id);
    let mut committed = None;
    if response.lost_focus() {
        if ui.input(|i| i.key_pressed(Key::Escape)) {
            draft.text = shown.clone().unwrap_or_default();
        } else {
            let changed = match &shown {
                Some(v) => draft.text != *v,
                None => !draft.text.is_empty(),
            };
            if changed {
                committed = Some(draft.text.clone());
            }
        }
    }
    ui.data_mut(|d| d.insert_temp(id, draft));
    committed
}

/// Adds a free-form field (MPN, `LCSC`, notes…) to every selected Component.
fn new_field(ui: &mut Ui, edits: &mut Vec<Edit>) {
    let id = Id::NULL.with("properties-new-field");
    let (mut name, mut value): (String, String) = ui.data(|d| d.get_temp(id)).unwrap_or_default();
    ui.horizontal(|ui| {
        let name_label = ui.label(tr!("properties-new-field-name"));
        ui.add(TextEdit::singleline(&mut name).desired_width(80.0))
            .labelled_by(name_label.id);
        let value_label = ui.label(tr!("properties-new-field-value"));
        ui.add(TextEdit::singleline(&mut value).desired_width(80.0))
            .labelled_by(value_label.id);
        let ready = !name.trim().is_empty() && !value.is_empty();
        if ui
            .add_enabled(ready, egui::Button::new(tr!("properties-add-field")))
            .clicked()
        {
            edits.push(Edit::Field(name.trim().to_owned(), value.clone()));
            name.clear();
            value.clear();
        }
    });
    ui.data_mut(|d| d.insert_temp(id, (name, value)));
}
