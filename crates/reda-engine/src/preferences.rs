//! Preferences: one user's settings, kept in `preferences.toml` in the config folder.
//!
//! The file holds only the values that differ from the defaults, and REDA edits it with
//! `toml_edit` so the user's comments survive. [`Preferences`] is the schema: its serde
//! shape gives the keys (`section.name`, such as `schematic.repeat_placement`) and its
//! `schemars` schema gives the types, defaults and descriptions. An unknown or invalid key
//! warns and falls back to its default; renamed keys go through [`ALIASES`] instead of a
//! format migration chain.
//!
//! Every read and every write goes back to the file, so other instances' edits are seen
//! and kept. Front ends reach the values through the `app.preferences.get` and
//! `app.preferences.set` Commands, or in-process through [`load`].

mod file;

use std::num::NonZeroU32;

use reda_command::UserDirs;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub use file::{Loaded, load, parse, set, value_of};

/// Renamed keys: `(old, new)`. A file that still uses an old key is read as the new one,
/// and the next write of that key replaces the old spelling.
pub const ALIASES: &[(&str, &str)] = &[];

/// Every Preference with its default. The serde shape is the file's shape.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Preferences {
    pub general: General,
    /// Editing in Schematic Sheets.
    pub schematic: Editing,
    /// Editing in the PCB Document.
    pub pcb: Editing,
    pub display: Display,
    pub libraries: Libraries,
    pub agents: Agents,
    pub keymap: Keymap,
}

/// General settings.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct General {
    /// The colour theme.
    pub theme: Theme,
    /// Scales the whole user interface, from 0.5 to 3.
    pub ui_scale: f64,
    /// Reopen the last Project when REDA starts.
    pub reopen_last_project: bool,
    /// The most undo steps kept per Document. Unset means unlimited.
    pub undo_depth: Option<NonZeroU32>,
    /// The Saved Layout new Projects start from. Unset means REDA's Default Layout.
    pub default_layout: Option<String>,
}

impl Default for General {
    fn default() -> General {
        General {
            theme: Theme::FollowOs,
            ui_scale: 1.0,
            reopen_last_project: true,
            undo_depth: None,
            default_layout: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    FollowOs,
    Dark,
    Light,
}

/// Editing settings for one Document type.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Editing {
    /// The units new values are shown and typed in.
    pub default_units: Units,
    /// Keep placing copies of an object until the placement is cancelled.
    pub repeat_placement: bool,
    /// Snap the cursor to nearby electrical points, such as pins and wire ends.
    pub electrical_snap: bool,
    /// Pan the view when the cursor reaches the canvas edge during an operation.
    pub auto_pan: bool,
}

impl Default for Editing {
    fn default() -> Editing {
        Editing {
            default_units: Units::Mm,
            repeat_placement: true,
            electrical_snap: true,
            auto_pan: true,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Units {
    Mm,
    Mil,
}

/// Display settings.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Display {
    /// The cursor drawn on the canvas.
    pub cursor: Cursor,
    /// How Layers other than the active one are drawn in a new view.
    pub inactive_layer_mode: InactiveLayerMode,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Cursor {
    /// A small crosshair.
    #[default]
    Small,
    /// A crosshair across the whole canvas.
    FullScreen,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum InactiveLayerMode {
    #[default]
    Normal,
    Dimmed,
    Hidden,
}

/// The user's Libraries.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Libraries {
    /// Folders of user Libraries.
    pub paths: Vec<String>,
}

/// The agent bridge.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Agents {
    /// Let agents connect to this instance through the control port.
    pub bridge: bool,
}

impl Default for Agents {
    fn default() -> Agents {
        Agents { bridge: true }
    }
}

/// Keyboard shortcuts. The bindings themselves live in `keymap.toml`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Keymap {
    /// The Keymap the user's bindings are laid over.
    pub base: KeymapBase,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum KeymapBase {
    #[default]
    Reda,
}

impl Preferences {
    /// Range rules serde cannot express. Returns the offending key and why.
    fn check(&self) -> Result<(), (&'static str, String)> {
        let scale = self.general.ui_scale;
        if !(0.5..=3.0).contains(&scale) {
            return Err(("general.ui_scale", format!("{scale} is outside 0.5 to 3")));
        }
        Ok(())
    }
}

/// The JSON schema (draft 2020-12) of [`Preferences`], for the Preferences dialog and
/// agents.
pub fn schema() -> Value {
    serde_json::to_value(schemars::schema_for!(Preferences)).expect("schemas are JSON")
}

/// The schema's description of one dotted key, if it has one.
pub fn describe(key: &str) -> Option<String> {
    let schema = schema();
    let resolve = |node: &Value| -> Value {
        match node["$ref"]
            .as_str()
            .and_then(|r| r.strip_prefix("#/$defs/"))
        {
            Some(name) => schema["$defs"][name].clone(),
            None => node.clone(),
        }
    };
    let mut node = schema.clone();
    for segment in key.split('.') {
        node = resolve(&node)["properties"][segment].clone();
    }
    let own = node["description"].as_str().map(str::to_owned);
    own.or_else(|| resolve(&node)["description"].as_str().map(str::to_owned))
}

/// The Preferences for `dirs`, reading the file now. A front end with no per-user folder
/// uses the defaults.
pub fn current(dirs: Option<&UserDirs>) -> Preferences {
    dirs.map(|d| load(d).values).unwrap_or_default()
}
