//! The Grid page's preset tables (docs/spec/v0.1/editing.md#grid): per Document type an
//! imperial and a metric table, each row with Snap, Electrical Range (sheets and Symbols),
//! Visible and In cycle, then Add row, under **Restore shipped presets**. Each table is one
//! Preference (`editing.grid.<type>.<imperial|metric>`), written whole on every edit.

use egui::{Role, TextEdit, Ui, WidgetInfo};
use reda_engine::command::Length;
use reda_engine::model::geom::Nm;
use reda_engine::preferences::GridRow;
use serde_json::Value;

use super::PreferencesDialog;
use crate::shell::Shell;
use crate::tr;

/// The prefix of the tables' keys.
pub const GRID_TABLES: &[&str] = &[
    "editing.grid.schematic.",
    "editing.grid.symbol.",
    "editing.grid.pcb.",
    "editing.grid.footprint.",
];

/// Whether `key` is one of the tables, which this page draws itself.
pub fn is_table(key: &str) -> bool {
    GRID_TABLES.iter().any(|p| key.starts_with(p))
}

/// A table column a cell edits.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Column {
    Snap,
    Range,
    Visible,
}

/// Draws Restore shipped presets, then every table.
pub fn show(dialog: &mut PreferencesDialog, ui: &mut Ui, shell: &mut Shell) {
    ui.add_space(12.0);
    ui.strong(tr!("pref-grid-tables"));
    ui.label(tr!("pref-grid-tables-intro"));
    ui.add_space(4.0);
    let keys: Vec<String> = ["schematic", "symbol", "pcb", "footprint"]
        .iter()
        .flat_map(|t| ["imperial", "metric"].map(|u| format!("editing.grid.{t}.{u}")))
        .collect();
    let changed = keys.iter().any(|k| dialog.can_reset(shell, k));
    let restore = ui.add_enabled(changed, egui::Button::new(tr!("pref-grid-restore")));
    if restore.clicked() {
        dialog.texts.retain(|k, _| !is_table(k));
        let _ = dialog.reset_keys(shell, &keys);
    }
    let types = [
        ("schematic", tr!("document-kind-schematic"), true),
        ("symbol", tr!("document-kind-symbol"), true),
        ("pcb", tr!("document-kind-pcb"), false),
        ("footprint", tr!("document-kind-footprint"), false),
    ];
    for (ty, title, range) in types {
        for (unit, unit_title) in [
            ("imperial", tr!("pref-grid-imperial")),
            ("metric", tr!("pref-grid-metric")),
        ] {
            let key = format!("editing.grid.{ty}.{unit}");
            ui.add_space(6.0);
            ui.label(egui::RichText::new(format!("{title} · {unit_title}")).strong());
            table(dialog, ui, shell, &key, &title, &unit_title, range);
        }
    }
}

fn rows_of(dialog: &PreferencesDialog, shell: &Shell, key: &str) -> Vec<GridRow> {
    serde_json::from_value(dialog.value(shell, key)).unwrap_or_default()
}

fn write(dialog: &mut PreferencesDialog, shell: &mut Shell, key: &str, rows: &[GridRow]) {
    let value = serde_json::to_value(rows).unwrap_or(Value::Null);
    let _ = dialog.set(shell, key, value);
}

/// A length as a cell shows it: `50mil` or `0.25mm`.
fn cell_text(nm: Nm) -> String {
    if nm.0 % 25_400 == 0 {
        format!("{}mil", nm.0 / 25_400)
    } else {
        format!("{}mm", nm.0 as f64 / 1e6)
    }
}

