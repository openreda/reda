//! Board and selection controls for the stored unused-land policy.
use std::any::Any;
use std::collections::BTreeSet;

use egui::{Role, Ui, WidgetInfo};
use reda_engine::command::CommandError;
use reda_engine::commands::unused_layers::Target;
use reda_engine::model::{PadKind, PadRef, PcbDocument, UnusedLayers, Uuid};
use serde_json::json;

use crate::command_dialog::{Dialogs, Outcome, ParamsDialog};
use crate::properties::{MIXED, Shared};
use crate::selection::Selection;
use crate::shell::Shell;
use crate::{modal, tr, widgets};

pub(crate) const COMMAND: &str = "pcb.unused_layers.set";

/// Selected Footprints contribute their plated Pads. Explicit child selections keep
/// their instance identity even when two Footprints share a library copy.
#[derive(Default)]
struct Selected {
    pads: Vec<(Target, UnusedLayers)>,
    vias: Vec<(Target, UnusedLayers)>,
}

impl Selected {
    fn on(board: &PcbDocument, selection: &Selection) -> Self {
        let mut selected = Self::default();
        let mut placed: BTreeSet<_> = selection.pads.iter().copied().collect();
        for &id in &selection.objects {
            if let Some(via) = board.via(id) {
                selected.vias.push((Target::Object(id), via.unused_layers));
            } else if let Some(pad) = board.free_pad(id)
                && pad.kind == PadKind::PlatedThroughHole
            {
                selected.pads.push((Target::Object(id), pad.unused_layers));
            } else if let Some(instance) = board.footprint(id)
                && let Some(copy) = board.footprint_copy(instance.footprint)
            {
                placed.extend(
                    copy.footprint
                        .pads
                        .iter()
                        .filter(|pad| pad.kind == PadKind::PlatedThroughHole)
                        .map(|pad| PadRef {
                            footprint: id,
                            pad: pad.uuid,
                        }),
                );
            }
        }
        for reference in placed {
            if let Some(pad) = board
                .footprint(reference.footprint)
                .and_then(|i| board.footprint_copy(i.footprint))
                .and_then(|c| c.footprint.pads.iter().find(|p| p.uuid == reference.pad))
                && pad.kind == PadKind::PlatedThroughHole
            {
                selected
                    .pads
                    .push((Target::Pad(reference), pad.unused_layers));
            }
        }
        selected
    }

    fn targets(&self) -> Vec<Target> {
        self.pads
            .iter()
            .chain(&self.vias)
            .map(|(target, _)| *target)
            .collect()
    }
}

fn mode_label(mode: UnusedLayers) -> String {
    match mode {
        UnusedLayers::Keep => tr!("unused-lands-keep"),
        UnusedLayers::Remove => tr!("unused-lands-remove"),
        UnusedLayers::RemoveKeepEnds => tr!("unused-lands-ends"),
    }
}

fn menu(
    ui: &mut Ui,
    label: &str,
    current: Option<UnusedLayers>,
    mixed: bool,
    via: bool,
    unchanged: bool,
) -> Option<Option<UnusedLayers>> {
    let caption = if mixed {
        MIXED.to_owned()
    } else {
        current.map_or_else(|| tr!("unused-lands-unchanged"), mode_label)
    };
    let mut picked = None;
    let response = widgets::dropdown(label, caption).show(ui, |ui| {
        if unchanged
            && ui
                .add(widgets::choice(
                    current.is_none(),
                    tr!("unused-lands-unchanged"),
                ))
                .clicked()
        {
            picked = Some(None);
        }
        for mode in [
            UnusedLayers::Keep,
            UnusedLayers::Remove,
            UnusedLayers::RemoveKeepEnds,
        ] {
            if (via || mode != UnusedLayers::RemoveKeepEnds)
                && ui
                    .add(widgets::choice(
                        !mixed && current == Some(mode),
                        mode_label(mode),
                    ))
                    .clicked()
            {
                picked = Some(Some(mode));
            }
        }
    });
    response
        .response
        .widget_info(|| WidgetInfo::labeled(Role::ComboBox, true, label));
    picked
}

/// The default inherited when a new Via is created from the selected Via type.
pub(crate) fn via_default(ui: &mut Ui, current: UnusedLayers) -> Option<UnusedLayers> {
    let label = tr!("unused-lands-via-default");
    ui.label(&label);
    menu(ui, &label, Some(current), false, true, false)
        .flatten()
        .filter(|mode| *mode != current)
}

