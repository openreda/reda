//! Cached fill outcomes on the Teardrops page, with navigation to the Track end.
use egui::Ui;

use super::super::length::mm;
use crate::settings::{PageCx, layout};
use crate::{tr, widgets};

pub(super) fn outcomes(ui: &mut Ui, cx: &mut PageCx<'_>) {
    let Some(project) = cx.shell.engine().project() else {
        return;
    };
    let Some(pcb) = project.pcb() else { return };
    let document = pcb.uuid;
    let status = reda_engine::pcb::teardrop::status(project.zone_fills(document));
    layout::text(
        ui,
        tr!(
            "board-setup-teardrop-counts",
            generated = status.generated as u64,
            shortened = status.shortened as u64,
            omitted = status.left_out as u64
        ),
    );
    let mut selected = None;
    for (label, items) in [
        (
            tr!("board-setup-teardrop-shortened"),
            status.shortened_items,
        ),
        (tr!("board-setup-teardrop-omitted"), status.left_out_items),
    ] {
        if items.is_empty() {
            continue;
        }
        ui.collapsing(label, |ui| {
            for item in items {
                let layer = pcb.copper_number(item.layer).unwrap_or_default();
                let label = tr!(
                    "board-setup-teardrop-location",
                    layer = layer as u64,
                    x = mm(item.at.x),
                    y = mm(item.at.y)
                );
                ui.push_id(
                    (
                        item.track,
                        item.target,
                        item.layer,
                        item.at.x.0,
                        item.at.y.0,
                    ),
                    |ui| {
                        if ui.add(widgets::button(label)).clicked() {
                            selected = Some(item);
                        }
                    },
                );
            }
        });
    }
    if let Some(item) = selected {
        cx.shell
            .show_teardrop(document, item.track, item.layer, item.at);
    }
}
