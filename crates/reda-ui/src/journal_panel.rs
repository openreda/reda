//! The Journal panel: every Command of the session, oldest first, with where it came from
//! and its result or error, and an Export button that saves the session as a script for
//! `reda run --script`.

use egui::{RichText, Ui};
use reda_engine::Engine;
use reda_engine::command::{ErrorCode, Kind};
use reda_engine::journal::{self, JournalEntry, Source};

use crate::tr;

/// What the user asked of the Journal panel this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JournalAction {
    /// Save the session as a script: the window asks where.
    Export,
}

/// Draws the Journal panel: an Export button over a table of every Command run so far,
/// newest at the bottom (the table follows new entries while scrolled to the end).
pub fn show(ui: &mut Ui, engine: &Engine) -> Option<JournalAction> {
    let mut action = None;
    let entries = engine.journal();
    ui.horizontal(|ui| {
        ui.add_space(4.0);
        let export = ui
            .button(tr!("journal-export"))
            .on_hover_text(tr!("journal-export-hint"));
        if export.clicked() {
            action = Some(JournalAction::Export);
        }
        ui.weak(tr!("journal-count", count = entries.len()));
    });
    egui::ScrollArea::both()
        .auto_shrink(false)
        .stick_to_bottom(true)
        .show(ui, |ui| {
            if entries.is_empty() {
                ui.weak(tr!("journal-empty"));
                return;
            }
            egui::Grid::new("journal-entries")
                .num_columns(4)
                .striped(true)
                .spacing([12.0, 2.0])
                .show(ui, |ui| {
                    ui.strong(tr!("journal-column-number"));
                    ui.strong(tr!("journal-column-command"));
                    ui.strong(tr!("journal-column-source"));
                    ui.strong(tr!("journal-column-result"));
                    ui.end_row();
                    for (i, entry) in entries.iter().enumerate() {
                        row(ui, i + 1, entry);
                        ui.end_row();
                    }
                });
        });
    action
}

/// One Journal entry: its number, the Command id (params on hover), the source, and
/// `OK` (result on hover) or the error code and message.
fn row(ui: &mut Ui, number: usize, entry: &JournalEntry) {
    ui.weak(number.to_string());
    ui.monospace(&entry.id)
        .on_hover_text(compact(&entry.params));
    ui.label(source_label(entry.source));
    match &entry.outcome {
        Ok(result) => {
            ui.label(tr!("journal-ok")).on_hover_text(compact(result));
        }
        Err(err) => {
            let text = tr!(
                "journal-error",
                code = error_code(err.code),
                message = err.message.clone()
            );
            ui.label(RichText::new(text).color(ui.visuals().error_fg_color));
        }
    }
}

fn source_label(source: Source) -> String {
    match source {
        Source::Gui => tr!("journal-source-gui"),
        Source::Cli => tr!("journal-source-cli"),
        Source::Mcp => tr!("journal-source-mcp"),
    }
}

/// The code as the Journal writes it, such as `invalid_params`.
fn error_code(code: ErrorCode) -> String {
    serde_json::to_value(code)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default()
}

/// `value` as one line of JSON, cut short when long.
fn compact(value: &serde_json::Value) -> String {
    const MAX: usize = 400;
    let text = value.to_string();
    match text.char_indices().nth(MAX) {
        Some((cut, _)) => format!("{}…", &text[..cut]),
        None => text,
    }
}

/// The session as a script for `reda run --script`: the Journal as JSON lines, without
/// view Commands (layout moves, Panels), which only the REDA app can run.
pub fn script(engine: &Engine) -> String {
    let entries: Vec<JournalEntry> = engine
        .journal()
        .iter()
        .filter(|e| !is_view(engine, &e.id))
        .cloned()
        .collect();
    journal::export(&entries)
}

/// Whether `id` (a Command or one of its Presets) is a view Command.
fn is_view(engine: &Engine, id: &str) -> bool {
    let base = id.split(':').next().unwrap_or(id);
    engine.registry().get(base).map(|m| m.kind) == Some(Kind::View)
}
