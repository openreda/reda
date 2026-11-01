//! `app.preferences.get` and `app.preferences.set`.
//!
//! Both are file Commands: they touch the user's `preferences.toml`, work headless, are
//! never undoable and never dirty the Project. Changes apply at once.

use reda_command::{Command, CommandError, Context, Kind, Meta, Registry, UserDirs};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::preferences::{self, Preferences};

pub fn register_all(registry: &mut Registry) {
    registry.add(PreferencesGet);
    registry.add(PreferencesSet);
}

fn user_dirs(cx: &mut Context<'_>) -> Result<UserDirs, CommandError> {
    cx.session()?.user_dirs.clone().ok_or_else(|| {
        CommandError::unavailable("there is no per-user folder; set REDA_HOME to choose one")
    })
}

fn unknown_key(key: &str) -> CommandError {
    CommandError::invalid_params("/key", format!("{key} is not a Preference"))
}

pub struct PreferencesGet;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GetParams {
    /// A dotted key such as `schematic.repeat_placement`. Without one, every Preference.
    #[serde(default)]
    pub key: Option<String>,
}

impl Command for PreferencesGet {
    const META: Meta = Meta {
        id: "app.preferences.get",
        title: "Get Preferences",
        description: "Reads the user's Preferences: one key with its default and description, \
                      or every value with warnings for keys in preferences.toml that were ignored.",
        category: "app",
        icon: "preferences",
        kind: Kind::File,
    };
    type Params = GetParams;
    type Output = Value;

    fn run(&self, cx: &mut Context<'_>, p: GetParams) -> Result<Value, CommandError> {
        let loaded = preferences::load(&user_dirs(cx)?);
        let Some(key) = p.key else {
            return Ok(json!({ "values": loaded.values, "warnings": loaded.warnings }));
        };
        let (key, value) =
            preferences::value_of(&loaded.values, &key).ok_or_else(|| unknown_key(&key))?;
        let (_, default) =
            preferences::value_of(&Preferences::default(), key).expect("the same key");
        Ok(json!({
            "key": key,
            "value": value,
            "default": default,
            "description": preferences::describe(key),
        }))
    }
}

pub struct PreferencesSet;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetParams {
    /// A dotted key such as `schematic.repeat_placement`.
    pub key: String,
    /// The new value; `null` resets the key to its default.
    pub value: Value,
}

impl Command for PreferencesSet {
    const META: Meta = Meta {
        id: "app.preferences.set",
        title: "Set Preference",
        description: "Sets one Preference in the user's preferences.toml, keeping every other \
                      line and comment. Null, or the default value, removes the key.",
        category: "app",
        icon: "preferences",
        kind: Kind::File,
    };
    type Params = SetParams;
    type Output = Value;

    fn run(&self, cx: &mut Context<'_>, p: SetParams) -> Result<Value, CommandError> {
        let dirs = user_dirs(cx)?;
        let value = preferences::set(&dirs, &p.key, p.value)?;
        let key = preferences::value_of(&Preferences::default(), &p.key)
            .map(|(key, _)| key)
            .ok_or_else(|| unknown_key(&p.key))?;
        Ok(json!({ "key": key, "value": value }))
    }
}
