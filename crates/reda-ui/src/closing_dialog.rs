//! Draws the closing and quitting prompts of [`crate::closing`]. Drawing only: the ticks
//! change the prompt's rows, and the button clicked is returned for the window to act on.

use egui::Id;

use crate::closing::{Answer, Ask, Closing, Item};
use crate::settings::layout;
use crate::tr;
use crate::widgets;

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
                    layout::text(ui, tr!("closing-tab-intro", document = title.as_str()));
                    (tr!("closing-save"), tr!("closing-dont-save"))
                }
                Ask::Settings { title, .. } => {
                    layout::text(ui, tr!("closing-settings-intro", window = title.as_str()));
                    (tr!("closing-apply"), tr!("closing-discard"))
                }
                Ask::List { rows, .. } => {
                    layout::text(ui, tr!("closing-list-intro"));
                    for row in rows.iter_mut() {
                        let label = match row.item {
                            Item::Settings(_) => {
                                tr!("closing-settings-row", window = row.title.as_str())
                            }
                            _ => row.title.clone(),
                        };
                        ui.add(widgets::checkbox(&mut row.ticked, label));
                    }
                    (tr!("closing-save-selected"), tr!("closing-dont-save"))
                }
            };
            if let Some(error) = &error {
                layout::error(ui, error);
            }
            crate::modal::buttons(ui, |ui| {
                if ui.add(widgets::button(save).primary()).clicked() {
                    answer = Some(Answer::Save);
                }
                if ui.add(widgets::button(tr!("closing-cancel"))).clicked() {
                    answer = Some(Answer::Cancel);
                }
                if ui.add(widgets::button(dont_save)).clicked() {
                    answer = Some(Answer::DontSave);
                }
            });
        });
    if answer.is_none() && modal.should_close() {
        answer = Some(Answer::Cancel);
    }
    answer
}
