//! The rule actions along the top of the rule pages: Apply Fab Preset rules… (one button
//! per Project preset copy) and Reset page to Fab Preset, which preview row by row before
//! drafting; then Save rules as Fab Preset…, Export rules… and Import rules…, which act
//! on the rules as applied and so wait until nothing is pending.

use egui::{Button, Checkbox, Role, TextEdit, Ui, WidgetInfo};
use reda_engine::model::Uuid;

use super::super::Rule;
use super::super::model::BoardSetupDraft;
use crate::settings::{FileRequest, PageCx};
use crate::tr;

/// The actions, then any preview, form or report they opened.
pub fn rule_actions(ui: &mut Ui, cx: &mut PageCx<'_>, board: &mut BoardSetupDraft, page: Option<Rule>) {
    let presets: Vec<(Uuid, String)> = board.presets().map(|(u, n)| (u, n.to_owned())).collect();
    ui.horizontal_wrapped(|ui| {
        ui.label(tr!("board-setup-fab-preset"));
        if presets.is_empty() {
            ui.weak(tr!("settings-preset-none"));
        }
        for (uuid, name) in presets {
            let label = tr!("board-setup-apply-preset", preset = name.as_str());
            if ui.button(label).clicked() {
                board.preview_fab_preset(uuid);
            }
        }
        if let Some(page) = page {
            let reset = ui.add_enabled(
                board.page_preset().is_some(),
                Button::new(tr!("board-setup-reset-page")),
            );
            if reset.clicked() {
                board.reset_page(page);
            }
        }
    });
    let pending = !board.steps().is_empty();
    ui.horizontal_wrapped(|ui| {
        let action = |ui: &mut Ui, label: String| {
            let button = ui.add_enabled(!pending, Button::new(label));
            button
                .on_disabled_hover_text(tr!("board-setup-apply-first"))
                .clicked()
        };
        if action(ui, tr!("board-setup-save-preset")) {
            board.save_form = Some(String::new());
        }
        if action(ui, tr!("board-setup-export")) {
            cx.pick_file(FileRequest::ExportRules);
        }
        if action(ui, tr!("board-setup-import")) {
            cx.pick_file(FileRequest::ImportRules);
        }
    });
    if let Some(notice) = board.notice() {
        ui.label(notice.to_owned());
    }
    save_form(ui, cx, board);
    import_preview(ui, cx, board);
    preset_preview(ui, board);
}

/// The preview of a preset's rules: one row per value that would change, each with a
/// box to take it or leave it.
fn preset_preview(ui: &mut Ui, board: &mut BoardSetupDraft) {
    let Some(preview) = board.fab_preview().cloned() else {
        return;
    };
    let mut accept = false;
    let mut cancel = false;
    let mut toggled = None;
    egui::Frame::group(ui.style()).show(ui, |ui| {
        if preview.changes.is_empty() {
            ui.label(tr!(
                "board-setup-preset-same",
                preset = preview.name.as_str()
            ));
        } else {
            ui.label(tr!(
                "board-setup-preset-preview",
                preset = preview.name.as_str()
            ));
            egui::Grid::new("board-setup-preset-diff")
                .num_columns(4)
                .striped(true)
                .spacing([12.0, 4.0])
                .show(ui, |ui| {
                    ui.label("");
                    ui.strong(tr!("board-setup-rule"));
                    ui.strong(tr!("board-setup-now"));
                    ui.strong(tr!("board-setup-preset"));
                    ui.end_row();
                    for (i, c) in preview.changes.iter().enumerate() {
                        let mut ticked = c.ticked;
                        let label = tr!("board-setup-take-row", rule = c.rule.name());
                        let response = ui.add(Checkbox::without_text(&mut ticked));
                        response.widget_info(|| {
                            WidgetInfo::selected(Role::CheckBox, true, ticked, &label)
                        });
                        if response.changed() {
                            toggled = Some((i, ticked));
                        }
                        ui.label(c.rule.name());
                        ui.monospace(&c.board);
                        ui.monospace(&c.preset);
                        ui.end_row();
                    }
                });
        }
        ui.horizontal(|ui| {
            let any = preview.changes.iter().any(|c| c.ticked);
            accept = ui
                .add_enabled(any, Button::new(tr!("board-setup-use-preset")))
                .clicked();
            cancel = ui.button(tr!("dialog-cancel")).clicked();
        });
    });
    if let Some((row, ticked)) = toggled {
        board.set_preview_ticked(row, ticked);
    }
    if accept {
        board.accept_fab_preview();
    } else if cancel {
        board.cancel_fab_preview();
    }
}

/// "Save rules as Fab Preset…": the new preset's name, then Save.
fn save_form(ui: &mut Ui, cx: &mut PageCx<'_>, board: &mut BoardSetupDraft) {
    let Some(mut name) = board.save_form.clone() else {
        return;
    };
    let (mut save, mut cancel) = (false, false);
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.horizontal(|ui| {
            let label = tr!("board-setup-preset-name");
            ui.label(&label);
            let edit = ui.add(TextEdit::singleline(&mut name).desired_width(200.0));
            edit.widget_info(|| WidgetInfo::labeled(Role::TextInput, true, &label));
            save = ui
                .add_enabled(!name.trim().is_empty(), Button::new(tr!("board-setup-save")))
                .clicked();
            cancel = ui.button(tr!("dialog-cancel")).clicked();
        });
        ui.weak(tr!("board-setup-save-preset-note"));
    });
    if cancel {
        board.save_form = None;
    } else if save {
        board.save_form = None;
        let _ = board.save_as_preset(cx.shell, &name);
    } else {
        board.save_form = Some(name);
    }
}

/// What importing the picked file would change, with Import and Cancel.
fn import_preview(ui: &mut Ui, cx: &mut PageCx<'_>, board: &mut BoardSetupDraft) {
    let Some(preview) = board.import_preview().cloned() else {
        return;
    };
    let (mut import, mut cancel) = (false, false);
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.label(tr!(
            "board-setup-import-preview",
            path = preview.path.display().to_string()
        ));
        if preview.rows.is_empty() {
            ui.weak(tr!("board-setup-import-nothing"));
        }
        for row in &preview.rows {
            ui.label(row);
        }
        ui.horizontal(|ui| {
            import = ui.button(tr!("board-setup-import-confirm")).clicked();
            cancel = ui.button(tr!("dialog-cancel")).clicked();
        });
    });
    if import {
        board.confirm_import(cx.shell);
    } else if cancel {
        board.cancel_import();
    }
}
