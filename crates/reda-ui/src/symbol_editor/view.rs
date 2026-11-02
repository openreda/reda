//! The Symbol editor's panels around the shared canvas: the Unit tabs (and the Text
//! Tool's typing) over it, the pin table with its paste box under it. Nothing here runs a
//! Command: edits come back as Invocations for the window to run after the frame.

use egui::{Id, RichText, Role, Ui, WidgetInfo};
use reda_engine::model::geom::{Millideg, Nm};
use reda_engine::model::{Pin, PinType, Symbol, Uuid};
use serde_json::{Value, json};

use super::{canvas, commit_text, paste_command, set_text, shown_unit, symbol_of};
use crate::element_editor::{self, Invocation};
use crate::i18n;
use crate::shell::Shell;
use crate::tr;

/// The pin table's share of the tab's height, at first.
const TABLE_SHARE: f32 = 0.32;
const MIL: f64 = 25_400.0;

/// Draws the panels of `symbol`'s tab into `ui`, leaving the rest for the canvas.
pub(crate) fn panels(ui: &mut Ui, shell: &mut Shell, symbol: Uuid) -> Vec<Invocation> {
    let mut out = Vec::new();
    egui::Panel::top(Id::NULL.with(("symbol-units", symbol)))
        .resizable(false)
        .show(ui, |ui| unit_tabs(ui, shell, symbol, &mut out));
    let height = (ui.available_height() * TABLE_SHARE).max(96.0);
    egui::Panel::bottom(Id::NULL.with(("symbol-pins", symbol)))
        .resizable(true)
        .default_size(height)
        .show(ui, |ui| pin_table(ui, shell, symbol, &mut out));
    out
}

fn unit_tabs(ui: &mut Ui, shell: &mut Shell, symbol: Uuid, out: &mut Vec<Invocation>) {
    let Some(s) = symbol_of(shell.engine(), symbol).cloned() else {
        return;
    };
    let shown = shown_unit(shell, symbol);
    let locked = element_editor::read_only(shell.engine(), symbol);
    ui.horizontal(|ui| {
        for unit in &s.units {
            let label = tr!("symbol-unit-tab", unit = unit.name.as_str());
            if ui
                .selectable_label(shown == Some(unit.uuid), label)
                .clicked()
            {
                out.push((
                    "library.symbol.unit.show".to_owned(),
                    json!({"symbol": symbol, "unit": unit.uuid}),
                ));
            }
        }
        let add = ui.add_enabled(!locked, egui::Button::new(tr!("symbol-unit-add")));
        if add.clicked() {
            out.push((
                "library.symbol.unit.add".to_owned(),
                json!({ "symbol": symbol }),
            ));
        }
        if locked {
            ui.weak(tr!("symbol-read-only"));
        }
        text_entry(ui, shell, symbol, out);
    });
}

/// The Text Tool's typing: Enter places the text, Escape or leaving the field drops it.
fn text_entry(ui: &mut Ui, shell: &mut Shell, symbol: Uuid, out: &mut Vec<Invocation>) {
    let Some((at, mut text)) = canvas(shell, symbol).text else {
        return;
    };
    ui.separator();
    let edit = ui.add(
        egui::TextEdit::singleline(&mut text)
            .id(Id::NULL.with(("symbol-text", symbol)))
            .hint_text(tr!("symbol-text-hint"))
            .desired_width(200.0),
    );
    edit.widget_info(|| WidgetInfo::labeled(Role::TextInput, true, tr!("symbol-text-hint")));
    if !edit.has_focus() && !edit.lost_focus() {
        edit.request_focus();
    }
    if edit.lost_focus() {
        if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            set_text(shell, symbol, Some((at, text.clone())));
            out.extend(commit_text(shell, symbol, &text));
        } else {
            set_text(shell, symbol, None);
        }
    } else {
        set_text(shell, symbol, Some((at, text)));
    }
}

