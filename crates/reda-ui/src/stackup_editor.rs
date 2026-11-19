//! A PCB Document's Stackup tab. The tables form one scrolling pane beside its
//! cross-section. Edits use the same PCB Commands as scripts.
mod apply;
pub(crate) mod commands;
mod construction;
mod fields;
mod layer_actions;
mod preview;
mod types;

use crate::settings::{layout, strong};
use crate::shell::Shell;
use crate::theme::TokensExt as _;
use crate::widgets::{Column, RowLook, Selection, Tooltip as _};
use crate::{tr, widgets};
use egui::{Align, Layout, Rect, Role, Sense, Ui, UiBuilder, WidgetInfo};
use fields::{BoardField, Edits, Field, Land, LayerField, SheetField, ViaField, View};
use reda_engine::model::{
    CopperRole, DielectricEntry, DielectricKind, PcbDocument, Project, StackupEntry, StackupInfo,
    Uuid,
    geom::{Nm, exact_decimal},
};
use serde_json::json;

const TABLE_WIDTH: f32 = 468.0;
const POSITION: f32 = 32.0;
const KIND: f32 = 96.0;
const THICKNESS: f32 = 87.0;

pub fn show(ui: &mut Ui, shell: &mut Shell, document: Uuid) {
    let Some(project) = shell.engine().project() else {
        return;
    };
    let Some(pcb) = project.pcb_document(document) else {
        return;
    };
    let key = egui::Id::NULL.with(("stackup-editor", document));
    let mut view = ui.data(|d| d.get_temp::<View>(key)).unwrap_or_default();
    let display = shell
        .engine()
        .view()
        .get::<commands::Displays>()
        .expect("installed by Shell")
        .get(document);
    view.selected = display.selected;
    view.scale = display.scale;
    let mut edits = Edits::new(document);
    let t = ui.tokens();
    let full = ui.available_rect_before_wrap().shrink(t.space.panel);
    // The dimension and the split button need to fit side by side in a small
    // drawing pane. Both panes scroll when the tab is narrower still.
    let drawing_width = t.size.of(210.0).min(full.width() * 0.5);
    let width = t
        .size
        .of(TABLE_WIDTH + 44.0)
        .min(full.width() * 0.66)
        .min(full.width() - drawing_width);
    let info = pcb.stackup().info();
    let left = full.with_max_x(full.left() + width);
    let right = full.with_min_x((left.right() + t.space.panel).min(full.right()));
    pane(ui, left, (document, "tables"), |ui| {
        let table_width =
            (ui.available_width() - t.space.gap).clamp(t.size.of(392.0), t.size.of(TABLE_WIDTH));
        egui::ScrollArea::both().auto_shrink(false).show(ui, |ui| {
            ui.set_width(table_width);
            strong(ui, tr!("panel-layers"));
            layers(ui, pcb, &info, &mut view, &mut edits);
            if let Some(layer) = view.plane_offer
                && pcb
                    .layers()
                    .layer(layer)
                    .is_some_and(|l| l.role == CopperRole::Plane)
                && !pcb.zones().any(|z| z.layer == layer)
                && ui.add(widgets::button(tr!("plane-fill-layer"))).clicked()
            {
                view.plane_picker = Some(crate::pcb::plane::Picker::new(layer));
            }
            ui.add_space(t.space.section);
            types::table(ui, pcb, &mut view, &mut edits);
            ui.add_space(t.space.section);
            board(ui, project, pcb, &info, &mut view, &mut edits);
            apply::buttons(ui, project, pcb, &mut view, &mut edits);
            if ui
                .add(widgets::button(tr!("construction-save-action")))
                .clicked()
            {
                view.save_construction = Some(construction::SaveForm::new(
                    project,
                    shell.engine().user_dirs(),
                ));
            }
        });
    });
    pane(ui, right, (document, "drawing"), |ui| {
        preview::show(ui, pcb, &info, &mut view, &mut edits)
    });
    ui.allocate_rect(full, Sense::hover());
    if view
        .apply_construction
        .as_mut()
        .is_some_and(|form| form.show(ui, project, pcb, &mut edits))
    {
        view.apply_construction = None;
    }
    if let Some(params) = crate::pcb::plane::Picker::show(ui, pcb, &mut view.plane_picker) {
        let _ = shell.run_reporting("pcb.zone.add_from_board_outline", params);
    }
    for (field, command, mut params) in edits.commands {
        if let Some(Field::Land(id, land)) = field
            && let Some(kind) = shell
                .engine()
                .project()
                .and_then(|p| p.pcb_document(document))
                .and_then(|p| p.stackup().via_types.iter().find(|v| v.uuid == id))
        {
            let mut diameter =
                serde_json::to_value(kind.diameter).expect("Via diameter serializes");
            diameter[land.key()] = params["diameter"].clone();
            params["diameter"] = diameter;
        }
        if matches!(
            command,
            "pcb.layer.add" | "pcb.layer.move" | "pcb.layer.remove" | apply::COMMAND
        ) && !layer_actions::prepare(shell, &mut view, command, &params)
        {
            continue;
        }
        if command == "pcb.via_type.remove"
            || matches!(field, Some(Field::Via(_, ViaField::From | ViaField::To)))
        {
            match shell.run_reporting(
                "pcb.stackup.change_preview",
                json!({"change":{"id":command,"params":params}}),
            ) {
                Ok(out) => {
                    let count = out["vias_using_span"].as_u64().unwrap_or(0);
                    let emptied = out["allowed_lists_emptied"].as_u64().unwrap_or(0);
                    if count > 0 || emptied > 0 {
                        view.pending = Some(types::Pending {
                            command,
                            params,
                            count,
                            emptied,
                        });
                        continue;
                    }
                }
                Err(_) => continue,
            }
        }
        if let Some(field) = field {
            let plane = params["role"] == "plane";
            let result = shell.run(command, params);
            if let Field::Layer(layer, LayerField::Role) = field
                && result.is_ok()
            {
                view.plane_offer = plane.then_some(layer);
            }
            view.settled(field, result.err().map(|e| e.message));
        } else if let Ok(out) = shell.run_reporting(command, params) {
            layer_actions::settle(shell, document, view.selected, command, &out);
        }
    }
    types::confirm(ui, &mut view, document, shell);
    layer_actions::confirm(ui, &mut view, document, shell);
    if view
        .save_construction
        .as_mut()
        .is_some_and(|form| form.show(ui, document, shell))
    {
        view.save_construction = None;
    }
    ui.data_mut(|d| d.insert_temp(key, view));
}

