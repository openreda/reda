//! Drawing the Update from Library dialog: page 1's choices, page 2's rows, sections and
//! field grid, and the local-edit compare the PCB dialog shares.

use egui::{Checkbox, ComboBox, Grid, Id, RichText, Role, Ui, WidgetInfo};
use reda_engine::model::Footprint;
use reda_engine::model::library_update::{
    ComponentFieldMissing, FootprintChoice, LibraryFieldMissing, LocallyEdited,
};
use reda_engine::model::local_edit::LocalEdit;
use reda_engine::schematic::{Aspect, LibraryStatus};
use serde_json::Value;

use super::describe::{Section, case, cell_name};
use super::{Page, UpdateDialog, edited_footprint};
use crate::schematic_dialogs::replace::Outcome;
use crate::shell::Shell;
use crate::tr;

/// What a click on page 2 asks for, done once the frame is drawn.
enum Click {
    Next,
    Back,
    Include(reda_engine::model::Uuid, bool),
    Aspect(reda_engine::model::Uuid, Aspect, bool),
    Row(reda_engine::model::Uuid, bool),
    Cell(reda_engine::model::Uuid, String, bool),
    UpdateSelected,
    RejectSelected,
    Choose,
    ReturnToDefault,
}

pub(super) fn aspect_name(aspect: Aspect) -> String {
    match aspect {
        Aspect::Symbol => tr!("library-update-aspect-symbol"),
        Aspect::Fields => tr!("library-update-aspect-fields"),
        Aspect::Footprints => tr!("library-update-aspect-footprints"),
    }
}

pub(crate) fn status_text(status: LibraryStatus, aspects: &[Aspect]) -> String {
    match status {
        LibraryStatus::UpToDate => tr!("library-status-up-to-date"),
        LibraryStatus::LibraryDiffers => {
            let names: Vec<String> = aspects.iter().map(|a| aspect_name(*a)).collect();
            tr!("library-status-library-differs", aspects = names.join(", "))
        }
        LibraryStatus::SourceMissing => tr!("library-status-source-missing"),
    }
}

/// A tick box labelled for AccessKit with `name`, which may differ from its text.
fn tick(ui: &mut Ui, on: &mut bool, text: &str, name: &str, enabled: bool) -> bool {
    let response = ui.add_enabled(enabled, Checkbox::new(on, text));
    let name = name.to_owned();
    let value = *on;
    response.widget_info(|| WidgetInfo::selected(Role::CheckBox, enabled, value, &name));
    response.clicked()
}

