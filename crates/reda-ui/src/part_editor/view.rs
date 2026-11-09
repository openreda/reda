//! Drawing the Part editor's form: Save and Check, the name and Component Type, the
//! Symbol picker with its preview, the ordered alternate Footprints with theirs and the
//! fields. The pin-pad map is in the Pins Panel ([`pin_map_panel`]). Edits become the Commands of [`super`]; the Edit buttons
//! open the Symbol or a Footprint in its own editor. A starter Library Part shows the same
//! form with the edits disabled.

use egui::{ComboBox, Grid, Id, Role, ScrollArea, TextEdit, Ui, WidgetInfo};
use reda_engine::model::{ComponentType, LibraryElement, Part, Uuid};
use serde_json::json;

use super::{Choice, FootprintMove, PinRow};
use crate::element_editor::{self, Invocation, text_field};
use crate::panels::library::preview;
use crate::shell::Shell;
use crate::tr;

const FIELD_WIDTH: f32 = 260.0;

/// Draws the form for the Part `element`; returns the Commands its controls ask for.
pub(crate) fn show(ui: &mut Ui, shell: &mut Shell, element: Uuid) -> Vec<Invocation> {
    let Some(LibraryElement::Part(part)) = shell.engine().element(element).cloned() else {
        return Vec::new();
    };
    let locked = element_editor::read_only(shell.engine(), element);
    let mut out = Vec::new();
    let id = Id::NULL.with(("part-editor", element));
    ScrollArea::vertical()
        .id_salt(id)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.set_max_width(720.0);
            header(ui, shell, &part, locked, &mut out);
            ui.separator();
            properties(ui, id, &part, locked, &mut out);
            ui.add_space(8.0);
            symbol(ui, shell, id, &part, locked, &mut out);
            ui.add_space(8.0);
            footprints(ui, shell, id, &part, locked, &mut out);
            ui.add_space(8.0);
            fields(ui, id, &part, locked, &mut out);
        });
    out
}

fn open(element: Uuid) -> Invocation {
    (
        "library.element.open".to_owned(),
        json!({ "element": element }),
    )
}

fn header(ui: &mut Ui, shell: &mut Shell, part: &Part, locked: bool, out: &mut Vec<Invocation>) {
    ui.heading(&part.name);
    if locked {
        ui.colored_label(ui.visuals().warn_fg_color, tr!("part-read-only"));
    }
    if let Some(provenance) = &part.provenance {
        ui.weak(tr!(
            "part-imported-from",
            service = provenance.service.as_str()
        ));
        if !provenance.terms_url.is_empty() {
            ui.hyperlink(&provenance.terms_url);
        }
    }
    ui.horizontal(|ui| {
        let save = ui.add_enabled(!locked, egui::Button::new(tr!("footprint-save")));
        if save.clicked() {
            out.push((
                "library.element.save".to_owned(),
                json!({ "element": part.uuid }),
            ));
        }
        if ui.button(tr!("footprint-check")).clicked() {
            // An error shows in the status bar; the lines go to the Messages panel.
            let _ = element_editor::check(shell, part.uuid);
        }
        // Saves the Part first, then previews the update of its Components.
        let in_project = shell.engine().project().is_some();
        if ui
            .add_enabled(in_project, egui::Button::new(tr!("part-update-in-project")))
            .clicked()
        {
            out.push((
                "schematic.update_from_library.open".to_owned(),
                json!({ "element": part.uuid }),
            ));
        }
    });
}

fn heading(ui: &mut Ui, text: String) {
    ui.add_space(4.0);
    ui.strong(text);
}

