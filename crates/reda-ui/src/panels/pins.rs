//! The Pins Panel: a dockable table that follows the active Library editor, so it stays in
//! view while drawing. On a Symbol it lists every Pin (number, name, type, Unit, side,
//! length, description, decorations, name and number shown), sorts by several columns
//! (a heading click sorts by it, Shift+click adds it), edits a cell on every selected
//! row at once, pastes rows through the column-mapping paste, and jumps to a Pin. On a
//! Part it holds the Part's pin-pad map. Edits come back as Invocations.

pub mod rows;

use egui::{ComboBox, Grid, Id, Role, ScrollArea, Ui, WidgetInfo};
use reda_engine::model::{InsideEdge, LibraryElement, OutsideEdge, Pin, PinType, Symbol, Uuid};
use serde_json::json;

pub use rows::{CellValue, PinColumn, PinSide, Sort, edit, sorted};

use crate::element_editor::{self, Invocation};
use crate::i18n;
use crate::library_dialogs::{PasteMapping, PasteOutcome};
use crate::shell::Shell;
use crate::tr;

/// The Pins Panel's view state.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PinsPanel {
    pub sort: Sort,
    /// The column-mapping paste, while it is open, and the Symbol it pastes into.
    pub paste: Option<(Uuid, PasteMapping)>,
}

/// What the Panel follows: the active Document, when it is an open Symbol or Part.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Follows {
    Symbol(Uuid),
    Part(Uuid),
    Nothing,
}

/// The editor the Panel follows now.
pub fn follows(shell: &Shell) -> Follows {
    let Some(active) = shell.layout().active_document() else {
        return Follows::Nothing;
    };
    match shell.engine().element(active) {
        Some(LibraryElement::Symbol(_)) => Follows::Symbol(active),
        Some(LibraryElement::Part(_)) => Follows::Part(active),
        _ => Follows::Nothing,
    }
}

/// The Panel's state.
pub fn state(shell: &Shell) -> PinsPanel {
    shell
        .engine()
        .view()
        .get::<PinsPanel>()
        .cloned()
        .unwrap_or_default()
}

fn state_mut(shell: &mut Shell) -> &mut PinsPanel {
    let view = shell.engine.view_mut();
    if view.get::<PinsPanel>().is_none() {
        view.set(PinsPanel::default());
    }
    view.get_mut::<PinsPanel>().expect("just set")
}

/// A click on `column`'s heading ([`Sort::click`]).
pub fn sort_by(shell: &mut Shell, column: PinColumn, add: bool) {
    state_mut(shell).sort.click(column, add);
}

/// Opens the column-mapping paste for `symbol` on `text`.
pub fn open_paste(shell: &mut Shell, symbol: Uuid, text: &str) {
    state_mut(shell).paste = Some((symbol, PasteMapping::new(text)));
}

/// The column-mapping paste, while open.
pub fn paste_mut(shell: &mut Shell) -> Option<&mut PasteMapping> {
    state_mut(shell).paste.as_mut().map(|(_, m)| m)
}

/// Applies the open column-mapping paste to its Symbol (one `batch`) and closes it.
pub fn apply_paste(shell: &mut Shell) -> Option<Invocation> {
    let (symbol, mapping) = state_mut(shell).paste.take()?;
    let pins = match mapping.pins() {
        Ok(pins) => pins,
        Err(why) => {
            shell.report(tr!("pin-table-paste-error", error = why));
            return None;
        }
    };
    match crate::symbol_editor::paste_command(shell, symbol, &pins) {
        Ok((command, _)) => Some(command),
        Err(why) => {
            shell.report(tr!("pin-table-paste-error", error = why));
            None
        }
    }
}

