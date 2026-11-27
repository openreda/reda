//! The Layers panel's Mechanical Layer edits: a row's context menu (Edit, Move Up, Move
//! Down, Remove), Add Mechanical Layer, the dialog that names, re-purposes and sides a
//! Layer, and the question removing one with objects on it asks. Each edit is one
//! `pcb.layer.*` Command, returned for the window to run.

use egui::{Id, Response, Role, Ui, WidgetInfo};
use reda_engine::model::{Layer, LayerKind, PcbDocument, Purpose, Side, Uuid};
use serde_json::{Value, json};

use super::{Invocation, layer_name};
use crate::{modal, tr, widgets};

/// The dialog open over the panel, if any.
#[derive(Clone, Default)]
pub(super) enum Dialog {
    #[default]
    Closed,
    /// Adding a Layer (`layer: None`) or editing one.
    Edit {
        layer: Option<Uuid>,
        name: String,
        purpose: Purpose,
        side: Option<Side>,
    },
    /// Removing a Layer that has objects on it.
    Remove { layer: Uuid },
}

impl Dialog {
    fn id(pcb: &PcbDocument) -> Id {
        Id::NULL.with(("layers-mechanical", pcb.uuid))
    }

    pub fn load(ui: &Ui, pcb: &PcbDocument) -> Dialog {
        ui.data(|d| d.get_temp(Self::id(pcb))).unwrap_or_default()
    }

    pub fn store(self, ui: &Ui, pcb: &PcbDocument) {
        ui.data_mut(|d| d.insert_temp(Self::id(pcb), self));
    }

    /// Add Mechanical Layer: a General, unsided Layer named after its Purpose unless the
    /// user names it.
    pub fn add() -> Dialog {
        Dialog::Edit {
            layer: None,
            name: String::new(),
            purpose: Purpose::General,
            side: None,
        }
    }
}

/// A Purpose's name in the UI.
pub fn purpose_title(purpose: Purpose) -> String {
    match purpose {
        Purpose::General => tr!("layers-purpose-general"),
        Purpose::Dimensions => tr!("layers-purpose-dimensions"),
        Purpose::FabricationNotes => tr!("layers-purpose-fabrication-notes"),
        Purpose::AssemblyNotes => tr!("layers-purpose-assembly-notes"),
        Purpose::Reference => tr!("layers-purpose-reference"),
    }
}

fn side_title(side: Option<Side>) -> String {
    match side {
        None => tr!("layers-side-none"),
        Some(Side::Top) => tr!("layers-side-top"),
        Some(Side::Bottom) => tr!("layers-side-bottom"),
    }
}

/// A Mechanical Layer row's trailing text: its Purpose, with its side when sided.
pub fn trailing(layer: &Layer) -> String {
    let purpose = purpose_title(layer.purpose);
    match layer.side {
        None => purpose,
        Some(side) => format!("{purpose} · {}", side_title(Some(side))),
    }
}

/// The context menu of Mechanical Layer `layer`'s row: Move Up and Move Down go out at
/// once, Edit and Remove may open a dialog.
pub fn menu(
    row: &Response,
    pcb: &PcbDocument,
    layer: &Layer,
    dialog: &mut Dialog,
    out: &mut Vec<Invocation>,
) {
    let order: Vec<Uuid> = pcb
        .layers()
        .layers
        .iter()
        .filter(|l| l.kind == LayerKind::Mechanical)
        .map(|l| l.uuid)
        .collect();
    let at = order.iter().position(|l| *l == layer.uuid).unwrap_or(0);
    widgets::row_context_menu(row, |ui| {
        if ui
            .add(widgets::menu_item(tr!("layers-mechanical-edit")).icon("layers"))
            .clicked()
        {
            *dialog = Dialog::Edit {
                layer: Some(layer.uuid),
                name: layer.name.clone().unwrap_or_default(),
                purpose: layer.purpose,
                side: layer.side,
            };
        }
        for (direction, label, enabled) in [
            ("up", tr!("layers-mechanical-move-up"), at > 0),
            (
                "down",
                tr!("layers-mechanical-move-down"),
                at + 1 < order.len(),
            ),
        ] {
            if ui.add_enabled(enabled, widgets::menu_item(label)).clicked() {
                out.push((
                    "pcb.layer.move".into(),
                    json!({"layer": layer.uuid, "direction": direction}),
                ));
            }
        }
        widgets::menu_separator(ui);
        if ui
            .add(widgets::menu_item(tr!("layers-mechanical-remove")))
            .clicked()
        {
            if deleted(pcb, layer.uuid).is_empty() {
                out.push(remove(layer.uuid));
            } else {
                *dialog = Dialog::Remove { layer: layer.uuid };
            }
        }
    });
}

fn remove(layer: Uuid) -> Invocation {
    ("pcb.layer.remove".into(), json!({ "layer": layer }))
}

/// What removing `layer` deletes, as the counted lines `pcb.stackup.change_preview`
/// reports, Tracks first as in the Stackup editor.
fn deleted(pcb: &PcbDocument, layer: Uuid) -> Vec<String> {
    let count = |kind: &str| {
        pcb.layer_objects()
            .filter(|(on, k, _)| *on == layer && *k == kind)
            .count() as i64
    };
    [
        "track",
        "zone",
        "zone_cutout",
        "keepout",
        "text",
        "graphic",
        "dimension",
    ]
    .into_iter()
    .filter_map(|kind| {
        let n = Some(count(kind)).filter(|n| *n > 0)?;
        Some(match kind {
            "track" => tr!("layer-count-tracks", count = n),
            "zone" => tr!("layer-count-zones", count = n),
            "zone_cutout" => tr!("layer-count-cutouts", count = n),
            "keepout" => tr!("layer-count-keepouts", count = n),
            "text" => tr!("layer-count-texts", count = n),
            "graphic" => tr!("layer-count-graphics", count = n),
            _ => tr!("layer-count-dimensions", count = n),
        })
    })
    .collect()
}

