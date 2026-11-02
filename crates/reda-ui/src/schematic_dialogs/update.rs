//! Update from Library: previews what updating each outdated Component to its Library
//! Part would change (`schematic.component.update_preview`), then updates the ticked ones
//! (`schematic.component.update_from_library`). Nothing updates without the preview.

use egui::{Id, Modal};
use reda_engine::command::CommandError;
use reda_engine::model::Uuid;
use serde_json::{Value, json};

use super::buttons;
use crate::shell::Shell;
use crate::tr;

/// One outdated Component.
#[derive(Clone, Debug, PartialEq)]
pub struct UpdateRow {
    pub component: Uuid,
    pub designator: String,
    /// The changes, in words.
    pub changes: Vec<String>,
    /// A change stops the update (a placed Unit is gone from the Symbol).
    pub blocked: bool,
    /// Whether Apply updates it.
    pub checked: bool,
}

/// The Update from Library dialog.
#[derive(Clone, Debug, Default)]
pub struct UpdateDialog {
    rows: Vec<UpdateRow>,
    error: Option<String>,
}

impl UpdateDialog {
    /// The dialog with the preview of every outdated Component, each ticked unless it is
    /// blocked.
    pub fn new(shell: &mut Shell) -> UpdateDialog {
        let mut dialog = UpdateDialog::default();
        match shell.run("schematic.component.update_preview", json!({})) {
            Ok(out) => {
                for c in out["components"].as_array().into_iter().flatten() {
                    let Some(component) = c["component"].as_str().and_then(|u| u.parse().ok())
                    else {
                        continue;
                    };
                    let blocked = c["blocked"].as_bool().unwrap_or(false);
                    dialog.rows.push(UpdateRow {
                        component,
                        designator: c["designator"].as_str().unwrap_or_default().to_owned(),
                        changes: c["changes"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .map(describe)
                            .collect(),
                        blocked,
                        checked: !blocked,
                    });
                }
            }
            Err(e) => dialog.error = Some(e.message),
        }
        dialog
    }

    /// The outdated Components, by Designator.
    pub fn rows(&self) -> &[UpdateRow] {
        &self.rows
    }

    /// Ticks or unticks `component`.
    pub fn set(&mut self, component: Uuid, on: bool) {
        for row in &mut self.rows {
            if row.component == component && !row.blocked {
                row.checked = on;
            }
        }
    }

    fn ticked(&self) -> Vec<Uuid> {
        self.rows
            .iter()
            .filter(|r| r.checked)
            .map(|r| r.component)
            .collect()
    }

    /// Updates the ticked Components.
    pub fn apply(&mut self, shell: &mut Shell) -> Result<(), CommandError> {
        let components = self.ticked();
        shell.run_reporting(
            "schematic.component.update_from_library",
            json!({ "components": components }),
        )?;
        Ok(())
    }

    /// Draws the dialog. Returns whether it is still open.
    pub fn show(&mut self, ctx: &egui::Context, shell: &mut Shell) -> bool {
        let mut result = (false, false);
        let modal = Modal::new(Id::NULL.with("schematic-update-from-library")).show(ctx, |ui| {
            ui.set_width(520.0);
            ui.heading(tr!("library-update-title"));
            if self.rows.is_empty() && self.error.is_none() {
                ui.label(tr!("library-update-none"));
            }
            egui::ScrollArea::vertical()
                .id_salt("library-update-preview")
                .max_height(320.0)
                .show(ui, |ui| {
                    for row in &mut self.rows {
                        ui.add_enabled(
                            !row.blocked,
                            egui::Checkbox::new(&mut row.checked, &row.designator),
                        );
                        ui.indent(row.component, |ui| {
                            for change in &row.changes {
                                ui.label(change);
                            }
                            if row.blocked {
                                ui.colored_label(
                                    ui.visuals().warn_fg_color,
                                    tr!("library-update-blocked"),
                                );
                            }
                        });
                    }
                });
            if let Some(error) = &self.error {
                ui.colored_label(ui.visuals().error_fg_color, error);
            }
            result = buttons(
                ui,
                &tr!("library-update-apply"),
                !self.ticked().is_empty(),
                &tr!("library-update-none-ticked"),
            );
        });
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

/// One `PartChange` of the preview, in words.
fn describe(change: &Value) -> String {
    let s = |key: &str| change[key].as_str().unwrap_or_default().to_owned();
    let kept = if change["edited"].as_bool().unwrap_or(false) {
        tr!("library-update-kept")
    } else {
        String::new()
    };
    match change["kind"].as_str().unwrap_or_default() {
        "name" => tr!("library-update-name", from = s("from"), to = s("to")),
        "component_type" => tr!(
            "library-update-component-type",
            from = s("from"),
            to = s("to"),
            kept = kept
        ),
        "field" => tr!(
            "library-update-field",
            name = s("name"),
            from = s("from"),
            to = s("to"),
            kept = kept
        ),
        "pin_map" => tr!("library-update-pin-map"),
        "symbol" => tr!("library-update-symbol", name = s("name")),
        "unit_removed" => tr!("library-update-unit-removed", unit = s("unit")),
        "footprint" => tr!(
            "library-update-footprint",
            name = s("name"),
            change = s("change")
        ),
        "footprint_order" => tr!("library-update-footprint-order"),
        "footprint_choice" => tr!("library-update-footprint-choice"),
        other => other.to_owned(),
    }
}
