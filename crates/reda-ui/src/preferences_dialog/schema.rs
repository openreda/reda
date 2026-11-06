//! Which settings each page shows and how each is edited, read from the Preferences
//! schema (`reda_engine::preferences::schema`), so a Preference added to the engine shows
//! on its section's page without a change here. Labels come from the catalogue
//! (`pref-<key>`, dots and underscores written as `-`), else from the schema's
//! description.

use reda_engine::preferences::{self, pages};
use serde_json::Value;

use super::PageId;
use crate::{i18n, tr};

/// One setting as a page shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct Setting {
    /// The dotted Preference key, such as `schematic.repeat_placement`.
    pub key: String,
    pub kind: Kind,
    pub label: String,
    /// The schema's description, shown on hover and searched.
    pub description: String,
    /// The heading a nested setting sits under (a layer colour's theme), if any.
    pub heading: Option<String>,
}

/// How a setting is edited.
#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    /// A checkbox.
    Toggle,
    /// One of a few values.
    Choice(Vec<Choice>),
    /// A number typed in.
    Number,
    /// A whole number of at least 1, or blank for none (unlimited).
    Count,
    /// Text, or blank for none.
    Text,
    /// A list of text, such as folders.
    List,
}

/// One value of a [`Kind::Choice`].
#[derive(Clone, Debug, PartialEq)]
pub struct Choice {
    /// The value as the file writes it, such as `follow_os`.
    pub value: String,
    pub label: String,
}

/// Where each page's settings come first; other keys of a section follow in key order.
const ORDER: &[&str] = &[
    "general.theme",
    "general.ui_scale",
    "general.reopen_last_project",
    "general.undo_depth",
    "general.default_layout",
    "default_units",
    "repeat_placement",
    "electrical_snap",
    "auto_pan",
    "display.cursor",
    "display.inactive_layer_mode",
];

/// Keys no page shows: the base Keymap has only one choice in v0.1. All settings still
/// lists them.
pub const HIDDEN: &[&str] = &["keymap.base"];

/// Every Preference key, in key order.
pub fn preference_keys() -> Vec<String> {
    preferences::keys()
}

/// The settings `page` shows, in order: every key on All settings, else the page's own
/// keys (from [`pages::page_of`]) without the hidden ones. On the Editing pages, which
/// hold a value per Document type, the Schematic values come first under their heading,
/// then the PCB ones.
pub fn settings_on(page: PageId) -> Vec<Setting> {
    let schema = preferences::schema();
    if page == PageId::ALL_SETTINGS {
        return preference_keys()
            .into_iter()
            .map(|k| setting_in(&schema, k))
            .collect();
    }
    let mut keys: Vec<String> = pages::keys_on(page.id(), false)
        .into_iter()
        .filter(|k| !HIDDEN.contains(&k.as_str()))
        .collect();
    let rank = |key: &str| {
        let leaf = key.split_once('.').map_or(key, |(_, rest)| rest);
        let order = ORDER
            .iter()
            .position(|o| *o == key || *o == leaf)
            .unwrap_or(ORDER.len());
        (per_document(key).is_some_and(|s| s != "schematic"), order)
    };
    keys.sort_by_key(|k| rank(k));
    keys.into_iter().map(|k| setting_in(&schema, k)).collect()
}

/// The setting of one key, wherever it is shown.
pub fn setting(key: &str) -> Setting {
    setting_in(&preferences::schema(), key.to_owned())
}

/// The Document type section of a key on an Editing page, where each Document type has
/// its own value.
fn per_document(key: &str) -> Option<&str> {
    let section = key.split('.').next()?;
    pages::is_in(pages::page_of(key), "editing").then_some(section)
}

/// Every setting whose label, description or key contains `query` (ignoring case), with
/// its page, in page order. An empty query finds nothing.
pub fn search(query: &str) -> Vec<(PageId, Setting)> {
    if query.trim().is_empty() {
        return Vec::new();
    }
    PageId::tree()
        .into_iter()
        .filter(|&page| page != PageId::ALL_SETTINGS)
        .flat_map(|page| settings_on(page).into_iter().map(move |s| (page, s)))
        .filter(|(_, s)| matches(s, query))
        .collect()
}