/// Draws the open dialog and returns its Command once the user confirms it.
pub fn show(ui: &Ui, pcb: &PcbDocument, dialog: &mut Dialog) -> Option<Invocation> {
    let (result, closed) = match dialog {
        Dialog::Closed => return None,
        Dialog::Edit {
            layer,
            name,
            purpose,
            side,
        } => {
            let current = match layer {
                Some(uuid) => match pcb.layers().layer(*uuid) {
                    Some(l) => Some(l),
                    None => {
                        *dialog = Dialog::Closed;
                        return None;
                    }
                },
                None => None,
            };
            let title = match current {
                Some(_) => tr!("layers-mechanical-edit-title"),
                None => tr!("layers-mechanical-add-title"),
            };
            let shown = modal::Dialog::new(Id::NULL.with(("mechanical-edit", pcb.uuid)), title)
                .show(ui.ctx(), |ui| {
                    edit_fields(ui, name, purpose, side);
                    modal::buttons(ui, |ui| {
                        let confirm = match current {
                            Some(_) => tr!("settings-apply"),
                            None => tr!("layers-mechanical-add"),
                        };
                        if ui.add(widgets::button(confirm).primary()).clicked() {
                            return Some(match current {
                                Some(l) => set(l, name, *purpose, *side),
                                None => Some(add(pcb, name, *purpose, *side)),
                            });
                        }
                        if ui.add(widgets::button(tr!("dialog-cancel"))).clicked() {
                            return Some(None);
                        }
                        None
                    })
                });
            let closed = shown.should_close();
            (shown.inner, closed)
        }
        Dialog::Remove { layer } => {
            let Some(l) = pcb.layers().layer(*layer) else {
                *dialog = Dialog::Closed;
                return None;
            };
            let lines = deleted(pcb, l.uuid);
            let shown = modal::Dialog::new(
                Id::NULL.with(("mechanical-remove", pcb.uuid)),
                tr!("layers-mechanical-remove-title"),
            )
            .show(ui.ctx(), |ui| {
                ui.label(tr!(
                    "layer-objects-deleted",
                    objects = lines.join(", "),
                    layer = layer_name(pcb, l)
                ));
                modal::buttons(ui, |ui| {
                    if ui
                        .add(widgets::button(tr!("layers-mechanical-remove-confirm")).primary())
                        .clicked()
                    {
                        return Some(Some(remove(l.uuid)));
                    }
                    if ui.add(widgets::button(tr!("dialog-cancel"))).clicked() {
                        return Some(None);
                    }
                    None
                })
            });
            let closed = shown.should_close();
            (shown.inner, closed)
        }
    };
    let result = result.or(closed.then_some(None))?;
    *dialog = Dialog::Closed;
    result
}

fn edit_fields(ui: &mut Ui, name: &mut String, purpose: &mut Purpose, side: &mut Option<Side>) {
    ui.horizontal(|ui| {
        let label = ui.label(tr!("layers-mechanical-name"));
        let hint = purpose_title(*purpose);
        ui.add(widgets::text_field(name).hint(hint).width(220.0))
            .labelled_by(label.id);
    });
    ui.horizontal(|ui| {
        let label = tr!("layers-mechanical-purpose");
        ui.label(&label);
        let shown =
            widgets::dropdown("mechanical-purpose", purpose_title(*purpose)).show(ui, |ui| {
                for p in Purpose::ALL {
                    widgets::choice_value(ui, purpose, p, purpose_title(p));
                }
            });
        shown
            .response
            .widget_info(|| WidgetInfo::labeled(Role::ComboBox, true, &label));
    });
    ui.horizontal(|ui| {
        let label = tr!("layers-mechanical-side");
        ui.label(&label);
        let shown = widgets::dropdown("mechanical-side", side_title(*side)).show(ui, |ui| {
            for s in [None, Some(Side::Top), Some(Side::Bottom)] {
                widgets::choice_value(ui, side, s, side_title(s));
            }
        });
        shown
            .response
            .widget_info(|| WidgetInfo::labeled(Role::ComboBox, true, &label));
    });
}

/// `pcb.layer.add` for a new Mechanical Layer; a blank name takes the Purpose's.
fn add(pcb: &PcbDocument, name: &str, purpose: Purpose, side: Option<Side>) -> Invocation {
    let mut params = json!({"document": pcb.uuid, "kind": "mechanical", "purpose": purpose});
    if !name.trim().is_empty() {
        params["name"] = json!(name);
    }
    if let Some(side) = side {
        params["side"] = json!(side);
    }
    ("pcb.layer.add".into(), params)
}

/// `pcb.layer.set` with only what changed; none when nothing did.
fn set(layer: &Layer, name: &str, purpose: Purpose, side: Option<Side>) -> Option<Invocation> {
    let mut params = serde_json::Map::new();
    params.insert("layer".into(), json!(layer.uuid));
    if layer.name.as_deref() != Some(name.trim()) {
        params.insert("name".into(), json!(name));
    }
    if layer.purpose != purpose {
        params.insert("purpose".into(), json!(purpose));
    }
    if layer.side != side {
        params.insert("side".into(), json!(side));
    }
    (params.len() > 1).then(|| ("pcb.layer.set".into(), Value::Object(params)))
}
