//! Editing the user's `keymap.toml` in place with `toml_edit`, so comments and the other
//! entries survive: dropping the entries for one key in one context, and adding a
//! `[[bind]]` or an `[[unbind]]`. The format is the Keymap's (see [`crate::keymap`]).

use std::io;
use std::path::Path;

use serde_json::Value;
use toml_edit::{Array, ArrayOfTables, DocumentMut, InlineTable, Item, Table};

use crate::bars::Binding;
use crate::keymap::{Chord, Context};

/// The user's file, or an empty one when there is none. A file that does not parse is an
/// error: it is never overwritten.
pub fn read(path: &Path) -> io::Result<DocumentMut> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e),
    };
    text.parse::<DocumentMut>()
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))
}

/// Writes `doc` through a temp file and a rename.
pub fn write(path: &Path, doc: &DocumentMut) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let temp = path.with_extension("toml.tmp");
    std::fs::write(&temp, doc.to_string())?;
    std::fs::rename(&temp, path)
}

/// Whether an entry is for `chord` in `context`. Entries that do not read are left alone.
fn is_for(table: &Table, context: &Context, chord: &Chord) -> bool {
    let own_context = match table.get("context").and_then(Item::as_str) {
        None => Some(Context::Global),
        Some(name) => Context::parse(name),
    };
    let own_chord = table
        .get("key")
        .and_then(Item::as_str)
        .and_then(|k| Chord::parse(k).ok());
    own_context.as_ref() == Some(context) && own_chord.as_ref() == Some(chord)
}

/// Drops every `[[bind]]` and `[[unbind]]` for `chord` in `context`.
pub fn remove(doc: &mut DocumentMut, context: &Context, chord: &Chord) {
    // Comments above a removed entry move to the entry after it, or to the end.
    let mut orphaned = String::new();
    for section in ["bind", "unbind"] {
        let Some(tables) = doc.get_mut(section).and_then(Item::as_array_of_tables_mut) else {
            continue;
        };
        let mut i = 0;
        while i < tables.len() {
            let table = tables.get(i).expect("in range");
            if !is_for(table, context, chord) {
                i += 1;
                continue;
            }
            let prefix = comments(table.decor().prefix());
            tables.remove(i);
            if prefix.is_empty() {
                continue;
            }
            match tables.get_mut(i) {
                Some(next) => {
                    let own = next.decor().prefix().and_then(|p| p.as_str());
                    let joined = format!("{prefix}{}", own.unwrap_or_default());
                    next.decor_mut().set_prefix(joined);
                }
                None => orphaned.push_str(&prefix),
            }
        }
        if tables.is_empty() {
            doc.remove(section);
        }
    }
    if !orphaned.is_empty() {
        let trailing = doc.trailing().as_str().unwrap_or_default().to_owned();
        doc.set_trailing(format!("{orphaned}{trailing}"));
    }
}

/// The comment lines of a table's prefix, each ending in a newline.
fn comments(prefix: Option<&toml_edit::RawString>) -> String {
    prefix
        .and_then(|p| p.as_str())
        .unwrap_or_default()
        .lines()
        .filter(|l| l.trim_start().starts_with('#'))
        .map(|l| format!("{l}\n"))
        .collect()
}

fn entry(context: &Context, chord: &Chord) -> Table {
    let mut table = Table::new();
    if *context != Context::Global {
        table["context"] = toml_edit::value(context.to_string());
    }
    table["key"] = toml_edit::value(chord.portable());
    table
}

fn push(doc: &mut DocumentMut, section: &str, mut table: Table) {
    // Text after the last table (such as a comment-only file) stays above the new entry.
    let trailing = doc.trailing().as_str().unwrap_or_default().to_owned();
    if !trailing.trim().is_empty() {
        doc.set_trailing("");
        table.decor_mut().set_prefix(format!("{trailing}\n"));
    }
    if !doc.contains_key(section) {
        doc.insert(section, Item::ArrayOfTables(ArrayOfTables::new()));
    }
    if let Some(tables) = doc.get_mut(section).and_then(Item::as_array_of_tables_mut) {
        tables.push(table);
    }
}

/// Adds a `[[bind]]` of `chord` in `context` to `binding`.
pub fn add_bind(doc: &mut DocumentMut, context: &Context, chord: &Chord, binding: &Binding) {
    let mut table = entry(context, chord);
    match binding {
        Binding::Command(id) | Binding::Preset(id) => {
            table["command"] = toml_edit::value(id.as_str());
        }
        Binding::Raw { command, params } => {
            table["command"] = toml_edit::value(command.as_str());
            if let toml_edit::Value::InlineTable(params) = toml(params) {
                table["params"] = toml_edit::value(params);
            }
        }
    }
    push(doc, "bind", table);
}

/// Adds an `[[unbind]]` of `chord` in `context`.
pub fn add_unbind(doc: &mut DocumentMut, context: &Context, chord: &Chord) {
    push(doc, "unbind", entry(context, chord));
}

/// A JSON value as TOML (null, which TOML lacks, as an empty string).
fn toml(value: &Value) -> toml_edit::Value {
    match value {
        Value::Null => "".into(),
        Value::Bool(b) => (*b).into(),
        Value::Number(n) => match n.as_i64() {
            Some(i) => i.into(),
            None => n.as_f64().unwrap_or_default().into(),
        },
        Value::String(s) => s.as_str().into(),
        Value::Array(items) => {
            let mut array = Array::new();
            for item in items {
                array.push(toml(item));
            }
            toml_edit::Value::Array(array)
        }
        Value::Object(map) => {
            let mut table = InlineTable::new();
            for (k, v) in map {
                table.insert(k, toml(v));
            }
            toml_edit::Value::InlineTable(table)
        }
    }
}