fn properties(ui: &mut Ui, id: Id, part: &Part, locked: bool, out: &mut Vec<Invocation>) {
    ui.add_enabled_ui(!locked, |ui| {
        Grid::new(id.with("properties"))
            .num_columns(2)
            .show(ui, |ui| {
                let label = tr!("part-name");
                ui.label(&label);
                if let Some(name) = text_field(ui, id.with("name"), &label, &part.name, FIELD_WIDTH)
                {
                    out.extend(super::rename(part, &name));
                }
                ui.end_row();
                let label = tr!("library-filter-type");
                ui.label(&label);
                let mut kind = part.component_type;
                let name = crate::panels::library::type_name;
                ComboBox::from_id_salt(id.with("type"))
                    .selected_text(name(kind))
                    .show_ui(ui, |ui| {
                        for t in ComponentType::ALL {
                            ui.selectable_value(&mut kind, t, name(t));
                        }
                    })
                    .response
                    .widget_info(|| WidgetInfo::labeled(Role::ComboBox, true, label.clone()));
                out.extend(super::set_component_type(part, kind));
                ui.end_row();
                ui.label("");
                let mut in_bom = part.in_bom;
                ui.checkbox(&mut in_bom, tr!("part-in-bom"));
                out.extend(super::set_in_bom(part, in_bom));
                ui.end_row();
                ui.label("");
                let mut in_cpl = part.in_cpl;
                ui.checkbox(&mut in_cpl, tr!("part-in-cpl"));
                out.extend(super::set_in_cpl(part, in_cpl));
                ui.end_row();
            });
    });
}

/// A drop-down of `choices` with a filter box at its top; `none` adds a "None" entry.
/// Returns the pick: `Some(None)` for None.
fn picker(
    ui: &mut Ui,
    id: Id,
    label: &str,
    shown: String,
    choices: &[Choice],
    none: bool,
) -> Option<Option<Uuid>> {
    let mut picked = None;
    let filter_id = id.with("filter");
    let response = ComboBox::from_id_salt(id)
        .selected_text(shown)
        .width(FIELD_WIDTH)
        .height(320.0)
        // Typing in the filter must not close it; a pick does.
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show_ui(ui, |ui| {
            let mut filter: String = ui.data(|d| d.get_temp(filter_id)).unwrap_or_default();
            let hint = tr!("part-filter");
            ui.add(
                TextEdit::singleline(&mut filter)
                    .hint_text(hint.clone())
                    .desired_width(f32::INFINITY),
            )
            .widget_info(|| WidgetInfo::labeled(Role::TextInput, true, hint.clone()));
            ui.data_mut(|d| d.insert_temp(filter_id, filter.clone()));
            let needle = filter.to_lowercase();
            if none && ui.selectable_label(false, tr!("part-none")).clicked() {
                picked = Some(None);
            }
            for choice in choices
                .iter()
                .filter(|c| c.label.to_lowercase().contains(&needle))
            {
                if ui.selectable_label(false, &choice.label).clicked() {
                    picked = Some(Some(choice.uuid));
                }
            }
            if picked.is_some() {
                ui.data_mut(|d| d.remove::<String>(filter_id));
                ui.close();
            }
        })
        .response;
    let label = label.to_owned();
    response.widget_info(|| WidgetInfo::labeled(Role::ComboBox, true, label.clone()));
    picked
}

fn symbol(
    ui: &mut Ui,
    shell: &mut Shell,
    id: Id,
    part: &Part,
    locked: bool,
    out: &mut Vec<Invocation>,
) {
    heading(ui, tr!("part-symbol"));
    let found = part.symbol.and_then(|s| super::symbol(shell, s));
    let shown = match (&found, part.symbol) {
        (Some(s), _) => s.name.clone(),
        (None, Some(_)) => tr!("part-missing"),
        (None, None) => tr!("part-none"),
    };
    let choices = if locked {
        Vec::new()
    } else {
        super::symbol_choices(shell)
    };
    ui.horizontal(|ui| {
        let label = tr!("part-symbol");
        let picked = ui
            .add_enabled_ui(!locked, |ui| {
                picker(ui, id.with("symbol"), &label, shown, &choices, true)
            })
            .inner;
        if let Some(picked) = picked {
            out.extend(super::set_symbol(part, picked));
        }
        let wizard = ui.add_enabled(!locked, egui::Button::new(tr!("part-symbol-wizard")));
        if wizard.clicked() {
            out.push((
                "library.symbol.generate".to_owned(),
                json!({ "part": part.uuid }),
            ));
        }
        let edit = ui.add_enabled(found.is_some(), egui::Button::new(tr!("part-edit-symbol")));
        if edit.clicked()
            && let Some(symbol) = part.symbol
        {
            crate::shared_element::opened_from_part(shell, symbol, part.uuid);
            out.push(open(symbol));
        }
    });
    match (&found, part.symbol) {
        (Some(s), _) => preview::symbol(ui, s, None),
        (None, Some(uuid)) => {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                tr!("part-not-found", uuid = uuid.to_string()),
            );
        }
        (None, None) if part.component_type.needs_symbol_and_footprint() => {
            ui.colored_label(ui.visuals().warn_fg_color, tr!("part-needs-symbol"));
        }
        (None, None) => {}
    }
}

