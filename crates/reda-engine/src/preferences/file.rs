//! Reading `preferences.toml` key by key, and editing one key in place.

use std::io::ErrorKind;

use reda_command::{CommandError, UserDirs};
use serde_json::{Map, Value};
use toml_edit::{DocumentMut, Item, Table, TableLike};

use super::{ALIASES, Preferences};

/// The Preferences as read from the file, with a warning for every key that was ignored.
#[derive(Clone, Debug, PartialEq)]
pub struct Loaded {
    pub values: Preferences,
    pub warnings: Vec<String>,
}

/// Reads the user's `preferences.toml`. A missing file means every default; a file that
/// cannot be read or parsed gives the defaults and a warning.
pub fn load(dirs: &UserDirs) -> Loaded {
    let path = dirs.preferences_file();
    match std::fs::read_to_string(&path) {
        Ok(text) => parse(&text),
        Err(e) if e.kind() == ErrorKind::NotFound => parse(""),
        Err(e) => Loaded {
            values: Preferences::default(),
            warnings: vec![format!("could not read {}: {e}", path.display())],
        },
    }
}

/// Reads the text of a `preferences.toml`.
pub fn parse(text: &str) -> Loaded {
    parse_with(text, ALIASES)
}

pub(super) fn parse_with(text: &str, aliases: &[(&str, &str)]) -> Loaded {
    match text.parse::<DocumentMut>() {
        Ok(doc) => {
            let (values, warnings) = read_doc(&doc, aliases);
            Loaded {
                values: serde_json::from_value(values).expect("read_doc keeps the values valid"),
                warnings,
            }
        }
        Err(e) => Loaded {
            values: Preferences::default(),
            warnings: vec![format!(
                "preferences.toml does not parse, so every Preference is at its default: {e}"
            )],
        },
    }
}

/// Sets one Preference in the user's file and returns its new value. The file is re-read
/// first and only the touched key changes; setting the default value (or `null`) removes
/// the key. The write goes through a temp file and a rename.
pub fn set(dirs: &UserDirs, key: &str, value: Value) -> Result<Value, CommandError> {
    set_with(dirs, key, value, ALIASES)
}

pub(super) fn set_with(
    dirs: &UserDirs,
    key: &str,
    value: Value,
    aliases: &[(&str, &str)],
) -> Result<Value, CommandError> {
    let key = canonical(key, aliases);
    let defaults = defaults();
    let Some(default) = leaf(&defaults, key).cloned() else {
        return Err(CommandError::invalid_params(
            "/key",
            format!("{key} is not a Preference"),
        ));
    };

    let path = dirs.preferences_file();
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == ErrorKind::NotFound => String::new(),
        Err(e) => {
            return Err(CommandError::internal(format!(
                "reading {}: {e}",
                path.display()
            )));
        }
    };
    let mut doc = text.parse::<DocumentMut>().map_err(|e| {
        CommandError::rejected(format!(
            "{} does not parse, so REDA will not overwrite it; fix it first: {e}",
            path.display()
        ))
    })?;

    let new = if value.is_null() {
        default.clone()
    } else {
        let (mut current, _) = read_doc(&doc, aliases);
        apply(&mut current, key, value)
            .map_err(|why| CommandError::invalid_params("/value", format!("{key}: {why}")))?;
        leaf(&current, key).cloned().expect("a known key")
    };

    for (old, _) in aliases.iter().filter(|(_, new)| *new == key) {
        remove_leaf(doc.as_table_mut(), &segments(old));
    }
    if new == default {
        remove_leaf(doc.as_table_mut(), &segments(key));
    } else {
        set_leaf(doc.as_table_mut(), &segments(key), to_toml(&new)?);
    }

    std::fs::create_dir_all(&dirs.config)
        .and_then(|()| crate::fs::write_atomic(&path, doc.to_string().as_bytes()))
        .map_err(|e| CommandError::internal(format!("writing {}: {e}", path.display())))?;
    Ok(new)
}

/// Puts `keys` back to their defaults in one write, keeping every other line, and returns
/// the keys whose value changed, sorted. Every key must be a Preference (checked by the
/// caller); an old spelling is removed too.
pub fn reset(dirs: &UserDirs, keys: &[String]) -> Result<Vec<String>, CommandError> {
    let path = dirs.preferences_file();
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => {
            return Err(CommandError::internal(format!(
                "reading {}: {e}",
                path.display()
            )));
        }
    };
    let mut doc = text.parse::<DocumentMut>().map_err(|e| {
        CommandError::rejected(format!(
            "{} does not parse, so REDA will not overwrite it; fix it first: {e}",
            path.display()
        ))
    })?;
    let (current, _) = read_doc(&doc, ALIASES);
    let defaults = defaults();
    let mut changed: Vec<String> = Vec::new();
    for key in keys {
        let key = canonical(key, ALIASES);
        if leaf(&current, key) != leaf(&defaults, key) && !changed.iter().any(|k| k == key) {
            changed.push(key.to_owned());
        }
        for (old, _) in ALIASES.iter().filter(|(_, new)| *new == key) {
            remove_leaf(doc.as_table_mut(), &segments(old));
        }
        remove_leaf(doc.as_table_mut(), &segments(key));
    }
    let after = doc.to_string();
    if after != text {
        crate::fs::write_atomic(&path, after.as_bytes())
            .map_err(|e| CommandError::internal(format!("writing {}: {e}", path.display())))?;
    }
    changed.sort();
    Ok(changed)
}