fn pane(ui: &mut Ui, rect: Rect, id: (Uuid, &str), draw: impl FnOnce(&mut Ui)) {
    let t = ui.tokens();
    let mut child = ui.new_child(
        UiBuilder::new()
            .id_salt(id)
            .max_rect(rect)
            .layout(Layout::top_down(Align::Min)),
    );
    child.set_clip_rect(ui.clip_rect().intersect(rect));
    widgets::card_frame(&t).show(&mut child, |ui| {
        let body =
            (rect.size() - egui::Vec2::splat(2.0 * t.space.panel + 2.0)).max(egui::Vec2::ZERO);
        ui.set_min_size(body);
        ui.set_max_size(body);
        draw(ui);
    });
}

fn entry_name(pcb: &PcbDocument, index: usize) -> String {
    match &pcb.stackup().entries[index] {
        StackupEntry::Copper(c) => pcb
            .layers()
            .layer(c.layer)
            .map(|l| crate::panels::layers::layer_name(pcb, l))
            .unwrap_or_default(),
        StackupEntry::Dielectric(d) => d.name.clone().unwrap_or_else(|| {
            let number = pcb.stackup().entries[..=index]
                .iter()
                .filter(|e| matches!(e, StackupEntry::Dielectric(_)))
                .count();
            tr!("board-setup-dielectric", number = number)
        }),
    }
}

