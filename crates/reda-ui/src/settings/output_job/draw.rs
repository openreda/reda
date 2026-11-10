//! Drawing the Output Job window's pages.

use egui::{ComboBox, Grid, RichText, Role, ScrollArea, Ui, WidgetInfo};
use reda_engine::model::{NotFittedPaste, PdfColour, SlotCommand};
use reda_engine::output::Table;
use reda_engine::output_preview::{FileState, JobPreview, PlannedFile};

use super::last_run::{FileCheck, LastRun, Manifest};
use super::{JobDraft, Section};
use crate::settings::{PageCx, field, problem_label};
use crate::tr;

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
    ui.horizontal(|ui| {
        let badge = egui::Frame::new()
            .fill(ui.visuals().faint_bg_color)
            .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
            .corner_radius(4.0)
            .inner_margin(egui::Margin::symmetric(6, 1));
        badge.show(ui, |ui| {
            ui.label(
                RichText::new(tr!("output-job-from-preset", preset = draft.preset_name())).small(),
            );
        });
        if ui.button(tr!("output-job-edit-preset")).clicked() {
            cx.edit_preset(*draft.fab_preset.value(), page);
        }
    });
    ui.add_space(6.0);
}

/// Announces a combo box as `label`.
fn labelled(response: &egui::Response, label: String) {
    response.widget_info(|| WidgetInfo::labeled(Role::ComboBox, true, label.clone()));
}

fn general(ui: &mut Ui, cx: &mut PageCx<'_>, draft: &mut JobDraft) {
    Grid::new("output-job-general")
        .num_columns(2)
        .spacing([12.0, 8.0])
        .show(ui, |ui| {
            let label = tr!("output-job-name");
            ui.label(&label);
            field(
                ui,
                &label,
                draft.name.value_mut(),
                280.0,
                cx.problem("name"),
            );
            ui.end_row();

            ui.label(tr!("output-job-pcb"));
            ui.vertical(|ui| {
                let combo = ComboBox::from_id_salt("output-job-pcb")
                    .selected_text(draft.pcb_title())
                    .width(280.0)
                    .show_ui(ui, |ui| {
                        for (uuid, title) in draft.pcbs.clone() {
                            let mut picked = *draft.pcb.value();
                            ui.selectable_value(&mut picked, uuid, title);
                            draft.pcb.set(picked);
                        }
                    });
                labelled(&combo.response, tr!("output-job-pcb"));
                if let Some(problem) = cx.problem("pcb") {
                    problem_label(ui, problem);
                }
            });
            ui.end_row();

            ui.label(tr!("output-job-fab-preset"));
            let combo = ComboBox::from_id_salt("output-job-fab-preset")
                .selected_text(draft.preset_name())
                .width(280.0)
                .show_ui(ui, |ui| {
                    for (uuid, name) in draft.presets.clone() {
                        let mut picked = *draft.fab_preset.value();
                        ui.selectable_value(&mut picked, uuid, name);
                        draft.fab_preset.set(picked);
                    }
                });
            labelled(&combo.response, tr!("output-job-fab-preset"));
            ui.end_row();

            let label = tr!("output-job-destination");
            ui.label(&label);
            ui.vertical(|ui| {
                let problem = cx.problem("destination");
                field(ui, &label, draft.destination.value_mut(), 280.0, problem);
                if draft.destination.value().trim().is_empty() {
                    let default = draft.default_destination();
                    ui.weak(tr!("output-job-destination-default", folder = default));
                }
            });
            ui.end_row();

            let label = tr!("output-job-name-template");
            ui.label(&label);
            ui.vertical(|ui| {
                let problem = cx.problem("name_template");
                field(ui, &label, draft.name_template.value_mut(), 280.0, problem);
                ui.weak(tr!("output-job-name-template-tokens"));
                if problem.is_none()
                    && let Some(warning) = draft.name_warning(cx.shell.engine())
                {
                    ui.colored_label(ui.visuals().warn_fg_color, warning);
                }
            });
            ui.end_row();
        });
    let Some(preview) = draft.preview(cx.shell.engine()).cloned() else {
        return;
    };
    ui.add_space(10.0);
    ui.strong(tr!("output-job-paths"));
    match &preview.destination {
        Ok(folder) => {
            for file in &preview.beside {
                ui.monospace(format!("{folder}/{file}"));
            }
        }
        Err(e) => {
            ui.colored_label(ui.visuals().error_fg_color, e);
        }
    }
    ui.add_space(10.0);
    ui.strong(tr!("output-job-zip-contents"));
    from_preset(ui, cx, draft, Section::General);
    if preview.zip.is_empty() {
        ui.weak(tr!("output-job-zip-empty"));
    }
    for name in &preview.zip {
        ui.monospace(name);
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
        ui.checkbox(value.value_mut(), label);
    }
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label(tr!("output-job-pdf-colour"));
        let colour = draft.schematic_pdf_colour.value_mut();
        ui.radio_value(
            colour,
            PdfColour::Colour,
            tr!("output-job-pdf-colour-colour"),
        );
        ui.radio_value(colour, PdfColour::Mono, tr!("output-job-pdf-colour-mono"));
    });
    ui.horizontal(|ui| {
        ui.label(tr!("output-job-not-fitted-paste"));
        let paste = draft.not_fitted_paste.value_mut();
        ui.radio_value(
            paste,
            NotFittedPaste::Keep,
            tr!("output-job-not-fitted-keep"),
        );
        ui.radio_value(
            paste,
            NotFittedPaste::Remove,
            tr!("output-job-not-fitted-remove"),
        );
    });
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
        let text = tr!("output-job-out-of-range", file = file.as_str());
        ui.colored_label(ui.visuals().error_fg_color, text);
        return;
    }
    files(ui, "output-job-gerber-files", &preview.gerber, true);
}

