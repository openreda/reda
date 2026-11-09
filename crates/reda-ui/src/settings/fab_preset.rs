//! The Fab Preset pages (docs/spec/v0.1/outputs.md#fab-preset-pages): Rules, Stackup,
//! Gerber, Drill, BOM, CPL and Packaging, the same for a Project copy (in Project
//! Settings, committed on Apply with `fab_preset.set`) and a user preset (in Preferences,
//! written with `fab_preset.set_user`).
//!
//! A [`PresetEditor`] drafts one preset. Each [`PresetPage`] owns some of its keys: the
//! page's changes are those keys where the draft differs from what is stored, and Reset
//! page to source puts those keys back from the source preset. The model has no drawing;
//! [`draw`] draws a page.

use std::collections::BTreeMap;

use reda_engine::model::FabPreset;
use serde_json::{Map, Value};

use crate::settings::Problem;
use crate::tr;

pub mod draw;
mod field;

pub use field::{Field, source_token};

/// One page of a preset.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PresetPage {
    Rules,
    Stackup,
    Gerber,
    Drill,
    Bom,
    Cpl,
    Packaging,
}

impl PresetPage {
    pub const ALL: [PresetPage; 7] = [
        PresetPage::Rules,
        PresetPage::Stackup,
        PresetPage::Gerber,
        PresetPage::Drill,
        PresetPage::Bom,
        PresetPage::Cpl,
        PresetPage::Packaging,
    ];

    pub fn title(self) -> String {
        match self {
            PresetPage::Rules => tr!("preset-page-rules"),
            PresetPage::Stackup => tr!("preset-page-stackup"),
            PresetPage::Gerber => tr!("preset-page-gerber"),
            PresetPage::Drill => tr!("preset-page-drill"),
            PresetPage::Bom => tr!("preset-page-bom"),
            PresetPage::Cpl => tr!("preset-page-cpl"),
            PresetPage::Packaging => tr!("preset-page-packaging"),
        }
    }

    /// The preset keys the page edits, as `section.key` or a top-level key. A whole
    /// section (`rules`) is owned key by key.
    pub fn keys(self) -> &'static [&'static str] {
        match self {
            PresetPage::Rules => &["rules"],
            PresetPage::Stackup => &["stackup", "stackup_suggestions"],
            PresetPage::Gerber => &[
                "gerber.naming",
                "gerber.file_names",
                "gerber.x2_attributes",
                "gerber.file_origin",
                "gerber.outline_on_every_plot",
            ],
            PresetPage::Drill => &["gerber.slot_command", "gerber.merge_drill"],
            PresetPage::Bom => &["bom"],
            PresetPage::Cpl => &["cpl"],
            PresetPage::Packaging => &["zip_assembly"],
        }
    }

    /// Whether the key or field `id` (such as `rules.clearance`) is on this page.
    pub fn owns(self, id: &str) -> bool {
        self.keys()
            .iter()
            .any(|k| id == *k || id.starts_with(&format!("{k}.")))
    }
}

/// A draft of one preset: what is stored, what the pages show, and the text of typed
/// fields.
#[derive(Clone, Debug, PartialEq)]
pub struct PresetEditor {
    base: FabPreset,
    value: FabPreset,
    /// Text typed into a field, kept while it is being typed or is not a value.
    texts: BTreeMap<Field, String>,
    /// Why a field's text is not a value.
    problems: BTreeMap<Field, String>,
}

impl PresetEditor {
    pub fn new(preset: FabPreset) -> PresetEditor {
        PresetEditor {
            base: preset.clone(),
            value: preset,
            texts: BTreeMap::new(),
            problems: BTreeMap::new(),
        }
    }

    /// What the pages show.
    pub fn value(&self) -> &FabPreset {
        &self.value
    }

    pub fn value_mut(&mut self) -> &mut FabPreset {
        &mut self.value
    }

    /// What is stored.
    pub fn base(&self) -> &FabPreset {
        &self.base
    }

    /// Whether anything differs from what is stored.
    pub fn changed(&self) -> bool {
        self.value != self.base || !self.problems.is_empty()
    }

    /// Takes in the stored preset: with `reset`, or while nothing was changed, the draft
    /// becomes it.
    pub fn load(&mut self, fresh: FabPreset, reset: bool) {
        if fresh == self.base && !reset {
            return;
        }
        if reset || !self.changed() {
            self.value = fresh.clone();
            self.texts.clear();
            self.problems.clear();
        }
        self.base = fresh;
    }

    /// Notes that what is stored is now the draft (after a user preset is written).
    pub fn saved(&mut self) {
        self.base = self.value.clone();
    }

    /// The text `field` shows.
    pub fn text(&self, field: Field) -> String {
        self.texts
            .get(&field)
            .cloned()
            .unwrap_or_else(|| field.text(&self.value))
    }

