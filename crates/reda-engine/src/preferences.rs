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
pub mod pages;

use std::num::NonZeroU32;

use reda_command::{Length, UserDirs};
use reda_model::{CrossReferenceStyles, LocationStyle, SheetStyle};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub use file::{Loaded, load, parse, reset, set, value_of};

/// Renamed keys: `(old, new)`. A file that still uses an old key is read as the new one,
/// and the next write of that key replaces the old spelling.
pub const ALIASES: &[(&str, &str)] = &[];

/// Every Preference with its default. The serde shape is the file's shape.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Preferences {
    pub general: General,
    /// Selection behavior shared by every canvas.
    pub editing: SelectionEditing,
    pub user: User,
    /// Editing in Schematic Sheets.
    pub schematic: SchematicEditing,
    /// Editing in the PCB Document.
    pub pcb: Editing,
    pub display: Display,
    /// ERC and DRC results on the canvas.
    pub checks: Checks,
    pub libraries: Libraries,
    pub agents: Agents,
    pub keymap: Keymap,
}

/// The identity recorded when a user accepts a Violation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct User {
    /// The author of new Waivers; defaults to the OS user name.
    pub name: String,
}

impl Default for User {
    fn default() -> Self {
        Self {
            name: reda_command::os_user_name(None),
        }
    }
}

/// Resolves the author at the instant a Waiver is created.
pub(crate) fn waiver_author(dirs: Option<&UserDirs>) -> String {
    let name = current(dirs).user.name;
    if name.trim().is_empty() {
        reda_command::os_user_name(dirs)
    } else {
        name
    }
}

/// Selection preferences apply equally to Documents and Library editors.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct SelectionEditing {
    /// A click on empty canvas clears the selection.
    pub click_clears_selection: bool,
    /// A rectangle dragged right to left selects objects it touches.
    pub directional_rectangle: bool,
    /// Click and rectangle skip locked objects. Explicit picks can still select them.
    pub protect_locked_objects: bool,
}

impl Default for SelectionEditing {
    fn default() -> Self {
        Self {
            click_clears_selection: true,
            directional_rectangle: true,
            protect_locked_objects: true,
        }
    }
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
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
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
    /// The snap-grid preset new views start on, such as `50mil` or `0.5mm`; unset, or
    /// not one of the Document type's presets, is the type's own default.
    pub default_grid: Option<Length>,
    /// Zone settings.
    pub zone: ZonePreferences,
    /// The Net block of Track Properties.
    pub net_info: NetInfoPreferences,
}

impl Default for Editing {
    fn default() -> Editing {
        Editing {
            default_units: Units::Mm,
            repeat_placement: true,
            electrical_snap: true,
            auto_pan: true,
            default_grid: None,
            zone: ZonePreferences::default(),
            net_info: NetInfoPreferences::default(),
        }
    }
}

/// The Net block of Track Properties.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct NetInfoPreferences {
    /// The temperature rise in °C the estimated maximum current of a Track allows
    /// (IPC-2221). REDA's own default is 10 °C.
    pub delta_t: f64,
}

impl Default for NetInfoPreferences {
    fn default() -> NetInfoPreferences {
        NetInfoPreferences {
            delta_t: reda_pcb::net_info::DEFAULT_DELTA_T,
        }
    }
}

/// Zone settings of the PCB editor.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct ZonePreferences {
    /// When Zones refill on their own: `after_edit` (after each committed edit, in the
    /// background, the edited Zones and those depending on them) or `off` (only on
    /// Refill). Outputs and DRC refill stale Zones first either way.
    pub auto_refill: AutoRefill,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AutoRefill {
    /// Refill stale Zones in the background after each committed edit.
    #[default]
    AfterEdit,
    /// Refill only when asked.
    Off,
}

