//! The words of new free text: the Text Tool's click calls `schematic.text.add` without
//! them, and this dialog asks for them instead of the generated form.

use std::any::Any;

use egui::{Id, Key, Modal, Role, WidgetInfo};
use reda_engine::command::CommandError;
use serde_json::Value;

use crate::command_dialog::{Outcome, ParamsDialog};
use crate::shell::Shell;
use crate::tr;

/// Asks for the words of text the Text Tool is placing.
pub struct TextDialog {
    /// The params the call gave: the sheet, the point and the rotation.
    given: Value,
    /// The words typed so far.
    pub words: String,
    error: Option<String>,
}

impl TextDialog {
    pub fn new(given: Value) -> TextDialog {
        TextDialog {
            given,
            words: String::new(),
            error: None,
        }
    }

    /// The params to run with the words typed, once there are any.
    pub fn params(&self) -> Option<Value> {
        if self.words.trim().is_empty() {
            return None;
        }
        let mut params = self.given.clone();
        params["text"] = Value::String(self.words.clone());
        Some(params)
    }
}

impl ParamsDialog for TextDialog {
    fn command(&self) -> &str {
        "schematic.text.add"
    }

    fn show(&mut self, ctx: &egui::Context, _shell: &mut Shell) -> Outcome {
        let mut outcome = Outcome::Open;
        let modal = Modal::new(Id::NULL.with("schematic-text-words")).show(ctx, |ui| {
            ui.heading(tr!("schematic-text-title"));
            let field = ui.text_edit_singleline(&mut self.words);
            field.widget_info(|| {
                WidgetInfo::labeled(Role::TextInput, true, tr!("schematic-text-words"))
            });
            field.request_focus();
            let enter = field.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
            if let Some(error) = &self.error {
                ui.colored_label(ui.visuals().error_fg_color, error);
            }
            ui.horizontal(|ui| {
                let place = ui.add_enabled(
                    self.params().is_some(),
                    egui::Button::new(tr!("schematic-text-place")),
                );
                if (place.clicked() || enter)
                    && let Some(params) = self.params()
                {
                    outcome = Outcome::Run(params);
                }
                if ui.button(tr!("dialog-cancel")).clicked() {
                    outcome = Outcome::Cancel;
                }
            });
        });
        if modal.should_close() && outcome == Outcome::Open {
            outcome = Outcome::Cancel;
        }
        outcome
    }

    fn failed(&mut self, error: &CommandError) {
        self.error = Some(error.message.clone());
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}
