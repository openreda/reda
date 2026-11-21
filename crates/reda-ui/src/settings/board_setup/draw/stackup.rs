//! Board Setup's summary of the board being ordered: the Stackup, the finish with the
//! edge connector's bevel and hard gold beside it, and the free-form Fab Notes.
use super::super::model::{BoardSetupDraft, EDGE_ANGLE_FIELD, EDGE_DEPTH_FIELD};
use crate::settings::{PageCx, field, layout, strong};
use crate::theme::TokensExt as _;
use crate::{tr, widgets};
use egui::{Role, Ui, WidgetInfo};

const NOTES_WIDTH: f32 = 480.0;
const NOTES_ROWS: usize = 4;
/// The bevel's angle and depth fields: room for `22.5` or `1.30`.
const BEVEL_WIDTH: f32 = 96.0;

pub fn stackup_page(ui: &mut Ui, cx: &mut PageCx<'_>, board: &mut BoardSetupDraft) {
    let Some(pcb) = cx.shell.engine().project().and_then(|p| p.pcb()) else {
        layout::muted(ui, tr!("board-setup-no-pcb"));
        return;
    };
    let stack = pcb.stackup();
    let summary = tr!(
        "stackup-summary",
        count = stack.copper().count() as i64,
        thickness = stack.thickness.mm_exact()
    );
    layout::text(ui, summary);
    let finish = match stack.finish.as_deref() {
        Some(finish) => tr!("board-setup-finish-line", finish = finish),
        None => tr!("board-setup-finish-unset"),
    };
    layout::text(ui, finish);
    if ui.add(widgets::button(tr!("stackup-open"))).clicked() {
        cx.open_stackup(pcb.uuid);
    }
    let t = ui.tokens();
    ui.add_space(t.space.section);
    edge_connector(ui, cx, board);
    ui.add_space(t.space.section);
    let label = tr!("board-setup-fab-notes");
    strong(ui, &label);
    let notes = ui.add(
        widgets::text_field(board.fab_notes_mut())
            .rows(NOTES_ROWS)
            .width(t.size.of(NOTES_WIDTH)),
    );
    notes.widget_info(|| WidgetInfo::labeled(Role::MultilineTextInput, true, &label));
    layout::muted(ui, tr!("board-setup-fab-notes-note"));
}

/// The card edge: a bevel with its angle and depth, and selective hard gold on the Edge
/// Connector Pads, which leaves the finish as it is.
fn edge_connector(ui: &mut Ui, cx: &PageCx<'_>, board: &mut BoardSetupDraft) {
    strong(ui, tr!("board-setup-edge-connector"));
    let has_pads = board.has_edge_connector_pads();
    let edge = board.edge_mut();
    ui.add(widgets::checkbox(
        &mut edge.bevel,
        tr!("board-setup-edge-bevel"),
    ));
    if edge.bevel {
        ui.horizontal(|ui| {
            field(
                ui,
                &tr!("board-setup-edge-angle"),
                &mut edge.angle,
                BEVEL_WIDTH,
                cx.problem(EDGE_ANGLE_FIELD),
            );
            field(
                ui,
                &tr!("board-setup-edge-depth"),
                &mut edge.depth,
                BEVEL_WIDTH,
                cx.problem(EDGE_DEPTH_FIELD),
            );
        });
        layout::muted(ui, tr!("board-setup-edge-bevel-note"));
    }
    ui.add(widgets::checkbox(
        &mut edge.hard_gold,
        tr!("board-setup-edge-hard-gold"),
    ));
    layout::muted(ui, tr!("board-setup-edge-hard-gold-note"));
    if (edge.bevel || edge.hard_gold) && !has_pads {
        layout::muted(ui, tr!("board-setup-edge-no-pads"));
    }
}