fn layers(ui: &mut Ui, pcb: &PcbDocument, info: &StackupInfo, view: &mut View, edits: &mut Edits) {
    let t = ui.tokens();
    let stack = pcb.stackup();
    let impedances = reda_engine::pcb::impedance::layers(pcb, info);
    let selected = view.selected;
    widgets::table("layers")
        .columns([
            Column::fixed(t.size.of(POSITION)),
            Column::fill(),
            Column::fixed(t.size.of(KIND)),
            Column::fixed(t.size.of(THICKNESS)),
            Column::fixed(t.size.of(56.0)),
        ])
        .show(ui, |table| {
            table.header(|h| {
                h.text("#");
                h.text(tr!("board-setup-stackup-name"));
                h.text(tr!("board-setup-stackup-kind"));
                h.text(tr!("stackup-thickness-mm"))
                    .tooltip(tr!("board-setup-stackup-units"));
                h.text("Z0");
            });
            for (i, entry) in stack.entries.iter().enumerate() {
                let name = entry_name(pcb, i);
                let uuid = entry.uuid();
                let look = RowLook::selected(if selected == Some(uuid) {
                    Selection::Live
                } else {
                    Selection::None
                });
                let mut interacted = false;
                let row = table.row_with(look, |row| match entry {
                    StackupEntry::Copper(c) => {
                        let Some(layer) = pcb.layers().layer(c.layer) else {
                            return;
                        };
                        let position =
                            format!("L{}", pcb.copper_number(c.layer).expect("a copper entry"));
                        row.text(&position);
                        if layer.side.is_some() {
                            row.text(&name);
                        } else {
                            row.ui(|ui| {
                                let label = tr!("stackup-entry-name", entry = position.as_str());
                                interacted |= view
                                    .text(
                                        ui,
                                        Field::Layer(uuid, LayerField::Name),
                                        layer.name.as_deref().unwrap_or(""),
                                        &position,
                                        &label,
                                        edits,
                                    )
                                    .gained_focus();
                            });
                        }
                        row.ui(|ui| {
                            let options = [
                                (CopperRole::Signal, tr!("stackup-signal")),
                                (CopperRole::Plane, tr!("stackup-plane")),
                                (CopperRole::Mixed, tr!("stackup-mixed")),
                            ];
                            let label = tr!("stackup-entry-kind", entry = name.as_str());
                            if let Some(role) =
                                choice(ui, (uuid, "role"), &label, layer.role, &options)
                            {
                                edits.set(Field::Layer(uuid, LayerField::Role), json!(role));
                                interacted = true;
                            }
                        });
                        row.ui(|ui| {
                            let label =
                                tr!("board-setup-stackup-thickness-of", entry = name.as_str());
                            interacted |= view
                                .text(
                                    ui,
                                    Field::Copper(uuid),
                                    &c.thickness.mm_exact(),
                                    "",
                                    &label,
                                    edits,
                                )
                                .gained_focus();
                        });
                        if let Some(estimate) = impedances
                            .iter()
                            .find(|r| r.layer == uuid)
                            .and_then(|r| r.estimate.as_ref())
                        {
                            let prefix = if estimate.approximate { "≈" } else { "" };
                            let response = row.text(format!("{prefix}{:.1} Ω", estimate.ohms));
                            if estimate.approximate {
                                use reda_engine::pcb::impedance::Model;
                                let mut why = Vec::new();
                                match estimate.model {
                                    Model::EmbeddedMicrostrip => {
                                        why.push(tr!("stackup-z0-embedded"))
                                    }
                                    Model::OffsetStripline => why.push(tr!("stackup-z0-offset")),
                                    _ => {}
                                }
                                if estimate.missing_dk {
                                    why.push(tr!("stackup-z0-missing-dk"));
                                }
                                response.tooltip(why.join("\n"));
                            }
                        }
                    }
                    StackupEntry::Dielectric(d) => {
                        row.ui(|ui| ui.add(widgets::swatch(preview::color(entry))));
                        row.ui(|ui| {
                            let label = tr!("stackup-entry-name", entry = name.as_str());
                            interacted |= view
                                .text(
                                    ui,
                                    Field::Sheet(uuid, SheetField::Name),
                                    d.name.as_deref().unwrap_or(""),
                                    &name,
                                    &label,
                                    edits,
                                )
                                .gained_focus();
                        });
                        row.ui(|ui| {
                            let options = [
                                (DielectricKind::Core, tr!("stackup-core")),
                                (DielectricKind::Prepreg, tr!("stackup-prepreg")),
                            ];
                            let label = tr!("stackup-entry-kind", entry = name.as_str());
                            if let Some(kind) = choice(ui, (uuid, "kind"), &label, d.kind, &options)
                            {
                                edits.set(Field::Sheet(uuid, SheetField::Kind), json!(kind));
                                interacted = true;
                            }
                        });
                        row.ui(|ui| {
                            let label =
                                tr!("board-setup-stackup-thickness-of", entry = name.as_str());
                            let value = d.thickness.map(|n| n.mm_exact()).unwrap_or_default();
                            let nm = info.entries[i].thickness;
                            let hint = Nm(nm.round() as i64).mm_exact();
                            let response = view.text(
                                ui,
                                Field::Sheet(uuid, SheetField::Thickness),
                                &value,
                                &hint,
                                &label,
                                edits,
                            );
                            interacted |= response.gained_focus();
                            if d.thickness.is_none() {
                                response
                                    .tooltip(tr!("stackup-resolved", thickness = preview::mm(nm)));
                            }
                        });
                    }
                });
                row.widget_info(|| WidgetInfo::labeled(Role::Row, true, &name));
                if let StackupEntry::Copper(c) = entry
                    && let Some(picker) = crate::pcb::plane::Picker::menu(&row, c.layer)
                {
                    view.plane_picker = Some(picker);
                }
                if (row.clicked() || interacted) && selected != Some(uuid) {
                    edits.select(uuid);
                }
            }
        });
    let mut widths = std::collections::BTreeMap::<Nm, Vec<String>>::new();
    for (i, reading) in impedances.iter().enumerate() {
        widths
            .entry(reading.width)
            .or_default()
            .push(format!("L{}", i + 1));
    }
    let several = widths.len() > 1;
    let widths = widths
        .into_iter()
        .map(|(width, layers)| {
            let mm = width.mm_exact();
            if several {
                format!("{} {mm} mm", layers.join(", "))
            } else {
                format!("{mm} mm")
            }
        })
        .collect::<Vec<_>>()
        .join(" · ");
    ui.label(tr!("stackup-z0-width", widths = widths.as_str()));
    layer_actions::buttons(ui, pcb, selected, edits);
    let sheet = stack
        .entries
        .iter()
        .enumerate()
        .find_map(|(index, entry)| match entry {
            StackupEntry::Dielectric(d) if Some(d.uuid) == view.selected => Some((index, d)),
            _ => None,
        });
    if let Some((_, d)) = sheet {
        sheet_details(ui, view, d, edits);
    }
    ui.horizontal(|ui| {
        let add = ui.add_enabled(sheet.is_some(), widgets::button(tr!("stackup-add-sheet")));
        let can_remove = sheet.is_some_and(|(index, _)| {
            matches!(
                stack.entries.get(index - 1),
                Some(StackupEntry::Dielectric(_))
            ) || matches!(
                stack.entries.get(index + 1),
                Some(StackupEntry::Dielectric(_))
            )
        });
        let remove = ui
            .add_enabled(can_remove, widgets::button(tr!("stackup-remove-sheet")))
            .disabled_tooltip(tr!("stackup-keep-sheet"));
        if let Some((_, d)) = sheet {
            if add.clicked() {
                edits
                    .commands
                    .push((None, "pcb.stackup.dielectric.add", json!({"beside":d.uuid})));
            }
            if remove.clicked() {
                edits.commands.push((
                    None,
                    "pcb.stackup.dielectric.remove",
                    json!({"dielectric":d.uuid}),
                ));
            }
        }
    });
    view.problems(ui, false);
}

