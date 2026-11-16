//! Drawing the generated form as a modal: a row per field with its problem under it, and
//! Run / Cancel.

use std::any::Any;

use egui::{Id, Ui};
use reda_engine::command::CommandError;
use serde_json::Value;

use super::{Field, Input, Outcome, ParamForm, ParamsDialog};
use crate::settings::layout;
use crate::shell::Shell;
use crate::theme::TokensExt as _;
use crate::tr;
use crate::widgets::{self, Tooltip as _};

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
                        layout::muted(ui, self.description().to_owned());
                    }
                    ui.add_space(ui.tokens().space.stack);
                    widgets::form("command-form-fields").show(ui, |ui| {
                        for field in self.fields_mut() {
                            let problem = problems
                                .iter()
                                .find(|(name, _)| *name == field.name)
                                .map(|(_, m)| m.as_str());
                            row(ui, field, problem);
                        }
                    });
                    if let Some((_, whole)) = problems.iter().find(|(name, _)| name.is_empty()) {
                        layout::error(ui, whole);
                    }
                    if let Some(error) = &self.error {
                        layout::error(ui, error);
                    }
                    for note in self.notes() {
                        layout::warning(ui, note);
                    }
                    crate::modal::buttons(ui, |ui| {
                        let run = ui.add_enabled(
                            ready.is_some(),
                            widgets::button(tr!("form-run")).primary(),
                        );
                        if ready.is_none() {
                            run.disabled_tooltip(tr!("form-fix-fields"));
                        } else if run.clicked() {
                            outcome = Outcome::Run(ready.clone().unwrap_or(Value::Null));
                        }
                        if ui.add(widgets::button(tr!("dialog-cancel"))).clicked() {
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

/// How wide a field's control is.
const FIELD_WIDTH: f32 = 260.0;

/// One field: its label, its input, and its problem under the input.
fn row(ui: &mut Ui, field: &mut Field, problem: Option<&str>) {
    let label = if field.required {
        field.label.clone()
    } else {
        tr!("form-optional", label = field.label.as_str())
    };
    let label = ui.add(widgets::form_name(label));
    if !field.description.is_empty() {
        label.clone().tooltip(&field.description);
    }
    ui.vertical(|ui| {
        let id = Id::NULL.with(("command-form-field", field.name.as_str()));
        let response = match &mut field.input {
            Input::Text(t) | Input::Integer(t) | Input::Measure(t) => {
                ui.add(widgets::text_field(t).id(id).width(FIELD_WIDTH))
            }
            Input::Json(t) => ui.add(
                widgets::text_field(t)
                    .id(id)
                    .mono()
                    .rows(2)
                    .width(FIELD_WIDTH),
            ),
            Input::Flag(flag) => flag_box(ui, id, flag, field.required),
            Input::Choice { options, selected } => {
                choice_box(ui, id, options, selected, field.required)
            }
        };
        response.labelled_by(label.id);
        if let Some(problem) = problem {
            layout::error(ui, problem);
        }
    });
    ui.end_row();
}

fn flag_box(ui: &mut Ui, id: Id, flag: &mut Option<bool>, required: bool) -> egui::Response {
    if required {
        let mut on = flag.unwrap_or(false);
        let response = ui.add(widgets::checkbox(&mut on, ""));
        *flag = Some(on);
        return response;
    }
    let text = |v: Option<bool>| match v {
        None => tr!("form-default"),
        Some(true) => tr!("form-yes"),
        Some(false) => tr!("form-no"),
    };
    widgets::dropdown(id, text(*flag))
        .width(FIELD_WIDTH)
        .show(ui, |ui| {
            for v in [None, Some(true), Some(false)] {
                widgets::choice_value(ui, flag, v, text(v));
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
    widgets::dropdown(id, text(*selected))
        .width(FIELD_WIDTH)
        .show(ui, |ui| {
            if !required {
                widgets::choice_value(ui, selected, None, tr!("form-default"));
            }
            for i in 0..options.len() {
                widgets::choice_value(ui, selected, Some(i), text(Some(i)));
            }
        })
        .response
}
