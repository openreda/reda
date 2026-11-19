//! Drawing the Output Job window's pages.

use egui::{Role, ScrollArea, Ui, WidgetInfo};
use reda_engine::model::{NotFittedPaste, PdfColour, SlotCommand};
use reda_engine::output::Table;
use reda_engine::output_preview::{FileState, JobPreview, PlannedFile};

use super::last_run::{FileCheck, LastRun, Manifest};
use super::{JobDraft, Section};
use crate::settings::fab_preset::placement::{CorrectionForm, FormAction, angle, match_name};
use crate::settings::{PageCx, field, layout, mono, problem_label, strong};
use crate::theme::TokensExt as _;
use crate::tr;
use crate::widgets::{self, Column, Tooltip as _};

/// The fields and dropdowns of the General page.
const FIELD_WIDTH: f32 = 280.0;

/// Draws `section`'s page of the job.
pub(super) fn page(ui: &mut Ui, cx: &mut PageCx<'_>, section: Section, draft: &mut JobDraft) {
    if draft.saved.is_none() {
        problem_label(
            ui,
            &crate::settings::Problem::new("job", tr!("output-job-gone")),
        );
        return;
    }
    match section {
        Section::General => general(ui, cx, draft),
        Section::Outputs => outputs(ui, draft),
        Section::Gerber => gerber(ui, cx, draft),
        Section::Drill => drill(ui, cx, draft),
        Section::Bom => assembly(ui, cx, draft, Section::Bom),
        Section::Cpl => assembly(ui, cx, draft, Section::Cpl),
        Section::LastRun => last_run(ui, cx, draft),
    }
}

/// The "from preset" badge and **Edit preset copy…**, which shows the preset page that
/// `section`'s read-only values come from.
fn from_preset(ui: &mut Ui, cx: &mut PageCx<'_>, draft: &JobDraft, section: Section) {
    let Some(page) = section.preset_page() else {
        return;
    };
    let t = ui.tokens();
    ui.horizontal(|ui| {
        let badge = egui::Frame::new()
            .fill(t.color.fill.control)
            .corner_radius(t.radius.mark)
            .inner_margin(egui::Margin::symmetric(
                t.space.row_x as i8,
                t.space.row_y as i8,
            ));
        badge.show(ui, |ui| {
            let text = tr!("output-job-from-preset", preset = draft.preset_name());
            ui.label(t.ty.small.rich(text).color(t.color.text.muted));
        });
        if ui
            .add(widgets::button(tr!("output-job-edit-preset")))
            .clicked()
        {
            cx.edit_preset(*draft.fab_preset.value(), page);
        }
    });
    ui.add_space(t.space.stack);
}

/// Announces a combo box as `label`.
fn labelled(response: &egui::Response, label: String) {
    response.widget_info(|| WidgetInfo::labeled(Role::ComboBox, true, label.clone()));
}