/// Editing settings for Schematic Sheets: those of every Document type, and how names
/// are drawn.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct SchematicEditing {
    /// The units new values are shown and typed in.
    pub default_units: Units,
    /// Keep placing copies of an object until the placement is cancelled.
    pub repeat_placement: bool,
    /// Snap the cursor to nearby electrical points, such as pins and wire ends.
    pub electrical_snap: bool,
    /// Pan the view when the cursor reaches the canvas edge during an operation.
    pub auto_pan: bool,
    /// The snap-grid preset new views start on, such as `50mil` or `0.5mm`; unset, or
    /// not one of the presets, is the schematic default.
    pub default_grid: Option<Length>,
    /// Single '\' Negation: a leading backslash draws an overbar over a whole Net Label,
    /// Port or Power Port name, as in `\RESET`. Off, a leading backslash has no effect and
    /// isn't drawn. Only how names are drawn changes, never the names.
    pub single_negation: bool,
    /// Port Cross References Sheet Style (`none`, `name` or `number`), used by Projects
    /// that follow the Preferences for cross-references.
    pub cross_reference_sheet_style: SheetStyle,
    /// Port Cross References Location Style (`none`, `zone` or `xy`), used by Projects
    /// that follow the Preferences for cross-references.
    pub cross_reference_location_style: LocationStyle,
    /// Optimize Wires: overlapping collinear Wires merge, and so do collinear Wires
    /// meeting end to end with nothing else at that point; collinear vertices inside a
    /// Wire are removed. Off, Wires stay as drawn.
    pub optimize_wires: bool,
    /// Components Cut Wires (needs Optimize Wires): a Component dropped with two or more
    /// Pins on one straight stretch of a Wire cuts out the pieces between them, so the
    /// part isn't shorted. Off, the Pins connect and the part may be shorted.
    pub components_cut_wires: bool,
    /// Break Wires At Autojunctions: a Wire splits at each Junction on its interior. Off,
    /// it stays one Wire through the Junction.
    pub break_wires_at_autojunctions: bool,
    /// Convert Cross-Junctions: a Wire that would make a 4-way is offset into two
    /// adjacent 3-ways, one snap grid apart. Off, the 4-way stays as drawn.
    pub convert_cross_junctions: bool,
    /// How a multi-Unit Component's Unit is written after its Designator, from the
    /// Unit's position in the Symbol: `alpha` (`U1A`) or `numeric` (`U1:1`). Only the
    /// drawing changes, never the Designator.
    pub unit_suffix: reda_model::UnitSuffix,
    /// How far repeat placement steps a trailing number in the next name.
    pub auto_increment: AutoIncrement,
}

/// The steps of Auto-increment. Only a trailing number steps, keeping its padding; a
/// negative step counts down and stops at 0.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct AutoIncrement {
    /// The step for Designators, Net Labels and Ports.
    pub primary: i64,
    /// The step for Pin names in the Symbol editor.
    pub secondary: i64,
}

impl Default for AutoIncrement {
    fn default() -> AutoIncrement {
        AutoIncrement {
            primary: 1,
            secondary: 1,
        }
    }
}

impl Default for SchematicEditing {
    fn default() -> SchematicEditing {
        let Editing {
            default_units,
            repeat_placement,
            electrical_snap,
            auto_pan,
            default_grid,
            zone: _,
            net_info: _,
        } = Editing::default();
        SchematicEditing {
            default_units,
            repeat_placement,
            electrical_snap,
            auto_pan,
            default_grid,
            single_negation: false,
            cross_reference_sheet_style: CrossReferenceStyles::DEFAULT.sheet,
            cross_reference_location_style: CrossReferenceStyles::DEFAULT.location,
            optimize_wires: true,
            components_cut_wires: true,
            break_wires_at_autojunctions: true,
            convert_cross_junctions: true,
            unit_suffix: reda_model::UnitSuffix::Alpha,
            auto_increment: AutoIncrement::default(),
        }
    }
}

impl SchematicEditing {
    /// The cross-reference styles a Project that follows the Preferences uses.
    pub fn cross_reference_styles(&self) -> CrossReferenceStyles {
        CrossReferenceStyles {
            sheet: self.cross_reference_sheet_style,
            location: self.cross_reference_location_style,
        }
    }

    /// The `wiring` parameter the app passes to schematic edit Commands: these four
    /// Preferences, with the sheet's snap grid `snap_grid`.
    pub fn wiring(&self, snap_grid: reda_model::geom::Nm) -> reda_schematic::Wiring {
        reda_schematic::Wiring {
            optimize_wires: self.optimize_wires,
            components_cut_wires: self.components_cut_wires,
            break_wires_at_autojunctions: self.break_wires_at_autojunctions,
            convert_cross_junctions: self.convert_cross_junctions,
            snap_grid: Length(snap_grid),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Units {
    Mm,
    Mil,
}

/// How check results show on the canvas.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Checks {
    /// The most Violation markers drawn on one canvas; the rest are counted as "not
    /// shown". Every Violation is still detected and listed.
    pub max_markers: u32,
}

impl Default for Checks {
    fn default() -> Checks {
        Checks { max_markers: 500 }
    }
}

/// Display settings.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Display {
    /// The cursor drawn on the canvas.
    pub cursor: Cursor,
    /// How Layers other than the active one are drawn in a new view.
    pub inactive_layer_mode: InactiveLayerMode,
    /// Board Layer colours that replace the shipped ones, per theme.
    pub layer_colors: ThemeLayerColors,
}

/// Board Layer colour overrides for each theme.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct ThemeLayerColors {
    /// With the dark theme.
    pub dark: LayerColors,
    /// With the light theme.
    pub light: LayerColors,
}

