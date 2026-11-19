//! The rule actions that act on the board at once rather than on Apply: Save rules as
//! Fab Preset…, Export rules… and Import rules… (after a preview). Each works on the
//! rules as applied, so the dialog offers them only while nothing is pending.

use std::path::{Path, PathBuf};

use reda_engine::command::CommandError;
use reda_engine::model::{LayerGroup, PortableLayers, PortableSpan, RuleKind, Uuid};
use reda_engine::pcb::rules::transfer::{ImportPreview as Changes, PositionRecord};
use serde_json::json;
use std::collections::BTreeSet;

use super::super::draw::{check_name, severity_name};
use super::{BoardSetupDraft, kind_name};
use crate::settings::FileRequest;
use crate::shell::Shell;
use crate::tr;

/// What importing a rules file would change, shown before it does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportPreview {
    pub path: PathBuf,
    /// One line per record overwritten, class created and severity changed.
    pub rows: Vec<String>,
    pub positions: Vec<PositionRecord>,
}

fn positions(numbers: &[u8]) -> String {
    numbers
        .iter()
        .map(|n| format!("L{n}"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn span_names(spans: &[PortableSpan]) -> String {
    spans
        .iter()
        .map(|s| {
            let span = format!("L{}–L{}", s.from, s.to);
            if s.micro {
                tr!("board-setup-import-laser-span", span = span)
            } else {
                span
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn record_label(
    kind: RuleKind,
    class: Option<&str>,
    pair: Option<&[String; 2]>,
    layers: Option<&PortableLayers>,
) -> String {
    let rule = capitalised(&kind_name(kind));
    let mut text = if let Some([first, second]) = pair {
        tr!(
            "board-setup-import-pair-record",
            rule = rule,
            first = first.as_str(),
            second = second.as_str()
        )
    } else if let Some(class) = class {
        tr!(
            "board-setup-import-class-record",
            rule = rule,
            class = class
        )
    } else if layers.is_some() {
        rule
    } else {
        tr!("board-setup-import-board-record", rule = rule)
    };
    if let Some(layers) = layers {
        let layer = match layers {
            PortableLayers::Group(LayerGroup::Inner) => tr!("board-layer-inner"),
            PortableLayers::Group(LayerGroup::Outer) => tr!("board-layer-outer"),
            PortableLayers::Group(LayerGroup::Plane) => tr!("board-layer-plane"),
            PortableLayers::Positions(set) => positions(set),
        };
        text = tr!(
            "board-setup-import-layer-record",
            rule = text,
            layer = layer
        );
    }
    text
}

impl ImportPreview {
    pub fn label(record: &PositionRecord) -> String {
        let text = record_label(
            record.kind,
            record.net_class.as_deref(),
            record.net_class_pair.as_ref(),
            record.layers.as_ref(),
        );
        match &record.allowed {
            Some(spans) if !spans.is_empty() => tr!(
                "board-setup-import-allowed",
                rule = text,
                spans = span_names(spans)
            ),
            Some(_) => tr!("board-setup-import-unrestricted", rule = text),
            None => text,
        }
    }

    pub fn warnings(record: &PositionRecord) -> Vec<String> {
        let mut warnings = Vec::new();
        if record.different_copper_count {
            warnings.push(tr!("board-setup-import-count-mismatch"));
        }
        if !record.missing_layers.is_empty() {
            warnings.push(tr!(
                "board-setup-import-missing-layers",
                layers = positions(&record.missing_layers)
            ));
        }
        if !record.missing_types.is_empty() {
            warnings.push(tr!(
                "board-setup-import-missing-types",
                spans = span_names(&record.missing_types)
            ));
        }
        if !record.available {
            warnings.push(tr!("board-setup-import-skipped"));
        }
        warnings
    }

    fn selected(&self) -> BTreeSet<usize> {
        self.positions
            .iter()
            .filter(|r| r.selected)
            .map(|r| r.index)
            .collect()
    }
}

/// The lines of a `pcb.rules.import` preview.
fn preview_rows(result: &Changes) -> Vec<String> {
    let mut rows = Vec::new();
    for o in &result.overwrites {
        if !result.position_records.iter().any(|r| r.index == o.index) {
            rows.push(record_label(
                o.kind,
                o.net_class.as_deref(),
                o.net_class_pair.as_ref(),
                o.layers.as_ref(),
            ));
        }
    }
    for class in &result.new_classes {
        rows.push(tr!("board-setup-import-new-class", class = class.as_str()));
    }
    for s in &result.severities {
        rows.push(tr!(
            "board-setup-import-severity",
            check = check_name(s.check),
            before = severity_name(s.before),
            after = severity_name(s.after)
        ));
    }
    rows
}

fn capitalised(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

impl BoardSetupDraft {
    /// What the last action reported, such as where it exported the rules.
    pub fn notice(&self) -> Option<&str> {
        self.notice.as_deref()
    }

    /// The import waiting for its preview to be confirmed.
    pub fn import_preview(&self) -> Option<&ImportPreview> {
        self.import.as_ref()
    }

    pub fn cancel_import(&mut self) {
        self.import = None;
    }

    /// A file picked for `request`: exports the rules there, or previews importing them.
    pub fn picked_file(&mut self, shell: &mut Shell, request: FileRequest, path: &Path) {
        match request {
            FileRequest::ExportRules => {
                self.notice = Some(
                    match shell.run("pcb.rules.export", json!({ "path": path })) {
                        Ok(out) => tr!(
                            "board-setup-exported",
                            count = out.get("result").unwrap_or(&out)["rules"]
                                .as_u64()
                                .unwrap_or(0),
                            path = path.display().to_string()
                        ),
                        Err(e) => tr!("error-command", error = e.message),
                    },
                );
            }
            FileRequest::ImportRules => {
                self.import = None;
                self.preview_import(shell, path, None);
            }
            FileRequest::ExportChangeList => {}
        }
    }

    fn preview_import(
        &mut self,
        shell: &mut Shell,
        path: &Path,
        selected: Option<BTreeSet<usize>>,
    ) {
        let mut params = json!({ "path": path, "preview": true });
        if let Some(selected) = selected {
            params["position_records"] = json!(selected);
        }
        let result = shell.run("pcb.rules.import", params).and_then(|out| {
            serde_json::from_value::<Changes>(out.get("result").unwrap_or(&out).clone())
                .map_err(|err| CommandError::internal(err.to_string()))
        });
        match result {
            Ok(changes) => {
                self.notice = None;
                self.import = Some(ImportPreview {
                    path: path.to_owned(),
                    rows: preview_rows(&changes),
                    positions: changes.position_records,
                });
            }
            Err(e) => self.notice = Some(tr!("error-command", error = e.message)),
        }
    }

    /// Refreshes the complete preview, including classes and conflicts introduced by this choice.
    pub fn select_import_position(&mut self, shell: &mut Shell, index: usize, ticked: bool) {
        let Some(preview) = &self.import else { return };
        let mut selected = preview.selected();
        if ticked {
            selected.insert(index);
        } else {
            selected.remove(&index);
        }
        self.preview_import(shell, &preview.path.clone(), Some(selected));
    }

    /// Imports the previewed file into the board, as one undo step.
    pub fn confirm_import(&mut self, shell: &mut Shell) {
        let Some(import) = self.import.take() else {
            return;
        };
        self.notice = Some(
            match shell.run(
                "pcb.rules.import",
                json!({ "path": import.path, "position_records": import.selected() }),
            ) {
                Ok(_) => tr!(
                    "board-setup-imported",
                    path = import.path.display().to_string()
                ),
                Err(e) => tr!("error-command", error = e.message),
            },
        );
    }

    /// Adds a Fab Preset copy to the Project holding the board's rules, called `name`.
    pub fn save_as_preset(&mut self, shell: &mut Shell, name: &str) -> Result<Uuid, CommandError> {
        let out = shell.run("pcb.rules.save_as_preset", json!({ "name": name }));
        self.notice = Some(match &out {
            Ok(_) => tr!("board-setup-saved-preset", preset = name.trim()),
            Err(e) => tr!("error-command", error = e.message.clone()),
        });
        let out = out?;
        let uuid = out["result"]["uuid"].as_str().unwrap_or_default();
        uuid.parse()
            .map_err(|_| CommandError::internal("no preset UUID in the result"))
    }
}
