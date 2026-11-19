//! Board Setup's summary of the board being ordered and its free-form Fab Notes.
use super::super::model::BoardSetupDraft;
use crate::settings::{PageCx, layout, strong};
use crate::theme::TokensExt as _;
use crate::{tr, widgets};
use egui::{Role, Ui, WidgetInfo};

const NOTES_WIDTH: f32 = 480.0;
const NOTES_ROWS: usize = 4;

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
    if ui.add(widgets::button(tr!("stackup-open"))).clicked() {
        cx.open_stackup(pcb.uuid);
    }
    let t = ui.tokens();
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
