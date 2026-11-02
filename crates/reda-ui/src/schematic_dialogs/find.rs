//! Find: searches every sheet by Designator, value or Net name (`schematic.find`), and
//! shows a match by opening its sheet, selecting it and zooming to it.

use egui::{Id, Key};
use reda_engine::model::Uuid;
use serde_json::{Value, json};

use crate::shell::Shell;
use crate::tr;

/// One match.
#[derive(Clone, Debug, PartialEq)]
pub struct FindRow {
    /// What the list shows: "R2 (Designator) on Sheet 2".
    pub label: String,
    /// The sheet it is on.
    pub document: Uuid,
    /// What going to it selects: the Unit, or the objects on the Net on that sheet.
    pub objects: Vec<Uuid>,
}

/// The Find dialog.
#[derive(Clone, Debug, Default)]
pub struct FindDialog {
    /// The text to look for; case does not matter.
    pub text: String,
    /// Match the whole Designator, value or Net name rather than any part of it.
    pub exact: bool,
    results: Vec<FindRow>,
    /// The row shown last.
    current: Option<usize>,
    searched: bool,
    error: Option<String>,
    /// The text field has had the keyboard once, on opening.
    focused: bool,
}

impl FindDialog {
    /// The matches of the last search.
    pub fn results(&self) -> &[FindRow] {
        &self.results
    }

    /// Whether a search has run (so an empty list means nothing was found).
    pub fn searched(&self) -> bool {
        self.searched
    }

    /// Runs the search and shows its first match.
    pub fn search(&mut self, shell: &mut Shell) {
        self.results.clear();
        self.current = None;
        self.error = None;
        self.searched = false;
        if self.text.trim().is_empty() {
            return;
        }
        let out = match shell.run(
            "schematic.find",
            json!({"text": self.text, "exact": self.exact}),
        ) {
            Ok(out) => out,
            Err(e) => {
                self.error = Some(e.message);
                return;
            }
        };
        self.searched = true;
        let project = shell.engine().project();
        let title = |document: Uuid| {
            project
                .and_then(|p| p.document(document))
                .map(|d| d.title.clone())
                .unwrap_or_default()
        };
        for m in out["matches"].as_array().into_iter().flatten() {
            let Some(document) = uuid(&m["document"]) else {
                continue;
            };
            let field = match m["field"].as_str() {
                Some("designator") => tr!("find-field-designator"),
                Some("value") => tr!("find-field-value"),
                _ => tr!("find-field-net"),
            };
            let objects: Vec<Uuid> = match uuid(&m["unit"]) {
                Some(unit) => vec![unit],
                None => m["objects"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(uuid)
                    .collect(),
            };
            let label = tr!(
                "find-row",
                text = m["text"].as_str().unwrap_or_default().to_owned(),
                field = field,
                sheet = title(document)
            );
            self.results.push(FindRow {
                label,
                document,
                objects,
            });
        }
        if !self.results.is_empty() {
            self.go(shell, 0);
        }
    }

    /// Shows match `index`: opens its sheet, selects it and zooms to it.
    pub fn go(&mut self, shell: &mut Shell, index: usize) {
        let Some(row) = self.results.get(index) else {
            return;
        };
        shell.reveal(row.document, &row.objects);
        self.current = Some(index);
    }

    /// Draws the dialog (a window that leaves the canvas usable). Returns whether it is
    /// still open.
    pub fn show(&mut self, ctx: &egui::Context, shell: &mut Shell) -> bool {
        let mut open = true;
        let mut search = false;
        let mut go = None;
        egui::Window::new(tr!("find-title"))
            .id(Id::NULL.with("schematic-find"))
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_width(360.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let field = ui.add(
                        egui::TextEdit::singleline(&mut self.text)
                            .hint_text(tr!("find-hint"))
                            .desired_width(220.0),
                    );
                    field.widget_info(|| {
                        egui::WidgetInfo::labeled(egui::Role::TextInput, true, tr!("find-text"))
                    });
                    if !self.focused {
                        field.request_focus();
                        self.focused = true;
                    }
                    if field.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                        search = true;
                    }
                    if ui.button(tr!("find-search")).clicked() {
                        search = true;
                    }
                });
                ui.checkbox(&mut self.exact, tr!("find-exact"));
                if let Some(error) = &self.error {
                    ui.colored_label(ui.visuals().error_fg_color, error);
                } else if self.searched && self.results.is_empty() {
                    ui.weak(tr!("find-nothing"));
                }
                egui::ScrollArea::vertical()
                    .max_height(260.0)
                    .show(ui, |ui| {
                        for (i, row) in self.results.iter().enumerate() {
                            if ui
                                .selectable_label(self.current == Some(i), &row.label)
                                .clicked()
                            {
                                go = Some(i);
                            }
                        }
                    });
            });
        if search {
            self.search(shell);
        } else if let Some(i) = go {
            self.go(shell, i);
        }
        open && !ctx.input(|i| i.key_pressed(Key::Escape))
    }
}

fn uuid(v: &Value) -> Option<Uuid> {
    v.as_str()?.parse().ok()
}