/// The value of a dotted key (or an old spelling of one) in `prefs`, with the current
/// spelling of the key; `None` when the key is not a Preference.
pub fn value_of<'k>(prefs: &Preferences, key: &'k str) -> Option<(&'k str, Value)> {
    let key = canonical(key, ALIASES);
    let json = serde_json::to_value(prefs).expect("Preferences serialise");
    Some((key, leaf(&json, key)?.clone()))
}

/// The defaults as JSON: every leaf is a key.
fn defaults() -> Value {
    serde_json::to_value(Preferences::default()).expect("Preferences serialise")
}

fn canonical<'a>(key: &'a str, aliases: &[(&'a str, &'a str)]) -> &'a str {
    aliases
        .iter()
        .find(|(old, _)| *old == key)
        .map_or(key, |(_, new)| new)
}

fn segments(key: &str) -> Vec<&str> {
    key.split('.').collect()
}

/// The value at a dotted key, if the key is a leaf (not a section).
fn leaf<'v>(json: &'v Value, key: &str) -> Option<&'v Value> {
    let mut node = json;
    for segment in key.split('.') {
        node = node.as_object()?.get(segment)?;
    }
    (!node.is_object()).then_some(node)
}

/// Sets `key` in a valid Preferences value, keeping it valid: refuses a value that does
/// not fit the schema or its range rules. Leaves `json` normalised (`2` becomes `2.0` for
/// a float).
fn apply(json: &mut Value, key: &str, value: Value) -> Result<(), String> {
    let mut candidate = json.clone();
    let mut node = &mut candidate;
    for segment in key.split('.') {
        node = node
            .get_mut(segment)
            .expect("apply is only called for known keys");
    }
    *node = value;
    let prefs: Preferences = serde_json::from_value(candidate).map_err(|e| e.to_string())?;
    prefs.check().map_err(|(_, why)| why)?;
    *json = serde_json::to_value(prefs).expect("Preferences serialise");
    Ok(())
}

/// Every key in the file applied over the defaults, old (aliased) spellings first so the
/// current spelling wins. Returns the values and a warning per ignored key.
fn read_doc(doc: &DocumentMut, aliases: &[(&str, &str)]) -> (Value, Vec<String>) {
    let mut leaves = Vec::new();
    collect_leaves(doc.as_table(), "", &mut leaves);
    leaves.sort_by_key(|(key, _)| canonical(key, aliases) == key.as_str());

    let defaults = defaults();
    let mut values = defaults.clone();
    let mut warnings = Vec::new();
    for (key, value) in leaves {
        let key = canonical(&key, aliases).to_owned();
        if leaf(&defaults, &key).is_none() {
            warnings.push(format!(
                "preferences.toml: {key} is not a Preference; ignored"
            ));
            continue;
        }
        let result = to_json(&value)
            .ok_or_else(|| "this kind of value is not supported".to_owned())
            .and_then(|v| apply(&mut values, &key, v));
        if let Err(why) = result {
            warnings.push(format!(
                "preferences.toml: {key} is invalid ({why}); using the default"
            ));
        }
    }
    (values, warnings)
}

fn collect_leaves(table: &dyn TableLike, prefix: &str, out: &mut Vec<(String, Item)>) {
    for (name, item) in table.iter() {
        let key = format!("{prefix}{name}");
        match item.as_table_like() {
            Some(inner) => collect_leaves(inner, &format!("{key}."), out),
            None => out.push((key, item.clone())),
        }
    }
}

fn to_json(item: &Item) -> Option<Value> {
    value_to_json(item.as_value()?)
}

fn value_to_json(value: &toml_edit::Value) -> Option<Value> {
    use toml_edit::Value as T;
    Some(match value {
        T::String(s) => Value::String(s.value().clone()),
        T::Integer(i) => Value::from(*i.value()),
        T::Float(f) => Value::from(*f.value()),
        T::Boolean(b) => Value::Bool(*b.value()),
        T::Datetime(d) => Value::String(d.value().to_string()),
        T::Array(a) => Value::Array(a.iter().map(value_to_json).collect::<Option<_>>()?),
        T::InlineTable(t) => Value::Object(
            t.iter()
                .map(|(k, v)| Some((k.to_owned(), value_to_json(v)?)))
                .collect::<Option<Map<_, _>>>()?,
        ),
    })
}