/// The pin table: the shown Unit's Pins, each cell editable, then the paste box.
fn pin_table(ui: &mut Ui, shell: &mut Shell, symbol: Uuid, out: &mut Vec<Invocation>) {
    let Some(s) = symbol_of(shell.engine(), symbol).cloned() else {
        return;
    };
    let unit = shown_unit(shell, symbol);
    let locked = element_editor::read_only(shell.engine(), symbol);
    let mut pins: Vec<&Pin> = s.pins.iter().filter(|p| Some(p.unit) == unit).collect();
    pins.sort_by(|a, b| reda_engine::schematic::natural_cmp(&a.number, &b.number));
    let paste_height = if locked { 0.0 } else { 64.0 };
    egui::ScrollArea::vertical()
        .id_salt(("pin-table", symbol))
        .auto_shrink([false, true])
        .max_height((ui.available_height() - paste_height).max(32.0))
        .show(ui, |ui| {
            ui.add_enabled_ui(!locked, |ui| {
                egui::Grid::new(("pin-table-grid", symbol))
                    .striped(true)
                    .num_columns(9)
                    .show(ui, |ui| {
                        for key in [
                            "pin-table-number",
                            "pin-table-name",
                            "pin-table-type",
                            "pin-table-x",
                            "pin-table-y",
                            "pin-table-rotation",
                            "pin-table-length",
                            "pin-table-name-visible",
                            "pin-table-number-visible",
                        ] {
                            ui.label(RichText::new(i18n::translate(key, &[])).strong());
                        }
                        ui.end_row();
                        for pin in pins {
                            pin_row(ui, &s, pin, out);
                            ui.end_row();
                        }
                    });
            });
        });
    if !locked {
        paste_box(ui, shell, symbol, out);
    }
}

fn pin_row(ui: &mut Ui, s: &Symbol, pin: &Pin, out: &mut Vec<Invocation>) {
    let edit = |field: &str, value: Value| {
        (
            "library.symbol.pin.edit".to_owned(),
            json!({"symbol": s.uuid, "pin": pin.uuid, field: value}),
        )
    };
    let id = |column: &str| Id::NULL.with(("pin-cell", pin.uuid, column.to_owned()));
    // Each cell is named for screen readers and tests: "<number> <column>".
    let name = |key: &str| format!("{} {}", pin.number, i18n::translate(key, &[]));
    if let Some(v) = cell(
        ui,
        id("number"),
        &name("pin-table-number"),
        &pin.number,
        48.0,
    ) {
        out.push(edit("number", json!(v)));
    }
    if let Some(v) = cell(ui, id("name"), &name("pin-table-name"), &pin.name, 96.0) {
        out.push(edit("name", json!(v)));
    }
    let mut kind = pin.electrical_type;
    let label = name("pin-table-type");
    egui::ComboBox::from_id_salt(id("type"))
        .selected_text(pin_type_name(kind))
        .show_ui(ui, |ui| {
            for t in PinType::ALL {
                ui.selectable_value(&mut kind, *t, pin_type_name(*t));
            }
        })
        .response
        .widget_info(|| WidgetInfo::labeled(Role::ComboBox, true, label.clone()));
    if kind != pin.electrical_type {
        out.push(edit("electrical_type", json!(kind)));
    }
    if let Some(v) = cell(ui, id("x"), &name("pin-table-x"), &mil(pin.at.x), 56.0) {
        out.push(edit("at", json!({"x": length(&v), "y": pin.at.y.0})));
    }
    if let Some(v) = cell(ui, id("y"), &name("pin-table-y"), &mil(pin.at.y), 56.0) {
        out.push(edit("at", json!({"x": pin.at.x.0, "y": length(&v)})));
    }
    let mut rotation = pin.rotation;
    let label = name("pin-table-rotation");
    egui::ComboBox::from_id_salt(id("rotation"))
        .selected_text(format!("{}°", rotation.0 / 1000))
        .width(56.0)
        .show_ui(ui, |ui| {
            for deg in [0, 90, 180, 270] {
                ui.selectable_value(&mut rotation, Millideg(deg * 1000), format!("{deg}°"));
            }
        })
        .response
        .widget_info(|| WidgetInfo::labeled(Role::ComboBox, true, label.clone()));
    if rotation != pin.rotation {
        out.push(edit("rotation", json!(rotation.0)));
    }
    let length_name = name("pin-table-length");
    if let Some(v) = cell(ui, id("length"), &length_name, &mil(pin.length), 56.0) {
        out.push(edit("length", json!(length(&v))));
    }
    for (field, key, visible) in [
        ("name_visible", "pin-table-name-visible", pin.name_visible),
        (
            "number_visible",
            "pin-table-number-visible",
            pin.number_visible,
        ),
    ] {
        let mut shown = visible;
        let label = name(key);
        let check = ui.checkbox(&mut shown, "");
        check.widget_info(|| WidgetInfo::labeled(Role::CheckBox, true, label.clone()));
        if check.on_hover_text(i18n::translate(key, &[])).changed() {
            out.push(edit(field, json!(shown)));
        }
    }
}

