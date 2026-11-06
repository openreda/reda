//! Draws the closing and quitting prompts of [`crate::closing`]. Drawing only: the ticks
//! change the prompt's rows, and the button clicked is returned for the window to act on.

use egui::Id;

use crate::closing::{Answer, Ask, Closing, Item};
use crate::tr;

/// Draws the open prompt, if any; returns the answer clicked. Escape (or a click outside)
/// is Cancel.
pub(crate) fn show(ctx: &egui::Context, closing: &mut Closing) -> Option<Answer> {
    let error = closing.error().map(str::to_owned);
    let ask = closing.ask_mut()?;
    let mut answer = None;
    let modal = crate::modal::Dialog::new(Id::NULL.with("closing-prompt"), tr!("closing-title"))
        .width(480.0)
        .show(ctx, |ui| {
            let (save, dont_save) = match ask {
                Ask::Tab { title, .. } => {
                    ui.label(tr!("closing-tab-intro", document = title.as_str()));
                    (tr!("closing-save"), tr!("closing-dont-save"))
                }
                Ask::Settings { title, .. } => {
                    ui.label(tr!("closing-settings-intro", window = title.as_str()));
                    (tr!("closing-apply"), tr!("closing-discard"))
                }
                Ask::List { rows, .. } => {
                    ui.label(tr!("closing-list-intro"));
                    ui.separator();
                    for row in rows.iter_mut() {
                        let label = match row.item {
                            Item::Settings(_) => {
                                tr!("closing-settings-row", window = row.title.as_str())
                            }
                            _ => row.title.clone(),
                        };
                        ui.checkbox(&mut row.ticked, label);
                    }
                    (tr!("closing-save-selected"), tr!("closing-dont-save"))
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
                if ui.button(dont_save).clicked() {
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