/// One table: a row per preset, then Add row.
fn table(
    dialog: &mut PreferencesDialog,
    ui: &mut Ui,
    shell: &mut Shell,
    key: &str,
    title: &str,
    unit: &str,
    range: bool,
) {
    let rows = rows_of(dialog, shell, key);
    let mut columns = vec![(Column::Snap, tr!("pref-grid-snap"))];
    if range {
        columns.push((Column::Range, tr!("pref-grid-range")));
    }
    columns.push((Column::Visible, tr!("pref-grid-visible")));
    let mut edited: Option<Vec<GridRow>> = None;
    egui::Grid::new(("preferences-grid-table", key))
        .striped(true)
        .num_columns(columns.len() + 2)
        .show(ui, |ui| {
            for (_, name) in &columns {
                ui.strong(name.as_str());
            }
            ui.strong(tr!("pref-grid-in-cycle"));
            ui.end_row();
            for (i, row) in rows.iter().enumerate() {
                for (column, name) in &columns {
                    let value = match column {
                        Column::Snap => Some(row.snap.0),
                        Column::Range => row.range.map(|r| r.0),
                        Column::Visible => Some(row.visible.0),
                    };
                    let cell = format!("{key}[{i}].{}", *column as u8);
                    let mut text = dialog
                        .texts
                        .get(&cell)
                        .cloned()
                        .unwrap_or_else(|| value.map(cell_text).unwrap_or_default());
                    let label = tr!(
                        "pref-grid-cell",
                        table = format!("{title} {unit}"),
                        row = i + 1,
                        column = name.as_str()
                    );
                    let response = ui.add(TextEdit::singleline(&mut text).desired_width(70.0));
                    response
                        .widget_info(|| WidgetInfo::labeled(Role::TextInput, true, label.clone()));
                    if response.changed() {
                        dialog.texts.insert(cell.clone(), text.clone());
                    }
                    if response.lost_focus() {
                        let parsed = Nm::parse(text.trim()).ok().filter(|n| n.0 > 0);
                        if let Some(nm) = parsed {
                            dialog.texts.remove(&cell);
                            let mut new = rows.clone();
                            let r = &mut new[i];
                            match column {
                                // A new Snap keeps Visible equal to it when it was.
                                Column::Snap => {
                                    if r.visible == r.snap {
                                        r.visible = Length(nm);
                                    }
                                    r.snap = Length(nm);
                                }
                                Column::Range => r.range = Some(Length(nm)),
                                Column::Visible => r.visible = Length(nm),
                            }
                            edited = Some(new);
                        }
                    }
                }
                let mut in_cycle = row.in_cycle;
                let label = tr!(
                    "pref-grid-cell",
                    table = format!("{title} {unit}"),
                    row = i + 1,
                    column = tr!("pref-grid-in-cycle")
                );
                let response = ui.checkbox(&mut in_cycle, "");
                response.widget_info(|| {
                    WidgetInfo::selected(Role::CheckBox, true, in_cycle, label.clone())
                });
                if response.changed() {
                    let mut new = rows.clone();
                    new[i].in_cycle = in_cycle;
                    edited = Some(new);
                }
                ui.horizontal(|ui| {
                    let remove = ui.small_button(tr!("pref-remove"));
                    let name = tr!(
                        "pref-grid-remove-row",
                        table = format!("{title} {unit}"),
                        row = i + 1
                    );
                    remove.widget_info(|| WidgetInfo::labeled(Role::Button, true, name.clone()));
                    if remove.clicked() && rows.len() > 1 {
                        let mut new = rows.clone();
                        new.remove(i);
                        edited = Some(new);
                    }
                    if row.range_warning() {
                        ui.colored_label(
                            ui.visuals().warn_fg_color,
                            tr!("pref-grid-range-warning"),
                        );
                    }
                });
                ui.end_row();
            }
        });
    let add = ui.small_button(tr!("pref-grid-add-row"));
    let name = tr!("pref-grid-add-row-to", table = format!("{title} {unit}"));
    add.widget_info(|| WidgetInfo::labeled(Role::Button, true, name.clone()));
    if add.clicked()
        && let Some(last) = rows.last()
    {
        let mut new = rows.clone();
        let snap = Nm(last.snap.0.0 * 2);
        new.push(GridRow::new(
            snap,
            range.then(|| crate::grid::default_range(snap)),
        ));
        edited = Some(new);
    }
    if let Some(rows) = edited {
        dialog.texts.retain(|k, _| !k.starts_with(key));
        write(dialog, shell, key, &rows);
    }
}
