//! The Layer children, clearance pair grid and Layer-by-Layer resolved values in Board Setup.

use super::{BoardSetupDraft, RuleField, kind_name, value_field};
use crate::widgets::Column;
use crate::{tr, widgets};
use egui::Ui;
use reda_engine::model::{LayerGroup, PcbDocument, RuleKind, RuleLayers, RuleScope, Uuid};
use reda_engine::pcb::rules::resolve_for_classes;

use crate::properties::rules::{layer_name, layer_scope_label};

fn edit(ui: &mut Ui, board: &mut BoardSetupDraft, scope: &RuleScope, field: RuleField, name: &str) {
    let old = board.scoped_value(scope, field).to_owned();
    let mut value = old.clone();
    value_field(ui, field, name, &mut value, true, None);
    if value != old {
        board.set_scoped(scope, field, &value);
    }
}

fn layer_rows(
    ui: &mut Ui,
    pcb: &PcbDocument,
    board: &mut BoardSetupDraft,
    owner: &RuleScope,
    kind: RuleKind,
    fields: &[RuleField],
) {
    for scope in board.layer_scopes(owner, kind) {
        let layers = scope.layers.as_ref().expect("Layer child");
        ui.push_id(&scope, |ui| {
            ui.label(layer_scope_label(pcb, layers));
            for &field in fields {
                ui.horizontal_wrapped(|ui| {
                    ui.label(field.name());
                    edit(ui, board, &scope, field, &field.name());
                });
            }
        });
    }
    widgets::section(tr!("board-rule-add-layer"))
        .id_salt(("add", owner, kind))
        .show(ui, |ui| {
            let mut add = None;
            ui.horizontal_wrapped(|ui| {
                for group in [LayerGroup::Outer, LayerGroup::Inner, LayerGroup::Plane] {
                    let layers = RuleLayers::Group(group);
                    if ui
                        .add(widgets::button(layer_scope_label(pcb, &layers)))
                        .clicked()
                    {
                        add = Some(layers);
                    }
                }
            });
            let id = ui.scope_id().with("named-layers");
            let mut selected = ui.data(|d| d.get_temp::<Vec<Uuid>>(id)).unwrap_or_default();
            ui.label(tr!("board-rule-named"));
            ui.horizontal_wrapped(|ui| {
                for layer in pcb.copper_layers() {
                    let mut on = selected.contains(&layer.uuid);
                    if ui
                        .add(widgets::checkbox(&mut on, layer_name(pcb, layer.uuid)))
                        .changed()
                    {
                        if on {
                            selected.push(layer.uuid);
                        } else {
                            selected.retain(|id| *id != layer.uuid);
                        }
                    }
                }
                if ui
                    .add_enabled(!selected.is_empty(), widgets::button(tr!("board-rule-add")))
                    .clicked()
                {
                    selected.sort();
                    add = Some(RuleLayers::Named(std::mem::take(&mut selected)));
                }
            });
            ui.data_mut(|d| d.insert_temp(id, selected));
            if let Some(layers) = add {
                let scope = RuleScope {
                    layers: Some(layers),
                    ..owner.clone()
                };
                let first = fields[0];
                let current = board.scoped_value(&scope, first).to_owned();
                board.set_scoped(&scope, first, &current);
            }
        });
}

pub(super) fn class_section(
    ui: &mut Ui,
    pcb: &PcbDocument,
    board: &mut BoardSetupDraft,
    class: Uuid,
) {
    let owner = RuleScope::class(class);
    for kind in [
        RuleKind::TrackWidth,
        RuleKind::Clearance,
        RuleKind::AnnularRing,
        RuleKind::ZoneConnection,
    ] {
        let fields: Vec<_> = RuleField::ALL
            .into_iter()
            .filter(|f| f.rule().0 == kind)
            .collect();
        widgets::section(kind_name(kind))
            .id_salt(kind)
            .show(ui, |ui| {
                layer_rows(ui, pcb, board, &owner, kind, &fields);
            });
    }
    let rule = super::Rule::Teardrops;
    widgets::section(rule.title())
        .id_salt(RuleKind::Teardrop)
        .show(ui, |ui| {
            for section in rule.sections() {
                let RuleField::Teardrop(target, _) = section.fields[0] else {
                    continue;
                };
                let name = super::super::model::tear_target_name(target);
                widgets::section(name)
                    .id_salt(section.fields[0])
                    .show(ui, |ui| {
                        layer_rows(ui, pcb, board, &owner, RuleKind::Teardrop, section.fields);
                    });
            }
        });
}

fn classes(pcb: &PcbDocument) -> Vec<(Uuid, String)> {
    let mut classes: Vec<_> = pcb.net_classes().collect();
    classes.sort_by_key(|c| c.rank);
    std::iter::once((Uuid::NIL, reda_engine::pcb::DEFAULT_NET_CLASS.to_owned()))
        .chain(classes.into_iter().map(|c| (c.uuid, c.name.clone())))
        .collect()
}