fn general(ui: &mut Ui, cx: &mut PageCx<'_>, draft: &mut JobDraft) {
    widgets::form("output-job-general").show(ui, |ui| {
        let label = tr!("output-job-name");
        ui.add(widgets::form_name(&label));
        field(
            ui,
            &label,
            draft.name.value_mut(),
            FIELD_WIDTH,
            cx.problem("name"),
        );
        ui.end_row();

        ui.add(widgets::form_name(tr!("output-job-pcb")));
        ui.vertical(|ui| {
            let combo = widgets::dropdown("output-job-pcb", draft.pcb_title())
                .width(FIELD_WIDTH)
                .show(ui, |ui| {
                    for (uuid, title) in draft.pcbs.clone() {
                        let mut picked = *draft.pcb.value();
                        widgets::choice_value(ui, &mut picked, uuid, title);
                        draft.pcb.set(picked);
                    }
                });
            labelled(&combo.response, tr!("output-job-pcb"));
            if let Some(problem) = cx.problem("pcb") {
                problem_label(ui, problem);
            }
        });
        ui.end_row();

        ui.add(widgets::form_name(tr!("output-job-fab-preset")));
        let combo = widgets::dropdown("output-job-fab-preset", draft.preset_name())
            .width(FIELD_WIDTH)
            .show(ui, |ui| {
                for (uuid, name) in draft.presets.clone() {
                    let mut picked = *draft.fab_preset.value();
                    widgets::choice_value(ui, &mut picked, uuid, name);
                    draft.fab_preset.set(picked);
                }
            });
        labelled(&combo.response, tr!("output-job-fab-preset"));
        ui.end_row();

        let label = tr!("output-job-destination");
        ui.add(widgets::form_name(&label));
        ui.vertical(|ui| {
            let problem = cx.problem("destination");
            field(
                ui,
                &label,
                draft.destination.value_mut(),
                FIELD_WIDTH,
                problem,
            );
            if draft.destination.value().trim().is_empty() {
                let default = draft.default_destination();
                layout::muted(ui, tr!("output-job-destination-default", folder = default));
            }
        });
        ui.end_row();

        let label = tr!("output-job-name-template");
        ui.add(widgets::form_name(&label));
        ui.vertical(|ui| {
            let problem = cx.problem("name_template");
            let template = draft.name_template.value_mut();
            field(ui, &label, template, FIELD_WIDTH, problem);
            layout::muted(ui, tr!("output-job-name-template-tokens"));
            if problem.is_none()
                && let Some(warning) = draft.name_warning(cx.shell.engine())
            {
                layout::warning(ui, warning);
            }
        });
        ui.end_row();
    });
    let Some(preview) = draft.preview(cx.shell.engine()).cloned() else {
        return;
    };
    let t = ui.tokens();
    ui.add_space(t.space.section);
    strong(ui, tr!("output-job-paths"));
    match &preview.destination {
        Ok(folder) => {
            for file in &preview.beside {
                mono(ui, format!("{folder}/{file}"));
            }
        }
        Err(e) => {
            layout::error(ui, e);
        }
    }
    ui.add_space(t.space.section);
    strong(ui, tr!("output-job-zip-contents"));
    from_preset(ui, cx, draft, Section::General);
    if preview.zip.is_empty() {
        layout::muted(ui, tr!("output-job-zip-empty"));
    }
    for name in &preview.zip {
        mono(ui, name);
    }
}

fn outputs(ui: &mut Ui, draft: &mut JobDraft) {
    for (label, value) in [
        (tr!("output-job-gerber"), &mut draft.gerber),
        (tr!("output-job-drill"), &mut draft.drill),
        (tr!("output-job-bom"), &mut draft.bom),
        (tr!("output-job-cpl"), &mut draft.cpl),
        (tr!("output-job-fab-notes"), &mut draft.fab_notes),
        (tr!("output-job-drill-report"), &mut draft.drill_report),
        (tr!("output-job-gerber-job"), &mut draft.gerber_job),
        (tr!("output-job-reports"), &mut draft.reports),
        (tr!("output-job-schematic-pdf"), &mut draft.schematic_pdf),
    ] {
        ui.add(widgets::checkbox(value.value_mut(), label));
    }
    let t = ui.tokens();
    ui.add_space(t.space.section);
    ui.horizontal(|ui| {
        layout::text(ui, tr!("output-job-pdf-colour"));
        let colour = draft.schematic_pdf_colour.value_mut();
        radio(
            ui,
            colour,
            PdfColour::Colour,
            tr!("output-job-pdf-colour-colour"),
        );
        radio(
            ui,
            colour,
            PdfColour::Mono,
            tr!("output-job-pdf-colour-mono"),
        );
    });
    ui.horizontal(|ui| {
        layout::text(ui, tr!("output-job-not-fitted-paste"));
        let paste = draft.not_fitted_paste.value_mut();
        radio(
            ui,
            paste,
            NotFittedPaste::Keep,
            tr!("output-job-not-fitted-keep"),
        );
        radio(
            ui,
            paste,
            NotFittedPaste::Remove,
            tr!("output-job-not-fitted-remove"),
        );
    });
}

