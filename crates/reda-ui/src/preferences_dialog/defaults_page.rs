//! The Defaults page (docs/spec/v0.3/input-and-defaults.md#the-defaults-page): the kinds
//! with Object Defaults grouped by editor, with a filter; the chosen kind's fields,
//! generated from the Preferences schema one row per leaf as Select Similar's rows are
//! from an object's; and Reset, Save As… and Load… for the chosen kind or every kind
//! (`app.defaults.reset`, `app.defaults.save`, `app.defaults.load`).
//!
//! The model here is drawn by `draw/defaults.rs` and tested without a window.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::OnceLock;

use reda_engine::preferences::{self, DefaultsEditor, DefaultsKind, KINDS};
use serde_json::json;

use super::{PreferencesDialog, Setting, schema};
use crate::settings::FileRequest;
use crate::shell::Shell;
use crate::{i18n, tr};

/// The Defaults page's own state.
#[derive(Clone, Debug, Default)]
pub struct DefaultsPage {
    /// The filter over kinds and their fields.
    pub filter: String,
    /// The kind shown; the first one listed when none was chosen.
    chosen: Option<&'static str>,
    /// Save As… is for this kind, or every kind (`None`), until the file is picked.
    saving: Option<&'static str>,
    /// What the last Reset, Save or Load did, or why it failed.
    pub message: Option<String>,
}

/// One heading of the kinds list: the editors whose Tools place from its kinds.
#[derive(Clone, Debug, PartialEq)]
pub struct KindGroup {
    pub title: String,
    pub kinds: Vec<KindRow>,
}

/// One kind in the list.
#[derive(Clone, Debug, PartialEq)]
pub struct KindRow {
    /// `pcb.line`, as `app.defaults.*` takes it.
    pub id: &'static str,
    pub title: String,
    /// How many of its fields differ from the shipped values.
    pub modified: usize,
}

/// A kind's title, such as Line: `defaults-kind-<id>`.
pub fn kind_title(id: &str) -> String {
    let key = format!("defaults-kind-{}", id.replace(['.', '_'], "-"));
    if i18n::has_key(&key) {
        i18n::translate(&key, &[])
    } else {
        id.to_owned()
    }
}

/// The heading of the kinds placed by `editors`, such as "PCB and Footprint".
fn group_title(editors: &[DefaultsEditor]) -> String {
    let name = |e: &DefaultsEditor| match e {
        DefaultsEditor::Pcb => "pcb",
        DefaultsEditor::Footprint => "footprint",
        DefaultsEditor::Symbol => "symbol",
        DefaultsEditor::Schematic => "schematic",
    };
    let key = format!(
        "defaults-group-{}",
        editors.iter().map(name).collect::<Vec<_>>().join("-")
    );
    i18n::translate(&key, &[])
}

/// The title of a segment between a kind and its field, such as a Layer kind.
fn part_title(segment: &str) -> String {
    let key = format!("defaults-part-{}", segment.replace('_', "-"));
    if i18n::has_key(&key) {
        i18n::translate(&key, &[])
    } else {
        segment.to_owned()
    }
}

/// The fields of `kind`, one row per leaf of its record, generated from the schema. A
/// field's label is its Tool option's (`tool-option-<field>`), else the schema's; the
/// segments between the kind and the field (a Layer kind, Justify) are its heading.
///
/// They only change with the build, so each kind's are made once.
pub fn fields(kind: &DefaultsKind) -> Vec<Setting> {
    static FIELDS: OnceLock<BTreeMap<&'static str, Vec<Setting>>> = OnceLock::new();
    FIELDS
        .get_or_init(|| KINDS.iter().map(|k| (k.id, make_fields(k))).collect())
        .get(kind.id)
        .cloned()
        .unwrap_or_default()
}

fn make_fields(kind: &DefaultsKind) -> Vec<Setting> {
    let prefix = format!("{}.", kind.prefix());
    kind.keys()
        .into_iter()
        .map(|key| {
            let mut setting = schema::setting(&key);
            let rest = key.strip_prefix(&prefix).unwrap_or(&key);
            let (parts, field) = rest.rsplit_once('.').unwrap_or(("", rest));
            let label = format!("tool-option-{}", field.replace('_', "-"));
            if i18n::has_key(&label) {
                setting.label = i18n::translate(&label, &[]);
            }
            setting.heading = (!parts.is_empty()).then(|| {
                parts
                    .split('.')
                    .map(part_title)
                    .collect::<Vec<_>>()
                    .join(" · ")
            });
            setting
        })
        .collect()
}