/// Whether `setting` matches the search `query`: its label, description, key or heading
/// contains it, ignoring case. An empty query matches everything.
pub fn matches(setting: &Setting, query: &str) -> bool {
    let query = query.trim().to_lowercase();
    [&setting.label, &setting.description, &setting.key]
        .iter()
        .any(|t| t.to_lowercase().contains(&query))
        || setting
            .heading
            .as_ref()
            .is_some_and(|h| h.to_lowercase().contains(&query))
}

fn setting_in(schema: &Value, key: String) -> Setting {
    let node = node(schema, &key);
    let resolved = resolve(schema, &node);
    let description = node["description"]
        .as_str()
        .or_else(|| resolved["description"].as_str())
        .unwrap_or_default()
        .to_owned();
    let label = catalogue(&key).unwrap_or_else(|| sentence(&description, &key));
    let document = per_document(&key)
        .and_then(PageId::from_id)
        .map(PageId::title);
    let heading = document.or_else(|| {
        key.rsplit_once('.').and_then(|(parent, _)| {
            // Only nested settings (deeper than `section.name`) get a heading.
            parent.contains('.').then(|| {
                catalogue(parent).unwrap_or_else(|| {
                    let parent_node = self::node(schema, parent);
                    let text = parent_node["description"].as_str().unwrap_or_default();
                    sentence(text, parent)
                })
            })
        })
    });
    Setting {
        kind: kind(schema, &node, &key),
        label,
        description,
        heading,
        key,
    }
}

/// The schema node of a dotted key, as its parent's `properties` has it.
fn node(schema: &Value, key: &str) -> Value {
    let mut node = schema.clone();
    for segment in key.split('.') {
        node = resolve(schema, &node)["properties"][segment].clone();
    }
    node
}

/// Follows a `$ref` into `$defs`.
fn resolve(schema: &Value, node: &Value) -> Value {
    match node["$ref"]
        .as_str()
        .and_then(|r| r.strip_prefix("#/$defs/"))
    {
        Some(name) => schema["$defs"][name].clone(),
        None => node.clone(),
    }
}

fn kind(schema: &Value, node: &Value, key: &str) -> Kind {
    let node = resolve(schema, node);
    if let Some(values) = node["enum"].as_array() {
        let values = values.iter().filter_map(Value::as_str);
        return Kind::Choice(values.map(|v| choice(key, v, None)).collect());
    }
    if let Some(variants) = node["oneOf"].as_array() {
        let choices = variants
            .iter()
            .filter_map(|v| {
                let value = v["const"].as_str()?;
                Some(choice(key, value, v["description"].as_str()))
            })
            .collect();
        return Kind::Choice(choices);
    }
    let types: Vec<&str> = match &node["type"] {
        Value::String(t) => vec![t.as_str()],
        Value::Array(ts) => ts.iter().filter_map(Value::as_str).collect(),
        _ => Vec::new(),
    };
    let nullable = types.contains(&"null");
    if types.contains(&"boolean") {
        Kind::Toggle
    } else if types.contains(&"integer") && nullable {
        Kind::Count
    } else if types.contains(&"integer") || types.contains(&"number") {
        Kind::Number
    } else if types.contains(&"array") {
        Kind::List
    } else {
        Kind::Text
    }
}

fn choice(key: &str, value: &str, description: Option<&str>) -> Choice {
    let label = catalogue(&format!("{key}.{value}"))
        .or_else(|| description.map(|d| sentence(d, value)))
        .unwrap_or_else(|| humanise(value));
    Choice {
        value: value.to_owned(),
        label,
    }
}

/// The catalogue's label for a key, `pref-<key>` with `.` and `_` written as `-`.
fn catalogue(key: &str) -> Option<String> {
    let id = format!("pref-{}", key.replace(['.', '_'], "-"));
    i18n::has_key(&id).then(|| i18n::translate(&id, &[]))
}

/// A description as a label: without its closing full stop. A missing one falls back to
/// the key's last segment.
fn sentence(description: &str, key: &str) -> String {
    let text = description.trim().trim_end_matches('.');
    if text.is_empty() {
        humanise(key.rsplit('.').next().unwrap_or(key))
    } else {
        text.to_owned()
    }
}

/// `follow_os` as `Follow os`.
fn humanise(value: &str) -> String {
    let text = value.replace('_', " ");
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// What a blank [`Kind::Count`] means, for its hint.
pub fn unlimited() -> String {
    tr!("pref-unlimited")
}