    /// Takes `text` typed into `field`: a value goes into the draft, anything else is
    /// flagged and kept as typed.
    pub fn set_text(&mut self, field: Field, text: &str) {
        self.texts.insert(field, text.to_owned());
        match field.apply(&mut self.value, text) {
            Ok(()) => {
                self.problems.remove(&field);
            }
            Err(why) => {
                self.problems.insert(field, why);
            }
        }
    }

    /// Drops the typed text of every field (after the draft changed under them).
    pub fn forget_texts(&mut self) {
        self.texts.clear();
        self.problems.clear();
    }

    /// The invalid fields on `page`.
    pub fn problems(&self, page: PresetPage) -> Vec<Problem> {
        self.problems
            .iter()
            .filter(|(f, _)| f.page() == page)
            .map(|(f, why)| Problem::new(f.id(), why.clone()))
            .collect()
    }

    /// The why of `field`'s problem, if its text is not a value.
    pub fn problem(&self, field: Field) -> Option<Problem> {
        self.problems
            .get(&field)
            .map(|why| Problem::new(field.id(), why.clone()))
    }

    /// The page's keys that differ from what is stored, in the shape `fab_preset.set`
    /// takes; `None` when none does.
    pub fn changes(&self, page: PresetPage) -> Option<Value> {
        let (value, base) = (json(&self.value), json(&self.base));
        let mut out = Map::new();
        for key in page.keys() {
            for (section, key) in expand(key, &value, &base) {
                let now = lookup(&value, &section, key.as_deref());
                if now == lookup(&base, &section, key.as_deref()) {
                    continue;
                }
                let now = now.unwrap_or(Value::Null);
                match key {
                    None => {
                        out.insert(section, now);
                    }
                    Some(key) => {
                        let entry = out
                            .entry(section)
                            .or_insert_with(|| Value::Object(Map::new()));
                        entry.as_object_mut().expect("a section").insert(key, now);
                    }
                }
            }
        }
        (!out.is_empty()).then_some(Value::Object(out))
    }

    /// Every page's changes together.
    pub fn all_changes(&self) -> Option<Value> {
        let mut out = Map::new();
        for page in PresetPage::ALL {
            let Some(Value::Object(changes)) = self.changes(page) else {
                continue;
            };
            for (key, value) in changes {
                match (out.get_mut(&key), value) {
                    (Some(Value::Object(section)), Value::Object(more)) => section.extend(more),
                    (_, value) => {
                        out.insert(key, value);
                    }
                }
            }
        }
        (!out.is_empty()).then_some(Value::Object(out))
    }

    /// Reset page to source: the page's keys take `source`'s values, as pending edits.
    pub fn reset_page(&mut self, page: PresetPage, source: &FabPreset) {
        let mut value = json(&self.value);
        let from = json(source);
        for key in page.keys() {
            for (section, key) in expand(key, &value, &from) {
                let taken = lookup(&from, &section, key.as_deref());
                let target = value.as_object_mut().expect("a preset object");
                match key {
                    None => match taken {
                        Some(v) => {
                            target.insert(section, v);
                        }
                        None => {
                            target.remove(&section);
                        }
                    },
                    Some(key) => {
                        let entry = target
                            .entry(section)
                            .or_insert_with(|| Value::Object(Map::new()));
                        let entry = entry.as_object_mut().expect("a section");
                        match taken {
                            Some(v) => {
                                entry.insert(key, v);
                            }
                            None => {
                                entry.remove(&key);
                            }
                        }
                    }
                }
            }
        }
        self.value = serde_json::from_value(value).expect("a preset's own keys");
        self.texts.retain(|f, _| f.page() != page);
        self.problems.retain(|f, _| f.page() != page);
    }
}

fn json(preset: &FabPreset) -> Value {
    serde_json::to_value(preset).expect("presets serialise")
}

/// A page key as (section, key) pairs: `gerber.naming` is one; a whole section such as
/// `rules` is each of its keys in either preset; a top-level value is `(key, None)`.
fn expand(key: &str, a: &Value, b: &Value) -> Vec<(String, Option<String>)> {
    if let Some((section, inner)) = key.split_once('.') {
        return vec![(section.to_owned(), Some(inner.to_owned()))];
    }
    let keys_of = |v: &Value| -> Vec<String> {
        v.get(key)
            .and_then(Value::as_object)
            .map(|o| o.keys().cloned().collect())
            .unwrap_or_default()
    };
    let is_section = |v: &Value| v.get(key).is_none_or(Value::is_object);
    if key != "zip_assembly" && is_section(a) && is_section(b) {
        let mut keys = keys_of(a);
        for k in keys_of(b) {
            if !keys.contains(&k) {
                keys.push(k);
            }
        }
        keys.into_iter()
            .map(|k| (key.to_owned(), Some(k)))
            .collect()
    } else {
        vec![(key.to_owned(), None)]
    }
}

fn lookup(v: &Value, section: &str, key: Option<&str>) -> Option<Value> {
    let at = v.get(section)?;
    match key {
        None => Some(at.clone()),
        Some(key) => at.get(key).cloned(),
    }
}