/// A length typed in a cell: bare numbers are mil, as the cells show them.
fn length(text: &str) -> String {
    match text.trim().parse::<f64>() {
        Ok(_) => format!("{}mil", text.trim()),
        Err(_) => text.trim().to_owned(),
    }
}

fn mil(n: Nm) -> String {
    let v = n.0 as f64 / MIL;
    if v.fract() == 0.0 {
        format!("{v:.0}")
    } else {
        format!("{v}")
    }
}

/// A text cell: shows `value`, and while focused keeps what is typed; returns the new
/// value when focus leaves (Enter or Tab) with a change.
fn cell(ui: &mut Ui, id: Id, name: &str, value: &str, width: f32) -> Option<String> {
    let mut text: String = ui
        .data(|d| d.get_temp::<String>(id))
        .unwrap_or_else(|| value.to_owned());
    let response = ui.add(
        egui::TextEdit::singleline(&mut text)
            .id(id)
            .desired_width(width),
    );
    response.widget_info(|| WidgetInfo::labeled(Role::TextInput, true, name));
    if response.has_focus() {
        ui.data_mut(|d| d.insert_temp(id, text.clone()));
        None
    } else {
        ui.data_mut(|d| d.remove::<String>(id));
        (response.lost_focus() && text != value).then_some(text)
    }
}

/// The paste box: pasting rows of Pins into it applies them at once, as one step.
fn paste_box(ui: &mut Ui, shell: &mut Shell, symbol: Uuid, out: &mut Vec<Invocation>) {
    let id = Id::NULL.with(("pin-paste", symbol));
    let mut text: String = ui.data(|d| d.get_temp(id)).unwrap_or_default();
    let response = ui.add(
        egui::TextEdit::multiline(&mut text)
            .id(id)
            .desired_rows(1)
            .desired_width(f32::INFINITY)
            .hint_text(tr!("pin-table-paste-hint")),
    );
    response.widget_info(|| WidgetInfo::labeled(Role::TextInput, true, tr!("pin-table-paste")));
    if text.trim().is_empty() {
        ui.data_mut(|d| d.remove::<String>(id));
        return;
    }
    // A paste of rows (or a typed row ended with Enter) applies them.
    if text.contains('\n') {
        match paste_command(shell, symbol, &text) {
            Ok((command, _)) => out.push(command),
            Err(why) => shell.report(tr!("pin-table-paste-error", error = why)),
        }
        text.clear();
    }
    ui.data_mut(|d| d.insert_temp(id, text));
}

fn pin_type_name(t: PinType) -> String {
    let key = match t {
        PinType::Input => "pin-type-input",
        PinType::Output => "pin-type-output",
        PinType::Bidirectional => "pin-type-bidirectional",
        PinType::TriState => "pin-type-tri-state",
        PinType::Passive => "pin-type-passive",
        PinType::PowerIn => "pin-type-power-in",
        PinType::PowerOut => "pin-type-power-out",
        PinType::OpenCollector => "pin-type-open-collector",
        PinType::OpenEmitter => "pin-type-open-emitter",
        PinType::NotConnected => "pin-type-not-connected",
    };
    i18n::translate(key, &[])
}
