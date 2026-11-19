//! The Via types table and its counted span-edit confirmation.
use super::{Edits, Field, View, choice, strong};
use crate::theme::TokensExt as _;
use crate::widgets::{Column, RowLook, Selection};
use crate::{modal, tr, widgets};
use egui::{Role, Ui, WidgetInfo};
use reda_engine::model::{LayerSpan, PcbDocument, RuleField, RuleKind, Uuid, ViaType};
use serde_json::{Value, json};

pub(super) fn table(ui: &mut Ui, pcb: &PcbDocument, view: &mut View, edits: &mut Edits) {
    let stack = pcb.stackup();
    let t = ui.tokens();
    let layers: Vec<_> = pcb
        .copper_layers()
        .enumerate()
        .map(|(i, l)| (l.uuid, format!("L{}", i + 1)))
        .collect();
    let through = stack.through_type().uuid;
    strong(ui, tr!("via-types"));
    widgets::table("via-types")
        .columns([
            Column::fill(),
            Column::fixed(t.size.of(50.0)),
            Column::fixed(t.size.of(50.0)),
            Column::fixed(t.size.of(64.0)),
            Column::fixed(t.size.of(64.0)),
        ])
        .show(ui, |table| {
            table.header(|h| {
                for title in [
                    tr!("board-setup-stackup-name"),
                    tr!("via-from"),
                    tr!("via-to"),
                    tr!("via-drill"),
                    tr!("via-pad"),
                ] {
                    h.text(title);
                }
            });
            for kind in &stack.via_types {
                let id = kind.uuid;
                let name = kind.display_name(pcb);
                let selected = if view.selected == Some(id) {
                    Selection::Live
                } else {
                    Selection::None
                };
                let mut focused = false;
                let row = table.row_with(RowLook::selected(selected), |row| {
                    row.ui(|ui| {
                        focused |= view
                            .text(
                                ui,
                                Field::Via(id, "name"),
                                kind.name.as_deref().unwrap_or(""),
                                &name,
                                &tr!("stackup-entry-name", entry = name.as_str()),
                                edits,
                            )
                            .gained_focus();
                    });
                    for (key, current, label) in [
                        (
                            "from",
                            kind.from,
                            tr!("via-from-field", name = name.as_str()),
                        ),
                        ("to", kind.to, tr!("via-to-field", name = name.as_str())),
                    ] {
                        row.ui(|ui| {
                            ui.add_enabled_ui(id != through, |ui| {
                                if let Some(layer) = choice(ui, (id, key), &label, current, &layers)
                                {
                                    edits.set(Field::Via(id, key), json!(layer));
                                    focused = true;
                                }
                            });
                        });
                    }
                    for (key, value, title) in [
                        (
                            "hole",
                            kind.hole,
                            tr!("via-drill-field", name = name.as_str()),
                        ),
                        (
                            "diameter",
                            kind.diameter.start,
                            tr!("via-pad-field", name = name.as_str()),
                        ),
                    ] {
                        row.ui(|ui| {
                            focused |= view
                                .text(
                                    ui,
                                    Field::Via(id, key),
                                    &value.mm_exact(),
                                    "",
                                    &title,
                                    edits,
                                )
                                .gained_focus();
                        });
                    }
                });
                row.widget_info(|| WidgetInfo::labeled(Role::Row, true, &name));
                if row.clicked() || focused {
                    view.selected = Some(id);
                }
                if kind.micro && pcb.span_layers(kind.span()).len() > 3 {
                    table.row(|row| {
                        row.ui(|ui| {
                            ui.add(
                                widgets::badge(tr!("layer-micro-stretched"))
                                    .status(widgets::Status::Warning),
                            );
                        })
                    });
                }
                if outside_limits(pcb, kind) {
                    table.row(|r| {
                        r.ui(|ui| {
                            ui.label(
                                t.ty.body
                                    .rich(tr!("via-limits", name = name.as_str()))
                                    .color(t.color.warning.text),
                            );
                        });
                    });
                }
            }
        });
    let next = layers.iter().enumerate().find_map(|(i, (from, _))| {
        layers[i + 1..].iter().find_map(|(to, _)| {
            let span = LayerSpan {
                from: *from,
                to: *to,
            };
            stack.type_for_span(span).is_none().then_some(span)
        })
    });
    let selection = stack
        .via_types
        .iter()
        .position(|v| Some(v.uuid) == view.selected);
    ui.horizontal_wrapped(|ui| {
        if ui
            .add_enabled(next.is_some(), widgets::button(tr!("via-add-type")))
            .clicked()
            && let Some(span) = next
        {
            edits.commands.push((
                None,
                "pcb.via_type.add",
                json!({"document":pcb.uuid,"from":span.from,"to":span.to}),
            ));
        }
        if let Some(i) = selection {
            let kind = &stack.via_types[i];
            if ui
                .add_enabled(
                    kind.uuid != through,
                    widgets::button(tr!("via-remove-type")),
                )
                .clicked()
            {
                edits
                    .commands
                    .push((None, "pcb.via_type.remove", json!({"via_type":kind.uuid})));
            }
            if ui
                .add_enabled(i > 0, widgets::button(tr!("via-move-up")))
                .clicked()
            {
                edits.commands.push((
                    None,
                    "pcb.via_type.reorder",
                    json!({"via_type":kind.uuid,"before":stack.via_types[i-1].uuid}),
                ));
            }
            if ui
                .add_enabled(
                    i + 1 < stack.via_types.len(),
                    widgets::button(tr!("via-move-down")),
                )
                .clicked()
            {
                edits.commands.push((
                    None,
                    "pcb.via_type.reorder",
                    json!({"via_type":kind.uuid,"before":stack.via_types.get(i+2).map(|v|v.uuid)}),
                ));
            }
            if let Some(mode) = crate::pcb::unused_layers::via_default(ui, kind.unused_layers) {
                edits.set(Field::Via(kind.uuid, "unused_layers"), json!(mode));
            }
            let mut micro = kind.micro;
            if ui
                .add_enabled(
                    kind.uuid != through && (kind.micro || pcb.span_layers(kind.span()).len() <= 3),
                    widgets::checkbox(&mut micro, tr!("via-micro")),
                )
                .changed()
            {
                edits.set(Field::Via(kind.uuid, "micro"), json!(micro));
            }
        }
    });
}

