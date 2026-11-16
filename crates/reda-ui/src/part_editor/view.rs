//! Drawing the Part editor's form: Save and Check, the name and Component Type, the
//! Symbol picker with its preview, the ordered alternate Footprints with theirs and the
//! fields. The pin-pad map is in the Pins Panel ([`pin_map_panel`]). Edits become the Commands of [`super`]; the Edit buttons
//! open the Symbol or a Footprint in its own editor. A starter Library Part shows the same
//! form with the edits disabled.

use egui::{Id, Role, ScrollArea, Ui, WidgetInfo};
use reda_engine::model::{ComponentType, LibraryElement, Part, Uuid};
use serde_json::json;

use super::{Choice, FootprintMove};
use crate::element_editor::{self, Invocation, text_field};
use crate::panels::library::preview;
use crate::settings::layout;
use crate::shell::Shell;
use crate::theme::TokensExt as _;
use crate::tr;
use crate::widgets::{self, Column};

const FIELD_WIDTH: f32 = 260.0;
/// A custom field's name.
const NAME_WIDTH: f32 = 120.0;
/// A Pin's Pads in the pin-pad map.
const PADS_WIDTH: f32 = 110.0;

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
            region(ui, id, "part", tr!("part-region-part"), |ui| {
                properties(ui, id, &part, locked, &mut out);
            });
            region(ui, id, "symbol", tr!("part-region-symbol"), |ui| {
                symbol(ui, shell, id, &part, locked, &mut out);
            });
            region(ui, id, "footprints", tr!("part-footprints"), |ui| {
                footprints(ui, shell, id, &part, locked, &mut out);
            });
            region(ui, id, "fields", tr!("part-fields"), |ui| {
                fields(ui, id, &part, locked, &mut out);
            });
        });
    out
}

/// One of the form's four regions (library.md#part-editor), open unless folded.
fn region(ui: &mut Ui, id: Id, key: &str, title: String, add: impl FnOnce(&mut Ui)) {
    widgets::section(title)
        .id_salt(id.with(("region", key)))
        .default_open(true)
        .show(ui, add);
}

/// Runs a sequence the form starts itself (New, Duplicate & Edit): errors reach the
/// status bar.
fn report(shell: &mut Shell, outcome: Result<Uuid, reda_engine::command::CommandError>) {
    if let Err(e) = outcome {
        shell.report(e.message);
    }
}

fn open(element: Uuid) -> Invocation {
    (
        "library.element.open".to_owned(),
        json!({ "element": element }),
    )
}

