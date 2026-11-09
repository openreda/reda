//! Column-mapping paste: the sub-dialog that turns pasted text into Pin rows, shared by the
//! Symbol Wizard's pin grid and the Pins Panel. The text box takes the paste; the table
//! under it shows the cells split by the delimiter (detected, or chosen), a heading per
//! column naming the Pin field it fills (detected from a header row or guessed from the
//! cells), and Remove Row and Remove Column. The rows are read with
//! [`PasteTable::pins`], whose type aliases Import part shares.

use egui::{ComboBox, Grid, Id, Role, ScrollArea, Ui, WidgetInfo};
use reda_engine::library::wizard::WizardPin;
use reda_engine::library::wizard::paste::{Column, Delimiter, PasteTable};

use crate::tr;

/// The sub-dialog's state: the pasted text and the table made of it.
#[derive(Clone, Debug, PartialEq)]
pub struct PasteMapping {
    pub text: String,
    /// The delimiter chosen; None detects it.
    pub delimiter: Option<Delimiter>,
    pub table: PasteTable,
}

/// What the user did with the sub-dialog this frame.
#[derive(Clone, Debug, PartialEq)]
pub enum PasteOutcome {
    Open,
    Cancel,
    /// Apply these rows.
    Apply(Vec<WizardPin>),
}

impl Default for PasteMapping {
    fn default() -> PasteMapping {
        PasteMapping::new("")
    }
}

impl PasteMapping {
    /// The sub-dialog on `text`.
    pub fn new(text: &str) -> PasteMapping {
        PasteMapping {
            text: text.to_owned(),
            delimiter: None,
            table: PasteTable::parse(text, None),
        }
    }

    /// Takes new text (a paste into the box): the table is split and mapped afresh.
    pub fn set_text(&mut self, text: &str) {
        self.text = text.to_owned();
        self.table = PasteTable::parse(text, self.delimiter);
    }

    /// Splits the text by `delimiter` (None detects it) and maps it afresh.
    pub fn set_delimiter(&mut self, delimiter: Option<Delimiter>) {
        self.delimiter = delimiter;
        self.table = PasteTable::parse(&self.text, delimiter);
    }

    /// The rows the table gives, or why it gives none.
    pub fn pins(&self) -> Result<Vec<WizardPin>, String> {
        let pins = self.table.pins().map_err(|e| match (e.row, e.column) {
            (Some(r), Some(c)) => tr!(
                "paste-map-error-cell",
                row = (r + 1).to_string(),
                column = (c + 1).to_string(),
                error = e.message
            ),
            _ => e.message,
        })?;
        if pins.is_empty() {
            return Err(tr!("pin-table-paste-empty"));
        }
        Ok(pins)
    }

    /// Draws the sub-dialog as a modal over everything, with `apply` as its button.
    pub fn show(&mut self, ctx: &egui::Context, id: Id, apply: &str) -> PasteOutcome {
        let mut outcome = PasteOutcome::Open;
        let tall = (ctx.content_rect().height() * 0.7).clamp(240.0, 700.0);
        let modal = crate::modal::Dialog::new(id, tr!("paste-map-title"))
            .width(640.0)
            .show(ctx, |ui| {
                outcome = self.contents(ui, id, apply, tall - 200.0);
            });
        if modal.should_close() && outcome == PasteOutcome::Open {
            outcome = PasteOutcome::Cancel;
        }
        outcome
    }

