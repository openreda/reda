//! Choose a Project Construction and the inner copper to remove before its counted preview.
use super::{Edits, View};
use crate::settings::{dropdown, layout};
use crate::theme::TokensExt as _;
use crate::{modal, tr, widgets};
use egui::Ui;
use reda_engine::model::{PcbDocument, Project, Uuid};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};

pub(super) const COMMAND: &str = "pcb.stackup.apply_construction";

#[derive(Clone)]
pub(super) struct Form {
    preset: Uuid,
    name: String,
    revert: bool,
    order: Vec<Uuid>,
    target: usize,
    remove: BTreeSet<Uuid>,
    via_types: bool,
}

pub(super) fn buttons(
    ui: &mut Ui,
    project: &Project,
    pcb: &PcbDocument,
    view: &mut View,
    edits: &mut Edits,
) {
    let builds = || {
        project
            .fab_presets()
            .flat_map(|p| p.constructions.iter().map(move |b| (p.uuid, b)))
    };
    if let Some(name) = &pcb.stackup().construction {
        if project.construction_matches(pcb.stackup()) {
            layout::text(ui, tr!("construction-started", name = name.as_str()));
        } else {
            ui.horizontal(|ui| {
                layout::text(ui, tr!("construction-modified", name = name.as_str()));
                let mut matching = builds().filter(|(_, b)| b.name.eq_ignore_ascii_case(name));
                let first = matching.next();
                if ui
                    .add_enabled(first.is_some(), widgets::button(tr!("construction-revert")))
                    .clicked()
                    && let Some((preset, build)) = first
                {
                    if matching.next().is_none()
                        && build.layer_count() >= pcb.copper_layers().count()
                    {
                        edits.commands.push((
                            None,
                            COMMAND,
                            json!({"document":pcb.uuid,"preset":preset,"construction":build.name}),
                        ));
                    } else {
                        view.apply_construction = Some(Form::new(preset, &build.name, true));
                    }
                }
            });
        }
    }
    if ui
        .add_enabled(
            builds().next().is_some(),
            widgets::button(tr!("construction-apply-action")),
        )
        .clicked()
        && let Some((preset, build)) = builds()
            .find(|(_, b)| Some(&b.name) == pcb.stackup().construction.as_ref())
            .or_else(|| builds().next())
    {
        view.apply_construction = Some(Form::new(preset, &build.name, false));
    }
}

impl Form {
    fn new(preset: Uuid, name: &str, revert: bool) -> Self {
        Self {
            preset,
            name: name.to_owned(),
            revert,
            order: Vec::new(),
            target: 0,
            remove: BTreeSet::new(),
            via_types: false,
        }
    }

    pub(super) fn show(
        &mut self,
        ui: &Ui,
        project: &Project,
        pcb: &PcbDocument,
        edits: &mut Edits,
    ) -> bool {
        let mut close = false;
        let dialog = modal::Dialog::new(
            egui::Id::NULL.with(("apply-construction", pcb.uuid)),
            tr!("construction-apply-title"),
        )
        .show(ui.ctx(), |ui| {
            let options: Vec<_> = project
                .fab_presets()
                .flat_map(|preset| preset.constructions.iter().map(move |b| (preset, b)))
                .filter(|(_, b)| !self.revert || b.name.eq_ignore_ascii_case(&self.name))
                .map(|(preset, b)| {
                    (
                        (preset.uuid, b.name.clone()),
                        tr!(
                            "construction-choice",
                            preset = preset.name.as_str(),
                            name = b.name.as_str(),
                            layers = b.layer_count() as i64
                        ),
                    )
                })
                .collect();
            let mut selected = (self.preset, self.name.clone());
            dropdown(
                ui,
                "apply-build",
                &tr!("construction-title"),
                &mut selected,
                &options,
                280.,
            );
            if selected.0 != self.preset || selected.1 != self.name {
                (self.preset, self.name) = selected;
                self.order.clear();
                self.via_types = false;
            }
            let Some(build) = project
                .fab_preset(self.preset)
                .and_then(|p| p.constructions.iter().find(|b| b.name == self.name))
            else {
                ui.label(tr!("construction-unavailable"));
                close = ui.add(widgets::button(tr!("dialog-cancel"))).clicked();
                return;
            };
            let needed = self.removal_choices(ui, pcb, build.layer_count());
            if !build.via_types.is_empty() {
                ui.add(widgets::checkbox(
                    &mut self.via_types,
                    tr!("construction-apply-types"),
                ));
            }
            modal::buttons(ui, |ui| {
                if ui
                    .add_enabled(
                        self.remove.len() == needed,
                        widgets::button(tr!("settings-apply")).primary(),
                    )
                    .clicked()
                {
                    let mut params =
                        json!({"document":pcb.uuid,"preset":self.preset,"construction":self.name});
                    if needed > 0 {
                        params["remove"] = json!(self.remove);
                    }
                    if self.via_types && !build.via_types.is_empty() {
                        params["via_types"] = json!(true);
                    }
                    edits.commands.push((None, COMMAND, params));
                    close = true;
                }
                close |= ui.add(widgets::button(tr!("dialog-cancel"))).clicked();
            });
        });
        close || dialog.should_close()
    }

    /// Show every inner Layer and count. Reset the suggested removal only when
    /// the board order or the selected Construction changes.
    fn removal_choices(&mut self, ui: &mut Ui, pcb: &PcbDocument, target: usize) -> usize {
        let order: Vec<_> = pcb.copper_layers().map(|l| l.uuid).collect();
        let needed = order.len().saturating_sub(target);
        if needed > 0 {
            let mut counts = BTreeMap::<_, i64>::new();
            for (layer, _, _) in pcb.layer_objects() {
                *counts.entry(layer).or_default() += 1;
            }
            let inner = &order[1..order.len() - 1];
            if self.order != order || self.target != target {
                let mut rank: Vec<_> = inner.iter().copied().enumerate().collect();
                rank.sort_by_key(|(i, id)| {
                    (
                        counts.contains_key(id),
                        (2 * (i + 1)).abs_diff(order.len() - 1),
                    )
                });
                self.remove = rank.into_iter().take(needed).map(|(_, id)| id).collect();
            }
            ui.label(tr!("construction-choose-removals", count = needed as i64));
            egui::ScrollArea::vertical()
                .max_height(ui.tokens().size.of(240.0))
                .show(ui, |ui| {
                    for id in inner {
                        let layer = pcb.inner_layer_label(*id).unwrap();
                        let count = counts.get(id).copied().unwrap_or_default();
                        let label = tr!(
                            "construction-layer-objects",
                            layer = layer.as_str(),
                            count = count
                        );
                        let mut selected = self.remove.contains(id);
                        if ui.add(widgets::checkbox(&mut selected, label)).changed() {
                            if selected {
                                self.remove.insert(*id);
                            } else {
                                self.remove.remove(id);
                            }
                        }
                    }
                });
        } else {
            self.remove.clear();
        }
        self.order = order;
        self.target = target;
        needed
    }
}