fn footprints(
    ui: &mut Ui,
    shell: &mut Shell,
    id: Id,
    part: &Part,
    locked: bool,
    out: &mut Vec<Invocation>,
) {
    heading(ui, tr!("part-footprints"));
    let shown_id = id.with("shown-footprint");
    let mut shown: usize = ui.data(|d| d.get_temp(shown_id)).unwrap_or(0);
    shown = shown.min(part.footprints.len().saturating_sub(1));
    let count = part.footprints.len();
    let mut preview_of = None;
    Grid::new(id.with("footprints"))
        .num_columns(2)
        .show(ui, |ui| {
            for (i, uuid) in part.footprints.iter().enumerate() {
                let found = super::footprint(shell, *uuid);
                let name = found
                    .as_ref()
                    .map_or_else(|| tr!("part-missing"), |f| f.name.clone());
                let text = if i == 0 {
                    tr!("part-default-footprint", name = name.as_str())
                } else {
                    name.clone()
                };
                let row = ui.selectable_label(shown == i, text.clone());
                let label = text;
                row.widget_info(|| {
                    WidgetInfo::selected(Role::ListItem, true, shown == i, label.clone())
                });
                if row.clicked() {
                    shown = i;
                }
                if shown == i {
                    preview_of = Some((*uuid, found.clone()));
                }
                ui.horizontal(|ui| {
                    let mut button = |ui: &mut Ui, enabled: bool, text: String, how| {
                        let b = ui.add_enabled(enabled && !locked, egui::Button::new(text));
                        if b.clicked() {
                            out.extend(super::move_footprint(part, i, how));
                        }
                    };
                    button(ui, i > 0, tr!("part-move-up"), FootprintMove::Up);
                    button(
                        ui,
                        i + 1 < count,
                        tr!("part-move-down"),
                        FootprintMove::Down,
                    );
                    button(ui, true, tr!("part-remove"), FootprintMove::Remove);
                    let edit = ui.add_enabled(found.is_some(), egui::Button::new(tr!("part-edit")));
                    let what = tr!("part-edit-footprint", name = name.as_str());
                    edit.widget_info(|| WidgetInfo::labeled(Role::Button, true, what.clone()));
                    if edit.clicked() {
                        crate::shared_element::opened_from_part(shell, *uuid, part.uuid);
                        out.push(open(*uuid));
                    }
                });
                ui.end_row();
            }
        });
    ui.data_mut(|d| d.insert_temp(shown_id, shown));
    if part.footprints.is_empty() {
        ui.weak(tr!("part-no-footprints"));
    }
    if !locked {
        let choices: Vec<Choice> = super::footprint_choices(shell)
            .into_iter()
            .filter(|c| !part.footprints.contains(&c.uuid))
            .collect();
        let label = tr!("part-add-footprint");
        let picked = picker(
            ui,
            id.with("add-footprint"),
            &label,
            label.clone(),
            &choices,
            false,
        );
        if let Some(Some(uuid)) = picked {
            out.extend(super::add_footprint(part, uuid));
        }
    }
    match preview_of {
        Some((_, Some(f))) => preview::footprint(ui, &f),
        Some((uuid, None)) => {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                tr!("part-not-found", uuid = uuid.to_string()),
            );
        }
        None => {}
    }
}

/// The Pins Panel on the Part `element`: its pin-pad map, each Pin's Pads and the Pads
/// left unconnected.
pub(crate) fn pin_map_panel(
    ui: &mut Ui,
    shell: &mut Shell,
    element: Uuid,
    out: &mut Vec<Invocation>,
) {
    let Some(LibraryElement::Part(part)) = shell.engine().element(element).cloned() else {
        return;
    };
    let locked = element_editor::read_only(shell.engine(), element);
    let id = Id::NULL.with(("part-editor", element));
    ScrollArea::vertical()
        .id_salt(id.with("pin-map-scroll"))
        .auto_shrink([false, false])
        .show(ui, |ui| pin_map(ui, shell, id, &part, locked, out));
}