fn choice<T: Copy + PartialEq>(
    ui: &mut Ui,
    id: impl egui::AsIdSalt,
    label: &str,
    current: T,
    options: &[(T, String)],
) -> Option<T> {
    let mut selected = current;
    let text = options
        .iter()
        .find(|(k, _)| *k == current)
        .map(|(_, s)| s.as_str())
        .unwrap_or("");
    let width = ui.available_width();
    let menu = widgets::dropdown(id, text)
        .width(width)
        .max_width(width)
        .show(ui, |ui| {
            for (v, name) in options {
                widgets::choice_value(ui, &mut selected, *v, name);
            }
        });
    menu.response
        .widget_info(|| WidgetInfo::labeled(Role::ComboBox, true, label));
    (selected != current).then_some(selected)
}

fn sheet_details(ui: &mut Ui, view: &mut View, d: &DielectricEntry, edits: &mut Edits) {
    widgets::form(("sheet", d.uuid)).show(ui, |ui| {
        for (key, label, value) in [
            (
                SheetField::Material,
                tr!("stackup-material"),
                d.material.clone().unwrap_or_default(),
            ),
            (
                SheetField::Dk,
                tr!("stackup-dk"),
                d.dk.map(|v| exact_decimal(v.into(), 1_000))
                    .unwrap_or_default(),
            ),
            (
                SheetField::Df,
                tr!("stackup-df"),
                d.df.map(|v| exact_decimal(v.into(), 1_000_000))
                    .unwrap_or_default(),
            ),
            (
                SheetField::TestFrequency,
                tr!("stackup-frequency"),
                d.test_frequency.map(|v| v.to_string()).unwrap_or_default(),
            ),
        ] {
            ui.add(widgets::form_name(&label));
            ui.scope(|ui| {
                ui.set_width(ui.tokens().size.of(148.0));
                view.text(ui, Field::Sheet(d.uuid, key), &value, "", &label, edits);
            });
            ui.end_row();
        }
    });
}