/// One edit affects every selected object of this kind, including mixed mode selections.
pub(crate) fn properties(ui: &mut Ui, shell: &Shell) -> Vec<crate::panels::properties::Out> {
    let Some(board) = shell.engine().project().and_then(|p| p.pcb()) else {
        return Vec::new();
    };
    let selection = Selected::on(board, shell.selection());
    let mut out = Vec::new();
    for (objects, key, label, via) in [
        (&selection.pads, "pads", tr!("unused-lands-pads"), false),
        (&selection.vias, "vias", tr!("unused-lands-vias"), true),
    ] {
        let Some(value) = Shared::of(objects.iter().map(|(_, mode)| *mode)) else {
            continue;
        };
        let mode = value.same().copied();
        widgets::form(("unused-lands", key)).show(ui, |ui| {
            widgets::form_row(ui, &label, |ui| {
                if let Some(Some(chosen)) = menu(ui, &label, mode, mode.is_none(), via, false)
                    && mode != Some(chosen)
                {
                    let targets: Vec<_> = objects.iter().map(|(id, _)| *id).collect();
                    out.push(crate::panels::properties::Out::Run(
                        COMMAND,
                        json!({"document":board.uuid,"objects":targets,(key):chosen}),
                    ));
                }
            });
        });
    }
    out
}

pub(crate) fn register(dialogs: &mut Dialogs) {
    dialogs.register(COMMAND, |shell, _| {
        let board = shell.engine().project().and_then(|p| p.pcb());
        let targets = board
            .map(|b| Selected::on(b, shell.selection()).targets())
            .unwrap_or_default();
        Box::new(Bulk {
            document: board.map(|b| b.uuid),
            selection: !targets.is_empty(),
            targets,
            pads: Some(UnusedLayers::Remove),
            vias: Some(UnusedLayers::Remove),
            error: None,
        })
    });
}

struct Bulk {
    document: Option<Uuid>,
    targets: Vec<Target>,
    selection: bool,
    pads: Option<UnusedLayers>,
    vias: Option<UnusedLayers>,
    error: Option<String>,
}

impl ParamsDialog for Bulk {
    fn command(&self) -> &str {
        COMMAND
    }
    fn show(&mut self, ctx: &egui::Context, shell: &mut Shell) -> Outcome {
        let Some(board) = shell
            .engine()
            .project()
            .and_then(|p| p.pcb())
            .filter(|b| Some(b.uuid) == self.document)
        else {
            return Outcome::Cancel;
        };
        let mut outcome = Outcome::Open;
        let dialog = modal::Dialog::new(
            egui::Id::NULL.with("unused-lands"),
            tr!("unused-lands-title"),
        )
        .show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                if ui
                    .add(widgets::choice(!self.selection, tr!("unused-lands-board")))
                    .clicked()
                {
                    self.selection = false;
                }
                if ui
                    .add_enabled(
                        !self.targets.is_empty(),
                        widgets::choice(self.selection, tr!("unused-lands-selection")),
                    )
                    .clicked()
                {
                    self.selection = true;
                }
            });
            widgets::form("unused-lands-options").show(ui, |ui| {
                for (mode, via, label) in [
                    (&mut self.pads, false, tr!("unused-lands-pads")),
                    (&mut self.vias, true, tr!("unused-lands-vias")),
                ] {
                    widgets::form_row(ui, &label, |ui| {
                        if let Some(selected) = menu(ui, &label, *mode, false, via, true) {
                            *mode = selected;
                        }
                    });
                }
            });
            if let Some(error) = &self.error {
                crate::settings::layout::error(ui, error);
            }
            modal::buttons(ui, |ui| {
                if ui
                    .add_enabled(
                        self.pads.is_some() || self.vias.is_some(),
                        widgets::button(tr!("settings-apply")).primary(),
                    )
                    .clicked()
                {
                    let mut params = json!({"document":board.uuid});
                    if let Some(mode) = self.pads {
                        params["pads"] = json!(mode);
                    }
                    if let Some(mode) = self.vias {
                        params["vias"] = json!(mode);
                    }
                    if self.selection {
                        params["objects"] = json!(self.targets);
                    }
                    outcome = Outcome::Run(params);
                }
                if ui.add(widgets::button(tr!("dialog-cancel"))).clicked() {
                    outcome = Outcome::Cancel;
                }
            });
        });
        if dialog.should_close() && outcome == Outcome::Open {
            Outcome::Cancel
        } else {
            outcome
        }
    }
    fn failed(&mut self, error: &CommandError) {
        self.error = Some(error.message.clone());
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}