fn to_toml(value: &Value) -> Result<toml_edit::Value, CommandError> {
    Ok(match value {
        Value::Bool(b) => (*b).into(),
        Value::Number(n) => match n.as_i64() {
            Some(i) => i.into(),
            None => n.as_f64().unwrap_or_default().into(),
        },
        Value::String(s) => s.as_str().into(),
        Value::Array(items) => {
            let mut array = toml_edit::Array::new();
            for item in items {
                array.push(to_toml(item)?);
            }
            array.into()
        }
        Value::Object(map) => {
            let mut table = toml_edit::InlineTable::new();
            for (k, v) in map {
                table.insert(k, to_toml(v)?);
            }
            table.into()
        }
        Value::Null => {
            return Err(CommandError::internal(
                "a Preference value cannot be null in TOML",
            ));
        }
    })
}

/// Sets the leaf at `path`, creating sections as needed. An existing value keeps its
/// surrounding whitespace and trailing comment.
fn set_leaf(table: &mut dyn TableLike, path: &[&str], value: toml_edit::Value) {
    match path {
        [] => {}
        [leaf] => match table.get_mut(leaf) {
            Some(Item::Value(old)) => {
                let decor = old.decor().clone();
                *old = value;
                *old.decor_mut() = decor;
            }
            Some(item) => *item = Item::Value(value),
            None => {
                table.insert(leaf, Item::Value(value));
            }
        },
        [section, rest @ ..] => {
            let item = table
                .entry(section)
                .or_insert_with(|| Item::Table(Table::new()));
            if !item.is_table_like() {
                *item = Item::Table(Table::new());
            }
            let inner = item.as_table_like_mut().expect("made a table above");
            set_leaf(inner, rest, value);
        }
    }
}

/// Removes the leaf at `path` if it is there. Sections are kept, with any comments.
fn remove_leaf(table: &mut dyn TableLike, path: &[&str]) {
    match path {
        [] => {}
        [leaf] => {
            table.remove(leaf);
        }
        [section, rest @ ..] => {
            if let Some(inner) = table.get_mut(section).and_then(Item::as_table_like_mut) {
                remove_leaf(inner, rest);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RENAMED: &[(&str, &str)] = &[("schematic.repeat", "schematic.repeat_placement")];

    #[test]
    fn an_old_key_is_read_as_its_new_name() {
        let loaded = parse_with("[schematic]\nrepeat = false\n", RENAMED);
        assert!(!loaded.values.schematic.repeat_placement);
        assert_eq!(loaded.warnings, Vec::<String>::new());
    }

    #[test]
    fn the_four_wiring_preferences_are_schematic_preferences_that_start_on() {
        let schematic = parse_with("", &[]).values.schematic;
        assert!(schematic.optimize_wires);
        assert!(schematic.components_cut_wires);
        assert!(schematic.break_wires_at_autojunctions);
        assert!(schematic.convert_cross_junctions);
        let loaded = parse_with("[schematic]\noptimize_wires = false\n", &[]);
        assert!(!loaded.values.schematic.optimize_wires);
        assert_eq!(loaded.warnings, Vec::<String>::new());
    }

    #[test]
    fn display_cross_overs_is_a_schematic_preference_that_starts_off() {
        assert!(!parse_with("", &[]).values.schematic.display_cross_overs);
        let loaded = parse_with("[schematic]\ndisplay_cross_overs = true\n", &[]);
        assert!(loaded.values.schematic.display_cross_overs);
        assert_eq!(loaded.warnings, Vec::<String>::new());
    }

    #[test]
    fn single_negation_is_a_schematic_preference_that_starts_off() {
        assert!(!parse_with("", &[]).values.schematic.single_negation);
        let loaded = parse_with("[schematic]\nsingle_negation = true\n", &[]);
        assert!(loaded.values.schematic.single_negation);
        assert_eq!(loaded.warnings, Vec::<String>::new());
    }

    #[test]
    fn the_new_spelling_wins_over_the_old() {
        let loaded = parse_with(
            "[schematic]\nrepeat_placement = true\nrepeat = false\n",
            RENAMED,
        );
        assert!(loaded.values.schematic.repeat_placement);
    }

    #[test]
    fn writing_a_renamed_key_replaces_the_old_spelling() {
        let home = std::env::temp_dir().join(format!("reda-alias-{}", std::process::id()));
        let dirs = UserDirs::under(&home);
        std::fs::create_dir_all(&dirs.config).unwrap();
        std::fs::write(
            dirs.preferences_file(),
            "[schematic]\nrepeat = true\nin_place_editing = false\n",
        )
        .unwrap();
        let out = set_with(&dirs, "schematic.repeat", Value::Bool(false), RENAMED).unwrap();
        assert_eq!(out, Value::Bool(false));
        let text = std::fs::read_to_string(dirs.preferences_file()).unwrap();
        let _ = std::fs::remove_dir_all(&home);
        assert_eq!(
            text,
            "[schematic]\nin_place_editing = false\nrepeat_placement = false\n"
        );
    }
}
