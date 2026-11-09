//! The Properties panel: the selection's shared properties ([`Properties`]), with
//! `<mixed>` where the selected objects differ, in collapsible regions (General,
//! Location, Graphical, Rules, Checks) under kind chips that narrow the selection. With
//! nothing selected it shows the Document's overview ([`overview`]). Every edit becomes
//! one Command that applies to everything selected; nothing here changes the Project
//! directly.

mod overview;
mod rules;
mod unit;
mod zone;

use std::collections::BTreeSet;
use std::sync::Arc;

use egui::{Checkbox, ComboBox, Grid, Id, Key, RichText, Role, TextEdit, Ui, WidgetInfo};
use reda_engine::model::Uuid;
use serde_json::{Value, json};

use crate::placement::Placing;
use reda_engine::model::geom::{Nm, Point};
use reda_engine::preferences::Units;

use crate::panels::messages::Action;
use crate::properties::{
    ComponentProperties, Edit, Kind, MIXED, NetClassProperty, Numbers, Properties, Region, Shared,
    WireVertices,
};
use crate::shell::Shell;
use crate::tr;

/// What the panel asks for after a frame.
#[derive(Clone, Debug, PartialEq)]
pub enum Out {
    /// Runs this Command.
    Run(&'static str, Value),
    /// What a Messages click would ask for: a check run, or a settings page.
    Messages(Action),
}

/// Draws the panel for the Shell's selection; returns what its clicks and edits ask for.
pub fn show(ui: &mut Ui, shell: &Shell) -> Vec<Out> {
    let run = |commands: Vec<(&'static str, Value)>| {
        commands
            .into_iter()
            .map(|(id, params)| Out::Run(id, params))
            .collect()
    };
    if let Some(placing) = crate::placement::placing(shell) {
        return run(placing_section(ui, placing));
    }
    if let Some(commands) = zone::tool_options(ui, shell) {
        return run(commands);
    }
    // A running Tool's options replace the overview.
    let tool = shell.tool();
    if tool != crate::tools::Tool::Select && shell.selection().is_empty() {
        ui.weak(tr!("properties-tool-running", tool = tool.id()));
        return Vec::new();
    }
    if shell.selection().is_empty()
        && shell
            .active_document()
            .is_some_and(|d| crate::bars::document_type(shell, d.document).is_some())
    {
        return overview::show(ui, shell);
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
    let mut out = Vec::new();
    let mut edits = Vec::new();
    let mut numbers = Vec::new();
    // An always-shown bar keeps the width steady, so wrapped values can't make the bar
    // come and go from frame to frame.
    egui::ScrollArea::vertical()
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
        .show(ui, |ui| {
            ui.label(tr!("properties-selected", count = properties.count));
            chips(ui, shell, &properties, &mut out);
            for uuid in &shell.selection().objects {
                if let Some(reda_engine::model::Object::PcbText(text)) = project.object(*uuid) {
                    let omitted = reda_engine::pcb::text_geometry(&text, shell.engine().fonts())
                        .dropped_lines;
                    if omitted > 0 {
                        ui.colored_label(
                            ui.visuals().warn_fg_color,
                            tr!("properties-text-frame-omitted", count = omitted),
                        );
                    }
                }
            }
            let general = properties.component.is_some()
                || properties.net_class.is_some()
                || properties.local_edits.is_some();
            if general {
                region(
                    ui,
                    shell,
                    "general",
                    tr!("properties-region-general"),
                    |ui| {
                        if let Some(component) = &properties.component {
                            component_section(ui, component, &mut edits);
                        }
                        if let Some(rows) = unit::rows(project, &shell.selection().objects) {
                            numbers.extend(unit::section(ui, &rows, field_label));
                        }
                        if let Some(class) = &properties.net_class {
                            net_class_row(ui, class, &mut edits);
                        }
                        if let Some(local) = &properties.local_edits
                            && let Some(revert) = rules::local_edits(ui, local)
                        {
                            out.push(Out::Run(revert.0, revert.1));
                        }
                    },
                );
            }
            for (key, title, region_of) in [
                (
                    "location",
                    tr!("properties-region-location"),
                    Region::Location,
                ),
                (
                    "graphical",
                    tr!("properties-region-graphical"),
                    Region::Graphical,
                ),
            ] {
                if properties
                    .numbers
                    .rows
                    .iter()
                    .any(|r| r.region == region_of)
                {
                    region(ui, shell, key, title, |ui| {
                        numbers_section(
                            ui,
                            key,
                            &properties.numbers,
                            region_of,
                            units(shell),
                            &mut numbers,
                        );
                    });
                }
            }
            if let Some(list) = &properties.vertices {
                region(
                    ui,
                    shell,
                    "vertices",
                    tr!("properties-region-vertices"),
                    |ui| {
                        let grid = shell
                            .active_document()
                            .map_or(crate::grid::SCHEMATIC_DEFAULT, |t| shell.grid(t.document));
                        vertex_section(ui, list, units(shell), grid, &mut edits);
                    },
                );
            }
            zone::zones_section(ui, shell, &mut numbers);
            if let Some(found) = &properties.rules {
                rules::show(ui, shell, found, units(shell), &mut out);
            }
            let mut objects = shell.selection().objects.clone();
            objects.extend(shell.selection().pads.iter().map(|p| p.footprint));
            let violations = crate::properties::overview::violations_on(shell, &objects);
            if !violations.is_empty() {
                region(ui, shell, "checks", tr!("properties-region-checks"), |ui| {
                    for (_, severity, text) in &violations {
                        ui.horizontal(|ui| {
                            overview::severity_dot(ui, *severity);
                            ui.label(text);
                        });
                    }
                });
            }
            if properties.component.is_none()
                && properties.vertices.is_none()
                && !properties.details.is_empty()
            {
                region(ui, shell, "stored", tr!("properties-region-stored"), |ui| {
                    Grid::new("properties-details")
                        .num_columns(2)
                        .show(ui, |ui| {
                            for (name, value) in &properties.details {
                                ui.weak(name);
                                ui.label(value);
                                ui.end_row();
                            }
                        });
                });
            }
        });
    out.extend(
        edits
            .iter()
            .filter_map(|edit| properties.command(edit))
            .chain(numbers)
            .map(|(id, params)| Out::Run(id, params)),
    );
    out
}

/// The kind chips of a mixed selection, "Track (12) · Via (3)": a click narrows the
/// selection to that kind, and the clear button puts back the selection it narrowed.
fn chips(ui: &mut Ui, shell: &Shell, properties: &Properties, out: &mut Vec<Out>) {
    let id = Id::NULL.with("properties-narrowed-from");
    let saved: Option<Arc<crate::selection::Selection>> = ui.data(|d| d.get_temp(id));
    if properties.kinds.len() < 2 && saved.is_none() {
        return;
    }
    ui.horizontal_wrapped(|ui| {
        for (i, (kind, n)) in properties.kinds.iter().enumerate() {
            if i > 0 {
                ui.weak("·");
            }
            let label = format!("{} ({n})", kind_title(kind));
            if ui.button(label).clicked()
                && properties.kinds.len() > 1
                && let Some((cmd, params)) = properties.narrow(kind)
            {
                let before = Arc::new(shell.selection().clone());
                ui.data_mut(|d| d.insert_temp(id, before));
                out.push(Out::Run(cmd, params));
            }
        }
        if let Some(before) = &saved {
            let clear = ui.button(tr!("properties-chips-clear"));
            if clear.clicked() {
                let params = serde_json::to_value(before.as_ref()).unwrap_or_default();
                out.push(Out::Run("selection.set", params));
                ui.data_mut(|d| d.remove::<Arc<crate::selection::Selection>>(id));
            }
        }
    });
    // A selection made elsewhere forgets what the chips narrowed.
    if let Some(before) = saved
        && properties.kinds.len() > 1
        && *before != *shell.selection()
    {
        ui.data_mut(|d| d.remove::<Arc<crate::selection::Selection>>(id));
    }
}

/// An object kind's name for people: `sheet_text` reads "Sheet Text".
fn kind_title(kind: &str) -> String {
    kind.split('_')
        .map(|w| {
            let mut c = w.chars();
            c.next()
                .map(|f| f.to_uppercase().chain(c).collect::<String>())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// The regions the user collapsed, as `state.json` keeps them; read once per window.
fn collapsed(ui: &Ui, shell: &Shell) -> Arc<BTreeSet<String>> {
    let id = Id::NULL.with("properties-collapsed");
    if let Some(set) = ui.data(|d| d.get_temp::<Arc<BTreeSet<String>>>(id)) {
        return set;
    }
    let set = Arc::new(
        shell
            .engine()
            .user_dirs()
            .map(|dirs| reda_engine::state::read(dirs).properties_collapsed)
            .unwrap_or_default(),
    );
    ui.data_mut(|d| d.insert_temp(id, set.clone()));
    set
}

/// Opens `key` (`open`) or collapses it, and keeps that in `state.json`.
fn set_open(ui: &Ui, shell: &Shell, key: &str, open: bool) {
    let mut set = (*collapsed(ui, shell)).clone();
    match open {
        true => set.remove(key),
        false => set.insert(key.to_owned()),
    };
    ui.data_mut(|d| d.insert_temp(Id::NULL.with("properties-collapsed"), Arc::new(set)));
    if let Some(dirs) = shell.engine().user_dirs() {
        let key = key.to_owned();
        let _ = reda_engine::state::update(dirs, |s| match open {
            true => {
                s.properties_collapsed.remove(&key);
            }
            false => {
                s.properties_collapsed.insert(key);
            }
        });
    }
}

/// One collapsible region, `key` naming it in `state.json`; open unless the user
/// collapsed it.
fn region<R>(
    ui: &mut Ui,
    shell: &Shell,
    key: &str,
    title: String,
    add: impl FnOnce(&mut Ui) -> R,
) -> egui::CollapsingResponse<R> {
    let open = !collapsed(ui, shell).contains(key);
    let response = egui::CollapsingHeader::new(RichText::new(title).strong())
        .id_salt(("properties-region", key))
        .open(Some(open))
        .show(ui, add);
    if response.header_response.clicked() {
        set_open(ui, shell, key, !open);
    }
    response
}

/// The vertex list of the one Wire selected: Index, X and Y for each vertex (X and Y
/// typed as lengths, a bare number in `units`), a Remove button on each row while more
/// than two are left, and Add, which puts a new end one snap grid (`grid`) on.
fn vertex_section(ui: &mut Ui, list: &WireVertices, units: Units, grid: Nm, edits: &mut Vec<Edit>) {
    ui.label(tr!("properties-vertices"));
    let removable = list.vertices.len() > 2;
    Grid::new("properties-vertices")
        .num_columns(4)
        .show(ui, |ui| {
            ui.weak(tr!("properties-vertex-index"));
            ui.weak(tr!("properties-vertex-x"));
            ui.weak(tr!("properties-vertex-y"));
            ui.label("");
            ui.end_row();
            for (i, at) in list.vertices.iter().enumerate() {
                ui.label(i.to_string());
                let axes = [("x", at.x), ("y", at.y)];
                for (axis, value) in axes {
                    let key = format!("vertex-{}-{i}-{axis}", list.wire);
                    let shown = Shared::Same(crate::pcb::view::format_length(value, units));
                    let label = tr!("properties-vertex-field", index = i as i64, axis = axis);
                    if let Some(text) = vertex_field(ui, &key, &label, &shown)
                        && let Some(length) = parse_length(&text, units)
                    {
                        let to = match axis {
                            "x" => Point::new(length, at.y),
                            _ => Point::new(at.x, length),
                        };
                        edits.push(Edit::Vertex(i, to));
                    }
                }
                let remove = ui
                    .add_enabled(
                        removable,
                        egui::Button::new(tr!("properties-vertex-remove")),
                    )
                    .on_hover_text(tr!("properties-vertex-remove-tip", index = i as i64));
                if remove.clicked() {
                    edits.push(Edit::RemoveVertex(i));
                }
                ui.end_row();
            }
        });
    if ui.button(tr!("properties-vertex-add")).clicked() {
        edits.push(Edit::AddVertex(grid));
    }
}

/// A coordinate field in the vertex list, labelled for accessibility but drawn without
/// its own label (the column headers name it).
fn vertex_field(ui: &mut Ui, key: &str, label: &str, value: &Shared<String>) -> Option<String> {
    let id = Id::NULL.with(("properties-field", key));
    let shown = value.same().cloned().unwrap_or_default();
    let mut draft: Draft = ui.data(|d| d.get_temp(id)).unwrap_or_default();
    let focused = ui.memory(|m| m.has_focus(id));
    if draft.shown.as_ref() != Some(&shown) && !focused {
        draft = Draft {
            text: shown.clone(),
            shown: Some(shown.clone()),
            committed: None,
        };
    }
    let response = ui.add(
        TextEdit::singleline(&mut draft.text)
            .id(id)
            .desired_width(80.0),
    );
    let name = label.to_owned();
    response.widget_info(|| WidgetInfo::labeled(Role::TextInput, true, name.as_str()));
    if response.changed() {
        draft.committed = None;
    }
    let mut committed = None;
    if response.lost_focus() && draft.text != shown && draft.committed.as_ref() != Some(&draft.text)
    {
        if ui.input(|i| i.key_pressed(Key::Escape)) {
            draft.text = shown.clone();
        } else {
            draft.committed = Some(draft.text.clone());
            committed = Some(draft.text.clone());
        }
    }
    ui.data_mut(|d| d.insert_temp(id, draft));
    committed
}

/// `text` as a length: a bare number in `units`, else a length with its unit.
fn parse_length(text: &str, units: Units) -> Option<Nm> {
    let text = text.trim();
    match text.parse::<f64>() {
        Ok(v) => Some(Nm(match units {
            Units::Mm => (v * 1e6).round() as i64,
            Units::Mil => (v * 25_400.0).round() as i64,
        })),
        Err(_) => Nm::parse(&text.replace(' ', "")).ok(),
    }
}

/// The units a bare number typed into a length field is read in: the board's units on a
/// PCB, else those of the sheet's grid.
fn units(shell: &Shell) -> Units {
    let Some(tab) = shell.active_document() else {
        return Units::Mm;
    };
    if shell.is_pcb(tab.document) {
        shell.board_view().units
    } else if crate::grid::is_imperial(shell.grid(tab.document)) {
        Units::Mil
    } else {
        Units::Mm
    }
}

/// A numeric property's label.
fn number_label(name: &str) -> String {
    match name {
        "height" => tr!("properties-height"),
        "thickness" => tr!("properties-thickness"),
        "rotation" => tr!("properties-rotation"),
        "clearance" => tr!("properties-clearance"),
        "min_width" => tr!("properties-min-width"),
        "x" => tr!("properties-x"),
        "y" => tr!("properties-y"),
        "width" => tr!("properties-width"),
        "length" => tr!("properties-length"),
        "diameter" => tr!("properties-diameter"),
        "hole" => tr!("properties-hole"),
        other => other.to_owned(),
    }
}

/// The selection's numeric properties. Each field takes a numeric expression; it runs per
/// object and commits as one Command (a `batch` for several objects), or, when it fails
/// for any object, commits nothing and says why under the field.
fn numbers_section(
    ui: &mut Ui,
    key: &str,
    numbers: &Numbers,
    region: Region,
    units: Units,
    out: &mut Vec<(&'static str, Value)>,
) {
    Grid::new(("properties-numbers", key))
        .num_columns(2)
        .show(ui, |ui| {
            for row in numbers.rows.iter().filter(|r| r.region == region) {
                let error_id = Id::NULL.with(("properties-number-error", row.name));
                let value = match &row.value {
                    Shared::Mixed => Shared::Mixed,
                    Shared::Same(None) => Shared::Same(String::new()),
                    Shared::Same(Some(v)) => Shared::Same(match row.kind {
                        Kind::Length => crate::pcb::view::format_length(Nm(*v), units),
                        Kind::Angle => format!("{} deg", *v as f64 / 1000.0),
                    }),
                };
                let key = format!("number-{}", row.name);
                let label = number_label(row.name);
                if let Some(text) = text_field(ui, &key, &label, &value, true) {
                    match numbers.command(row.name, &text, units) {
                        Ok(command) => {
                            out.extend(command);
                            ui.data_mut(|d| d.remove::<String>(error_id));
                        }
                        Err(why) => ui.data_mut(|d| {
                            d.insert_temp(error_id, why);
                        }),
                    }
                }
                ui.end_row();
                if let Some(why) = ui.data(|d| d.get_temp::<String>(error_id)) {
                    ui.label("");
                    ui.colored_label(ui.visuals().error_fg_color, why);
                    ui.end_row();
                }
            }
        });
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
    /// The text last committed, until the user types again: focus can be reported lost
    /// on two frames running, and the second must not commit the same text again (an
    /// expression such as `! * 2` would apply twice).
    committed: Option<String>,
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
            committed: None,
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
    if response.changed() {
        draft.committed = None;
    }
    let mut committed = None;
    if response.lost_focus() && draft.committed.as_ref() != Some(&draft.text) {
        if ui.input(|i| i.key_pressed(Key::Escape)) {
            draft.text = shown.clone().unwrap_or_default();
        } else {
            let changed = match &shown {
                Some(v) => draft.text != *v,
                None => !draft.text.is_empty(),
            };
            if changed {
                draft.committed = Some(draft.text.clone());
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