fn board(
    ui: &mut Ui,
    project: &Project,
    pcb: &PcbDocument,
    info: &StackupInfo,
    view: &mut View,
    edits: &mut Edits,
) {
    let stack = pcb.stackup();
    strong(ui, tr!("new-project-board"));
    let label = tr!("board-setup-stackup-target");
    ui.horizontal(|ui| {
        ui.add(widgets::form_name(&label));
        ui.scope(|ui| {
            ui.set_width(ui.tokens().size.of(THICKNESS));
            view.text(
                ui,
                Field::Board(BoardField::Thickness),
                &stack.thickness.mm_exact(),
                "",
                &label,
                edits,
            );
        });
    });
    view.problems(ui, true);
    layout::text(
        ui,
        tr!(
            "board-setup-stackup-total",
            copper = info.totals.copper.mm_exact(),
            dielectric = (info.totals.total - info.totals.copper).mm_exact(),
            total = info.totals.total.mm_exact(),
            target = stack.thickness.mm_exact()
        ),
    );
    ui.horizontal_wrapped(|ui| {
        let (label, status) = if info.thickness_warning {
            (tr!("stackup-outside-tolerance"), widgets::Status::Warning)
        } else {
            (tr!("stackup-within-tolerance"), widgets::Status::Success)
        };
        ui.add(widgets::badge(label).status(status));
        let label = if info.symmetric {
            tr!("stackup-symmetric")
        } else {
            tr!("stackup-asymmetric")
        };
        let badge = ui.add(widgets::badge(label));
        if !info.symmetric {
            let mut why = Vec::new();
            if !stack.copper().count().is_multiple_of(2) {
                why.push(tr!("stackup-odd"));
            }
            for pair in &info.symmetry_mismatches {
                let names: Vec<_> = pair
                    .iter()
                    .filter_map(|id| info.entries.iter().position(|e| e.uuid == *id))
                    .map(|i| entry_name(pcb, i))
                    .collect();
                why.push(names.join(" ↔ "));
            }
            badge.tooltip(why.join("\n"));
        }
    });
    let preset = project
        .output_jobs()
        .find(|j| j.pcb == pcb.uuid)
        .and_then(|j| project.fab_preset(j.fab_preset))
        .or_else(|| project.fab_presets().next());
    let suggestions = preset.map(|p| &p.stackup_suggestions);
    widgets::form("board-looks").show(ui, |ui| {
        for (key, label, value, choices) in [
            (
                BoardField::Finish,
                tr!("board-setup-finish"),
                &stack.finish,
                suggestions.map(|s| &s.finish),
            ),
            (
                BoardField::MaskColor,
                tr!("board-setup-mask-color"),
                &stack.mask_color,
                suggestions.map(|s| &s.mask_color),
            ),
            (
                BoardField::SilkColor,
                tr!("board-setup-silk-color"),
                &stack.silk_color,
                suggestions.map(|s| &s.silk_color),
            ),
        ] {
            ui.add(widgets::form_name(&label));
            ui.horizontal(|ui| {
                ui.scope(|ui| {
                    ui.set_width(ui.tokens().size.of(110.0));
                    view.text(
                        ui,
                        Field::Board(key),
                        value.as_deref().unwrap_or(""),
                        "",
                        &label,
                        edits,
                    );
                });
                if let Some(choices) = choices.filter(|s| !s.is_empty()) {
                    let menu = widgets::dropdown(key, tr!("stackup-suggestions")).show(ui, |ui| {
                        for option in choices {
                            if ui
                                .add(widgets::choice(value.as_ref() == Some(option), option))
                                .clicked()
                            {
                                edits.set(Field::Board(key), json!(option));
                            }
                        }
                    });
                    menu.response
                        .widget_info(|| WidgetInfo::labeled(Role::ComboBox, true, &label));
                }
            });
            ui.end_row();
        }
    });
}
