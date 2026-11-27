//! The custom grid dialog (`Mod`+Shift+G, the status bar's Custom…): one value or two,
//! `0.5` or `0.5, 1.27` (docs/spec/v0.3/grids-guides.md#grid-x--y). A bare number is in
//! the Document's units (a board's own, else its grid's); a value may also carry its
//! unit, `10mil, 0.5mm`. A sheet or Symbol takes one value and an Electrical Range. It
//! runs `view.grid.custom`.

use std::any::Any;

use egui::{Id, Key, Role, WidgetInfo};
use reda_engine::command::CommandError;
use reda_engine::model::Uuid;
use reda_engine::model::geom::Nm;
use reda_engine::preferences::Units;
use serde_json::{Value, json};

use crate::command_dialog::{Dialogs, Outcome, ParamsDialog};
use crate::shell::Shell;
use crate::tr;
use crate::widgets;

/// The Command the dialog fills in.
pub const COMMAND: &str = "view.grid.custom";

/// Replaces `view.grid.custom`'s generated form with [`CustomGridDialog`].
pub fn register(dialogs: &mut Dialogs) {
    dialogs.register(COMMAND, |shell, given| {
        Box::new(CustomGridDialog::new(shell, given))
    });
}

/// `text` as a step: one length, or two separated by a comma (Step X, Step Y). A part
/// with no unit is in `units`. Two values are refused where `single` (sheets and
/// Symbols).
pub fn parse_step(text: &str, units: Units, single: bool) -> Result<(Nm, Option<Nm>), String> {
    let parts: Vec<&str> = text.split(',').map(str::trim).collect();
    let length = |part: &str| -> Result<Nm, String> {
        let bare = !part.is_empty() && part.chars().all(|c| c.is_ascii_digit() || c == '.');
        let typed = if bare {
            format!("{part}{}", unit_suffix(units))
        } else {
            part.to_owned()
        };
        match Nm::parse(&typed) {
            Ok(nm) if nm.0 > 0 => Ok(nm),
            Ok(_) => Err(tr!("grid-custom-positive")),
            Err(e) => Err(e.to_string()),
        }
    };
    match parts.as_slice() {
        [x] => Ok((length(x)?, None)),
        [_, _] if single => Err(tr!("grid-custom-single")),
        [x, y] => Ok((length(x)?, Some(length(y)?))),
        _ => Err(tr!("grid-custom-too-many")),
    }
}

fn unit_suffix(units: Units) -> &'static str {
    match units {
        Units::Mm => "mm",
        Units::Mil => "mil",
    }
}

/// Asks for a custom grid step: one value or two, and on a sheet or Symbol the
/// Electrical Range.
pub struct CustomGridDialog {
    /// The params the call gave, the Document among them.
    given: Value,
    document: Option<Uuid>,
    /// The units a bare number is in.
    units: Units,
    /// Sheets and Symbols: one step, and an Electrical Range.
    single: bool,
    /// The step typed so far.
    pub step: String,
    /// The Electrical Range typed so far (sheets and Symbols); empty is 0.8 of the step.
    pub range: String,
    error: Option<String>,
}

impl CustomGridDialog {
    pub fn new(shell: &Shell, given: Value) -> CustomGridDialog {
        let document = given["document"]
            .as_str()
            .and_then(|d| d.parse().ok())
            .or_else(|| shell.active_document().map(|t| t.document));
        let ty = document.and_then(|d| crate::bars::document_type(shell, d));
        let units = document.map_or(Units::Mm, |d| crate::status_bar::units(shell, d));
        CustomGridDialog {
            given,
            document,
            units,
            single: ty.is_some_and(super::has_range),
            step: String::new(),
            range: String::new(),
            error: None,
        }
    }

    /// The params to run with what is typed, or why they cannot be made.
    pub fn params(&self) -> Result<Value, String> {
        let (x, y) = parse_step(&self.step, self.units, self.single)?;
        let mut params = self.given.clone();
        if let Some(document) = self.document {
            params["document"] = json!(document);
        }
        params["grid"] = json!(x);
        if let Some(y) = y {
            params["grid_y"] = json!(y);
        }
        if self.single && !self.range.trim().is_empty() {
            let (range, _) = parse_step(&self.range, self.units, true)?;
            params["range"] = json!(range);
        }
        Ok(params)
    }
}

impl ParamsDialog for CustomGridDialog {
    fn command(&self) -> &str {
        COMMAND
    }

    fn show(&mut self, ctx: &egui::Context, _shell: &mut Shell) -> Outcome {
        let mut outcome = Outcome::Open;
        let modal = crate::modal::Dialog::new(
            Id::NULL.with("grid-custom"),
            tr!("grid-custom-title"),
        )
        .show(ctx, |ui| {
            let mut enter = false;
            widgets::form("grid-custom-form").show(ui, |ui| {
                let label = tr!("grid-custom-step");
                widgets::form_row(ui, &label, |ui| {
                    let hint = if self.single {
                        tr!("grid-custom-hint-single")
                    } else {
                        tr!("grid-custom-hint")
                    };
                    let field = ui.add(widgets::text_field(&mut self.step).hint(hint));
                    field.widget_info(|| WidgetInfo::labeled(Role::TextInput, true, &label));
                    crate::modal::focus_on_open(&field);
                    enter |= field.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
                });
                if self.single {
                    let label = tr!("grid-custom-range");
                    widgets::form_row(ui, &label, |ui| {
                        let field = ui.add(
                            widgets::text_field(&mut self.range)
                                .hint(tr!("grid-custom-range-hint")),
                        );
                        field.widget_info(|| WidgetInfo::labeled(Role::TextInput, true, &label));
                        enter |= field.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
                    });
                }
            });
            if let Some(error) = &self.error {
                crate::settings::layout::error(ui, error);
            }
            crate::modal::buttons(ui, |ui| {
                let set = ui.add_enabled(
                    !self.step.trim().is_empty(),
                    widgets::button(tr!("grid-custom-set")).primary(),
                );
                if set.clicked() || enter {
                    match self.params() {
                        Ok(params) => outcome = Outcome::Run(params),
                        Err(why) => self.error = Some(why),
                    }
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