fn pairs(ui: &mut Ui, pcb: &PcbDocument, board: &mut BoardSetupDraft, fields: &[RuleField]) {
    let classes = classes(pcb);
    widgets::table("pair-clearance")
        .columns(std::iter::repeat_n(Column::auto(), 3 + fields.len()))
        .show(ui, |table| {
            table.header(|header| {
                header.empty();
                header.empty();
                for &field in fields {
                    header.text(field.name());
                }
                header.empty();
            });
            for (i, (a, name)) in classes.iter().enumerate() {
                for (b, other) in &classes[i..] {
                    let mut ids = [*a, *b];
                    ids.sort();
                    let scope = RuleScope {
                        net_class_pair: Some(ids),
                        ..RuleScope::BOARD
                    };
                    table.row(|row| {
                        row.text(name);
                        row.text(other);
                        for &field in fields {
                            row.ui(|ui| {
                                let mut label = format!("{name} / {other}");
                                if field != RuleField::Clearance {
                                    label.push(' ');
                                    label.push_str(&field.name());
                                }
                                edit(ui, board, &scope, field, &label);
                                let value = field
                                    .parse(board.scoped_value(&scope, field))
                                    .ok()
                                    .flatten()
                                    .and_then(|v| v.as_i64());
                                if value.is_some_and(|n| {
                                    pcb.copper_layers().any(|l| {
                                        ids.iter().any(|id| {
                                            resolve_for_classes(
                                                pcb,
                                                &[*id],
                                                Some(l.uuid),
                                                RuleKind::Clearance,
                                                field.rule().1,
                                            )
                                            .and_then(|s| s.value.length())
                                            .is_some_and(|own| n < own.0)
                                        })
                                    })
                                }) {
                                    ui.weak(tr!("board-rule-pair-lower"));
                                }
                            });
                        }
                        row.ui(|ui| {
                            widgets::section(tr!("board-rule-layers"))
                                .id_salt(("pair", ids))
                                .show(ui, |ui| {
                                    layer_rows(ui, pcb, board, &scope, RuleKind::Clearance, fields);
                                });
                        });
                    });
                }
            }
        });
}

fn by_layer(
    ui: &mut Ui,
    pcb: &PcbDocument,
    board: &BoardSetupDraft,
    kind: RuleKind,
    fields: &[RuleField],
) {
    let classes = classes(pcb);
    let key = ui.scope_id().with("chosen-class");
    let mut chosen = ui.data(|d| d.get_temp(key)).unwrap_or(Uuid::NIL);
    let name = classes
        .iter()
        .find(|(id, _)| *id == chosen)
        .unwrap_or(&classes[0])
        .1
        .clone();
    widgets::dropdown("by-layer-class", name).show(ui, |ui| {
        for (id, name) in classes {
            widgets::choice_value(ui, &mut chosen, id, name);
        }
    });
    ui.data_mut(|d| d.insert_temp(key, chosen));
    widgets::table("by-layer-values")
        .columns(std::iter::repeat_n(Column::auto(), fields.len() + 1))
        .show(ui, |table| {
            table.header(|h| {
                h.text(tr!("board-rule-layers"));
                for f in fields {
                    h.text(f.name());
                }
            });
            for layer in pcb.copper_layers() {
                table.row(|row| {
                    row.text(layer_name(pcb, layer.uuid));
                    for &field in fields {
                        let text = resolve_for_classes(
                            pcb,
                            &[chosen],
                            Some(layer.uuid),
                            kind,
                            field.rule().1,
                        )
                        .and_then(|s| pcb.rule(s.rule))
                        .map(|r| field.text_in(&r.values))
                        .unwrap_or_default();
                        row.ui(|ui| {
                            ui.label(text);
                            if let Some((_, limit, preset)) =
                                board.scoped_limit(field, layer.side.is_none())
                            {
                                ui.weak(format!("{preset} · {} mm", super::mm(limit)));
                            }
                        });
                    }
                });
            }
        });
}

pub(super) fn rule_section(
    ui: &mut Ui,
    pcb: &PcbDocument,
    board: &mut BoardSetupDraft,
    kind: RuleKind,
    fields: &[RuleField],
) {
    widgets::section(tr!("board-rule-layers"))
        .id_salt("board-layer-rows")
        .show(ui, |ui| {
            layer_rows(ui, pcb, board, &RuleScope::BOARD, kind, fields);
        });
    if kind == RuleKind::Clearance {
        widgets::section(tr!("board-rule-class-pairs"))
            .id_salt("pair-grid")
            .show(ui, |ui| pairs(ui, pcb, board, fields));
    }
    widgets::section(tr!("board-rule-by-layer"))
        .id_salt("by-layer")
        .show(ui, |ui| by_layer(ui, pcb, board, kind, fields));
}