/// A radio button that sets `value` to `option` when clicked.
fn radio<T: PartialEq>(ui: &mut Ui, value: &mut T, option: T, text: String) {
    if ui.add(widgets::radio(*value == option, text)).clicked() {
        *value = option;
    }
}

/// The words for a planned file's state.
fn state_text(state: FileState) -> String {
    match state {
        FileState::Plotted => tr!("output-job-plotted"),
        FileState::Empty => tr!("output-job-plotted-empty"),
        FileState::Off => tr!("output-job-plotted-off"),
    }
}

fn preview_of(cx: &mut PageCx<'_>, draft: &mut JobDraft) -> Option<JobPreview> {
    draft.preview(cx.shell.engine()).cloned()
}

fn gerber(ui: &mut Ui, cx: &mut PageCx<'_>, draft: &mut JobDraft) {
    from_preset(ui, cx, draft, Section::Gerber);
    let Some(preview) = preview_of(cx, draft) else {
        return;
    };
    if let Some(file) = &preview.out_of_range {
        layout::error(ui, tr!("output-job-out-of-range", file = file.as_str()));
        return;
    }
    if let Some(why) = &preview.invalid_file_name {
        layout::error(ui, why);
        return;
    }
    files(ui, "output-job-gerber-files", &preview.gerber, true);
}

/// A read-only table of planned files.
fn files(ui: &mut Ui, id: &str, files: &[PlannedFile], layers: bool) {
    let count = if layers { 3 } else { 2 };
    widgets::table(id)
        .columns(std::iter::repeat_n(Column::auto(), count))
        .show(ui, |table| {
            table.header(|h| {
                if layers {
                    h.text(tr!("output-job-column-layer"));
                }
                h.text(tr!("output-job-column-file"));
                h.text(tr!("output-job-column-plotted"));
            });
            for file in files {
                table.row(|row| {
                    if layers {
                        row.text(&file.layer);
                    }
                    row.ui(|ui| mono(ui, &file.name));
                    let text = state_text(file.state);
                    if file.state == FileState::Plotted {
                        row.text(text);
                    } else {
                        row.muted(text);
                    }
                });
            }
        });
}

fn drill(ui: &mut Ui, cx: &mut PageCx<'_>, draft: &mut JobDraft) {
    from_preset(ui, cx, draft, Section::Drill);
    let settings = cx
        .shell
        .engine()
        .project()
        .and_then(|p| p.fab_preset(*draft.fab_preset.value()))
        .map(|p| p.gerber.clone());
    if let Some(settings) = settings {
        widgets::form("output-job-drill").show(ui, |ui| {
            widgets::form_row(ui, tr!("output-job-slot-command"), |ui| {
                layout::text(
                    ui,
                    match settings.slot_command {
                        SlotCommand::Route => tr!("output-job-slot-route"),
                        SlotCommand::G85 => tr!("output-job-slot-g85"),
                    },
                )
            });
            widgets::form_row(ui, tr!("output-job-drill-files"), |ui| {
                layout::text(
                    ui,
                    if settings.merge_drill {
                        tr!("output-job-drill-merged")
                    } else {
                        tr!("output-job-drill-split")
                    },
                )
            });
        });
        let t = ui.tokens();
        ui.add_space(t.space.section);
    }
    if let Some(preview) = preview_of(cx, draft) {
        files(ui, "output-job-drill-files", &preview.drill, false);
    }
}

/// The columns of `table` that are blank in every row: an upload would carry them empty.
pub fn empty_columns(table: &Table) -> Vec<usize> {
    (0..table.header.len())
        .filter(|&i| {
            table
                .rows
                .iter()
                .all(|row| row.get(i).is_none_or(|c| c.trim().is_empty()))
        })
        .collect()
}

