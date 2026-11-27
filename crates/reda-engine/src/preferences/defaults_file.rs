//! Saving and loading Object Defaults (docs/spec/v0.3/input-and-defaults.md#the-defaults-page).
//!
//! A saved file is a TOML document holding the `defaults.*` table, written exactly as
//! `preferences.toml` writes those keys, so it can be read, compared and edited by hand.
//! It holds every field of each kind saved, set or shipped, so loading it gives the same
//! records back. A load replaces the records of the kinds the file holds and leaves the
//! others alone; entries it does not know or cannot read are skipped and counted, and
//! `preferences.toml` is written once, so a load never stops part-way.

use std::io::ErrorKind;
use std::path::Path;

use reda_command::{CommandError, UserDirs};
use toml_edit::DocumentMut;

use super::defaults::{DefaultsKind, KINDS, kind_of_key};
use super::file::{
    apply, canonical, collect_leaves, defaults, drop_retired, leaf, read_doc, remove_leaf, retired,
    segments, set_leaf, to_json, to_toml,
};
use super::{ALIASES, KEPT_AT_DEFAULT};

/// What [`save`] wrote.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Saved {
    /// The kinds written, in [`KINDS`] order.
    pub kinds: Vec<&'static str>,
    /// How many values the file holds.
    pub entries: usize,
}

/// What [`load`] did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoadedDefaults {
    /// The kinds whose records the file replaced, in [`KINDS`] order.
    pub kinds: Vec<&'static str>,
    /// One line per entry skipped, saying which and why.
    pub skipped: Vec<String>,
}

/// `kinds`, or every kind.
fn chosen(kind: Option<&'static DefaultsKind>) -> Vec<&'static DefaultsKind> {
    match kind {
        Some(k) => vec![k],
        None => KINDS.iter().collect(),
    }
}

/// Every key of `kind`'s record, or of every record.
pub fn keys_of(kind: Option<&'static DefaultsKind>) -> Vec<String> {
    chosen(kind).into_iter().flat_map(|k| k.keys()).collect()
}

/// The TOML text of the records of `kind` (or of every kind) as the user's Preferences
/// hold them now: every field, in key order.
pub fn text(dirs: &UserDirs, kind: Option<&'static DefaultsKind>) -> (String, Saved) {
    let values = serde_json::to_value(super::load(dirs).values).expect("Preferences serialise");
    let mut doc = DocumentMut::new();
    let mut entries = 0;
    let kinds = chosen(kind);
    for k in &kinds {
        for key in k.keys() {
            // An unset optional value (a Zone's clearance) has no TOML spelling: it is
            // left out, and a load leaves it unset.
            let Some(value) = leaf(&values, &key).filter(|v| !v.is_null()) else {
                continue;
            };
            let value = to_toml(value).expect("a set value has a TOML spelling");
            set_leaf(doc.as_table_mut(), &segments(&key), value);
            entries += 1;
        }
    }
    let saved = Saved {
        kinds: kinds.iter().map(|k| k.id).collect(),
        entries,
    };
    (doc.to_string(), saved)
}

/// Writes the records of `kind` (or of every kind) to `path`.
pub fn save(
    dirs: &UserDirs,
    path: &Path,
    kind: Option<&'static DefaultsKind>,
) -> Result<Saved, CommandError> {
    let (text, saved) = text(dirs, kind);
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).map_err(|e| {
            CommandError::internal(format!("could not create {}: {e}", parent.display()))
        })?;
    }
    crate::fs::write_atomic(path, text.as_bytes())
        .map_err(|e| CommandError::internal(format!("could not write {}: {e}", path.display())))?;
    Ok(saved)
}

/// The user's `preferences.toml` as a document to edit; a missing file is empty.
fn preferences_doc(dirs: &UserDirs) -> Result<DocumentMut, CommandError> {
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
    text.parse::<DocumentMut>().map_err(|e| {
        CommandError::rejected(format!(
            "{} does not parse, so REDA will not overwrite it; fix it first: {e}",
            path.display()
        ))
    })
}

/// Replaces the records of the kinds the file at `path` holds with its values.
pub fn load(dirs: &UserDirs, path: &Path) -> Result<LoadedDefaults, CommandError> {
    let text = std::fs::read_to_string(path).map_err(|e| match e.kind() {
        ErrorKind::NotFound => {
            CommandError::not_found(format!("{} does not exist", path.display()))
        }
        _ => CommandError::internal(format!("could not read {}: {e}", path.display())),
    })?;
    let file = text.parse::<DocumentMut>().map_err(|e| {
        CommandError::invalid_params(
            "/path",
            format!("{} is not a TOML file: {e}", path.display()),
        )
    })?;
    let mut doc = preferences_doc(dirs)?;
    let (mut values, _) = read_doc(&doc, ALIASES);
    let shipped = defaults();

    // Sort the file's entries into the kinds they belong to.
    let mut leaves = Vec::new();
    collect_leaves(file.as_table(), "", &mut leaves);
    let mut skipped = Vec::new();
    let mut held: Vec<(&'static DefaultsKind, Vec<(String, toml_edit::Item)>)> = Vec::new();
    for (key, item) in leaves {
        let key = canonical(&key, ALIASES).to_owned();
        let kind = kind_of_key(&key).filter(|_| retired(&key).is_none());
        let Some(kind) = kind else {
            skipped.push(format!("{key} is not an Object Default"));
            continue;
        };
        if leaf(&shipped, &key).is_none() {
            skipped.push(format!("{key} is not a field of {}", kind.id));
            continue;
        }
        match held.iter_mut().find(|(k, _)| k.id == kind.id) {
            Some((_, entries)) => entries.push((key, item)),
            None => held.push((kind, vec![(key, item)])),
        }
    }
    held.sort_by_key(|(k, _)| KINDS.iter().position(|o| o.id == k.id));

    // Each record held starts from its shipped values and takes what can be read.
    for (kind, entries) in &held {
        for key in kind.keys() {
            let default = leaf(&shipped, &key).cloned().expect("a key of the record");
            apply(&mut values, &key, default).expect("shipped values are valid");
        }
        for (key, item) in entries {
            let read = to_json(item)
                .ok_or_else(|| "this kind of value is not supported".to_owned())
                .and_then(|v| apply(&mut values, key, v));
            if let Err(why) = read {
                skipped.push(format!("{key} is unreadable ({why})"));
            }
        }
    }

    // One write: each record held is taken out and its changed values put back.
    for (kind, _) in &held {
        for key in kind.keys() {
            for (old, _) in ALIASES.iter().filter(|(_, new)| *new == key) {
                remove_leaf(doc.as_table_mut(), &segments(old));
            }
            remove_leaf(doc.as_table_mut(), &segments(&key));
            let value = leaf(&values, &key).expect("a key of the record");
            if value != leaf(&shipped, &key).expect("a key") || KEPT_AT_DEFAULT.contains(&&*key) {
                set_leaf(doc.as_table_mut(), &segments(&key), to_toml(value)?);
            }
        }
    }
    if !held.is_empty() {
        drop_retired(&mut doc);
        let path = dirs.preferences_file();
        std::fs::create_dir_all(&dirs.config)
            .and_then(|()| crate::fs::write_atomic(&path, doc.to_string().as_bytes()))
            .map_err(|e| CommandError::internal(format!("writing {}: {e}", path.display())))?;
    }
    Ok(LoadedDefaults {
        kinds: held.iter().map(|(k, _)| k.id).collect(),
        skipped,
    })
}
