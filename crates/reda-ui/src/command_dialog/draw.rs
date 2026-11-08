//! Drawing the generated form as a modal: a row per field with its problem under it, and
//! Run / Cancel.

use std::any::Any;

use egui::{Id, Ui};
use reda_engine::command::CommandError;
use serde_json::Value;

use super::{Field, Input, Outcome, ParamForm, ParamsDialog};
use crate::shell::Shell;
use crate::tr;

impl ParamsDialog for ParamForm {
    fn command(&self) -> &str {
        ParamForm::command(self)
    }

    fn show(&mut self, ctx: &egui::Context, shell: &mut Shell) -> Outcome {
        let registry = shell.engine().registry();
        let problems = self.problems(registry);
        let ready = self.params(registry);
        let mut outcome = Outcome::Open;
        let modal =
            crate::modal::Dialog::new(Id::NULL.with("command-form"), self.title().to_owned())
                .width(420.0)
                .show(ctx, |ui| {
                    if !self.description().is_empty() {
                        ui.weak(self.description().to_owned());
                    }
                    ui.add_space(6.0);
                    egui::Grid::new("command-form-fields")
                        .num_columns(2)
                        .spacing([8.0, 4.0])
                        .show(ui, |ui| {
                            for field in self.fields_mut() {
                                let problem = problems
                                    .iter()
                                    .find(|(name, _)| *name == field.name)
                                    .map(|(_, m)| m.as_str());
                                row(ui, field, problem);
                            }
                        });
                    if let Some((_, whole)) = problems.iter().find(|(name, _)| name.is_empty()) {
                        ui.colored_label(ui.visuals().error_fg_color, whole);
                    }
                    if let Some(error) = &self.error {
                        ui.colored_label(ui.visuals().error_fg_color, error);
                    }
                    for note in self.notes() {
                        ui.colored_label(ui.visuals().warn_fg_color, note);
                    }
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        let run =
                            ui.add_enabled(ready.is_some(), egui::Button::new(tr!("form-run")));
                        if ready.is_none() {
                            run.on_disabled_hover_text(tr!("form-fix-fields"));
                        } else if run.clicked() {
                            outcome = Outcome::Run(ready.clone().unwrap_or(Value::Null));
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

/// One field: its label, its input, and its problem under the input.
pub(crate) fn row(ui: &mut Ui, field: &mut Field, problem: Option<&str>) {
    let label = if field.required {
        field.label.clone()
    } else {
        tr!("form-optional", label = field.label.as_str())
    };
    let label = ui.label(label);
    if !field.description.is_empty() {
        label.clone().on_hover_text(&field.description);
    }
    ui.vertical(|ui| {
        let id = Id::NULL.with(("command-form-field", field.name.as_str()));
        let response = match &mut field.input {
            Input::Text(t) | Input::Integer(t) | Input::Measure(t) => {
                ui.add(egui::TextEdit::singleline(t).id(id).desired_width(260.0))
            }
            Input::Json(t) => ui.add(
                egui::TextEdit::multiline(t)
                    .id(id)
                    .code_editor()
                    .desired_rows(2)
                    .desired_width(260.0),
            ),
            Input::Flag(flag) => flag_box(ui, id, flag, field.required),
            Input::Choice { options, selected } => {
                choice_box(ui, id, options, selected, field.required)
            }
        };
        response.labelled_by(label.id);
        if let Some(problem) = problem {
            ui.colored_label(ui.visuals().error_fg_color, problem);
        }
    });
    ui.end_row();
}

fn flag_box(ui: &mut Ui, id: Id, flag: &mut Option<bool>, required: bool) -> egui::Response {
    if required {
        let mut on = flag.unwrap_or(false);
        let response = ui.checkbox(&mut on, "");
        *flag = Some(on);
        return response;
    }
    let text = |v: Option<bool>| match v {
        None => tr!("form-default"),
        Some(true) => tr!("form-yes"),
        Some(false) => tr!("form-no"),
    };
    egui::ComboBox::from_id_salt(id)
        .selected_text(text(*flag))
        .show_ui(ui, |ui| {
            for v in [None, Some(true), Some(false)] {
                ui.selectable_value(flag, v, text(v));
            }
        })
        .response
}

fn choice_box(
    ui: &mut Ui,
    id: Id,
    options: &[Value],
    selected: &mut Option<usize>,
    required: bool,
) -> egui::Response {
    let text = |i: Option<usize>| match i.and_then(|i| options.get(i)) {
        Some(Value::String(s)) => s.clone(),
        Some(other) => other.to_string(),
        None => tr!("form-choose"),
    };
    egui::ComboBox::from_id_salt(id)
        .selected_text(text(*selected))
        .show_ui(ui, |ui| {
            if !required {
                ui.selectable_value(selected, None, tr!("form-default"));
            }
            for i in 0..options.len() {
                ui.selectable_value(selected, Some(i), text(Some(i)));
            }
        })
        .response
}