impl UpdateDialog {
    /// Draws the dialog. Returns whether it is still open.
    pub fn show(&mut self, ctx: &egui::Context, shell: &mut Shell) -> bool {
        if let Some(picker) = &mut self.picker {
            match picker.show(ctx, shell) {
                Outcome::Open => {}
                Outcome::Closed => self.picker = None,
                Outcome::Chosen(part) => {
                    self.picker = None;
                    self.choose_component(shell, part);
                }
            }
        }
        let mut clicks = Vec::new();
        let mut result = (false, false);
        let height = (ctx.content_rect().height() - 220.0).max(160.0);
        let modal = crate::modal::Dialog::new(
            Id::NULL.with("schematic-update-from-library"),
            tr!("library-update-title"),
        )
        .width(720.0)
        .min_size(egui::vec2(480.0, 240.0))
        .show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .id_salt(("library-update-page", self.page == Page::Setup))
                .max_height(height)
                .show(ui, |ui| match self.page {
                    Page::Setup => self.setup_page(ui),
                    Page::Preview => self.preview_page(ui, shell, &mut clicks),
                });
            if let Some(error) = &self.error {
                ui.colored_label(ui.visuals().error_fg_color, error);
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| match self.page {
                Page::Setup => {
                    let ready = self.sheets.iter().any(|s| s.on)
                        && self.parts.iter().any(|p| p.on)
                        && !self.aspects.is_empty();
                    if ui
                        .add_enabled(ready, egui::Button::new(tr!("library-update-next")))
                        .on_disabled_hover_text(tr!("library-update-next-why"))
                        .clicked()
                    {
                        clicks.push(Click::Next);
                    }
                    result.1 = ui.button(tr!("dialog-cancel")).clicked();
                }
                Page::Preview => {
                    if ui.button(tr!("library-update-back")).clicked() {
                        clicks.push(Click::Back);
                    }
                    result.0 = ui
                        .add_enabled(
                            self.can_apply(),
                            egui::Button::new(tr!("library-update-apply")),
                        )
                        .on_disabled_hover_text(tr!("library-update-none-ticked"))
                        .clicked();
                    result.1 = ui.button(tr!("dialog-cancel")).clicked();
                }
            });
        });
        for click in clicks {
            match click {
                Click::Next => self.next(shell),
                Click::Back => self.back(),
                Click::Include(c, on) => self.set_included(c, on),
                Click::Aspect(c, a, on) => self.set_component_aspect(shell, c, a, on),
                Click::Row(c, add) => self.select_row(c, add),
                Click::Cell(c, cell, add) => self.select_cell(c, &cell, add),
                Click::UpdateSelected => self.update_selected(),
                Click::RejectSelected => self.reject_selected(),
                Click::Choose => self.open_picker(shell),
                Click::ReturnToDefault => self.return_selected_to_default(shell),
            }
        }
        let (run, cancel) = result;
        if run {
            match self.apply(shell) {
                Ok(()) => return false,
                Err(e) => self.error = Some(e.message),
            }
        }
        let closing = self.picker.is_none() && modal.should_close();
        !(cancel || closing)
    }

    fn setup_page(&mut self, ui: &mut Ui) {
        ui.label(RichText::new(tr!("library-update-sheets")).strong());
        ui.horizontal(|ui| {
            if ui.button(tr!("library-update-all-on")).clicked() {
                self.all_sheets(true);
            }
            if ui.button(tr!("library-update-all-off")).clicked() {
                self.all_sheets(false);
            }
        });
        for sheet in &mut self.sheets {
            ui.checkbox(&mut sheet.on, &sheet.name);
        }
        ui.add_space(6.0);
        ui.label(RichText::new(tr!("library-update-parts")).strong());
        for part in &mut self.parts {
            let text = tr!(
                "library-update-part-count",
                name = part.name.clone(),
                count = part.count,
                outdated = part.outdated
            );
            tick(ui, &mut part.on, &text, &part.name, true);
        }
        let enabled = self.has_selection();
        tick(
            ui,
            &mut self.selected_only,
            &tr!("library-update-selected-only"),
            &tr!("library-update-selected-only"),
            enabled,
        );
        ui.add_space(6.0);
        ui.label(RichText::new(tr!("library-update-aspects")).strong());
        let mut full = self.aspects.len() == Aspect::ALL.len();
        if tick(
            ui,
            &mut full,
            &tr!("library-update-full-replace"),
            &tr!("library-update-full-replace"),
            true,
        ) {
            for aspect in Aspect::ALL {
                self.set_aspect(aspect, full);
            }
        }
        ui.indent("library-update-aspects", |ui| {
            for aspect in Aspect::ALL {
                let mut on = self.aspects.contains(&aspect);
                let name = aspect_name(aspect);
                if tick(ui, &mut on, &name, &name, true) {
                    self.set_aspect(aspect, on);
                }
            }
        });
        ui.add_space(6.0);
        ui.label(RichText::new(tr!("library-update-settings")).strong());
        self.settings_grid(ui);
    }

    fn settings_grid(&mut self, ui: &mut Ui) {
        let s = &mut self.settings;
        Grid::new("library-update-settings")
            .num_columns(2)
            .show(ui, |ui| {
                let label = ui.label(tr!("library-update-setting-library-field-missing"));
                combo(
                    ui,
                    label.id,
                    "library_field_missing",
                    &mut s.library_field_missing,
                    &[
                        (LibraryFieldMissing::Add, "library-update-add"),
                        (
                            LibraryFieldMissing::AddIfNotBlank,
                            "library-update-add-if-not-blank",
                        ),
                        (LibraryFieldMissing::Skip, "library-update-skip"),
                    ],
                );
                ui.end_row();
                let label = ui.label(tr!("library-update-setting-component-field-missing"));
                combo(
                    ui,
                    label.id,
                    "component_field_missing",
                    &mut s.component_field_missing,
                    &[
                        (ComponentFieldMissing::Keep, "library-update-keep"),
                        (ComponentFieldMissing::Remove, "library-update-remove"),
                    ],
                );
                ui.end_row();
                let label = ui.label(tr!("library-update-setting-locally-edited"));
                combo(
                    ui,
                    label.id,
                    "locally_edited",
                    &mut s.locally_edited,
                    &[
                        (LocallyEdited::Keep, "library-update-keep"),
                        (LocallyEdited::TakeNew, "library-update-take-new"),
                    ],
                );
                ui.end_row();
                let label = ui.label(tr!("library-update-setting-footprint-choice"));
                combo(
                    ui,
                    label.id,
                    "footprint_choice",
                    &mut s.footprint_choice,
                    &[
                        (
                            FootprintChoice::KeepIfPresent,
                            "library-update-keep-if-present",
                        ),
                        (FootprintChoice::Default, "library-update-default"),
                    ],
                );
                ui.end_row();
            });
        for (on, key) in [
            (
                &mut s.add_new_footprints,
                tr!("library-update-setting-add-new-footprints"),
            ),
            (
                &mut s.remove_missing_footprints,
                tr!("library-update-setting-remove-missing-footprints"),
            ),
            (
                &mut s.preserve_field_locations,
                tr!("library-update-setting-preserve-field-locations"),
            ),
            (
                &mut s.preserve_field_visibility,
                tr!("library-update-setting-preserve-field-visibility"),
            ),
        ] {
            ui.checkbox(on, key);
        }
        if !self.field_names.is_empty() {
            egui::CollapsingHeader::new(tr!("library-update-setting-fields-to-update"))
                .id_salt("library-update-fields-to-update")
                .show(ui, |ui| {
                    for name in self.field_names.clone() {
                        let mut on = self.settings.updates(&name);
                        let label = cell_name(&name);
                        let a11y = tr!("library-update-field-to-update", field = label.clone());
                        if tick(ui, &mut on, &label, &a11y, true) {
                            self.set_field_updated(&name, on);
                        }
                    }
                });
        }
    }

    fn preview_page(&self, ui: &mut Ui, shell: &Shell, clicks: &mut Vec<Click>) {
        if self.rows.is_empty() {
            ui.label(tr!("library-update-none"));
            return;
        }
        let add = ui.input(|i| i.modifiers.command || i.modifiers.shift);
        Grid::new("library-update-rows")
            .num_columns(6)
            .striped(true)
            .show(ui, |ui| {
                ui.label("");
                ui.strong(tr!("library-update-col-component"));
                ui.strong(tr!("library-update-col-part"));
                ui.strong(tr!("library-update-col-status"));
                ui.strong(tr!("library-update-col-aspects"));
                ui.end_row();
                for row in &self.rows {
                    let mut on = row.included;
                    let name = tr!(
                        "library-update-include",
                        designator = row.designator.clone()
                    );
                    if tick(ui, &mut on, "", &name, !row.blocked) {
                        clicks.push(Click::Include(row.component, on));
                    }
                    let selected = self.selected_rows.contains(&row.component);
                    let response = ui.selectable_label(selected, &row.designator);
                    let label = row.designator.clone();
                    response.widget_info(|| {
                        WidgetInfo::selected(Role::ListItem, true, selected, &label)
                    });
                    if response.clicked() {
                        clicks.push(Click::Row(row.component, add));
                    }
                    match self.replacement(row.component) {
                        Some(r) => {
                            ui.label(tr!(
                                "library-update-replaced-by",
                                from = row.part_name.clone(),
                                to = r.part_name.clone()
                            ));
                        }
                        None => {
                            ui.label(&row.part_name);
                        }
                    }
                    let status = status_text(row.status, &row.differs);
                    if row.status == LibraryStatus::SourceMissing {
                        ui.colored_label(ui.visuals().warn_fg_color, status);
                    } else {
                        ui.label(status);
                    }
                    ui.horizontal(|ui| {
                        for aspect in Aspect::ALL {
                            let differs = row.differs.contains(&aspect);
                            let mut on = row.aspects.contains(&aspect);
                            let name = tr!(
                                "library-update-row-aspect",
                                designator = row.designator.clone(),
                                aspect = aspect_name(aspect)
                            );
                            let enabled = differs
                                && !row.blocked
                                && self.replacement(row.component).is_none();
                            if tick(ui, &mut on, &aspect_name(aspect), &name, enabled) {
                                clicks.push(Click::Aspect(row.component, aspect, on));
                            }
                        }
                    });
                    ui.end_row();
                }
            });
        ui.horizontal(|ui| {
            if ui.button(tr!("library-update-choose-component")).clicked() {
                clicks.push(Click::Choose);
            }
            if ui.button(tr!("library-update-return-to-default")).clicked() {
                clicks.push(Click::ReturnToDefault);
            }
        });
        let Some(row) = self.shown() else {
            return;
        };
        ui.add_space(6.0);
        ui.heading(tr!(
            "library-update-preview-of",
            designator = row.designator.clone()
        ));
        let replacement = self.replacement(row.component);
        let (changes, problems) = match replacement {
            Some(r) => (&r.changes, &r.problems),
            None => (&row.changes, &row.problems),
        };
        for section in Section::ALL {
            let lines: Vec<&String> = (changes.iter())
                .filter(|(s, _)| *s == section)
                .map(|(_, t)| t)
                .collect();
            let grid = section == Section::Fields && replacement.is_none() && !row.cells.is_empty();
            if lines.is_empty() && !grid {
                continue;
            }
            ui.label(RichText::new(section.title()).strong());
            for line in lines {
                ui.label(line);
            }
            if grid {
                self.field_grid(ui, row, add, clicks);
            }
        }
        for problem in problems {
            ui.colored_label(ui.visuals().warn_fg_color, problem);
        }
        if !row.lost_local_edits.is_empty() && replacement.is_none() {
            ui.label(RichText::new(tr!("library-update-section-local-edits")).strong());
            let compare = edited_footprint(shell, row.component);
            local_edits(ui, row.component, &row.lost_local_edits, compare.as_ref());
        }
    }

    fn field_grid(
        &self,
        ui: &mut Ui,
        row: &super::ComponentRow,
        add: bool,
        clicks: &mut Vec<Click>,
    ) {
        Grid::new(("library-update-cells", row.component))
            .num_columns(6)
            .striped(true)
            .show(ui, |ui| {
                for title in [
                    "library-update-col-field",
                    "library-update-col-case",
                    "library-update-col-old",
                    "library-update-col-new",
                    "library-update-col-value",
                    "library-update-col-action",
                ] {
                    ui.strong(crate::i18n::translate(title, &[]));
                }
                ui.end_row();
                for cell in &row.cells {
                    let selected = self.cell_selected(row.component, &cell.cell);
                    let action = self.action(row.component, cell);
                    let action_text = match action.as_str() {
                        "update" => tr!("library-update-action-update"),
                        "reject" => tr!("library-update-action-reject"),
                        _ => tr!("library-update-action-none"),
                    };
                    let name = cell_name(&cell.cell);
                    let response = ui.selectable_label(selected, &name);
                    let label = tr!(
                        "library-update-cell",
                        designator = row.designator.clone(),
                        field = name.clone(),
                        action = action_text.clone()
                    );
                    response.widget_info(|| {
                        WidgetInfo::selected(Role::ListItem, true, selected, &label)
                    });
                    if response.clicked() {
                        clicks.push(Click::Cell(row.component, cell.cell.clone(), add));
                    }
                    ui.label(case(&cell.case));
                    ui.weak(&cell.old);
                    ui.label(&cell.new);
                    ui.label(&cell.component);
                    if !cell.overridable {
                        ui.weak(action_text);
                    } else if action == "update" {
                        ui.colored_label(ui.visuals().warn_fg_color, action_text);
                    } else {
                        ui.label(action_text);
                    }
                    ui.end_row();
                }
            });
        ui.horizontal(|ui| {
            if ui.button(tr!("library-update-update-selected")).clicked() {
                clicks.push(Click::UpdateSelected);
            }
            if ui.button(tr!("library-update-reject-selected")).clicked() {
                clicks.push(Click::RejectSelected);
            }
        });
    }
}

