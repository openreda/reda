//! Closed loops from graphics in the window (docs/spec/v0.1/pcb.md#closed-loops-from-graphics):
//! when an `add_from_graphics` Command is refused for a gap, the window says where the
//! gap is and offers the same Command along the outer edge of the strokes.

use egui::Id;
use reda_engine::command::CommandError;
use serde_json::Value;

use crate::shell::Shell;
use crate::tr;

/// A refused loop: the Command to run again with `mode: outer_edge`, and why it failed.
#[derive(Clone, Debug, PartialEq)]
pub struct LoopRetry {
    pub id: String,
    pub params: Value,
    pub message: String,
}

/// The retry for Command `id` run with `params` that failed with `error`, when that was
/// an `add_from_graphics` refused for a gap along the centre lines.
pub(crate) fn retry_for(id: &str, params: &Value, error: &CommandError) -> Option<LoopRetry> {
    if !id.ends_with(".add_from_graphics") {
        return None;
    }
    let retry = error.details.as_ref()?.get("retry")?;
    let mut params = params.clone();
    let map = params.as_object_mut()?;
    for (k, v) in retry.as_object()? {
        if map.get(k) == Some(v) {
            // Already along the outer edge: nothing else to offer.
            return None;
        }
        map.insert(k.clone(), v.clone());
    }
    Some(LoopRetry {
        id: id.to_owned(),
        params,
        message: error.message.clone(),
    })
}

/// The gap question, while a retry waits; runs the answer.
pub fn retry_dialog(ctx: &egui::Context, shell: &mut Shell) {
    let Some(pending) = shell.loop_retry() else {
        return;
    };
    let message = pending.message.clone();
    let mut answer = None;
    let response =
        crate::modal::Dialog::new(Id::NULL.with("loop-gap-retry"), tr!("loop-gap-title"))
            .width(360.0)
            .show(ctx, |ui| {
                ui.label(tr!("loop-gap-body", reason = message.as_str()));
                ui.horizontal(|ui| {
                    if ui.button(tr!("loop-gap-retry")).clicked() {
                        answer = Some(true);
                    }
                    if ui.button(tr!("loop-gap-cancel")).clicked() {
                        answer = Some(false);
                    }
                });
            });
    if answer.is_none() && response.should_close() {
        answer = Some(false);
    }
    if let Some(retry) = answer {
        let _ = shell.answer_loop_retry(retry);
    }
}
