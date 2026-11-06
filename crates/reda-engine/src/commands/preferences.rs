//! `app.preferences.get`, `app.preferences.set` and `app.preferences.reset`.
//!
//! Both are file Commands: they touch the user's `preferences.toml`, work headless, are
//! never undoable and never dirty the Project. Changes apply at once: setting
//! `general.undo_depth` caps the undo stacks of this engine straight away.

use reda_command::{Command, CommandError, Context, Kind, Meta, Registry, UserDirs};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::preferences::{self, Preferences};

pub fn register_all(registry: &mut Registry) {
    registry.add(PreferencesGet);
    registry.add(PreferencesSet);
    registry.add(PreferencesReset);
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
        if key == "general.undo_depth" {
            apply_undo_depth(cx, &dirs)?;
        }
        Ok(json!({ "key": key, "value": value }))
    }
}

/// Applies the undo depth at once: the undo stacks take the new cap (trimming when lower).
fn apply_undo_depth(cx: &mut Context<'_>, dirs: &UserDirs) -> Result<(), CommandError> {
    let depth = preferences::load(dirs).values.general.undo_depth;
    cx.session()?
        .history
        .set_depth(depth.map(|d| d.get() as usize));
    Ok(())
}

pub struct PreferencesReset;

/// Exactly one of `keys`, `page` and `all`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ResetParams {
    /// Dotted keys to put back to their defaults.
    #[serde(default)]
    pub keys: Option<Vec<String>>,
    /// A page of the Preferences tree, such as `editing` or `schematic.wires`: every key
    /// shown on it.
    #[serde(default)]
    pub page: Option<String>,
    /// With `page`: the page's whole branch, its sub-pages too ("Reset branch").
    #[serde(default)]
    pub branch: bool,
    /// `true` resets every Preference ("Reset all").
    #[serde(default)]
    pub all: Option<bool>,
}

impl Command for PreferencesReset {
    const META: Meta = Meta {
        id: "app.preferences.reset",
        title: "Reset Preferences",
        description: "Puts Preferences back to their defaults: the given keys, one page of the \
                      Preferences tree (or its branch), or all. Other lines and comments in \
                      preferences.toml stay. Returns the keys that changed.",
        category: "app",
        icon: "preferences",
        kind: Kind::File,
    };
    type Params = ResetParams;
    type Output = Value;

    fn run(&self, cx: &mut Context<'_>, p: ResetParams) -> Result<Value, CommandError> {
        let keys = match (p.keys, p.page, p.all) {
            (Some(keys), None, None) => {
                let known = preferences::keys();
                for (i, key) in keys.iter().enumerate() {
                    let canonical =
                        preferences::value_of(&Preferences::default(), key).map(|(k, _)| k);
                    if !canonical.is_some_and(|k| known.iter().any(|n| n == k)) {
                        return Err(CommandError::invalid_params(
                            &format!("/keys/{i}"),
                            format!("{key} is not a Preference"),
                        ));
                    }
                }
                keys
            }
            (None, Some(page), None) => {
                if preferences::pages::page(&page).is_none() {
                    return Err(CommandError::invalid_params(
                        "/page",
                        format!("{page} is not a Preferences page"),
                    ));
                }
                preferences::pages::keys_on(&page, p.branch)
            }
            (None, None, Some(true)) => preferences::keys(),
            (None, None, Some(false)) => {
                return Err(CommandError::invalid_params(
                    "/all",
                    "give all: true, or keys or a page instead",
                ));
            }
            _ => {
                return Err(CommandError::invalid_params(
                    "",
                    "give exactly one of keys, page and all",
                ));
            }
        };
        let dirs = user_dirs(cx)?;
        let reset = preferences::reset(&dirs, &keys)?;
        if reset.iter().any(|k| k == "general.undo_depth") {
            apply_undo_depth(cx, &dirs)?;
        }
        Ok(json!({ "reset": reset }))
    }
}