fn assembly(ui: &mut Ui, cx: &mut PageCx<'_>, draft: &mut JobDraft, section: Section) {
    from_preset(ui, cx, draft, section);
    let on = match section {
        Section::Bom => *draft.bom.value(),
        _ => *draft.cpl.value(),
    };
    if !on {
        layout::muted(ui, tr!("output-job-assembly-off"));
    }
    let Some(preview) = preview_of(cx, draft) else {
        return;
    };
    let preset_id = *draft.fab_preset.value();
    let corrections = cx
        .shell
        .engine()
        .project()
        .and_then(|p| p.fab_preset(preset_id))
        .map(|p| p.placement_corrections.as_slice())
        .unwrap_or_default();
    let explain = section == Section::Cpl;
    let table = match section {
        Section::Bom => &preview.bom,
        _ => &preview.cpl,
    };
    if table.rows.is_empty() {
        layout::muted(ui, tr!("output-job-assembly-no-rows"));
    }
    let empty = if table.rows.is_empty() {
        Vec::new()
    } else {
        empty_columns(table)
    };
    ScrollArea::horizontal()
        .id_salt(("output-job-table", section))
        .show(ui, |ui| {
            let t = ui.tokens();
            let widest = table.rows.iter().map(Vec::len).max().unwrap_or(0);
            let columns = table.header.len().max(widest);
            widgets::table(("output-job-rows", section))
                .columns(std::iter::repeat_n(
                    Column::auto(),
                    columns + if explain { 5 } else { 0 },
                ))
                .show(ui, |rows| {
                    rows.header(|h| {
                        if explain {
                            h.empty();
                        }
                        for (i, header) in table.header.iter().enumerate() {
                            if empty.contains(&i) {
                                let text = tr!("output-job-column-empty", column = header.as_str());
                                h.colored(text, t.color.warning.text)
                                    .tooltip(tr!("output-job-column-empty-hint"));
                            } else {
                                h.text(header);
                            }
                        }
                        if explain {
                            for label in [
                                tr!("placement-stored"),
                                tr!("placement-correction"),
                                tr!("placement-written"),
                                tr!("placement-match"),
                            ] {
                                h.text(label);
                            }
                        }
                    });
                    for (index, row) in table.rows.iter().enumerate() {
                        rows.row(|cells| {
                            let placement = explain.then(|| &preview.placements[index]);
                            if let Some(p) = placement
                                && cells.ui(|ui| {
                                    ui.add(widgets::button(tr!("placement-add"))).clicked()
                                })
                            {
                                draft.correction =
                                    Some((preset_id, CorrectionForm::for_preview(corrections, p)));
                                draft.correction_error = None;
                            }
                            for cell in row {
                                cells.text(cell);
                            }
                            if let Some(p) = placement {
                                let matched = p.matched.map(|i| (i, &corrections[i]));
                                cells.text(angle(p.stored_rotation));
                                cells.text(matched.map_or_else(String::new, |(_, c)| {
                                    let mut text = angle(c.adjustment().rotation);
                                    if let Some(offset) = c.adjustment().offset {
                                        text.push_str(&format!(
                                            " ({}, {} mm)",
                                            offset.x.mm_exact(),
                                            offset.y.mm_exact()
                                        ));
                                    }
                                    text
                                }));
                                cells.text(angle(p.written_rotation));
                                cells.text(matched.map_or_else(String::new, |(i, c)| {
                                    format!("{}. {}", i + 1, match_name(&c.matcher()))
                                }));
                            }
                        });
                    }
                });
        });
    if explain && let Some((target, form)) = &mut draft.correction {
        if *target != preset_id {
            draft.correction = None;
            return;
        }
        let mut rows = corrections.to_vec();
        match form.show(ui, &mut rows) {
            FormAction::Pending => {}
            FormAction::Cancelled => draft.correction = None,
            FormAction::Saved => {
                match cx.shell.run(
                    "fab_preset.set",
                    serde_json::json!({"preset":preset_id,"values":{"placement_corrections":rows}}),
                ) {
                    Ok(_) => {
                        draft.correction = None;
                        draft.correction_error = None;
                    }
                    Err(err) => draft.correction_error = Some(err.message),
                }
            }
        }
        if let Some(error) = &draft.correction_error {
            layout::error(ui, error);
        }
    }
}