/// Jump to Pin: shows the Pin's Unit, selects the Pin and frames it on the canvas.
pub fn jump_to_pin(symbol: &Symbol, pin: &Pin) -> Vec<Invocation> {
    vec![
        (
            "library.symbol.unit.show".to_owned(),
            json!({"symbol": symbol.uuid, "unit": pin.unit}),
        ),
        ("selection.set".to_owned(), json!({"objects": [pin.uuid]})),
        (
            "view.zoom_to".to_owned(),
            json!({"document": symbol.uuid, "objects": [pin.uuid]}),
        ),
    ]
}

/// Draws the Panel; returns the Commands its controls ask for.
pub fn show(ui: &mut Ui, shell: &mut Shell) -> Vec<Invocation> {
    let mut out = Vec::new();
    match follows(shell) {
        Follows::Symbol(symbol) => symbol_table(ui, shell, symbol, &mut out),
        Follows::Part(part) => crate::part_editor::view::pin_map_panel(ui, shell, part, &mut out),
        Follows::Nothing => {
            ui.weak(tr!("pins-panel-empty"));
        }
    }
    out
}

fn symbol_table(ui: &mut Ui, shell: &mut Shell, symbol: Uuid, out: &mut Vec<Invocation>) {
    let Some(s) = crate::symbol_editor::symbol_of(shell.engine(), symbol).cloned() else {
        return;
    };
    let locked = element_editor::read_only(shell.engine(), symbol);
    let panel = state(shell);
    let selected = element_editor::selected_on(shell, symbol);
    ui.horizontal(|ui| {
        ui.strong(tr!("pins-panel-title", symbol = s.name.as_str()));
        let paste = ui.add_enabled(!locked, egui::Button::new(tr!("pin-table-paste")));
        if paste.clicked() {
            open_paste(shell, symbol, "");
        }
    });
    ui.weak(tr!("pins-panel-hint"));
    let id = Id::NULL.with(("pins-panel", symbol));
    ScrollArea::both()
        .id_salt(id)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            Grid::new(id.with("grid"))
                .striped(true)
                .num_columns(PinColumn::ALL.len() + 2)
                .show(ui, |ui| {
                    ui.label("");
                    ui.label("");
                    for column in PinColumn::ALL {
                        heading(ui, shell, &panel.sort, column);
                    }
                    ui.end_row();
                    for pin in sorted(&s, &panel.sort) {
                        row(ui, &s, pin, &selected, locked, out);
                        ui.end_row();
                    }
                });
        });
    paste_dialog(ui.ctx(), shell, out);
}

/// A column heading: a button that sorts by it, marked with its place in the sort.
fn heading(ui: &mut Ui, shell: &mut Shell, sort: &Sort, column: PinColumn) {
    let title = i18n::translate(column.title_key(), &[]);
    let text = match sort.rank(column) {
        Some((i, true)) => format!("{title} ^{}", i + 1),
        Some((i, false)) => format!("{title} v{}", i + 1),
        None => title.clone(),
    };
    let label = tr!("pins-panel-sort", column = title.as_str());
    let b = ui.add(egui::Button::new(egui::RichText::new(text).strong()).frame(false));
    b.widget_info(|| WidgetInfo::labeled(Role::Button, true, label.clone()));
    if b.on_hover_text(tr!("pins-panel-sort-hint")).clicked() {
        let add = ui.input(|i| i.modifiers.shift);
        sort_by(shell, column, add);
    }
}