    fn contents(&mut self, ui: &mut Ui, id: Id, apply: &str, height: f32) -> PasteOutcome {
        ui.label(tr!("paste-map-intro"));
        let mut text = self.text.clone();
        let label = tr!("paste-map-text");
        let edit = ui.add(
            egui::TextEdit::multiline(&mut text)
                .id(id.with("text"))
                .desired_rows(3)
                .desired_width(f32::INFINITY)
                .hint_text(tr!("paste-map-hint")),
        );
        edit.widget_info(|| WidgetInfo::labeled(Role::MultilineTextInput, true, label.clone()));
        if text != self.text {
            self.set_text(&text);
        }
        ui.horizontal(|ui| {
            let label = tr!("paste-map-delimiter");
            ui.label(&label);
            let mut chosen = self.delimiter;
            ComboBox::from_id_salt(id.with("delimiter"))
                .selected_text(delimiter_name(chosen))
                .show_ui(ui, |ui| {
                    for d in [
                        None,
                        Some(Delimiter::Tab),
                        Some(Delimiter::Comma),
                        Some(Delimiter::Semicolon),
                        Some(Delimiter::Whitespace),
                    ] {
                        ui.selectable_value(&mut chosen, d, delimiter_name(d));
                    }
                })
                .response
                .widget_info(|| WidgetInfo::labeled(Role::ComboBox, true, label.clone()));
            if chosen != self.delimiter {
                self.set_delimiter(chosen);
            }
            ui.add_space(12.0);
            let has_rows = !self.table.rows.is_empty();
            ui.add_enabled(
                has_rows,
                egui::Checkbox::new(&mut self.table.header, tr!("paste-map-header")),
            );
        });
        ScrollArea::both()
            .id_salt(id.with("table"))
            .max_height(height.max(80.0))
            .auto_shrink([false, true])
            .show(ui, |ui| self.table(ui, id));
        let pins = self.pins();
        if let Err(why) = &pins
            && !self.table.rows.is_empty()
        {
            ui.colored_label(ui.visuals().error_fg_color, why);
        }
        let mut outcome = PasteOutcome::Open;
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            let run = ui.add_enabled(pins.is_ok(), egui::Button::new(apply));
            if run.clicked()
                && let Ok(pins) = pins
            {
                outcome = PasteOutcome::Apply(pins);
            }
            if ui.button(tr!("dialog-cancel")).clicked() {
                outcome = PasteOutcome::Cancel;
            }
        });
        outcome
    }

    fn table(&mut self, ui: &mut Ui, id: Id) {
        let width = self.table.columns.len();
        if width == 0 {
            return;
        }
        let (mut remove_row, mut remove_column) = (None, None);
        Grid::new(id.with("grid"))
            .striped(true)
            .num_columns(width + 1)
            .show(ui, |ui| {
                ui.label("");
                for c in 0..width {
                    ui.vertical(|ui| {
                        let label = tr!("paste-map-column", column = (c + 1).to_string());
                        let mut column = self.table.columns[c];
                        ComboBox::from_id_salt(id.with(("column", c)))
                            .selected_text(column_name(column))
                            .width(110.0)
                            .show_ui(ui, |ui| {
                                for k in COLUMNS {
                                    ui.selectable_value(&mut column, k, column_name(k));
                                }
                            })
                            .response
                            .widget_info(|| {
                                WidgetInfo::labeled(Role::ComboBox, true, label.clone())
                            });
                        if column != self.table.columns[c] {
                            self.table.set_column(c, column);
                        }
                        let what = tr!("paste-map-remove-column", column = (c + 1).to_string());
                        let b = ui.small_button(tr!("paste-map-remove"));
                        b.widget_info(|| WidgetInfo::labeled(Role::Button, true, what.clone()));
                        if b.clicked() {
                            remove_column = Some(c);
                        }
                    });
                }
                ui.end_row();
                for (r, row) in self.table.rows.iter().enumerate() {
                    let what = tr!("paste-map-remove-row", row = (r + 1).to_string());
                    let b = ui.small_button(tr!("paste-map-remove"));
                    b.widget_info(|| WidgetInfo::labeled(Role::Button, true, what.clone()));
                    if b.clicked() {
                        remove_row = Some(r);
                    }
                    let heading = r == 0 && self.table.header;
                    for cell in row {
                        if heading {
                            ui.strong(cell);
                        } else {
                            ui.label(cell);
                        }
                    }
                    ui.end_row();
                }
            });
        if let Some(r) = remove_row {
            self.table.remove_row(r);
        }
        if let Some(c) = remove_column {
            self.table.remove_column(c);
        }
    }
}

const COLUMNS: [Column; 6] = [
    Column::Number,
    Column::Name,
    Column::Type,
    Column::Side,
    Column::Group,
    Column::Ignore,
];

fn column_name(column: Column) -> String {
    match column {
        Column::Number => tr!("pin-table-number"),
        Column::Name => tr!("pin-table-name"),
        Column::Type => tr!("pin-table-type"),
        Column::Side => tr!("pin-table-side"),
        Column::Group => tr!("pin-table-group"),
        Column::Ignore => tr!("paste-map-ignore"),
    }
}

fn delimiter_name(delimiter: Option<Delimiter>) -> String {
    match delimiter {
        None => tr!("paste-map-delimiter-auto"),
        Some(Delimiter::Tab) => tr!("paste-map-delimiter-tab"),
        Some(Delimiter::Comma) => tr!("paste-map-delimiter-comma"),
        Some(Delimiter::Semicolon) => tr!("paste-map-delimiter-semicolon"),
        Some(Delimiter::Whitespace) => tr!("paste-map-delimiter-spaces"),
    }
}