fn outside_limits(pcb: &PcbDocument, v: &ViaType) -> bool {
    let kind = if v.micro {
        RuleKind::MicroviaSize
    } else {
        RuleKind::ViaSize
    };
    let breach = |value, min, max| {
        reda_engine::pcb::rules::breach(
            reda_engine::pcb::rules::limit(pcb, None, kind, min),
            reda_engine::pcb::rules::limit(pcb, None, kind, max),
            value,
        )
        .is_some()
    };
    use RuleField::*;
    breach(v.hole, HoleMin, HoleMax)
        || [Some(v.diameter.start), v.diameter.middle, v.diameter.end]
            .into_iter()
            .flatten()
            .any(|d| breach(d, DiameterMin, DiameterMax))
}

#[derive(Clone)]
pub(super) struct Pending {
    pub command: &'static str,
    pub params: Value,
    pub count: u64,
    pub emptied: u64,
}

pub(super) fn confirm(ui: &Ui, view: &mut View, document: Uuid, shell: &mut crate::shell::Shell) {
    let Some(p) = &view.pending else { return };
    let remove = p.command == "pcb.via_type.remove";
    let dialog = modal::Dialog::new(
        egui::Id::NULL.with(("via-change", document)),
        tr!("via-change-title"),
    )
    .show(ui.ctx(), |ui| {
        ui.label(tr!("via-using-span", count = p.count));
        if p.emptied > 0 {
            ui.label(tr!("via-allowed-emptied", count = p.emptied));
        }
        modal::buttons(ui, |ui| {
            if !remove
                && ui
                    .add(widgets::button(tr!("via-move-vias")).primary())
                    .clicked()
            {
                return Some(Some(true));
            }
            let label = if remove {
                tr!("via-remove-type")
            } else {
                tr!("via-leave-vias")
            };
            if ui.add(widgets::button(label)).clicked() {
                return Some(Some(false));
            }
            if ui.add(widgets::button(tr!("dialog-cancel"))).clicked() {
                return Some(None);
            }
            None
        })
    });
    let action = dialog
        .inner
        .or_else(|| dialog.should_close().then_some(None));
    if let Some(choice) = action {
        let mut p = view.pending.take().unwrap();
        if let Some(move_vias) = choice {
            if !remove {
                p.params["move_vias"] = json!(move_vias);
            }
            let _ = shell.run_reporting(p.command, p.params);
        }
    }
}
