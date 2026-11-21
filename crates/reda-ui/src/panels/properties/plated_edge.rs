//! The Edges region of the Properties panel, for the Board Outline or a Board Cutout being
//! edited (docs/spec/v0.2.5/plated-edges.md#ui): "Plate all edges" and "Unplate all" for
//! the contour and, for an edge clicked on the canvas, its Plated checkbox and Net picker.
//! Every change runs `pcb.contour.edge.set` or `pcb.contour.edge.set_all`.

use egui::{Role, Ui, WidgetInfo};
use reda_engine::model::{PcbDocument, Project, Uuid};
use serde_json::{Value, json};

use super::{Out, region, value_width};
use crate::shell::Shell;
use crate::tr;
use crate::widgets;

/// The Edges region of the one selected contour, if it is the Board Outline or a Board
/// Cutout.
pub(super) fn show(ui: &mut Ui, shell: &Shell, project: &Project, out: &mut Vec<Out>) {
    let selection = shell.selection();
    let [contour] = selection.objects[..] else {
        return;
    };
    let Some(pcb) = project.pcb() else {
        return;
    };
    if !crate::pcb::polygon_edit::is_board_contour(pcb, contour) {
        return;
    }
    let marks = crate::pcb::polygon_edit::marks_of(pcb, contour);
    let len = edge_count(pcb, contour);
    let edge = selection
        .edges
        .iter()
        .find(|e| e.contour == contour && e.edge < len)
        .map(|e| e.edge);
    region(ui, shell, "edges", tr!("properties-region-edges"), |ui| {
        widgets::form(("properties-edges", contour)).show(ui, |ui| {
            if let Some(edge) = edge {
                edge_rows(ui, pcb, contour, edge, marks.get(&edge).copied(), out);
            }
            ui.add(widgets::form_name(tr!("properties-edges-all")));
            ui.horizontal(|ui| {
                if ui
                    .add(widgets::button(tr!("properties-edges-plate-all")))
                    .clicked()
                {
                    out.push(set_all(contour, json!({})));
                }
                if ui
                    .add(widgets::button(tr!("properties-edges-unplate-all")))
                    .clicked()
                {
                    out.push(set_all(contour, Value::Null));
                }
            });
            ui.end_row();
        });
    });
}

fn edge_count(pcb: &PcbDocument, contour: Uuid) -> usize {
    if let Some(o) = pcb.outline().filter(|o| o.uuid == contour) {
        return o.vertices.len();
    }
    pcb.cutout(contour).map_or(0, |c| c.vertices.len())
}

/// The Plated checkbox and, while plated, the Net picker of edge `edge`.
fn edge_rows(
    ui: &mut Ui,
    pcb: &PcbDocument,
    contour: Uuid,
    edge: usize,
    mark: Option<reda_engine::model::PlatedEdge>,
    out: &mut Vec<Out>,
) {
    ui.add(widgets::form_name(tr!("properties-edge-plated")));
    let mut on = mark.is_some();
    let name = tr!("properties-edge-plated");
    let response = ui.add(widgets::checkbox(&mut on, ""));
    response.widget_info(|| WidgetInfo::selected(Role::CheckBox, true, on, name.clone()));
    if response.changed() {
        out.push(set(contour, edge, on.then(|| json!({}))));
    }
    ui.end_row();

    let Some(mark) = mark else {
        return;
    };
    ui.add(widgets::form_name(tr!("properties-edge-net")));
    let mut nets: Vec<(Uuid, &str)> = pcb.nets().map(|n| (n.uuid, n.name.as_str())).collect();
    nets.sort_by(|a, b| a.1.cmp(b.1));
    let selected = match mark.net {
        Some(net) => nets
            .iter()
            .find(|(uuid, _)| *uuid == net)
            .map_or_else(String::new, |(_, n)| (*n).to_owned()),
        None => tr!("properties-track-no-net"),
    };
    let width = value_width(ui);
    let response = widgets::dropdown(("properties-edge-net", contour, edge), selected)
        .width(width)
        .max_width(width)
        .show(ui, |ui| {
            let none = mark.net.is_none();
            if ui
                .add(widgets::choice(none, tr!("properties-track-no-net")))
                .clicked()
                && !none
            {
                out.push(set(contour, edge, Some(json!({}))));
            }
            for (uuid, name) in &nets {
                let on = mark.net == Some(*uuid);
                if ui.add(widgets::choice(on, *name)).clicked() && !on {
                    out.push(set(contour, edge, Some(json!({"net": uuid}))));
                }
            }
        })
        .response;
    let name = tr!("properties-edge-net");
    response.widget_info(|| WidgetInfo::labeled(Role::ComboBox, true, name.clone()));
    ui.end_row();
}

fn set(contour: Uuid, edge: usize, plated: Option<Value>) -> Out {
    Out::Run(
        "pcb.contour.edge.set",
        json!({"contour": contour, "edge": edge, "plated": plated.unwrap_or(Value::Null)}),
    )
}

fn set_all(contour: Uuid, plated: Value) -> Out {
    Out::Run(
        "pcb.contour.edge.set_all",
        json!({"contour": contour, "plated": plated}),
    )
}