/// A read-only table of planned files.
fn files(ui: &mut Ui, id: &str, files: &[PlannedFile], layers: bool) {
    Grid::new(id)
        .num_columns(if layers { 3 } else { 2 })
        .striped(true)
        .spacing([16.0, 4.0])
        .show(ui, |ui| {
            if layers {
                ui.strong(tr!("output-job-column-layer"));
            }
            ui.strong(tr!("output-job-column-file"));
            ui.strong(tr!("output-job-column-plotted"));
            ui.end_row();
            for file in files {
                if layers {
                    ui.label(&file.layer);
                }
                ui.monospace(&file.name);
                let text = state_text(file.state);
                if file.state == FileState::Plotted {
                    ui.label(text);
                } else {
                    ui.weak(text);
                }
                ui.end_row();
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
        Grid::new("output-job-drill")
            .num_columns(2)
            .spacing([12.0, 6.0])
            .show(ui, |ui| {
                ui.label(tr!("output-job-slot-command"));
                ui.label(match settings.slot_command {
                    SlotCommand::Route => tr!("output-job-slot-route"),
                    SlotCommand::G85 => tr!("output-job-slot-g85"),
                });
                ui.end_row();
                ui.label(tr!("output-job-drill-files"));
                ui.label(if settings.merge_drill {
                    tr!("output-job-drill-merged")
                } else {
                    tr!("output-job-drill-split")
                });
                ui.end_row();
            });
        ui.add_space(8.0);
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
        ui.weak(tr!("output-job-assembly-off"));
    }
    let Some(preview) = preview_of(cx, draft) else {
        return;
    };
    let table = match section {
        Section::Bom => &preview.bom,
        _ => &preview.cpl,
    };
    if table.rows.is_empty() {
        ui.weak(tr!("output-job-assembly-no-rows"));
    }
    let empty = if table.rows.is_empty() {
        Vec::new()
    } else {
        empty_columns(table)
    };
    ScrollArea::horizontal()
        .id_salt(("output-job-table", section))
        .show(ui, |ui| {
            Grid::new(("output-job-rows", section))
                .striped(true)
                .spacing([14.0, 4.0])
                .show(ui, |ui| {
                    for (i, header) in table.header.iter().enumerate() {
                        if empty.contains(&i) {
                            ui.label(
                                RichText::new(tr!(
                                    "output-job-column-empty",
                                    column = header.as_str()
                                ))
                                .strong()
                                .color(ui.visuals().warn_fg_color),
                            )
                            .on_hover_text(tr!("output-job-column-empty-hint"));
                        } else {
                            ui.strong(header);
                        }
                    }
                    ui.end_row();
                    for row in &table.rows {
                        for cell in row {
                            ui.label(cell);
                        }
                        ui.end_row();
                    }
                });
        });
}

fn last_run(ui: &mut Ui, cx: &mut PageCx<'_>, draft: &mut JobDraft) {
    let run = draft.last_run(cx.shell.engine());
    let folder = draft.destination_folder(cx.shell.engine());
    match &run {
        LastRun::NotRun => {
            ui.label(tr!("output-job-not-run"));
        }
        LastRun::Unreadable(why) => {
            let text = tr!("output-job-manifest-unreadable", error = why.as_str());
            ui.colored_label(ui.visuals().error_fg_color, text);
        }
        LastRun::Read(manifest) => manifest_view(ui, manifest),
    }
    ui.add_space(8.0);
    let exists = folder.as_ref().is_some_and(|f| f.is_dir());
    let button = ui.add_enabled(exists, egui::Button::new(tr!("output-job-show-in-folder")));
    if button.clicked()
        && let Some(folder) = folder
    {
        cx.show_folder(folder);
    }
}

fn manifest_view(ui: &mut Ui, manifest: &Manifest) {
    ui.label(tr!(
        "output-job-last-run-status",
        date = manifest.date.as_str(),
        reda = manifest.reda.as_str(),
        preset = manifest.preset.as_str()
    ));
    let changed = manifest.changed();
    if changed == 0 {
        ui.label(tr!(
            "output-job-last-run-match",
            count = manifest.files.len()
        ));
    } else {
        ui.colored_label(
            ui.visuals().warn_fg_color,
            tr!("output-job-last-run-changed", count = changed),
        );
    }
    if !manifest.warnings.is_empty() {
        ui.add_space(6.0);
        ui.strong(tr!("output-job-warnings"));
        for (_, message) in &manifest.warnings {
            ui.colored_label(ui.visuals().warn_fg_color, message);
        }
    }
    ui.add_space(6.0);
    ScrollArea::horizontal()
        .id_salt("output-job-last-run-files")
        .show(ui, |ui| {
            Grid::new("output-job-last-run")
                .striped(true)
                .spacing([14.0, 4.0])
                .show(ui, |ui| {
                    ui.strong(tr!("output-job-column-file"));
                    ui.strong(tr!("output-job-column-size"));
                    ui.strong(tr!("output-job-column-sha256"));
                    ui.strong(tr!("output-job-column-check"));
                    ui.end_row();
                    for file in &manifest.files {
                        ui.monospace(&file.path);
                        ui.label(size(file.size));
                        let short = file.sha256.get(..12).unwrap_or(&file.sha256);
                        ui.monospace(short).on_hover_text(&file.sha256);
                        match file.check {
                            FileCheck::Same => ui.weak(tr!("output-job-file-same")),
                            FileCheck::Changed => ui.colored_label(
                                ui.visuals().warn_fg_color,
                                tr!("output-job-file-changed"),
                            ),
                            FileCheck::Missing => ui.colored_label(
                                ui.visuals().warn_fg_color,
                                tr!("output-job-file-missing"),
                            ),
                        };
                        ui.end_row();
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