fn row(
    ui: &mut Ui,
    s: &Symbol,
    pin: &Pin,
    selected: &[Uuid],
    locked: bool,
    out: &mut Vec<Invocation>,
) {
    let is_selected = selected.contains(&pin.uuid);
    let mut on = is_selected;
    let label = tr!("pins-panel-select", pin = pin.number.as_str());
    let check = ui.checkbox(&mut on, "");
    check.widget_info(|| WidgetInfo::labeled(Role::CheckBox, true, label.clone()));
    if check.changed() {
        let mut objects: Vec<Uuid> = selected.to_vec();
        if on {
            objects.push(pin.uuid);
        } else {
            objects.retain(|o| *o != pin.uuid);
        }
        out.push(("selection.set".to_owned(), json!({ "objects": objects })));
    }
    let jump = ui.small_button(tr!("pins-panel-jump"));
    let label = tr!("pins-panel-jump-to", pin = pin.number.as_str());
    jump.widget_info(|| WidgetInfo::labeled(Role::Button, true, label.clone()));
    if jump.clicked() {
        out.extend(jump_to_pin(s, pin));
    }
    let mut set = |column: PinColumn, value: CellValue| {
        out.extend(edit(s, selected, pin.uuid, column, &value));
    };
    let name = |column: PinColumn| {
        format!(
            "{} {}",
            pin.number,
            i18n::translate(column.title_key(), &[])
        )
    };
    let cell_id = |column: PinColumn| Id::NULL.with(("pin-cell", pin.uuid, column.title_key()));
    ui.add_enabled_ui(!locked, |ui| {
        if let Some(v) = text_cell(
            ui,
            cell_id(PinColumn::Number),
            &name(PinColumn::Number),
            &pin.number,
            48.0,
        ) {
            set(PinColumn::Number, CellValue::Text(v));
        }
    });
    ui.add_enabled_ui(!locked, |ui| {
        if let Some(v) = text_cell(
            ui,
            cell_id(PinColumn::Name),
            &name(PinColumn::Name),
            &pin.name,
            96.0,
        ) {
            set(PinColumn::Name, CellValue::Text(v));
        }
    });
    ui.add_enabled_ui(!locked, |ui| {
        let options: Vec<(PinType, String)> = PinType::ALL
            .iter()
            .map(|t| (*t, pin_type_name(*t)))
            .collect();
        if let Some(t) = combo(
            ui,
            cell_id(PinColumn::Type),
            &name(PinColumn::Type),
            pin.electrical_type,
            &options,
        ) {
            set(PinColumn::Type, CellValue::Type(t));
        }
    });
    ui.add_enabled_ui(!locked, |ui| {
        let options: Vec<(Uuid, String)> =
            (s.units.iter()).map(|u| (u.uuid, u.name.clone())).collect();
        if let Some(u) = combo(
            ui,
            cell_id(PinColumn::Unit),
            &name(PinColumn::Unit),
            pin.unit,
            &options,
        ) {
            set(PinColumn::Unit, CellValue::Unit(u));
        }
    });
    ui.add_enabled_ui(!locked, |ui| {
        let options: Vec<(PinSide, String)> = (PinSide::ALL.iter())
            .map(|side| (*side, i18n::translate(side.title_key(), &[])))
            .collect();
        let side = PinSide::of(pin.rotation);
        if let Some(side) = combo(
            ui,
            cell_id(PinColumn::Side),
            &name(PinColumn::Side),
            side,
            &options,
        ) {
            set(PinColumn::Side, CellValue::Side(side));
        }
    });
    ui.add_enabled_ui(!locked, |ui| {
        let shown = rows::mil(pin.length);
        if let Some(v) = text_cell(
            ui,
            cell_id(PinColumn::Length),
            &name(PinColumn::Length),
            &shown,
            48.0,
        ) && let Some(length) = rows::parse_length(&v)
        {
            set(PinColumn::Length, CellValue::Length(length));
        }
    });
    ui.add_enabled_ui(!locked, |ui| {
        let label = name(PinColumn::Description);
        let typed = text_cell(
            ui,
            cell_id(PinColumn::Description),
            &label,
            &pin.description,
            140.0,
        );
        if let Some(v) = typed {
            set(PinColumn::Description, CellValue::Text(v));
        }
    });
    ui.add_enabled_ui(!locked, |ui| {
        let options = vec![
            (None, tr!("pin-decoration-none")),
            (Some(InsideEdge::Clock), tr!("pin-decoration-clock")),
        ];
        let label = name(PinColumn::InsideEdge);
        if let Some(d) = combo(
            ui,
            cell_id(PinColumn::InsideEdge),
            &label,
            pin.inside_edge,
            &options,
        ) {
            set(PinColumn::InsideEdge, CellValue::InsideEdge(d));
        }
    });
    ui.add_enabled_ui(!locked, |ui| {
        let options = vec![
            (None, tr!("pin-decoration-none")),
            (Some(OutsideEdge::Dot), tr!("pin-decoration-dot")),
            (
                Some(OutsideEdge::ActiveLowInput),
                tr!("pin-decoration-active-low-input"),
            ),
            (
                Some(OutsideEdge::ActiveLowOutput),
                tr!("pin-decoration-active-low-output"),
            ),
        ];
        let label = name(PinColumn::OutsideEdge);
        if let Some(d) = combo(
            ui,
            cell_id(PinColumn::OutsideEdge),
            &label,
            pin.outside_edge,
            &options,
        ) {
            set(PinColumn::OutsideEdge, CellValue::OutsideEdge(d));
        }
    });
    for (column, visible) in [
        (PinColumn::NameVisible, pin.name_visible),
        (PinColumn::NumberVisible, pin.number_visible),
    ] {
        ui.add_enabled_ui(!locked, |ui| {
            let mut shown = visible;
            let label = name(column);
            let check = ui.checkbox(&mut shown, "");
            check.widget_info(|| WidgetInfo::labeled(Role::CheckBox, true, label.clone()));
            if check.changed() {
                set(column, CellValue::Flag(shown));
            }
        });
    }
}

