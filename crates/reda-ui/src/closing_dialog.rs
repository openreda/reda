//! Draws the closing and quitting prompts of [`crate::closing`]. Drawing only: the ticks
//! change the prompt's rows, and the button clicked is returned for the window to act on.

use egui::{Id, Modal};

use crate::closing::{Answer, Ask, Closing};
use crate::tr;

/// Draws the open prompt, if any; returns the answer clicked. Escape (or a click outside)
/// is Cancel.
pub(crate) fn show(ctx: &egui::Context, closing: &mut Closing) -> Option<Answer> {
    let error = closing.error().map(str::to_owned);
    let ask = closing.ask_mut()?;
    let mut answer = None;
    let modal = Modal::new(Id::NULL.with("closing-prompt")).show(ctx, |ui| {
        ui.set_max_width(480.0);
        ui.heading(tr!("closing-title"));
        let save = match ask {
            Ask::Tab { title, .. } => {
                ui.label(tr!("closing-tab-intro", document = title.as_str()));
                tr!("closing-save")
            }
            Ask::List { rows, .. } => {
                ui.label(tr!("closing-list-intro"));
                ui.separator();
                for row in rows.iter_mut() {
                    ui.checkbox(&mut row.ticked, row.title.as_str());
                }
                tr!("closing-save-selected")
            }
        };
        if let Some(error) = &error {
            ui.colored_label(ui.visuals().error_fg_color, error);
        }
        ui.separator();
        ui.horizontal(|ui| {
            if ui.button(save).clicked() {
                answer = Some(Answer::Save);
            }
            if ui.button(tr!("closing-dont-save")).clicked() {
                answer = Some(Answer::DontSave);
            }
            if ui.button(tr!("closing-cancel")).clicked() {
                answer = Some(Answer::Cancel);
            }
        });
    });
    if answer.is_none() && modal.should_close() {
        answer = Some(Answer::Cancel);
    }
    answer
}
