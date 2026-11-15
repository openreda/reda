//! The Checks page's per-check display grid: a row per ERC and DRC check, with a
//! Details and an Overlay checkbox, each its own Preference (`checks.display.<check>.*`).

use egui::{Role, Ui, WidgetInfo};
use reda_engine::preferences::CheckDisplays;
use serde_json::json;

use super::PreferencesDialog;
use crate::settings::layout;
use crate::shell::Shell;
use crate::tr;
use crate::widgets::{self, Column};

/// The prefix of the grid's keys.
pub const CHECK_DISPLAY: &str = "checks.display.";

/// The most the grid takes of the page's height before it scrolls.
const LIST_HEIGHT: f32 = 320.0;

/// Draws the grid; a ticked box sets its Preference at once.
pub fn show(dialog: &mut PreferencesDialog, ui: &mut Ui, shell: &mut Shell) {
    layout::group(ui, &tr!("pref-checks-display"));
    layout::text(ui, tr!("pref-checks-display-intro"));
    let columns = [
        ("details", tr!("pref-checks-display-details")),
        ("overlay", tr!("pref-checks-display-overlay")),
    ];
    egui::ScrollArea::vertical()
        .id_salt("preferences-check-display")
        .max_height(LIST_HEIGHT)
        .show(ui, |ui| {
            widgets::table("preferences-check-display-grid")
                .columns([Column::auto(), Column::auto(), Column::auto()])
                .show(ui, |table| {
                    table.header(|h| {
                        h.text(tr!("pref-checks-display-check"));
                        for (_, title) in &columns {
                            h.text(title.as_str());
                        }
                    });
                    for check in CheckDisplays::checks() {
                        let title = crate::messages::check_title(check);
                        table.row(|row| {
                            row.text(title.as_str());
                            for (column, column_title) in &columns {
                                let key = format!("{CHECK_DISPLAY}{check}.{column}");
                                let mut on = dialog.value(shell, &key).as_bool().unwrap_or(false);
                                let response = row.ui(|ui| ui.add(widgets::checkbox(&mut on, "")));
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
                        });
                    }
                });
        });
}