macro_rules! layer_colors {
    ($($(#[$doc:meta])* $name:ident,)*) => {
        /// A colour per Layer kind and side, written `#rrggbb`; unset keeps the shipped
        /// colour.
        #[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
        #[serde(default, deny_unknown_fields)]
        pub struct LayerColors {
            $($(#[$doc])* pub $name: Option<String>,)*
        }

        impl LayerColors {
            /// Every override set, as (key, colour), such as `("copper_top", "#ff8800")`.
            pub fn overrides(&self) -> Vec<(&'static str, &str)> {
                let mut out = Vec::new();
                $(if let Some(c) = &self.$name {
                    out.push((stringify!($name), c.as_str()));
                })*
                out
            }
        }
    };
}

layer_colors! {
    /// Top Copper.
    copper_top,
    /// Bottom Copper.
    copper_bottom,
    /// Top Silkscreen.
    silkscreen_top,
    /// Bottom Silkscreen.
    silkscreen_bottom,
    /// Top Solder Mask.
    solder_mask_top,
    /// Bottom Solder Mask.
    solder_mask_bottom,
    /// Top Paste.
    paste_top,
    /// Bottom Paste.
    paste_bottom,
    /// Top Courtyard.
    courtyard_top,
    /// Bottom Courtyard.
    courtyard_bottom,
    /// Top Assembly.
    assembly_top,
    /// Bottom Assembly.
    assembly_bottom,
    /// The Board Outline.
    board_outline,
    /// The Mechanical Layer.
    mechanical,
    /// The Documentation Layer.
    documentation,
}

/// Whether `text` is a colour written `#rrggbb`.
pub fn is_hex_color(text: &str) -> bool {
    text.len() == 7 && text.starts_with('#') && text[1..].chars().all(|c| c.is_ascii_hexdigit())
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
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Libraries {
    /// Folders of user Libraries.
    pub paths: Vec<String>,
    /// User Library folders, as listed in `paths`, whose Active checkbox is off: they are
    /// not scanned or searched, so Components placed from them read Source Missing.
    pub inactive: Vec<String>,
    /// Whether the Starter Library is active: scanned and searched.
    pub starter_active: bool,
    /// How the Library check reports each of its checks: Error, Warning or Off. It never
    /// blocks a save.
    pub severities: reda_model::LibrarySeverities,
}

impl Default for Libraries {
    fn default() -> Libraries {
        Libraries {
            paths: vec![],
            inactive: vec![],
            starter_active: true,
            severities: reda_model::LibrarySeverities::default(),
        }
    }
}

impl Libraries {
    /// The user Library folders whose Active checkbox is on, in the listed order.
    pub fn active_paths(&self) -> impl Iterator<Item = &String> {
        self.paths.iter().filter(|p| !self.inactive.contains(p))
    }
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
        let themes = &self.display.layer_colors;
        for (theme, colors) in [("dark", &themes.dark), ("light", &themes.light)] {
            for (_, color) in colors.overrides() {
                if !is_hex_color(color) {
                    return Err((
                        if theme == "dark" {
                            "display.layer_colors.dark"
                        } else {
                            "display.layer_colors.light"
                        },
                        format!("{color} is not a colour written #rrggbb"),
                    ));
                }
            }
        }
        Ok(())
    }
}

/// The JSON schema (draft 2020-12) of [`Preferences`], for the Preferences dialog and
/// agents.
pub fn schema() -> Value {
    serde_json::to_value(schemars::schema_for!(Preferences)).expect("schemas are JSON")
}

/// Every Preference key, dotted (`schematic.repeat_placement`).
pub fn keys() -> Vec<String> {
    fn leaves(value: &Value, prefix: String, out: &mut Vec<String>) {
        match value {
            Value::Object(map) => {
                for (name, child) in map {
                    let key = if prefix.is_empty() {
                        name.clone()
                    } else {
                        format!("{prefix}.{name}")
                    };
                    leaves(child, key, out);
                }
            }
            _ => out.push(prefix),
        }
    }
    let defaults = serde_json::to_value(Preferences::default()).expect("Preferences serialise");
    let mut keys = Vec::new();
    leaves(&defaults, String::new(), &mut keys);
    keys
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

/// The Library check severities of the session's user, reading the file now; the
/// defaults with no per-user folder.
pub fn library_severities(session: &reda_command::Session) -> reda_model::LibrarySeverities {
    current(session.user_dirs.as_ref()).libraries.severities
}

/// The Preferences for `dirs`, reading the file now. A front end with no per-user folder
/// uses the defaults.
pub fn current(dirs: Option<&UserDirs>) -> Preferences {
    dirs.map(|d| load(d).values).unwrap_or_default()
}