/// Whether `kind` matches the filter `query`: its title, its editors' heading, or one of
/// its fields' labels or headings (a Layer kind) contains it, ignoring case. A blank filter matches every kind.
fn matches(kind: &DefaultsKind, query: &str) -> bool {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return true;
    }
    let has = |text: &str| text.to_lowercase().contains(&query);
    has(&kind_title(kind.id))
        || has(&group_title(kind.editors))
        || has(kind.id)
        || fields(kind)
            .iter()
            .any(|s| has(&s.label) || s.heading.as_deref().is_some_and(has))
}

impl PreferencesDialog {
    /// The kinds the filter leaves, under their editors' headings, in the spec's order.
    pub fn defaults_groups(&self, shell: &Shell) -> Vec<KindGroup> {
        let now = serde_json::to_value(shell.preferences()).expect("Preferences serialise");
        let shipped = super::defaults();
        let mut groups: Vec<KindGroup> = Vec::new();
        for kind in KINDS.iter().filter(|k| matches(k, &self.defaults.filter)) {
            let title = group_title(kind.editors);
            let modified = kind
                .keys()
                .iter()
                .filter(|k| super::leaf(&now, k) != super::leaf(shipped, k))
                .count();
            let row = KindRow {
                id: kind.id,
                title: kind_title(kind.id),
                modified,
            };
            match groups.iter_mut().find(|g| g.title == title) {
                Some(group) => group.kinds.push(row),
                None => groups.push(KindGroup {
                    title,
                    kinds: vec![row],
                }),
            }
        }
        groups
    }

    /// The kind shown: the one chosen, else the first the filter leaves.
    pub fn defaults_kind(&self, shell: &Shell) -> Option<&'static DefaultsKind> {
        let listed = self.defaults_groups(shell);
        let shown = |id: &str| listed.iter().flat_map(|g| &g.kinds).any(|k| k.id == id);
        self.defaults
            .chosen
            .filter(|id| shown(id))
            .or_else(|| listed.first().and_then(|g| g.kinds.first()).map(|k| k.id))
            .and_then(preferences::defaults_kind)
    }

    /// Shows `kind` (`pcb.line`).
    pub fn choose_defaults_kind(&mut self, kind: &str) {
        self.defaults.chosen = preferences::defaults_kind(kind).map(|k| k.id);
    }

    /// Reset for `kind`, or for every kind (`app.defaults.reset`).
    pub fn reset_defaults(&mut self, shell: &mut Shell, kind: Option<&str>) {
        let params = kind.map_or_else(|| json!({}), |k| json!({ "kind": k }));
        match shell.run("app.defaults.reset", params) {
            Ok(out) => {
                let keys: Vec<String> = out["reset"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|k| k.as_str().map(str::to_owned))
                    .collect();
                for key in &keys {
                    self.texts.remove(key);
                    self.problems.remove(key);
                }
                self.defaults.message = Some(tr!("defaults-reset-done", count = keys.len()));
            }
            Err(e) => self.defaults.message = Some(e.message),
        }
    }

    /// Save As… for `kind`, or every kind: the file picker it asks for.
    pub fn save_defaults_as(&mut self, kind: Option<&'static str>) -> FileRequest {
        self.defaults.saving = kind;
        FileRequest::SaveDefaults
    }

    /// Writes the kind Save As… was for (or every kind) to `path` (`app.defaults.save`).
    pub fn save_defaults(&mut self, shell: &mut Shell, path: &Path) {
        let mut params = json!({ "path": path });
        if let Some(kind) = self.defaults.saving.take() {
            params["kind"] = json!(kind);
        }
        self.defaults.message = Some(match shell.run("app.defaults.save", params) {
            Ok(out) => tr!(
                "defaults-saved",
                count = out["kinds"].as_array().map_or(0, Vec::len),
                path = path.display().to_string()
            ),
            Err(e) => e.message,
        });
    }

    /// Reads `path` (`app.defaults.load`) and says how many kinds it replaced and how many
    /// entries it skipped, on the page and in the status bar.
    pub fn load_defaults(&mut self, shell: &mut Shell, path: &Path) {
        match shell.run("app.defaults.load", json!({ "path": path })) {
            Ok(out) => {
                let kinds = out["kinds"].as_array().map_or(0, Vec::len);
                let skipped = out["skipped"].as_u64().unwrap_or(0);
                self.texts.retain(|k, _| !k.starts_with("defaults."));
                self.problems.retain(|k, _| !k.starts_with("defaults."));
                let message = tr!("defaults-loaded", kinds = kinds, skipped = skipped);
                shell.set_note(message.clone());
                self.defaults.message = Some(message);
            }
            Err(e) => self.defaults.message = Some(e.message),
        }
    }

    /// A file picked for the Defaults page.
    pub fn picked_file(&mut self, shell: &mut Shell, request: FileRequest, path: &Path) {
        match request {
            FileRequest::SaveDefaults => self.save_defaults(shell, path),
            FileRequest::LoadDefaults => self.load_defaults(shell, path),
            _ => {}
        }
    }
}
