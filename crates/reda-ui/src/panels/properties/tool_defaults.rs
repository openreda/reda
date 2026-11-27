//! A placing Tool's Object Defaults as Tool options in Properties
//! (docs/spec/v0.3/input-and-defaults.md#changing-a-value-while-placing): the fields of
//! the record the active Tool places from ([`object_defaults::record`]), each edit a
//! Command from [`object_defaults::edit`], which writes the record with Remember last used
//! on and keeps the value until the Tool is left with it off.
//!
//! Tab (`tool.edit_properties`) shows Properties and moves the keyboard to the first
//! field ([`focus_first`]).

use egui::{Id, Role, Ui, WidgetInfo};
use serde_json::{Value, json};

use super::{Shared, text_field, units};
use crate::object_defaults::{self, Field, FieldKind, Group};
use crate::preferences_dialog::{self as prefs, Kind};
use crate::shell::Shell;
use crate::tools::Tool;
use crate::{i18n, tr, widgets};

type Commands = Vec<(&'static str, Value)>;

/// Draws the active Tool's options when it places from a record and has no options of
/// its own here; returns the Commands its edits ask for, or `None` for other Tools.
pub(super) fn show(ui: &mut Ui, shell: &Shell) -> Option<Commands> {
    let groups = object_defaults::record(shell)?;
    ui.label(tr!(
        "properties-tool-options",
        tool = tool_title(shell.tool())
    ));
    ui.separator();
    let mut out = Vec::new();
    rows(ui, shell, &groups, &mut out);
    Some(out)
}

/// The record's rows of the active Tool, under options a Tool draws itself (the Via
/// Tool's type, the Arc Tool's mode).
pub(super) fn rows_of_tool(ui: &mut Ui, shell: &Shell, out: &mut Commands) {
    if let Some(groups) = object_defaults::record(shell) {
        ui.separator();
        rows(ui, shell, &groups, out);
    }
}

/// The Tool's name in the options' heading.
fn tool_title(tool: Tool) -> String {
    let key = format!("properties-tool-{}", tool.id().replace('_', "-"));
    if i18n::has_key(&key) {
        i18n::translate(&key, &[])
    } else {
        tool.id().to_owned()
    }
}

/// The id of the text field of `key`, which Tab focuses.
fn field_id(key: &str) -> Id {
    Id::NULL.with(("properties-field", key))
}

/// Puts the keyboard in the first field of the active Tool's options, if it has any.
pub(crate) fn focus_first(ctx: &egui::Context, shell: &Shell) {
    let Some(groups) = object_defaults::record(shell) else {
        // The Zone Tool's own rows start with its clearance (`zone.rs`).
        if shell.tool() == Tool::Zone {
            ctx.memory_mut(|m| m.request_focus(field_id("zone-tool-clearance")));
        }
        return;
    };
    let defaults = object_defaults::current(shell);
    let first = groups.iter().find_map(|g| {
        let record = object_defaults::value(&defaults, &g.prefix);
        g.fields
            .iter()
            .filter(|f| object_defaults::applies(&g.prefix, f.name, &record))
            .find(|f| typed(f.kind))
            .map(|f| format!("{}.{}", g.prefix, f.name))
    });
    if let Some(key) = first {
        ctx.memory_mut(|m| m.request_focus(field_id(&key)));
    }
}

/// Whether a field is typed into, so it can take the keyboard.
fn typed(kind: FieldKind) -> bool {
    matches!(
        kind,
        FieldKind::Length
            | FieldKind::OptionalLength
            | FieldKind::Count
            | FieldKind::Text
            | FieldKind::Percent
    )
}

fn rows(ui: &mut Ui, shell: &Shell, groups: &[Group], out: &mut Commands) {
    let defaults = object_defaults::current(shell);
    let units = units(shell);
    for group in groups {
        let record = object_defaults::value(&defaults, &group.prefix);
        let fields: Vec<&Field> = group
            .fields
            .iter()
            .filter(|f| object_defaults::applies(&group.prefix, f.name, &record))
            .collect();
        widgets::form(("tool-defaults", &group.prefix)).show(ui, |ui| {
            for field in fields.iter().filter(|f| f.kind != FieldKind::Toggle) {
                let key = format!("{}.{}", group.prefix, field.name);
                let current = &record[field.name];
                if let Some(value) = row(ui, &key, field, current, units) {
                    out.push(object_defaults::edit(shell, &key, value));
                }
                ui.end_row();
            }
        });
        for field in fields.iter().filter(|f| f.kind == FieldKind::Toggle) {
            let key = format!("{}.{}", group.prefix, field.name);
            let mut on = record[field.name].as_bool().unwrap_or(false);
            if ui
                .add(widgets::checkbox(&mut on, label(&key, field.name)))
                .clicked()
            {
                out.push(object_defaults::edit(shell, &key, json!(on)));
            }
        }
    }
}

/// One form row; returns the new value when it was edited.
fn row(
    ui: &mut Ui,
    key: &str,
    field: &Field,
    current: &Value,
    units: reda_engine::preferences::Units,
) -> Option<Value> {
    let label = label(key, field.name);
    match field.kind {
        FieldKind::Length | FieldKind::OptionalLength => {
            // In the unit it was typed in; a bare number typed is in the panel's units.
            let shown = object_defaults::shown_value(current, units, false);
            let typed = text_field(ui, key, &label, &Shared::Same(shown), true)?;
            let text = typed.trim();
            if text.is_empty() {
                // Blank unsets an optional length; a required one keeps its value.
                return (field.kind == FieldKind::OptionalLength).then_some(Value::Null);
            }
            Some(object_defaults::typed_value(text, units, false))
        }
        FieldKind::Count => {
            let shown = current.as_i64().map(|n| n.to_string()).unwrap_or_default();
            let typed = text_field(ui, key, &label, &Shared::Same(shown), true)?;
            Some(
                typed
                    .trim()
                    .parse::<i64>()
                    .map_or_else(|_| json!(typed), |n| json!(n)),
            )
        }
        FieldKind::Percent => {
            // Thousandths of a percent, shown and typed in percent.
            let shown = current
                .as_f64()
                .map(|r| format!("{} %", r / 1000.0))
                .unwrap_or_default();
            let typed = text_field(ui, key, &label, &Shared::Same(shown), true)?;
            let number = typed
                .trim()
                .trim_end_matches('%')
                .trim()
                .parse::<f64>()
                .ok()?;
            Some(json!((number * 1000.0).round() as i64))
        }
        FieldKind::Text => {
            let shown = current.as_str().unwrap_or_default().to_owned();
            let typed = text_field(ui, key, &label, &Shared::Same(shown), true)?;
            Some(json!(typed.trim()))
        }
        FieldKind::Choice => choice(ui, key, field.name, &label, current),
        FieldKind::Toggle => None,
    }
}

/// A choice among the schema's values for `key`.
fn choice(ui: &mut Ui, key: &str, name: &str, label: &str, current: &Value) -> Option<Value> {
    let Kind::Choice(choices) = prefs::setting(key).kind else {
        return None;
    };
    let title = |value: &Value| {
        let text = value.as_str().unwrap_or_default();
        // A Pin type has its own names, as Pin Properties shows them.
        let id = match name {
            "electrical_type" => format!("pin-type-{text}"),
            _ => format!("tool-option-{name}-{text}"),
        }
        .replace('_', "-");
        if i18n::has_key(&id) {
            i18n::translate(&id, &[])
        } else {
            choices
                .iter()
                .find(|c| c.value == *value)
                .map_or_else(|| text.to_owned(), |c| c.label.clone())
        }
    };
    ui.add(widgets::form_name(label));
    let mut picked = None;
    let response = widgets::dropdown(("tool-option", key), title(current))
        .show(ui, |ui| {
            for c in &choices {
                let selected = c.value == *current;
                if ui.add(widgets::choice(selected, title(&c.value))).clicked() && !selected {
                    picked = Some(c.value.clone());
                }
            }
        })
        .response;
    response.widget_info(|| WidgetInfo::labeled(Role::ComboBox, true, label));
    picked
}

/// A field's label: the catalogue's `tool-option-<field>`, else the setting's own.
fn label(key: &str, name: &str) -> String {
    let id = format!("tool-option-{}", name.replace('_', "-"));
    if i18n::has_key(&id) {
        i18n::translate(&id, &[])
    } else {
        prefs::setting(key).label
    }
}