fn pin_map(
    ui: &mut Ui,
    shell: &mut Shell,
    id: Id,
    part: &Part,
    locked: bool,
    out: &mut Vec<Invocation>,
) {
    heading(ui, tr!("part-pin-map"));
    let symbol = part.symbol.and_then(|s| super::symbol(shell, s));
    let rows = super::pin_rows(part, symbol.as_ref());
    if rows.is_empty() {
        ui.weak(tr!("part-pin-map-empty"));
        return;
    }
    ui.weak(tr!("part-pin-map-hint"));
    let footprints: Vec<_> = part
        .footprints
        .iter()
        .filter_map(|f| super::footprint(shell, *f))
        .collect();
    ui.add_enabled_ui(!locked, |ui| {
        Grid::new(id.with("pin-map"))
            .num_columns(4)
            .striped(true)
            .show(ui, |ui| {
                ui.strong(tr!("part-pin"));
                ui.strong(tr!("part-pin-name"));
                ui.strong(tr!("part-pads"));
                ui.label("");
                ui.end_row();
                for PinRow {
                    number,
                    name,
                    pads,
                    overridden,
                } in &rows
                {
                    ui.label(number);
                    ui.label(name);
                    let label = tr!("part-pads-of", pin = number.as_str());
                    let current = pads.join(", ");
                    let field = id.with(("pads", number.as_str()));
                    if let Some(typed) = text_field(ui, field, &label, &current, 120.0) {
                        out.extend(super::set_pin_pads(part, number, &typed));
                    }
                    if *overridden {
                        ui.weak(tr!("part-overridden"));
                    } else {
                        ui.label("");
                    }
                    ui.end_row();
                }
            });
        let loose = super::unmapped_pads(part, symbol.as_ref(), &footprints);
        if !loose.is_empty() {
            ui.add_space(4.0);
            ui.label(tr!("part-unmapped-pads"));
            ui.horizontal_wrapped(|ui| {
                for pad in &loose {
                    let mut on = part.pin_map.unconnected.contains(pad);
                    let text = tr!("part-pad-unconnected", pad = pad.as_str());
                    if ui.checkbox(&mut on, text).changed() {
                        out.extend(super::set_unconnected(part, pad, on));
                    }
                }
            });
        }
    });
}

fn builtin_label(key: &str) -> String {
    match key {
        "value" => tr!("part-field-value"),
        "description" => tr!("part-field-description"),
        "datasheet" => tr!("part-field-datasheet"),
        "manufacturer" => tr!("part-field-manufacturer"),
        "mpn" => tr!("part-field-mpn"),
        _ => tr!("part-field-keywords"),
    }
}

fn fields(ui: &mut Ui, id: Id, part: &Part, locked: bool, out: &mut Vec<Invocation>) {
    heading(ui, tr!("part-fields"));
    ui.add_enabled_ui(!locked, |ui| {
        Grid::new(id.with("fields")).num_columns(3).show(ui, |ui| {
            for key in super::BUILT_IN {
                let label = builtin_label(key);
                ui.label(&label);
                let current = super::builtin(part, key);
                if let Some(typed) = text_field(ui, id.with(key), &label, &current, FIELD_WIDTH) {
                    out.extend(super::set_builtin(part, key, &typed));
                }
                ui.label("");
                ui.end_row();
            }
            for (i, field) in part.fields.custom.iter().enumerate() {
                let label = tr!("part-field-name", n = (i + 1).to_string());
                let typed = text_field(ui, id.with(("cname", i)), &label, &field.name, 120.0);
                if let Some(name) = typed {
                    out.extend(super::set_field(part, i, Some(&name), None));
                }
                let label = tr!("part-field-of", name = field.name.as_str());
                let current = &field.value;
                if let Some(value) =
                    text_field(ui, id.with(("cvalue", i)), &label, current, FIELD_WIDTH)
                {
                    out.extend(super::set_field(part, i, None, Some(&value)));
                }
                let remove = ui.button(tr!("part-remove"));
                let what = tr!("part-remove-field", name = field.name.as_str());
                remove.widget_info(|| WidgetInfo::labeled(Role::Button, true, what.clone()));
                if remove.clicked() {
                    out.push(super::remove_field(part, i));
                }
                ui.end_row();
            }
        });
        if ui.button(tr!("part-add-field")).clicked() {
            out.push(super::add_field(part));
        }
    });
}
