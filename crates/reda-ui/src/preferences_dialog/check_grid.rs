//! The Checks page's per-check display grid: a row per ERC and DRC check, with a
//! Details and an Overlay checkbox, each its own Preference (`checks.display.<check>.*`).

use egui::{Role, Ui, WidgetInfo};
use reda_engine::preferences::CheckDisplays;
use serde_json::json;

use super::PreferencesDialog;
use crate::shell::Shell;
use crate::tr;

/// The prefix of the grid's keys.
pub const CHECK_DISPLAY: &str = "checks.display.";

/// Draws the grid; a ticked box sets its Preference at once.
pub fn show(dialog: &mut PreferencesDialog, ui: &mut Ui, shell: &mut Shell) {
    ui.add_space(12.0);
    ui.strong(tr!("pref-checks-display"));
    ui.label(tr!("pref-checks-display-intro"));
    ui.add_space(4.0);
    let columns = [
        ("details", tr!("pref-checks-display-details")),
        ("overlay", tr!("pref-checks-display-overlay")),
    ];
    egui::ScrollArea::vertical()
        .id_salt("preferences-check-display")
        .max_height(320.0)
        .show(ui, |ui| {
            egui::Grid::new("preferences-check-display-grid")
                .striped(true)
                .num_columns(3)
                .show(ui, |ui| {
                    ui.strong(tr!("pref-checks-display-check"));
                    for (_, title) in &columns {
                        ui.strong(title.as_str());
                    }
                    ui.end_row();
                    for check in CheckDisplays::checks() {
                        let title = crate::messages::check_title(check);
                        ui.label(title.as_str());
                        for (column, column_title) in &columns {
                            let key = format!("{CHECK_DISPLAY}{check}.{column}");
                            let mut on = dialog.value(shell, &key).as_bool().unwrap_or(false);
                            let response = ui.checkbox(&mut on, "");
                            let label = tr!(
                                "pref-checks-display-cell",
                                check = title.as_str(),
                                column = column_title.as_str()
                            );
                            response.widget_info(|| {
                                WidgetInfo::selected(Role::CheckBox, true, on, label.clone())
                            });
                            if response.changed() {
                                let _ = dialog.set(shell, &key, json!(on));
                            }
                        }
                        ui.end_row();
                    }
                });
        });
}
