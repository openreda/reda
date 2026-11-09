//! The PCB's Update from Library: the Mechanical Parts placed straight onto the board whose
//! Footprint copy differs from the Library's (`pcb.footprint.update_preview`), each with
//! its Pad changes and, for a Locally Edited Footprint, the local edits the update removes
//! with the overlay compare. Apply runs `pcb.footprint.update_from_library` with the
//! preview's hash, leaving out the unticked rows. Instances with a Component update
//! through the schematic's dialog and Sync.

use egui::{Grid, Id, RichText, Role, WidgetInfo};
use reda_engine::command::CommandError;
use reda_engine::model::Uuid;
use reda_engine::model::local_edit::LocalEdit;
use reda_engine::schematic::LibraryStatus;
use serde_json::json;

use crate::schematic_dialogs::update::{describe_change, edited_copy, local_edits_compare};
use crate::shell::Shell;
use crate::tr;

/// One Footprint instance of the preview.
#[derive(Clone, Debug, PartialEq)]
pub struct BoardRow {
    pub instance: Uuid,
    pub designator: String,
    /// The Footprint's name.
    pub name: String,
    pub status: LibraryStatus,
    pub reason: Option<String>,
    /// The Pad changes, in words.
    pub changes: Vec<String>,
    pub lost_local_edits: Vec<LocalEdit>,
    pub included: bool,
}

/// The PCB's Update from Library dialog.
#[derive(Debug, Default)]
pub struct BoardUpdateDialog {
    rows: Vec<BoardRow>,
    preview_hash: String,
    /// The row whose details show.
    shown: Option<Uuid>,
    error: Option<String>,
}

impl BoardUpdateDialog {
    /// The preview of every outdated Mechanical Part, each ticked unless Source Missing.
    pub fn new(shell: &mut Shell) -> BoardUpdateDialog {
        let mut dialog = BoardUpdateDialog::default();
        match shell.run("pcb.footprint.update_preview", json!({})) {
            Ok(out) => {
                dialog.preview_hash = out["preview_hash"].as_str().unwrap_or_default().to_owned();
                for f in out["footprints"].as_array().into_iter().flatten() {
                    let Some(instance) = f["instance"].as_str().and_then(|u| u.parse().ok())
                    else {
                        continue;
                    };
                    let status = if f["status"] == "source_missing" {
                        LibraryStatus::SourceMissing
                    } else {
                        LibraryStatus::LibraryDiffers
                    };
                    dialog.rows.push(BoardRow {
                        instance,
                        designator: f["designator"].as_str().unwrap_or_default().to_owned(),
                        name: f["name"].as_str().unwrap_or_default().to_owned(),
                        status,
                        reason: f["reason"].as_str().map(str::to_owned),
                        changes: (f["changes"].as_array().into_iter().flatten())
                            .map(|c| describe_change(c).1)
                            .collect(),
                        lost_local_edits: serde_json::from_value(f["lost_local_edits"].clone())
                            .unwrap_or_default(),
                        included: status != LibraryStatus::SourceMissing,
                    });
                }
            }
            Err(e) => dialog.error = Some(e.message),
        }
        dialog.shown = dialog.rows.first().map(|r| r.instance);
        dialog
    }

    /// The outdated Mechanical Parts, by Designator.
    pub fn rows(&self) -> &[BoardRow] {
        &self.rows
    }

    /// Ticks or unticks a row; a Source Missing one can't be ticked.
    pub fn set_included(&mut self, instance: Uuid, on: bool) {
        for row in &mut self.rows {
            if row.instance == instance && (row.status != LibraryStatus::SourceMissing || !on) {
                row.included = on;
            }
        }
    }

    /// Shows the details of `instance`.
    pub fn show_row(&mut self, instance: Uuid) {
        self.shown = Some(instance);
    }

    /// Updates the ticked rows, as previewed.
    pub fn apply(&mut self, shell: &mut Shell) -> Result<(), CommandError> {
        let excluded: Vec<Uuid> = (self.rows.iter())
            .filter(|r| !r.included)
            .map(|r| r.instance)
            .collect();
        shell.run_reporting(
            "pcb.footprint.update_from_library",
            json!({"preview_hash": self.preview_hash, "excluded": excluded}),
        )?;
        Ok(())
    }

