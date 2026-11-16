//! The Journal panel: every Command of the session, oldest first, with where it came from
//! and its result or error, and an Export button that saves the session as a script for
//! `reda run --script`.

use egui::Ui;
use reda_engine::Engine;
use reda_engine::command::{ErrorCode, Kind};
use reda_engine::journal::{self, JournalEntry, Source};

use crate::theme::{Tokens, TokensExt as _};
use crate::tr;
use crate::widgets::{self, Column, Tooltip as _};

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
        let export = ui
            .add(widgets::button(tr!("journal-export")))
            .tooltip(tr!("journal-export-hint"));
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
            let columns = [
                Column::auto().number(),
                Column::auto(),
                Column::auto(),
                Column::auto(),
            ];
            let t = ui.tokens();
            let table = widgets::table("journal-entries").columns(columns);
            table.show(ui, |table| {
                table.header(|h| {
                    h.text(tr!("journal-column-number"));
                    h.text(tr!("journal-column-command"));
                    h.text(tr!("journal-column-source"));
                    h.text(tr!("journal-column-result"));
                });
                for (i, entry) in entries.iter().enumerate() {
                    table.row(|cells| row(cells, &t, i + 1, entry));
                }
            });
        });
    action
}

/// One Journal entry: its number, the Command id (params on hover), the source, and
/// `OK` (result on hover) or the error code and message.
fn row(cells: &mut widgets::TableRow<'_>, t: &Tokens, number: usize, entry: &JournalEntry) {
    cells.muted(number.to_string());
    cells
        .ui(|ui| {
            let id = t.ty.mono.rich(&entry.id).color(t.color.text.primary);
            ui.add(egui::Label::new(id).selectable(false).extend())
        })
        .tooltip(compact(&entry.params));
    cells.text(source_label(entry.source));
    match &entry.outcome {
        Ok(result) => {
            cells.text(tr!("journal-ok")).tooltip(compact(result));
        }
        Err(err) => {
            let text = tr!(
                "journal-error",
                code = error_code(err.code),
                message = err.message.clone()
            );
            cells.colored(text, t.color.error.text);
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
