//! Update from Library: previews what updating each Component that is not Up to Date
//! would change (`schematic.component.update_preview`, Full Replace), then updates the
//! ticked ones (`schematic.component.update_from_library` with the preview's hash,
//! excluding the others). Nothing updates without the preview.

use egui::Id;
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
    /// A change stops the update (a placed Unit is gone from the Symbol), or the row is
    /// Source Missing.
    pub blocked: bool,
    /// No registered Library holds its Part or an element it references.
    pub source_missing: bool,
    /// Whether Apply updates it.
    pub checked: bool,
}

/// The Update from Library dialog.
#[derive(Clone, Debug, Default)]
pub struct UpdateDialog {
    rows: Vec<UpdateRow>,
    preview_hash: String,
    error: Option<String>,
}

impl UpdateDialog {
    /// The dialog with the preview of every outdated Component, each ticked unless it is
    /// blocked.
    pub fn new(shell: &mut Shell) -> UpdateDialog {
        let mut dialog = UpdateDialog::default();
        match shell.run("schematic.component.update_preview", json!({})) {
            Ok(out) => {
                dialog.preview_hash = out["preview_hash"].as_str().unwrap_or_default().to_owned();
                for c in out["components"].as_array().into_iter().flatten() {
                    let Some(component) = c["component"].as_str().and_then(|u| u.parse().ok())
                    else {
                        continue;
                    };
                    let missing = c["status"] == "source_missing";
                    let blocked = missing || c["blocked"].as_bool().unwrap_or(false);
                    let mut changes: Vec<String> = c["changes"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .map(describe)
                        .collect();
                    changes.extend(
                        c["cells"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .map(describe_cell),
                    );
                    changes.extend(
                        (c["problems"].as_array().into_iter().flatten())
                            .filter(|p| p["kind"] == "pin_without_pad")
                            .map(|p| {
                                tr!(
                                    "library-update-pin-without-pad",
                                    unit = p["unit"].as_str().unwrap_or_default(),
                                    number = p["number"].as_str().unwrap_or_default()
                                )
                            }),
                    );
                    if missing {
                        changes.push(tr!(
                            "library-update-source-missing",
                            reason = c["reason"].as_str().unwrap_or_default()
                        ));
                    }
                    dialog.rows.push(UpdateRow {
                        component,
                        designator: c["designator"].as_str().unwrap_or_default().to_owned(),
                        changes,
                        blocked,
                        source_missing: missing,
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

    /// Updates the ticked Components, as previewed.
    pub fn apply(&mut self, shell: &mut Shell) -> Result<(), CommandError> {
        let excluded: Vec<Uuid> = (self.rows.iter())
            .filter(|r| !r.checked)
            .map(|r| r.component)
            .collect();
        shell.run_reporting(
            "schematic.component.update_from_library",
            json!({ "preview_hash": self.preview_hash, "excluded_components": excluded }),
        )?;
        Ok(())
    }

    /// Draws the dialog. Returns whether it is still open.
    pub fn show(&mut self, ctx: &egui::Context, shell: &mut Shell) -> bool {
        let mut result = (false, false);
        let modal = crate::modal::Dialog::new(
            Id::NULL.with("schematic-update-from-library"),
            tr!("library-update-title"),
        )
        .width(520.0)
        .show(ctx, |ui| {
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
                            if row.blocked && !row.source_missing {
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
    match change["kind"].as_str().unwrap_or_default() {
        "name" => tr!("library-update-name", from = s("from"), to = s("to")),
        "pin_map" => tr!("library-update-pin-map"),
        "symbol" => tr!("library-update-symbol", name = s("name")),
        "unit_removed" => tr!("library-update-unit-removed", unit = s("unit")),
        "unit_added" => tr!("library-update-unit-added", unit = s("unit")),
        "pin" => {
            let nets = if change.get("leaves").is_some() || change.get("joins").is_some() {
                tr!(
                    "library-update-pin-nets",
                    leaves = s("leaves"),
                    joins = s("joins")
                )
            } else {
                String::new()
            };
            tr!(
                "library-update-pin",
                unit = s("unit"),
                number = s("number"),
                nets = nets
            )
        }
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

/// One field cell of the preview, in words.
fn describe_cell(cell: &Value) -> String {
    let s = |key: &str| cell[key].as_str().unwrap_or_default().to_owned();
    let kept = if cell["action"] == "update" {
        String::new()
    } else {
        tr!("library-update-kept")
    };
    match s("cell").as_str() {
        "component_type" => tr!(
            "library-update-component-type",
            from = s("old"),
            to = s("new"),
            kept = kept
        ),
        name => tr!(
            "library-update-field",
            name = name.to_owned(),
            from = s("old"),
            to = s("new"),
            kept = kept
        ),
    }
}