/// A text cell: shows `value`, and while focused keeps what is typed; returns the new
/// value when focus leaves (Enter or Tab) with a change. A long value shows on hover.
fn text_cell(ui: &mut Ui, id: Id, name: &str, value: &str, width: f32) -> Option<String> {
    let mut text: String = ui
        .data(|d| d.get_temp::<String>(id))
        .unwrap_or_else(|| value.to_owned());
    let response = ui.add(
        egui::TextEdit::singleline(&mut text)
            .id(id)
            .desired_width(width),
    );
    response.widget_info(|| WidgetInfo::labeled(Role::TextInput, true, name));
    let response = if value.is_empty() {
        response
    } else {
        response.on_hover_text(value)
    };
    if response.has_focus() {
        ui.data_mut(|d| d.insert_temp(id, text.clone()));
        None
    } else {
        ui.data_mut(|d| d.remove::<String>(id));
        (response.lost_focus() && text != value).then_some(text)
    }
}

/// A drop-down cell over `options`; returns the option picked when it differs.
fn combo<T: Copy + PartialEq>(
    ui: &mut Ui,
    id: Id,
    name: &str,
    current: T,
    options: &[(T, String)],
) -> Option<T> {
    let mut value = current;
    let shown = (options.iter())
        .find(|(v, _)| *v == current)
        .map(|(_, t)| t.clone())
        .unwrap_or_default();
    let name = name.to_owned();
    ComboBox::from_id_salt(id)
        .selected_text(shown)
        .show_ui(ui, |ui| {
            for (v, text) in options {
                ui.selectable_value(&mut value, *v, text);
            }
        })
        .response
        .widget_info(|| WidgetInfo::labeled(Role::ComboBox, true, name.clone()));
    (value != current).then_some(value)
}

/// The column-mapping paste, while open.
fn paste_dialog(ctx: &egui::Context, shell: &mut Shell, out: &mut Vec<Invocation>) {
    let Some(mapping) = paste_mut(shell) else {
        return;
    };
    let id = Id::NULL.with("pins-panel-paste");
    match mapping.show(ctx, id, &tr!("paste-map-apply")) {
        PasteOutcome::Open => {}
        PasteOutcome::Cancel => state_mut(shell).paste = None,
        PasteOutcome::Apply(_) => out.extend(apply_paste(shell)),
    }
}

/// A Pin type's name as the table shows it.
pub fn pin_type_name(t: PinType) -> String {
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
