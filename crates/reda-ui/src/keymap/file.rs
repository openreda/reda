//! Reading a Keymap file. The default Keymap and the user's `keymap.toml` share one format:
//!
//! ```toml
//! [[bind]]
//! key = "Mod+S"               # portable, by logical key
//! command = "project.save"    # a Command, or a Preset such as "panel.show:layers"
//!
//! [[bind]]
//! context = "schematic"       # global (the default), schematic, pcb, symbol,
//! key = "F"                   # footprint or tool:<name>
//! command = "view.zoom_fit"
//! params = { document = "$active_document" }   # a raw {command, params} binding
//!
//! [[unbind]]                  # the user's file only: drop a binding
//! context = "global"
//! key = "Mod+Y"
//!
//! [modifiers]                 # held modifiers of mouse gestures (see `modifiers`)
//! net_highlight_sheet = "Ctrl"
//! ```
//!
//! A bad entry is reported and skipped; the rest of the file still applies.

use serde_json::{Map, Number, Value};
use toml_edit::{DocumentMut, Item, Table};

use super::modifiers::{Held, Role};
use super::{Chord, Context};
use crate::bars::{Action, Binding};
use crate::tr;

/// One entry of a Keymap file.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Entry {
    Bind {
        context: Context,
        chord: Chord,
        action: Action,
    },
    Unbind {
        context: Context,
        chord: Chord,
    },
    /// A line of the `[modifiers]` table.
    Modifier {
        role: Role,
        held: Held,
    },
}

/// Reads `text`, the file named `file` in messages. Returns the good entries in file order
/// and a message for each bad one.
pub(super) fn parse(file: &str, text: &str, mac: bool) -> (Vec<Entry>, Vec<String>) {
    let mut problems = Vec::new();
    let doc: DocumentMut = match text.parse() {
        Ok(doc) => doc,
        Err(e) => {
            let error = e.to_string().trim().replace('\n', " ");
            return (
                Vec::new(),
                vec![tr!("keymap-unreadable", file = file, error = error)],
            );
        }
    };
    let mut entries = Vec::new();
    for (name, item) in doc.iter() {
        if name == "modifiers" {
            modifiers(file, item, &mut entries, &mut problems);
            continue;
        }
        let unbind = match name {
            "bind" => false,
            "unbind" => true,
            other => {
                problems.push(tr!("keymap-unknown-section", file = file, section = other));
                continue;
            }
        };
        let Some(tables) = item.as_array_of_tables() else {
            problems.push(tr!("keymap-unknown-section", file = file, section = name));
            continue;
        };
        for table in tables.iter() {
            match entry(table, unbind, mac) {
                Ok(entry) => entries.push(entry),
                Err(error) => problems.push(tr!("keymap-bad-entry", file = file, error = error)),
            }
        }
    }
    (entries, problems)
}

/// The `[modifiers]` table: one role per line.
fn modifiers(file: &str, item: &Item, entries: &mut Vec<Entry>, problems: &mut Vec<String>) {
    let Some(table) = item.as_table_like() else {
        problems.push(tr!(
            "keymap-unknown-section",
            file = file,
            section = "modifiers"
        ));
        return;
    };
    for (name, value) in table.iter() {
        let Some(role) = Role::parse(name) else {
            let error = tr!("keymap-unknown-role", role = name);
            problems.push(tr!("keymap-bad-entry", file = file, error = error));
            continue;
        };
        match value.as_str().and_then(Held::parse) {
            Some(held) => entries.push(Entry::Modifier { role, held }),
            None => {
                let error = tr!("keymap-bad-modifier", role = name);
                problems.push(tr!("keymap-bad-entry", file = file, error = error));
            }
        }
    }
}

fn entry(table: &Table, unbind: bool, mac: bool) -> Result<Entry, String> {
    let allowed: &[&str] = if unbind {
        &["context", "key"]
    } else {
        &["context", "key", "command", "params"]
    };
    if let Some((k, _)) = table.iter().find(|(k, _)| !allowed.contains(k)) {
        return Err(tr!("keymap-unknown-field", field = k));
    }
    let key = string(table, "key")?.ok_or_else(|| tr!("keymap-missing-field", field = "key"))?;
    let chord = Chord::parse_for(&key, mac)
        .map_err(|error| tr!("keymap-bad-key", key = key.as_str(), error = error))?;
    let context = match string(table, "context")? {
        None => Context::Global,
        Some(name) => Context::parse(&name)
            .ok_or_else(|| tr!("keymap-bad-context", key = key.as_str(), context = name))?,
    };
    if unbind {
        return Ok(Entry::Unbind { context, chord });
    }
    let command = string(table, "command")?.ok_or_else(|| {
        tr!(
            "keymap-missing-field-for",
            field = "command",
            key = key.as_str()
        )
    })?;
    let binding = match (table.get("params"), command.contains(':')) {
        (Some(_), true) => return Err(tr!("keymap-preset-params", key = key.as_str())),
        (Some(params), false) => Binding::Raw {
            command,
            params: json(params)?,
        },
        (None, true) => Binding::Preset(command),
        (None, false) => Binding::Command(command),
    };
    let action = Action {
        id: format!("keymap/{context}/{}", chord.portable()),
        binding,
        title: None,
        icon: None,
    };
    Ok(Entry::Bind {
        context,
        chord,
        action,
    })
}

fn string(table: &Table, field: &str) -> Result<Option<String>, String> {
    match table.get(field) {
        None => Ok(None),
        Some(item) => item
            .as_str()
            .map(|s| Some(s.to_owned()))
            .ok_or_else(|| tr!("keymap-not-a-string", field = field)),
    }
}

/// A TOML value as JSON, for a raw binding's params.
fn json(item: &Item) -> Result<Value, String> {
    match item {
        Item::Value(v) => value(v),
        Item::Table(t) => table_json(t.iter()),
        Item::ArrayOfTables(a) => a.iter().map(|t| table_json(t.iter())).collect(),
        Item::None => Ok(Value::Null),
    }
}

fn table_json<'a>(items: impl Iterator<Item = (&'a str, &'a Item)>) -> Result<Value, String> {
    let mut map = Map::new();
    for (k, v) in items {
        map.insert(k.to_owned(), json(v)?);
    }
    Ok(Value::Object(map))
}

fn value(v: &toml_edit::Value) -> Result<Value, String> {
    use toml_edit::Value as T;
    Ok(match v {
        T::String(s) => Value::String(s.value().clone()),
        T::Integer(i) => Value::from(*i.value()),
        T::Float(f) => Number::from_f64(*f.value())
            .map(Value::Number)
            .ok_or_else(|| tr!("keymap-bad-number"))?,
        T::Boolean(b) => Value::Bool(*b.value()),
        T::Datetime(d) => Value::String(d.value().to_string()),
        T::Array(a) => Value::Array(a.iter().map(value).collect::<Result<_, _>>()?),
        T::InlineTable(t) => {
            let mut map = Map::new();
            for (k, v) in t.iter() {
                map.insert(k.to_owned(), value(v)?);
            }
            Value::Object(map)
        }
    })
}
