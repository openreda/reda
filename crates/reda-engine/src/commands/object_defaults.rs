//! `app.defaults.reset`, `app.defaults.save` and `app.defaults.load`: Object Defaults for
//! one kind or all (docs/spec/v0.3/input-and-defaults.md#the-defaults-page).
//!
//! File Commands, as `app.preferences.*` are: they touch the user's `preferences.toml`
//! (and the file saved or loaded), work headless and are never undoable. Single values
//! are read and set with `app.preferences.get` and `app.preferences.set`.

use std::borrow::Cow;
use std::path::PathBuf;

use reda_command::{Command, CommandError, Context, Kind, Meta, Registry, UserDirs};
use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::preferences::{self, DefaultsKind, KINDS, defaults_file};

pub fn register_all(registry: &mut Registry) {
    registry.add(DefaultsReset);
    registry.add(DefaultsSave);
    registry.add(DefaultsLoad);
}

fn user_dirs(cx: &mut Context<'_>) -> Result<UserDirs, CommandError> {
    cx.session()?.user_dirs.clone().ok_or_else(|| {
        CommandError::unavailable("there is no per-user folder; set REDA_HOME to choose one")
    })
}

/// A kind of object with Object Defaults, such as `pcb.line` or `symbol.pin`.
#[derive(Debug, Deserialize)]
#[serde(transparent)]
pub struct KindId(String);

impl JsonSchema for KindId {
    fn schema_name() -> Cow<'static, str> {
        "ObjectDefaultsKind".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        let ids: Vec<&str> = KINDS.iter().map(|k| k.id).collect();
        json_schema!({
            "description": "A kind of object with Object Defaults: its keys are `defaults.<kind>.*`.",
            "type": "string",
            "enum": ids,
        })
    }
}

/// The kind named, or every kind; refuses a name that is not a kind.
fn kind_of(kind: Option<KindId>) -> Result<Option<&'static DefaultsKind>, CommandError> {
    let Some(KindId(id)) = kind else {
        return Ok(None);
    };
    preferences::defaults_kind(&id).map(Some).ok_or_else(|| {
        let ids: Vec<&str> = KINDS.iter().map(|k| k.id).collect();
        CommandError::invalid_params(
            "/kind",
            format!(
                "{id} is not a kind with Object Defaults; use one of {}",
                ids.join(", ")
            ),
        )
    })
}

/// `path`, made absolute against the Project folder.
fn resolve(cx: &mut Context<'_>, path: PathBuf) -> Result<PathBuf, CommandError> {
    if path.is_absolute() {
        return Ok(path);
    }
    match &cx.session()?.location {
        Some(folder) => Ok(folder.join(path)),
        None => Err(CommandError::invalid_params(
            "/path",
            "no Project folder to put a relative path in: give an absolute path",
        )),
    }
}

pub struct DefaultsReset;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ResetParams {
    /// The kind to reset; without one, every kind.
    #[serde(default)]
    pub kind: Option<KindId>,
}

impl Command for DefaultsReset {
    const META: Meta = Meta {
        id: "app.defaults.reset",
        title: "Reset Object Defaults",
        description: "Puts the Object Defaults of one kind (such as `pcb.line`), or of every \
                      kind, back to the shipped values. Remember last used and the rest of \
                      Preferences stay. Returns the keys that changed.",
        category: "app",
        icon: "preferences",
        kind: Kind::File,
    };
    type Params = ResetParams;
    type Output = Value;

    fn run(&self, cx: &mut Context<'_>, p: ResetParams) -> Result<Value, CommandError> {
        let kind = kind_of(p.kind)?;
        let dirs = user_dirs(cx)?;
        let reset = preferences::reset(&dirs, &defaults_file::keys_of(kind))?;
        Ok(json!({ "reset": reset }))
    }
}

pub struct DefaultsSave;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SaveParams {
    /// The TOML file to write: absolute, or relative to the Project folder.
    pub path: PathBuf,
    /// The kind to save; without one, every kind.
    #[serde(default)]
    pub kind: Option<KindId>,
}

impl Command for DefaultsSave {
    const META: Meta = Meta {
        id: "app.defaults.save",
        title: "Save Object Defaults",
        description: "Writes the Object Defaults of one kind, or of every kind, to a TOML file \
                      holding the `defaults.*` table as preferences.toml writes it, every \
                      field included. Returns the kinds and how many values were written.",
        category: "app",
        icon: "preferences",
        kind: Kind::File,
    };
    type Params = SaveParams;
    type Output = Value;

    fn run(&self, cx: &mut Context<'_>, p: SaveParams) -> Result<Value, CommandError> {
        let kind = kind_of(p.kind)?;
        let path = resolve(cx, p.path)?;
        let dirs = user_dirs(cx)?;
        let saved = defaults_file::save(&dirs, &path, kind)?;
        Ok(json!({ "path": path, "kinds": saved.kinds, "entries": saved.entries }))
    }
}

pub struct DefaultsLoad;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LoadParams {
    /// A TOML file of Object Defaults, as `app.defaults.save` writes: absolute, or
    /// relative to the Project folder.
    pub path: PathBuf,
}

impl Command for DefaultsLoad {
    const META: Meta = Meta {
        id: "app.defaults.load",
        title: "Load Object Defaults",
        description: "Replaces the Object Defaults of the kinds a TOML file holds with its \
                      values and leaves the other kinds alone. Entries it does not know or \
                      cannot read are skipped and counted; nothing is written unless the \
                      whole load can be. Returns the kinds replaced, the skipped count and a \
                      warning per skipped entry.",
        category: "app",
        icon: "preferences",
        kind: Kind::File,
    };
    type Params = LoadParams;
    type Output = Value;

    fn run(&self, cx: &mut Context<'_>, p: LoadParams) -> Result<Value, CommandError> {
        let path = resolve(cx, p.path)?;
        let dirs = user_dirs(cx)?;
        let loaded = defaults_file::load(&dirs, &path)?;
        Ok(json!({
            "kinds": loaded.kinds,
            "skipped": loaded.skipped.len(),
            "warnings": loaded.skipped,
        }))
    }
}
