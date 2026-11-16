//! Drawing the generator form: the sections on the left, the preview on the right.

use egui::{Id, Ui};
use serde_json::Value;

use super::{GenerateDialog, Preview, Section, Shown, callouts};
use crate::command_dialog::{Input, Outcome};
use crate::settings::{layout, strong};
use crate::shell::Shell;
use crate::theme::TokensExt as _;
use crate::tr;
use crate::widgets::{self, Tooltip as _};

/// How wide a row's field or dropdown is.
const FIELD_WIDTH: f32 = 160.0;
/// How wide the family dropdown is.
const FAMILY_WIDTH: f32 = 220.0;

pub(super) fn show(dialog: &mut GenerateDialog, ctx: &egui::Context, shell: &mut Shell) -> Outcome {
    let preview = dialog.preview(shell);
    let ready = dialog.run_params(shell);
    let title = match dialog.regenerating {
        Some(_) => shell
            .engine()
            .registry()
            .get(super::REGENERATE)
            .map_or_else(String::new, |m| {
                crate::bars::command_title(super::REGENERATE, m.title)
            }),
        None => dialog.form_title(shell),
    };
    let mut outcome = Outcome::Open;
    // The dialog takes most of the window's height; the sections scroll in it beside the
    // preview, leaving room for the title, notes and buttons.
    let tall = (ctx.content_rect().height() * 0.75).clamp(300.0, 900.0);
    let height = tall - 120.0;
    let wide = (ctx.content_rect().width() - 32.0).clamp(480.0, 920.0);
    let column = (wide - 40.0) / 2.0;
    let modal = crate::modal::Dialog::new(Id::NULL.with("generate-footprint"), title)
        .width(wide)
        .min_size(egui::vec2(wide - 20.0, tall))
        .show(ctx, |ui| {
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(column);
                    egui::ScrollArea::vertical()
                        .max_height(height)
                        .auto_shrink([false, true])
                        .id_salt("generate-sections")
                        .show(ui, |ui| sections(ui, dialog, shell, &preview));
                });
                // A separator here would take the whole height and grow the dialog.
                ui.add_space(ui.tokens().space.section);
                ui.vertical(|ui| {
                    ui.set_width(column);
                    egui::ScrollArea::vertical()
                        .max_height(height)
                        .auto_shrink([false, true])
                        .id_salt("generate-preview")
                        .show(ui, |ui| draw_preview(ui, &preview, height));
                });
            });
            if super::super::writable_libraries(shell).is_empty() && dialog.regenerating.is_none() {
                layout::warning(ui, tr!("library-target-missing"));
            }
            if let Some(error) = &dialog.error {
                layout::error(ui, error);
            }
            crate::modal::buttons(ui, |ui| {
                let label = match dialog.regenerating {
                    Some(_) => tr!("generate-regenerate-run"),
                    None => tr!("generate-run"),
                };
                let run = ui.add_enabled(ready.is_some(), widgets::button(label).primary());
                match &ready {
                    Some(params) if run.clicked() => outcome = Outcome::Run(params.clone()),
                    Some(_) => {}
                    None => {
                        run.disabled_tooltip(tr!("form-fix-fields"));
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

impl GenerateDialog {
    fn form_title(&self, shell: &Shell) -> String {
        let id = self.form.command();
        shell.engine().registry().get(id).map_or_else(
            || id.to_owned(),
            |m| crate::bars::command_title(id, m.title),
        )
    }
}

fn sections(ui: &mut Ui, dialog: &mut GenerateDialog, shell: &mut Shell, preview: &Preview) {
    match &dialog.regenerating {
        Some(old) => {
            layout::muted(ui, tr!("generate-regenerating", name = old.name.as_str()));
        }
        None => family_list(ui, dialog, shell),
    }
    let problems = dialog.form.problems(shell.engine().registry());
    let calculable = reda_engine::library::calculated_values(dialog.form.command()).unwrap_or(&[]);
    let mut clear = None;
    let density = dialog.form.given("/goals/density");
    for section in Section::ALL {
        let pointers: Vec<String> = dialog
            .form
            .rows()
            .iter()
            .filter(|r| r.section == section)
            .map(|r| r.pointer().to_owned())
            .filter(|p| !hidden(dialog, p))
            .collect();
        if pointers.is_empty() && section != Section::Target {
            continue;
        }
        layout::group(ui, &section_title(section));
        widgets::form(("generate-section", section as u8)).show(ui, |ui| {
            if section == Section::Target && dialog.regenerating.is_none() {
                super::super::library_picker(
                    ui,
                    shell,
                    Id::NULL.with("generate-library"),
                    &mut dialog.library,
                );
                ui.end_row();
            }
            for pointer in &pointers {
                let shown = dialog.shown(preview, pointer);
                let problem = problems
                    .iter()
                    .find(|(p, _)| p == pointer)
                    .map(|(_, m)| m.clone());
                let resettable = calculable.contains(&pointer.as_str())
                    || dialog
                        .form
                        .row(pointer)
                        .is_some_and(|r| r.default.is_some());
                let Some(row) = dialog
                    .form
                    .rows_mut()
                    .iter_mut()
                    .find(|r| r.pointer() == pointer)
                else {
                    continue;
                };
                if row_ui(
                    ui,
                    &mut row.field,
                    shown.as_ref(),
                    problem.as_deref(),
                    resettable,
                ) {
                    clear = Some(pointer.clone());
                }
            }
        });
    }
    if let Some((_, whole)) = problems.iter().find(|(p, _)| p.is_empty()) {
        layout::error(ui, whole);
    }
    if let Some(pointer) = clear {
        dialog.form.clear(&pointer);
    }
    dialog.form.follow_density(density.as_ref());
}

/// Rows the form leaves out: the Library has its picker, and regenerating keeps the
/// Footprint's name and Library.
fn hidden(dialog: &GenerateDialog, pointer: &str) -> bool {
    pointer == "/library" || (dialog.regenerating.is_some() && pointer == "/name")
}

fn section_title(section: Section) -> String {
    match section {
        Section::Target => tr!("generate-section-target"),
        Section::Package => tr!("generate-section-package"),
        Section::Goals => tr!("generate-section-goals"),
        Section::Tolerances => tr!("generate-section-tolerances"),
        Section::Pads => tr!("generate-section-pads"),
        Section::Outputs => tr!("generate-section-outputs"),
    }
}

/// The family list: every generator, by its title.
fn family_list(ui: &mut Ui, dialog: &mut GenerateDialog, shell: &Shell) {
    let label = tr!("generate-family");
    let mut picked = dialog.form.command().to_owned();
    let title = |id: &str| {
        shell.engine().registry().get(id).map_or_else(
            || id.to_owned(),
            |m| crate::bars::command_title(id, m.title),
        )
    };
    ui.horizontal(|ui| {
        ui.add(widgets::form_name(&label));
        widgets::dropdown("generate-family", title(&picked))
            .width(FAMILY_WIDTH)
            .show(ui, |ui| {
                for id in reda_engine::library::GENERATORS {
                    widgets::choice_value(ui, &mut picked, id.to_owned(), title(id));
                }
            })
            .response
            .widget_info(|| egui::WidgetInfo::labeled(egui::Role::ComboBox, true, label.clone()));
    });
    dialog.set_family(shell, &picked);
}

/// One row: its label, its input showing the calculated value while empty, the
/// IPC-7351 warning and a way back to the calculated value. True when that was clicked.
fn row_ui(
    ui: &mut Ui,
    field: &mut crate::command_dialog::Field,
    shown: Option<&Shown>,
    problem: Option<&str>,
    resettable: bool,
) -> bool {
    let label = if field.required {
        field.label.clone()
    } else {
        tr!("form-optional", label = field.label.as_str())
    };
    let label = ui.add(widgets::form_name(label));
    if !field.description.is_empty() {
        label.clone().tooltip(&field.description);
    }
    let calculated = shown.filter(|s| s.calculated).map(|s| s.text.clone());
    let mut reset = false;
    ui.vertical(|ui| {
        let id = Id::NULL.with(("generate-field", field.name.as_str()));
        let response = match &mut field.input {
            Input::Text(t) | Input::Integer(t) | Input::Measure(t) | Input::Json(t) => {
                let mut edit = widgets::text_field(t).id(id).width(FIELD_WIDTH);
                if let Some(value) = &calculated {
                    edit = edit.hint(tr!("generate-calculated", value = value.as_str()));
                }
                ui.add(edit)
            }
            Input::Flag(flag) => flag_box(ui, id, flag, calculated.as_deref()),
            Input::Choice { options, selected } => choice_box(
                ui,
                id,
                options,
                selected,
                field.required,
                calculated.as_deref(),
            ),
        };
        response.labelled_by(label.id);
        if shown.is_some_and(|s| s.overrides_ipc) {
            layout::warning(ui, tr!("generate-overrides-ipc"));
        }
        if resettable
            && shown.is_some_and(|s| !s.calculated)
            && ui
                .add(widgets::button(tr!("generate-use-calculated")).flat())
                .clicked()
        {
            reset = true;
        }
        if let Some(problem) = problem {
            layout::error(ui, problem);
        }
    });
    ui.end_row();
    reset
}

fn flag_box(
    ui: &mut Ui,
    id: Id,
    flag: &mut Option<bool>,
    calculated: Option<&str>,
) -> egui::Response {
    let yes_no = |v: bool| if v { tr!("form-yes") } else { tr!("form-no") };
    let text = |v: Option<bool>| match v {
        Some(v) => yes_no(v),
        None => match calculated {
            Some("true") => tr!("generate-calculated", value = yes_no(true)),
            Some("false") => tr!("generate-calculated", value = yes_no(false)),
            _ => tr!("form-default"),
        },
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
    calculated: Option<&str>,
) -> egui::Response {
    let text = |i: Option<usize>| match i.and_then(|i| options.get(i)) {
        Some(Value::String(s)) => s.clone(),
        Some(other) => other.to_string(),
        None => match calculated {
            Some(value) => tr!("generate-calculated", value = value),
            None if required => tr!("form-choose"),
            None => tr!("form-default"),
        },
    };
    widgets::dropdown(id, text(*selected))
        .width(FIELD_WIDTH)
        .show(ui, |ui| {
            if !required {
                widgets::choice_value(ui, selected, None, text(None));
            }
            for i in 0..options.len() {
                widgets::choice_value(ui, selected, Some(i), text(Some(i)));
            }
        })
        .response
}

fn draw_preview(ui: &mut Ui, preview: &Preview, height: f32) {
    strong(ui, tr!("generate-preview"));
    match preview {
        Preview::Incomplete => {
            layout::muted(ui, tr!("generate-preview-incomplete"));
        }
        Preview::Invalid(why) => {
            layout::error(ui, why);
        }
        Preview::Footprint(generated) => {
            let footprint = &generated.footprint;
            layout::text(ui, &footprint.name);
            layout::muted(
                ui,
                tr!(
                    "generate-preview-pads",
                    count = footprint.pads.len().to_string()
                ),
            );
            let fitted = crate::panels::library::preview::footprint_fitted(
                ui,
                footprint,
                (height * 0.5).clamp(120.0, 300.0),
                callouts::ROOM,
            );
            if let Some((painter, fit)) = fitted {
                let colour = ui.tokens().color.text.primary;
                callouts::paint(&painter, &fit, &callouts::callouts(footprint), colour);
            }
            if !generated.warnings.is_empty() {
                strong(ui, tr!("generate-warnings"));
                for w in &generated.warnings {
                    layout::warning(ui, &w.message);
                }
            }
            regenerate_changes(ui, generated);
        }
    }
}

fn regenerate_changes(ui: &mut Ui, generated: &super::Generated) {
    if generated.unchanged == Some(true) {
        layout::muted(ui, tr!("generate-unchanged"));
    }
    for lost in &generated.lost {
        let change = format!("{:?}", lost.change).to_lowercase();
        layout::text(
            ui,
            tr!(
                "generate-lost",
                what = lost.what.as_str(),
                change = change.as_str()
            ),
        );
    }
    for added in &generated.added {
        layout::text(ui, tr!("generate-added", what = added.what.as_str()));
    }
}