fn last_run(ui: &mut Ui, cx: &mut PageCx<'_>, draft: &mut JobDraft) {
    let run = draft.last_run(cx.shell.engine());
    let folder = draft.destination_folder(cx.shell.engine());
    match &run {
        LastRun::NotRun => {
            layout::text(ui, tr!("output-job-not-run"));
        }
        LastRun::Unreadable(why) => {
            let text = tr!("output-job-manifest-unreadable", error = why.as_str());
            layout::error(ui, text);
        }
        LastRun::Read(manifest) => manifest_view(ui, manifest),
    }
    let t = ui.tokens();
    ui.add_space(t.space.section);
    let exists = folder.as_ref().is_some_and(|f| f.is_dir());
    let show = widgets::button(tr!("output-job-show-in-folder"));
    let button = ui.add_enabled(exists, show);
    if button.clicked()
        && let Some(folder) = folder
    {
        cx.show_folder(folder);
    }
}

fn manifest_view(ui: &mut Ui, manifest: &Manifest) {
    let t = ui.tokens();
    layout::text(
        ui,
        tr!(
            "output-job-last-run-status",
            date = manifest.date.as_str(),
            reda = manifest.reda.as_str(),
            preset = manifest.preset.as_str()
        ),
    );
    let changed = manifest.changed();
    if changed == 0 {
        layout::text(
            ui,
            tr!("output-job-last-run-match", count = manifest.files.len()),
        );
    } else {
        layout::warning(ui, tr!("output-job-last-run-changed", count = changed));
    }
    if !manifest.warnings.is_empty() {
        ui.add_space(t.space.stack);
        strong(ui, tr!("output-job-warnings"));
        for (_, message) in &manifest.warnings {
            layout::warning(ui, message);
        }
    }
    ui.add_space(t.space.stack);
    ScrollArea::horizontal()
        .id_salt("output-job-last-run-files")
        .show(ui, |ui| {
            widgets::table("output-job-last-run")
                .columns([Column::auto(); 4])
                .show(ui, |table| {
                    table.header(|h| {
                        h.text(tr!("output-job-column-file"));
                        h.text(tr!("output-job-column-size"));
                        h.text(tr!("output-job-column-sha256"));
                        h.text(tr!("output-job-column-check"));
                    });
                    for file in &manifest.files {
                        table.row(|row| {
                            row.ui(|ui| mono(ui, &file.path));
                            row.text(size(file.size));
                            let short = file.sha256.get(..12).unwrap_or(&file.sha256);
                            row.ui(|ui| mono(ui, short).tooltip(&file.sha256));
                            let warning = t.color.warning.text;
                            match file.check {
                                FileCheck::Same => row.muted(tr!("output-job-file-same")),
                                FileCheck::Changed => {
                                    row.colored(tr!("output-job-file-changed"), warning)
                                }
                                FileCheck::Missing => {
                                    row.colored(tr!("output-job-file-missing"), warning)
                                }
                            };
                        });
                    }
                });
        });
}

/// A file size for people: bytes, kB or MB.
fn size(bytes: u64) -> String {
    match bytes {
        b if b < 1024 => tr!("output-job-size-bytes", count = b),
        b if b < 1024 * 1024 => format!("{:.1} kB", b as f64 / 1024.0),
        b => format!("{:.1} MB", b as f64 / (1024.0 * 1024.0)),
    }
}
