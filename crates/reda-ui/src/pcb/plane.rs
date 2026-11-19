//! The same Net picker creates an outline-following Zone from either Layer context menu
//! or the Stackup editor's explicit offer after a role change.
use egui::{Id, Response, Role, Ui, WidgetInfo};
use reda_engine::model::{PcbDocument, Uuid};
use serde_json::{Value, json};

use crate::{modal, tr, widgets};

#[derive(Clone, Copy)]
pub(crate) struct Picker {
    layer: Uuid,
    net: Option<Uuid>,
}

impl Picker {
    pub fn new(layer: Uuid) -> Self {
        Self { layer, net: None }
    }

    pub fn menu(row: &Response, layer: Uuid) -> Option<Self> {
        widgets::row_context_menu(row, |ui| {
            ui.add(widgets::menu_item(tr!("plane-fill-layer")).icon("zone"))
                .clicked()
        })
        .filter(|r| r.inner)
        .map(|_| Self::new(layer))
    }

    pub fn show(ui: &Ui, pcb: &PcbDocument, picker: &mut Option<Self>) -> Option<Value> {
        let state = picker.as_mut()?;
        let Some(layer) = pcb.layers().layer(state.layer) else {
            *picker = None;
            return None;
        };
        let dialog = modal::Dialog::new(
            Id::NULL.with(("plane-net", pcb.uuid)),
            tr!("plane-fill-layer"),
        )
        .show(ui.ctx(), |ui| {
            ui.label(crate::panels::layers::layer_name(pcb, layer));
            let mut nets: Vec<_> = pcb.nets().collect();
            nets.sort_by_key(|n| (&n.name, n.uuid));
            let selected = state.net.and_then(|id| pcb.net(id));
            let label = tr!("plane-net");
            ui.label(&label);
            let dropdown = widgets::dropdown(
                "plane-net",
                selected
                    .map(|n| n.name.clone())
                    .unwrap_or_else(|| tr!("plane-select-net")),
            );
            let shown = dropdown.show(ui, |ui| {
                for n in nets {
                    widgets::choice_value(ui, &mut state.net, Some(n.uuid), &n.name);
                }
            });
            shown
                .response
                .widget_info(|| WidgetInfo::labeled(Role::ComboBox, true, &label));
            modal::buttons(ui, |ui| {
                if ui
                    .add_enabled(
                        state.net.is_some() && pcb.outline().is_some(),
                        widgets::button(tr!("plane-create-zone")).primary(),
                    )
                    .clicked()
                {
                    return Some(Some(json!({"layers":[state.layer],"net":state.net})));
                }
                if ui.add(widgets::button(tr!("dialog-cancel"))).clicked() {
                    return Some(None);
                }
                None
            })
        });
        let closed = dialog.should_close();
        let result = dialog.inner.or(closed.then_some(None))?;
        *picker = None;
        result
    }
}
