//! Transient solo while the pointer rests on a Layer chip. The panel, Layer
//! control and status bar report the hovered chip during layout. At frame end
//! the board canvas gets a preview without changing its saved Layer state.

use crate::shell::Shell;
use egui::{Context, Id, Response};
use reda_engine::model::Uuid;
use std::time::Duration;

const DELAY: f64 = 0.3;

pub(crate) fn chip(response: &Response, layer: Uuid) {
    if response.hovered() {
        response.ctx.data_mut(|d| {
            d.insert_temp(Id::unique("hovered-layer-chip"), Some((response.id, layer)))
        });
    }
}

pub(crate) fn finish(ctx: &Context, shell: &mut Shell) {
    let chip = ctx.data_mut(|d| {
        d.remove_temp::<Option<(Id, Uuid)>>(Id::unique("hovered-layer-chip"))
            .flatten()
    });
    let chip = chip.filter(|_| {
        shell.preferences().pcb.layer.hover_solo
            && !shell.pcb_tool().in_progress()
            && !shell.pcb_draw().in_progress()
            && !shell.router().routing()
    });
    let timer = Id::unique("solo-layer-timer");
    let now = ctx.input(|i| i.time);
    let solo = if let Some((id, layer)) = chip {
        let since = ctx.data_mut(|d| {
            let (old, since) = d.get_temp::<(Id, f64)>(timer).unwrap_or((id, now));
            let since = if old == id { since } else { now };
            d.insert_temp(timer, (id, since));
            since
        });
        let left = DELAY - (now - since);
        if left > 0.0 {
            ctx.request_repaint_after(Duration::from_secs_f64(left));
            None
        } else {
            Some(layer)
        }
    } else {
        ctx.data_mut(|d| d.remove::<(Id, f64)>(timer));
        None
    };
    if shell.board_view().hover_solo != solo {
        shell.board_view_mut().hover_solo = solo;
        if let Some(pcb) = shell.engine().project().and_then(|p| p.pcb()) {
            shell.redraw(pcb.uuid);
            ctx.request_repaint();
        }
    }
}