/// A settings combo over `choices` (value, label key).
fn combo<T: Copy + PartialEq>(
    ui: &mut Ui,
    label: egui::Id,
    id: &str,
    value: &mut T,
    choices: &[(T, &str)],
) {
    let text = |key: &str| crate::i18n::translate(key, &[]);
    let selected = (choices.iter())
        .find(|(v, _)| v == value)
        .map(|(_, k)| text(k))
        .unwrap_or_default();
    ComboBox::from_id_salt(("library-update-setting", id))
        .selected_text(selected)
        .show_ui(ui, |ui| {
            for (v, key) in choices {
                ui.selectable_value(value, *v, text(key));
            }
        })
        .response
        .labelled_by(label);
}

/// The local edits an update removes from a Locally Edited Footprint: each Pad or graphic
/// property, edited against original, and the edited Pads drawn over the copy they came
/// from.
pub(crate) fn local_edits(
    ui: &mut Ui,
    salt: reda_engine::model::Uuid,
    edits: &[LocalEdit],
    compare: Option<&(Footprint, Footprint)>,
) {
    ui.colored_label(
        ui.visuals().warn_fg_color,
        tr!("library-update-local-edits-lost", count = edits.len()),
    );
    Grid::new(("library-update-local-edits", salt))
        .num_columns(3)
        .show(ui, |ui| {
            ui.strong(tr!("library-update-col-edit"));
            ui.strong(tr!("library-update-col-edited"));
            ui.strong(tr!("library-update-col-original"));
            ui.end_row();
            for edit in edits {
                let what = match (&edit.number, edit.graphic) {
                    (Some(number), _) => tr!("properties-compare-pad", number = number.clone()),
                    (None, Some(_)) => tr!("properties-compare-graphic"),
                    (None, None) => String::new(),
                };
                ui.label(format!("{what} {}", edit.property));
                ui.label(value(edit.edited.as_ref()));
                ui.weak(value(edit.original.as_ref()));
                ui.end_row();
            }
        });
    if let Some((edited, original)) = compare {
        crate::panels::properties::rules::compare_drawing(ui, edited, original);
    }
}

fn value(value: Option<&Value>) -> String {
    match value {
        None => "-".to_owned(),
        Some(Value::String(s)) => s.clone(),
        Some(v) => v.to_string(),
    }
}