fn header(ui: &mut Ui, shell: &mut Shell, part: &Part, locked: bool, out: &mut Vec<Invocation>) {
    let t = ui.tokens();
    ui.label(t.ty.title.rich(&part.name).color(t.color.text.primary));
    if locked {
        layout::warning(ui, tr!("part-read-only"));
    }
    if let Some(provenance) = &part.provenance {
        layout::muted(
            ui,
            tr!(
                "part-imported-from",
                service = provenance.service.as_str()
            ),
        );
        let url = &provenance.terms_url;
        if !url.is_empty() && ui.add(widgets::link(url)).clicked() {
            ui.ctx().open_url(egui::OpenUrl::new_tab(url));
        }
    }
    ui.horizontal(|ui| {
        let save = ui.add_enabled(!locked, widgets::button(tr!("footprint-save")));
        if save.clicked() {
            out.push((
                "library.element.save".to_owned(),
                json!({ "element": part.uuid }),
            ));
        }
        if ui.add(widgets::button(tr!("footprint-check"))).clicked() {
            // An error shows in the status bar; the lines go to the Messages panel.
            let _ = element_editor::check(shell, part.uuid);
        }
        // Saves the Part first, then previews the update of its Components.
        let in_project = shell.engine().project().is_some();
        if ui
            .add_enabled(in_project, widgets::button(tr!("part-update-in-project")))
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
    ui.add_space(ui.tokens().space.stack);
    crate::settings::strong(ui, text);
}

fn properties(ui: &mut Ui, id: Id, part: &Part, locked: bool, out: &mut Vec<Invocation>) {
    ui.add_enabled_ui(!locked, |ui| {
        widgets::form(id.with("properties")).show(ui, |ui| {
                let label = tr!("part-name");
                ui.add(widgets::form_name(&label));
                if let Some(name) = text_field(ui, id.with("name"), &label, &part.name, FIELD_WIDTH)
                {
                    out.extend(super::rename(part, &name));
                }
                ui.end_row();
                let label = tr!("library-filter-type");
                ui.add(widgets::form_name(&label));
                let mut kind = part.component_type;
                let name = crate::panels::library::type_name;
                widgets::dropdown(id.with("type"), name(kind))
                    .show(ui, |ui| {
                        for t in ComponentType::ALL {
                            widgets::choice_value(ui, &mut kind, t, name(t));
                        }
                    })
                    .response
                    .widget_info(|| WidgetInfo::labeled(Role::ComboBox, true, label.clone()));
                out.extend(super::set_component_type(part, kind));
                ui.end_row();
                ui.label("");
                let mut in_bom = part.in_bom;
                ui.add(widgets::checkbox(&mut in_bom, tr!("part-in-bom")));
                out.extend(super::set_in_bom(part, in_bom));
                ui.end_row();
                ui.label("");
                let mut in_cpl = part.in_cpl;
                ui.add(widgets::checkbox(&mut in_cpl, tr!("part-in-cpl")));
                out.extend(super::set_in_cpl(part, in_cpl));
                ui.end_row();
                for key in super::PART_FIELDS {
                    builtin_row(ui, id, part, key, out);
                }
            });
        layout::muted(ui, tr!("part-flags-hint"));
    });
}

fn builtin_row(ui: &mut Ui, id: Id, part: &Part, key: &str, out: &mut Vec<Invocation>) {
    let label = builtin_label(key);
    ui.add(widgets::form_name(&label));
    let current = super::builtin(part, key);
    if let Some(typed) = text_field(ui, id.with(key), &label, &current, FIELD_WIDTH) {
        out.extend(super::set_builtin(part, key, &typed));
    }
    ui.end_row();
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
    let response = widgets::dropdown(id, shown)
        .width(FIELD_WIDTH)
        // Typing in the filter must not close it; a pick does.
        .stay_open()
        .show(ui, |ui| {
            let mut filter: String = ui.data(|d| d.get_temp(filter_id)).unwrap_or_default();
            let hint = tr!("part-filter");
            ui.add(
                widgets::text_field(&mut filter)
                    .id(filter_id.with("field"))
                    .search()
                    .hint(hint.clone())
                    .width(f32::INFINITY),
            )
            .widget_info(|| WidgetInfo::labeled(Role::TextInput, true, hint.clone()));
            ui.data_mut(|d| d.insert_temp(filter_id, filter.clone()));
            let needle = filter.to_lowercase();
            if none && ui.add(widgets::choice(false, tr!("part-none"))).clicked() {
                picked = Some(None);
            }
            for choice in choices
                .iter()
                .filter(|c| c.label.to_lowercase().contains(&needle))
            {
                if ui.add(widgets::choice(false, &choice.label)).clicked() {
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
        let wizard = ui.add_enabled(!locked, widgets::button(tr!("part-symbol-wizard")));
        if wizard.clicked() {
            out.push((
                "library.symbol.generate".to_owned(),
                json!({ "part": part.uuid }),
            ));
        }
        let new = ui.add_enabled(!locked, widgets::button(tr!("part-symbol-new")));
        if new.clicked() {
            let made = super::new_element(shell, part.uuid, "symbol");
            report(shell, made);
        }
        let edit = ui.add_enabled(found.is_some(), widgets::button(tr!("part-edit-symbol")));
        if edit.clicked()
            && let Some(symbol) = part.symbol
        {
            crate::shared_element::opened_from_part(shell, symbol, part.uuid);
            out.push(open(symbol));
        }
        let duplicate = ui.add_enabled(
            found.is_some() && !locked,
            widgets::button(tr!("part-duplicate-edit")),
        );
        let what = tr!("part-duplicate-edit-symbol");
        duplicate.widget_info(|| WidgetInfo::labeled(Role::Button, true, what.clone()));
        if duplicate.clicked()
            && let Some(symbol) = part.symbol
        {
            let copy = crate::shared_element::duplicate_and_edit(shell, part.uuid, symbol);
            report(shell, copy);
        }
    });
    match (&found, part.symbol) {
        (Some(s), _) => {
            let symbol = part.symbol.and_then(|u| super::symbol(shell, u));
            let pins = super::probed_pins(shell, part, symbol.as_ref());
            preview::symbol_probed(ui, s, &pins);
        }
        (None, Some(uuid)) => {
            layout::warning(ui, tr!("part-not-found", uuid = uuid.to_string()));
        }
        (None, None) if part.component_type.needs_symbol_and_footprint() => {
            layout::warning(ui, tr!("part-needs-symbol"));
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
    let count = part.footprints.len();
    let symbol = part.symbol.and_then(|s| super::symbol(shell, s));
    let probed = super::probed_pads(shell, part, symbol.as_ref());
    let mut hovered_pad: Option<Option<String>> = None;
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
        let t = ui.tokens();
        widgets::card_frame(&t).show(ui, |ui| {
            crate::settings::strong(ui, &text);
            match &found {
                Some(f) => {
                    let hovered = preview::footprint_probed(ui, f, &probed);
                    if hovered.is_some() || hovered_pad.is_none() {
                        hovered_pad = Some(hovered);
                    }
                }
                None => {
                    layout::warning(ui, tr!("part-not-found", uuid = uuid.to_string()));
                }
            }
            ui.horizontal(|ui| {
                let mut moved = None;
                let mut button = |ui: &mut Ui, enabled: bool, text: String, what: String, how| {
                    let b = ui.add_enabled(enabled && !locked, widgets::button(text));
                    b.widget_info(|| WidgetInfo::labeled(Role::Button, true, what.clone()));
                    if b.clicked() {
                        moved = Some(how);
                    }
                };
                let of = |key: &str| {
                    crate::i18n::translate(key, &[("name", crate::i18n::arg(name.as_str()))])
                };
                button(
                    ui,
                    i > 0,
                    tr!("part-move-up"),
                    of("part-move-up-of"),
                    FootprintMove::Up,
                );
                button(
                    ui,
                    i + 1 < count,
                    tr!("part-move-down"),
                    of("part-move-down-of"),
                    FootprintMove::Down,
                );
                let edit = ui.add_enabled(found.is_some(), widgets::button(tr!("part-edit")));
                let what = tr!("part-edit-footprint", name = name.as_str());
                edit.widget_info(|| WidgetInfo::labeled(Role::Button, true, what.clone()));
                if edit.clicked() {
                    crate::shared_element::opened_from_part(shell, *uuid, part.uuid);
                    out.push(open(*uuid));
                }
                let duplicate = ui.add_enabled(
                    found.is_some() && !locked,
                    widgets::button(tr!("part-duplicate-edit")),
                );
                let what = of("part-duplicate-edit-footprint");
                duplicate.widget_info(|| WidgetInfo::labeled(Role::Button, true, what.clone()));
                if duplicate.clicked() {
                    let copy = crate::shared_element::duplicate_and_edit(shell, part.uuid, *uuid);
                    report(shell, copy);
                }
                button(
                    ui,
                    true,
                    tr!("part-remove"),
                    of("part-remove-footprint"),
                    FootprintMove::Remove,
                );
                if let Some(how) = moved {
                    out.extend(super::move_footprint(part, i, how));
                }
            });
        });
    }
    if let Some(pad) = hovered_pad {
        let now = super::probe(shell);
        if now.part != Some(part.uuid) || now.pad != pad {
            super::set_probe(shell, part.uuid, None, Some(pad));
        }
    }
    if part.footprints.is_empty() {
        layout::muted(ui, tr!("part-no-footprints"));
    }
    if locked {
        return;
    }
    ui.horizontal(|ui| {
        layout::text(ui, tr!("part-add"));
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
        widgets::menu_button(tr!("part-generate-footprint")).show(ui, |ui| {
            for generator in reda_engine::library::GENERATORS {
                let title = shell.engine().registry().get(generator).map_or_else(
                    || generator.to_owned(),
                    |m| crate::bars::command_title(generator, m.title),
                );
                if ui.add(widgets::menu_item(format!("{title}…"))).clicked() {
                    let mut params = json!({ "part": part.uuid });
                    if let Some((library, true)) = super::library_of(shell, part.uuid) {
                        params["library"] = json!(library);
                    }
                    shell.queue_call(generator, params);
                    ui.close();
                }
            }
        });
        if ui.add(widgets::button(tr!("part-new-footprint"))).clicked() {
            let made = super::new_element(shell, part.uuid, "footprint");
            report(shell, made);
        }
    });
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
    let footprints: Vec<_> = part
        .footprints
        .iter()
        .filter_map(|f| super::footprint(shell, *f))
        .collect();
    let rows = super::map_rows(part, symbol.as_ref(), &footprints);
    if rows.is_empty() {
        layout::muted(ui, tr!("part-pin-map-empty"));
        return;
    }
    layout::muted(ui, tr!("part-pin-map-hint"));
    let probed = super::probed_pins(shell, part, symbol.as_ref());
    let mut hovered: Option<String> = None;
    ui.add_enabled_ui(!locked, |ui| {
        let t = ui.tokens();
        // Pin, Name, a column of Pads per alternate (the default first), and the note.
        let columns = 3 + footprints.len().max(1);
        widgets::table(id.with("pin-map"))
            .columns(vec![Column::auto(); columns])
            .show(ui, |table| {
                table.header(|h| {
                    h.text(tr!("part-pin"));
                    h.text(tr!("part-pin-name"));
                    if footprints.is_empty() {
                        h.text(tr!("part-pads"));
                    }
                    for f in &footprints {
                        h.text(&f.name);
                    }
                    h.empty();
                });
                for row in &rows {
                    table.row(|cells| {
                        let lit = probed.contains(&row.number);
                        let mut over = if lit {
                            let number = t.ty.body_strong.rich(&row.number);
                            cells.ui(|ui| ui.label(number.color(t.color.accent.text)))
                        } else {
                            cells.text(&row.number)
                        }
                        .hovered();
                        over |= cells.text(&row.name).hovered();
                        let map_cells: Vec<super::MapCell> = if row.cells.is_empty() {
                            vec![super::MapCell {
                                text: String::new(),
                                marks: Vec::new(),
                            }]
                        } else {
                            row.cells.clone()
                        };
                        for (i, cell) in map_cells.iter().enumerate() {
                            let mut label = match footprints.get(i) {
                                Some(f) if i > 0 => tr!(
                                    "part-pads-of-on",
                                    pin = row.number.as_str(),
                                    footprint = f.name.as_str()
                                ),
                                _ => tr!("part-pads-of", pin = row.number.as_str()),
                            };
                            if !cell.marks.is_empty() {
                                let why: Vec<String> = cell.marks.iter().map(mark_text).collect();
                                label = format!("{label} ({})", why.join(", "));
                            }
                            let field = id.with(("pads", row.number.as_str(), i));
                            let (typed, response) = cells.ui(|ui| {
                                crate::element_editor::text_field_with(
                                    ui,
                                    field,
                                    &label,
                                    &cell.text,
                                    PADS_WIDTH,
                                    !cell.marks.is_empty(),
                                )
                            });
                            if let Some(typed) = typed {
                                out.extend(super::set_pin_pads(part, &row.number, &typed));
                            }
                            over |= response.hovered();
                        }
                        if row.overridden {
                            cells.muted(tr!("part-overridden"));
                        } else {
                            cells.empty();
                        }
                        if over {
                            hovered = Some(row.number.clone());
                        }
                    });
                }
            });
        let loose = super::unmapped_pads(part, symbol.as_ref(), &footprints);
        if !loose.is_empty() {
            ui.add_space(ui.tokens().space.stack);
            layout::text(ui, tr!("part-unmapped-pads"));
            ui.horizontal_wrapped(|ui| {
                for pad in &loose {
                    let mut on = part.pin_map.unconnected.contains(pad);
                    let text = tr!("part-pad-unconnected", pad = pad.as_str());
                    if ui.add(widgets::checkbox(&mut on, text)).changed() {
                        out.extend(super::set_unconnected(part, pad, on));
                    }
                }
            });
        }
    });
    let now = super::probe(shell);
    if now.part != Some(part.uuid) || now.pin != hovered {
        super::set_probe(shell, part.uuid, Some(hovered), None);
    }
}

fn mark_text(mark: &super::CellMark) -> String {
    match mark {
        super::CellMark::Empty => tr!("part-cell-empty"),
        super::CellMark::Unknown(pad) => tr!("part-cell-unknown", pad = pad.as_str()),
        super::CellMark::Shared(pad) => tr!("part-cell-shared", pad = pad.as_str()),
    }
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
        widgets::table(id.with("fields"))
            .columns([Column::auto(), Column::auto(), Column::auto()])
            .show(ui, |table| {
                for key in super::FIELD_GRID {
                    table.row(|cells| {
                        let label = builtin_label(key);
                        cells.ui(|ui| ui.add(widgets::form_name(&label)));
                        let current = super::builtin(part, key);
                        let typed = cells.ui(|ui| {
                            text_field(ui, id.with(key), &label, &current, FIELD_WIDTH)
                        });
                        if let Some(typed) = typed {
                            out.extend(super::set_builtin(part, key, &typed));
                        }
                        cells.empty();
                    });
                }
                for (i, field) in part.fields.custom.iter().enumerate() {
                    table.row(|cells| {
                        let label = tr!("part-field-name", n = (i + 1).to_string());
                        let typed = cells.ui(|ui| {
                            text_field(ui, id.with(("cname", i)), &label, &field.name, NAME_WIDTH)
                        });
                        if let Some(name) = typed {
                            out.extend(super::set_field(part, i, Some(&name), None));
                        }
                        let label = tr!("part-field-of", name = field.name.as_str());
                        let current = &field.value;
                        let typed = cells.ui(|ui| {
                            text_field(ui, id.with(("cvalue", i)), &label, current, FIELD_WIDTH)
                        });
                        if let Some(value) = typed {
                            out.extend(super::set_field(part, i, None, Some(&value)));
                        }
                        let remove = cells.ui(|ui| ui.add(widgets::button(tr!("part-remove")).flat()));
                        let what = tr!("part-remove-field", name = field.name.as_str());
                        remove.widget_info(|| WidgetInfo::labeled(Role::Button, true, what.clone()));
                        if remove.clicked() {
                            out.push(super::remove_field(part, i));
                        }
                    });
                }
            });
        if ui.add(widgets::button(tr!("part-add-field"))).clicked() {
            out.push(super::add_field(part));
        }
    });
}