    /// Draws the dialog. Returns whether it is still open.
    pub fn show(&mut self, ctx: &egui::Context, shell: &mut Shell) -> bool {
        let mut result = (false, false);
        let mut clicked = None;
        let mut ticked = None;
        let height = (ctx.content_rect().height() - 220.0).max(160.0);
        let modal = crate::modal::Dialog::new(
            Id::NULL.with("pcb-update-from-library"),
            tr!("library-update-board-title"),
        )
        .width(620.0)
        .show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("pcb-update-rows")
                .max_height(height)
                .show(ui, |ui| {
                    if self.rows.is_empty() && self.error.is_none() {
                        ui.label(tr!("library-update-board-none"));
                    }
                    Grid::new("pcb-update-grid")
                        .num_columns(4)
                        .striped(true)
                        .show(ui, |ui| {
                            for row in &self.rows {
                                let mut on = row.included;
                                let enabled = row.status != LibraryStatus::SourceMissing;
                                let name = tr!(
                                    "library-update-include",
                                    designator = row.designator.clone()
                                );
                                let response =
                                    ui.add_enabled(enabled, egui::Checkbox::without_text(&mut on));
                                response.widget_info(|| {
                                    WidgetInfo::selected(Role::CheckBox, enabled, on, &name)
                                });
                                if response.clicked() {
                                    ticked = Some((row.instance, on));
                                }
                                let shown = self.shown == Some(row.instance);
                                let label = ui.selectable_label(shown, &row.designator);
                                let designator = row.designator.clone();
                                label.widget_info(|| {
                                    WidgetInfo::selected(Role::ListItem, true, shown, &designator)
                                });
                                if label.clicked() {
                                    clicked = Some(row.instance);
                                }
                                ui.label(&row.name);
                                let status = crate::schematic_dialogs::update::status_text(
                                    row.status,
                                    &[reda_engine::schematic::Aspect::Footprints],
                                );
                                ui.label(status);
                                ui.end_row();
                            }
                        });
                    if let Some(row) = self.shown.and_then(|s| {
                        self.rows.iter().find(|r| r.instance == s)
                    }) {
                        ui.add_space(6.0);
                        ui.heading(tr!(
                            "library-update-preview-of",
                            designator = row.designator.clone()
                        ));
                        ui.label(RichText::new(tr!("library-update-section-footprints")).strong());
                        for change in &row.changes {
                            ui.label(change);
                        }
                        if let Some(reason) = &row.reason {
                            ui.colored_label(
                                ui.visuals().warn_fg_color,
                                tr!("library-update-source-missing", reason = reason.clone()),
                            );
                        }
                        if !row.lost_local_edits.is_empty() {
                            ui.label(
                                RichText::new(tr!("library-update-section-local-edits")).strong(),
                            );
                            let compare = edited_copy(shell, row.instance);
                            local_edits_compare(
                                ui,
                                row.instance,
                                &row.lost_local_edits,
                                compare.as_ref(),
                            );
                        }
                    }
                });
            if let Some(error) = &self.error {
                ui.colored_label(ui.visuals().error_fg_color, error);
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let any = self.rows.iter().any(|r| r.included);
                result.0 = ui
                    .add_enabled(any, egui::Button::new(tr!("library-update-apply")))
                    .on_disabled_hover_text(tr!("library-update-none-ticked"))
                    .clicked();
                result.1 = ui.button(tr!("dialog-cancel")).clicked();
            });
        });
        if let Some(instance) = clicked {
            self.show_row(instance);
        }
        if let Some((instance, on)) = ticked {
            self.set_included(instance, on);
        }
        let (run, cancel) = result;
        if run {
            match self.apply(shell) {
                Ok(()) => return false,
                Err(e) => self.error = Some(e.message),
            }
        }
        !(cancel || modal.should_close())
    }
}
