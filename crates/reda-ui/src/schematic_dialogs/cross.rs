//! Convert Cross Junctions: turns 4-ways into two adjacent 3-ways
//! (`schematic.junctions.convert_cross`), on this sheet or the whole Project, for All
//! 4-ways or only those whose every Wire is among the Selected Objects, with a Miter Size
//! of one snap grid at first.

use egui::Id;
use reda_engine::command::CommandError;
use reda_engine::model::Uuid;
use reda_engine::model::geom::Nm;
use reda_engine::preferences::Units;
use serde_json::{Value, json};

use super::buttons;
use crate::shell::Shell;
use crate::tr;

/// The Convert Cross Junctions dialog.
#[derive(Clone, Debug)]
pub struct ConvertCrossDialog {
    /// The sheet active when it opened.
    pub sheet: Uuid,
    /// Every sheet of the Project rather than this one.
    pub project: bool,
    /// Only the 4-ways whose every Wire is selected (Selected Objects), not All.
    pub selected_only: bool,
    /// The Wires selected when it opened, whole or by a segment.
    pub selected: Vec<Uuid>,
    /// The Miter Size as typed: a length, or a bare number in the sheet grid's units.
    pub miter: String,
    units: Units,
    error: Option<String>,
}

impl ConvertCrossDialog {
    /// The dialog on the active sheet; `None` when no sheet is active.
    pub fn new(shell: &Shell) -> Option<ConvertCrossDialog> {
        let sheet = shell.active_sheet()?;
        let grid = shell.grid(sheet);
        let units = if crate::grid::is_imperial(grid) {
            Units::Mil
        } else {
            Units::Mm
        };
        let selection = shell.selection();
        let mut selected = selection.objects.clone();
        selected.extend(selection.segments.iter().map(|s| s.wire));
        selected.sort();
        selected.dedup();
        Some(ConvertCrossDialog {
            sheet,
            project: false,
            selected_only: false,
            selected,
            miter: crate::pcb::view::format_length(grid, units),
            units,
            error: None,
        })
    }

    /// The Miter Size typed, if it reads as a positive length.
    pub fn miter_size(&self) -> Option<Nm> {
        let text = self.miter.trim();
        let nm = match text.parse::<f64>() {
            Ok(v) => Nm(match self.units {
                Units::Mm => (v * 1e6).round() as i64,
                Units::Mil => (v * 25_400.0).round() as i64,
            }),
            Err(_) => Nm::parse(&text.replace(' ', "")).ok()?,
        };
        (nm > Nm(0)).then_some(nm)
    }

    /// Whether it can run: a Miter Size, and with Selected Objects, something selected.
    pub fn can_apply(&self) -> bool {
        self.miter_size().is_some() && (!self.selected_only || !self.selected.is_empty())
    }

    /// The `schematic.junctions.convert_cross` params the dialog asks for.
    pub fn params(&self) -> Option<Value> {
        let miter = self.miter_size()?;
        let mut params = json!({ "miter": miter.0 });
        if !self.project {
            params["document"] = json!(self.sheet);
        }
        if self.selected_only {
            params["objects"] = json!(self.selected);
        }
        Some(params)
    }

    /// Converts the 4-ways. Returns how many.
    pub fn apply(&mut self, shell: &mut Shell) -> Result<usize, CommandError> {
        let params = self.params().ok_or_else(|| {
            CommandError::invalid_params("/miter", tr!("convert-cross-bad-miter"))
        })?;
        let out = shell.run("schematic.junctions.convert_cross", params)?;
        let converted = out["result"]["converted"]
            .as_array()
            .or_else(|| out["converted"].as_array())
            .map_or(0, Vec::len);
        shell.set_note(tr!("convert-cross-done", count = converted as i64));
        Ok(converted)
    }

    /// Draws the dialog. Returns whether it is still open.
    pub fn show(&mut self, ctx: &egui::Context, shell: &mut Shell) -> bool {
        let mut result = (false, false);
        let modal = crate::modal::Dialog::new(
            Id::NULL.with("schematic-convert-cross"),
            tr!("convert-cross-title"),
        )
        .width(360.0)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.radio_value(&mut self.project, false, tr!("convert-cross-scope-sheet"));
                ui.radio_value(&mut self.project, true, tr!("convert-cross-scope-project"));
            });
            ui.horizontal(|ui| {
                ui.radio_value(&mut self.selected_only, false, tr!("convert-cross-all"));
                ui.add_enabled_ui(!self.selected.is_empty(), |ui| {
                    ui.radio_value(&mut self.selected_only, true, tr!("convert-cross-selected"))
                        .on_disabled_hover_text(tr!("convert-cross-nothing-selected"));
                });
            });
            ui.horizontal(|ui| {
                let label = ui.label(tr!("convert-cross-miter"));
                ui.add(egui::TextEdit::singleline(&mut self.miter).desired_width(100.0))
                    .labelled_by(label.id);
            });
            if let Some(error) = &self.error {
                ui.colored_label(ui.visuals().error_fg_color, error);
            }
            result = buttons(
                ui,
                &tr!("convert-cross-convert"),
                self.can_apply(),
                &tr!("convert-cross-bad-miter"),
            );
        });
        let (run, cancel) = result;
        if run {
            match self.apply(shell) {
                Ok(_) => return false,
                Err(e) => self.error = Some(e.message),
            }
        }
        !(cancel || modal.should_close())
    }
}
