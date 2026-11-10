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

mod checks;
mod file;
mod grid;
pub mod pages;

use std::collections::BTreeSet;
use std::num::NonZeroU32;

use reda_command::{Length, UserDirs};
use reda_model::{CrossReferenceStyles, LocationStyle, SheetStyle};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub use checks::{CheckDisplay, CheckDisplays, OVERLAY_BY_DEFAULT, OverlayStyle, OverlayZoomedOut};
pub use file::{Loaded, load, parse, reset, set, value_of};
pub use grid::{DisplayPrecision, GridPreferences, GridRow, GridStyle, GridTables, HudPreferences};

/// Renamed keys: `(old, new)`. A file that still uses an old key is read as the new one,
/// and the next write of that key replaces the old spelling.
pub const ALIASES: &[(&str, &str)] = &[];

/// Every Preference with its default. The serde shape is the file's shape.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Preferences {
    pub general: General,
    /// Editing shared by every canvas: selection, grids, the HUD and display precision.
    pub editing: SelectionEditing,
    pub user: User,
    /// Editing in Schematic Sheets.
    pub schematic: SchematicEditing,
    /// Editing in the PCB Document.
    pub pcb: Editing,
    pub display: Display,
    /// Cross-select between the sheets and the board.
    pub cross_select: CrossSelect,
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

/// Editing preferences that apply equally to Documents and Library editors.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct SelectionEditing {
    /// A click on empty canvas clears the selection.
    pub click_clears_selection: bool,
    /// A rectangle dragged right to left selects objects it touches.
    pub directional_rectangle: bool,
    /// Click and rectangle skip locked objects. Explicit picks can still select them.
    pub protect_locked_objects: bool,
    /// Protect Locked Primitives In Component: on, a placed Footprint's graphics can be
    /// edited on the board only once they are unlocked; off, the GUI edits them anyway.
    pub protect_locked_primitives: bool,
    /// Object's Electrical Hot Spot: a moved electrical object is held at its nearest
    /// connection point (a Pin end, a Wire end, a label's anchor). Overrides Center of
    /// Object for it.
    pub grab_hot_spot: bool,
    /// Center of Object: a moved object is held by its reference point or centre. With
    /// neither grab Preference, it is held where it was clicked.
    pub grab_center: bool,
    /// Grid presets per Document type and the visible grid.
    pub grid: GridPreferences,
    /// The HUD by the cursor.
    pub hud: HudPreferences,
    /// How many decimals coordinates and lengths show with.
    pub display_precision: DisplayPrecision,
    /// Auto-pan: how a canvas pans when the cursor reaches its edge during an operation.
    pub auto_pan: AutoPan,
    /// How a result is shown: Messages, Find, Select Similar, Show and Cross-probe.
    pub highlight: HighlightPreferences,
}

impl Default for SelectionEditing {
    fn default() -> Self {
        Self {
            click_clears_selection: true,
            directional_rectangle: true,
            protect_locked_objects: true,
            protect_locked_primitives: true,
            grab_hot_spot: true,
            grab_center: false,
            grid: GridPreferences::default(),
            hud: HudPreferences::default(),
            display_precision: DisplayPrecision::default(),
            auto_pan: AutoPan::default(),
            highlight: HighlightPreferences::default(),
        }
    }
}

/// How a canvas pans when the cursor reaches its edge during an operation
/// (docs/spec/v0.1/editing.md#navigation).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct AutoPan {
    /// Auto-pan at the canvas edge: `off`, `ballistic` (faster the further past the
    /// edge), `fixed_size_jump` (a steady speed) or `re_center` (centres the view on the
    /// cursor).
    pub style: AutoPanStyle,
    /// The pan speed in pixels per second; Shift held pans faster.
    pub speed: f64,
}

impl Default for AutoPan {
    fn default() -> AutoPan {
        AutoPan {
            style: AutoPanStyle::Ballistic,
            speed: 1200.0,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AutoPanStyle {
    Off,
    #[default]
    Ballistic,
    FixedSizeJump,
    ReCenter,
}

/// The shared highlight Preference (docs/spec/v0.1/editing.md#highlighting): how
/// Messages, Find, Select Similar, Show from a row and Cross-probe show a result.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct HighlightPreferences {
    /// Select the result.
    pub select: bool,
    /// Zoom to the result.
    pub zoom: bool,
    /// How close the zoom comes, from 0 (fit with room around) to 1 (tight).
    pub closeness: f64,
    /// Dim everything else.
    pub dim: bool,
    /// How strongly dimmed objects fade into the background, from 0 to 1. Cross-select
    /// uses it too.
    pub dim_strength: f64,
    /// How strongly masked objects fade into the background, from 0 to 1.
    pub mask_strength: f64,
}

impl Default for HighlightPreferences {
    fn default() -> HighlightPreferences {
        HighlightPreferences {
            select: true,
            zoom: true,
            closeness: 0.6,
            dim: true,
            dim_strength: 0.6,
            mask_strength: 0.75,
        }
    }
}

/// General settings.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct General {
    /// The colour theme.
    pub theme: Theme,
    /// Scales the whole user interface: one of the steps in [`UI_SCALES`].
    #[schemars(schema_with = "ui_scale_schema")]
    pub ui_scale: f64,
    /// Reopen the last Project when REDA starts.
    pub reopen_last_project: bool,
    /// The most undo steps kept per Document. Unset means unlimited.
    pub undo_depth: Option<NonZeroU32>,
    /// The Saved Layout new Projects start from. Unset means REDA's Default Layout.
    pub default_layout: Option<String>,
}

/// The UI scale steps, 75 to 200 %: the only values `general.ui_scale` takes.
pub const UI_SCALES: [f64; 7] = [0.75, 0.85, 1.0, 1.15, 1.25, 1.5, 2.0];

/// The step of [`UI_SCALES`] nearest `scale`.
pub fn nearest_ui_scale(scale: f64) -> f64 {
    UI_SCALES
        .into_iter()
        .min_by(|a, b| (a - scale).abs().total_cmp(&(b - scale).abs()))
        .expect("there are steps")
}

/// The next UI scale step up (`up`) or down from `scale`, staying at the ends.
pub fn step_ui_scale(scale: f64, up: bool) -> f64 {
    let at = UI_SCALES
        .iter()
        .position(|s| *s == nearest_ui_scale(scale))
        .expect("a step");
    let next = if up {
        (at + 1).min(UI_SCALES.len() - 1)
    } else {
        at.saturating_sub(1)
    };
    UI_SCALES[next]
}

fn ui_scale_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    schemars::json_schema!({
        "type": "number",
        "enum": UI_SCALES,
        "description": "Scales the whole user interface: 75, 85, 100, 115, 125, 150 or 200 %.",
    })
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

/// A kind of point the cursor snaps to. Boards and Footprints offer the first eight,
/// sheets and Symbols the rest.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Hotspot {
    /// A Pad's centre (its anchor).
    PadCenter,
    /// A Via's centre.
    ViaCenter,
    /// Either end of a straight Track.
    TrackEnd,
    /// Either end of an arc, Track or graphic.
    ArcEnd,
    /// The centre of an arc, Track or graphic.
    ArcCenter,
    /// A vertex of the Board Outline, a Board Cutout or a drawn line or rectangle.
    OutlineVertex,
    /// A Footprint's origin.
    FootprintOrigin,
    /// A text's anchor.
    TextAnchor,
    /// A Pin's connecting end.
    PinEnd,
    /// Either end of a Wire.
    WireEnd,
    /// A corner inside a Wire.
    WireVertex,
    /// Anywhere along a Wire, on the grid where the Wire runs straight across it.
    Wire,
    /// A Port's or Power Port's anchor.
    PortAnchor,
    /// A Net Label's anchor.
    LabelAnchor,
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
    /// The default object snap mode: on, new views snap to Hotspots on the active Layer
    /// (through-hole Pads and Vias count on every copper Layer); off, they start with
    /// object snap Off.
    pub electrical_snap: bool,
    /// Snap Distance: how close the cursor must come to a Hotspot, on both axes, before
    /// it snaps there. Also used in the Footprint editor.
    pub snap_distance: Length,
    /// The Hotspot kinds new views leave out of the checklist, such as `text_anchor`:
    /// `pad_center`, `via_center`, `track_end`, `arc_end`, `arc_center`,
    /// `outline_vertex`, `footprint_origin` and `text_anchor`.
    pub hotspots_off: BTreeSet<Hotspot>,
    /// The snap-grid preset new views start on, such as `50mil` or `0.5mm`; unset, or
    /// not one of the Document type's presets, is the type's own default.
    pub default_grid: Option<Length>,
    /// Zone settings.
    pub zone: ZonePreferences,
    /// The Keepout and Zone Cutout Tools.
    pub keepout: KeepoutPreferences,
    /// The arc Tool.
    pub arc: ArcPreferences,
    /// The Net block of Track Properties.
    pub net_info: NetInfoPreferences,
    /// Interactive Routing: the Track Tool's values.
    pub route: RoutePreferences,
    /// Dragging Tracks.
    pub drag: DragPreferences,
    /// Placement: how Footprint moves behave.
    pub place: PlacePreferences,
    /// The Ratsnest.
    pub ratsnest: RatsnestPreferences,
    /// The board view: Net colours on copper and saved Layer sets.
    pub view: BoardViewPreferences,
}

/// The Ratsnest.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct RatsnestPreferences {
    /// Smart Track Ends: a partly routed connection's line starts at the Track end, not
    /// at the nearest Pad. On by default; it can slow high-density boards.
    pub smart_track_ends: bool,
}

impl Default for RatsnestPreferences {
    fn default() -> RatsnestPreferences {
        RatsnestPreferences {
            smart_track_ends: true,
        }
    }
}

/// The board view's Preferences.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct BoardViewPreferences {
    /// How Net colours lie over copper under `F5`: `solid`, or `checker` (alternate
    /// cells, the Layer colour between).
    pub net_color_pattern: NetColorPattern,
    /// What copper shows when zoomed out under `F5`: the `net_color` stays, or the
    /// `layer_color` comes back.
    pub net_color_zoom_out: NetColorZoomOut,
    /// Layer sets the user saved, each a name and its Layers by kind and side (such as
    /// `copper_top`), so a set applies to any board.
    pub layer_sets: Vec<SavedLayerSet>,
    /// How the Layer control shows on the canvas: a `swatch` at the top right (a chip
    /// of the active Layer, opening a list), or a `strip` of chips along the bottom.
    pub layer_control: LayerControl,
    /// Live hover: an outline around the Net under the cursor, `off`, `on`, or only
    /// while Shift is held (`shift_only`).
    pub live_hover: LiveHover,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LayerControl {
    #[default]
    Swatch,
    Strip,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LiveHover {
    Off,
    On,
    #[default]
    ShiftOnly,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum NetColorPattern {
    #[default]
    Solid,
    Checker,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum NetColorZoomOut {
    #[default]
    NetColor,
    LayerColor,
}

/// A user-saved Layer set.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SavedLayerSet {
    pub name: String,
    /// The Layers shown, by kind and side, such as `copper_top` or `board_outline`.
    pub layers: Vec<String>,
}

impl Default for Editing {
    fn default() -> Editing {
        Editing {
            default_units: Units::Mm,
            repeat_placement: true,
            electrical_snap: true,
            snap_distance: Length(reda_model::geom::Nm(8 * 25_400)),
            hotspots_off: BTreeSet::new(),
            default_grid: None,
            zone: ZonePreferences::default(),
            keepout: KeepoutPreferences::default(),
            arc: ArcPreferences::default(),
            net_info: NetInfoPreferences::default(),
            route: RoutePreferences::default(),
            drag: DragPreferences::default(),
            ratsnest: RatsnestPreferences::default(),
            view: BoardViewPreferences::default(),
            place: PlacePreferences::default(),
        }
    }
}

/// Placement: how Footprint moves behave (docs/spec/v0.1/pcb.md#moving-footprints).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct PlacePreferences {
    /// What a Footprint move does at other Footprints (`R`): `ignore` (goes anywhere,
    /// overlaps in the violation colour) or `avoid` (stops at the last position that keeps
    /// `courtyard_clearance`).
    pub obstacle_mode: reda_pcb::place::ObstacleMode,
    /// Guides to neighbours' Courtyard edges and Pad centres during a Footprint move.
    pub alignment_guides: bool,
    /// The Ratsnest during a Footprint move (`N`): `breaks`, `hidden` or `pad_to_pad`.
    pub ratsnest_mode: reda_pcb::place::RatsnestMode,
}

impl Default for PlacePreferences {
    fn default() -> PlacePreferences {
        PlacePreferences {
            obstacle_mode: reda_pcb::place::ObstacleMode::Ignore,
            alignment_guides: true,
            ratsnest_mode: reda_pcb::place::RatsnestMode::Breaks,
        }
    }
}

/// Dragging Tracks (docs/spec/v0.1/pcb.md#dragging-tracks). The drag Commands take
/// these as explicit values; the Tool passes them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct DragPreferences {
    /// Angle keeping (on): a slid segment's neighbours keep their lines and angles. Off,
    /// the segment moves freely and its neighbours' ends follow.
    pub preserve_angle: bool,
}

impl Default for DragPreferences {
    fn default() -> DragPreferences {
        DragPreferences {
            preserve_angle: true,
        }
    }
}

/// Interactive Routing: the Track Tool's values.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct RoutePreferences {
    /// What a route does at copper in its way (`Shift+R`): `stop` (clipped at the last
    /// clear point), `ignore` (goes anywhere, drawn in the violation colour) or
    /// `highlight` (goes anywhere and highlights what it breaks the rules with).
    pub obstacle_mode: reda_pcb::route::ObstacleMode,
    /// A route started on a Track takes that Track's width.
    pub pickup_width: bool,
    /// Which of the Net's widths a route takes (`3`): `min`, `preferred`, `max` or
    /// `user`, the last chosen value.
    pub width_mode: reda_pcb::route::WidthMode,
    /// Widths the width chooser (`Shift+W`) lists after the Net's own.
    pub favourite_widths: Vec<Length>,
    /// A route that joins two points already joined deletes the old path (`Shift+L`).
    pub loop_removal: bool,
    /// Loop removal takes dangling stubs on the old path too, unless they end in a Via.
    pub remove_antennas: bool,
    /// Loop removal takes an old path through Vias, with its Vias.
    pub remove_loops_through_vias: bool,
    /// Reaching a same-Net Pad, Via or Track ends the route (`Shift+A`). Off, a re-route
    /// can run along an existing path.
    pub auto_terminate: bool,
    /// Other-Net copper grown by its clearance, shaded while routing (`C`): `off`,
    /// `near` (within 5 mm of the cursor) or `all`.
    pub clearance_display: ClearanceDisplay,
}

/// Which clearance halos the Track Tool shades. Kept in `pcb.route.clearance_display`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ClearanceDisplay {
    Off,
    /// Copper within 5 mm of the cursor.
    #[default]
    Near,
    All,
}

impl ClearanceDisplay {
    /// The value `C` goes to next.
    pub fn next(self) -> ClearanceDisplay {
        match self {
            ClearanceDisplay::Off => ClearanceDisplay::Near,
            ClearanceDisplay::Near => ClearanceDisplay::All,
            ClearanceDisplay::All => ClearanceDisplay::Off,
        }
    }
}

impl Default for RoutePreferences {
    fn default() -> RoutePreferences {
        RoutePreferences {
            obstacle_mode: reda_pcb::route::ObstacleMode::Stop,
            pickup_width: true,
            width_mode: reda_pcb::route::WidthMode::Preferred,
            favourite_widths: Vec::new(),
            loop_removal: true,
            remove_antennas: true,
            remove_loops_through_vias: true,
            auto_terminate: true,
            clearance_display: ClearanceDisplay::Near,
        }
    }
}

impl RoutePreferences {
    /// The loop-removal switches as `propose` and `commit` take them.
    pub fn loops(&self) -> reda_pcb::route::LoopOptions {
        reda_pcb::route::LoopOptions {
            enabled: self.loop_removal,
            antennas: self.remove_antennas,
            through_vias: self.remove_loops_through_vias,
        }
    }

    /// The favourite widths in nm.
    pub fn favourites(&self) -> Vec<reda_model::geom::Nm> {
        self.favourite_widths.iter().map(|l| l.0).collect()
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
    /// The Zone Tool's values for the next Zone it draws, as last set in its options in
    /// Properties. `pcb.zone.add` itself never reads them.
    pub tool: ZoneToolPreferences,
}

/// The values the Zone Tool gives a new Zone (docs/spec/v0.1/pcb.md#creating-zones).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct ZoneToolPreferences {
    /// The gap to other Nets' copper, where it is larger than the clearance rule's;
    /// unset, the rule alone.
    pub clearance: Option<Length>,
    /// The narrowest copper the fill may leave; unset, the Net's `track_width` minimum.
    pub min_width: Option<Length>,
    /// How the fill treats copper of its own Net: `zones_only`, `all` or `none`.
    pub pour_over: reda_model::PourOver,
    /// Remove fill pieces that touch no copper of the Zone's Net.
    pub remove_unconnected: bool,
    /// Fill pieces smaller than this area are dropped, such as `0.5mm2`.
    pub min_island_area: reda_command::AreaValue,
}

impl Default for ZoneToolPreferences {
    fn default() -> ZoneToolPreferences {
        ZoneToolPreferences {
            clearance: None,
            min_width: None,
            pour_over: reda_model::PourOver::default(),
            remove_unconnected: true,
            min_island_area: reda_command::AreaValue(0),
        }
    }
}

/// The Keepout Tool's options, as last set in Properties.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct KeepoutPreferences {
    /// How the Keepout Tool draws: `polygon` (a click per corner), `rectangle` (two
    /// corners) or `circle` (centre, then a point on the edge). All store polygons.
    pub shape: KeepoutToolShape,
    /// Keep out on every copper Layer, including any added later; off, only the active
    /// copper Layer.
    pub all_layers: bool,
    /// What a new Keepout keeps out: any of `tracks`, `vias`, `zones`, `smd_pads`,
    /// `tht_pads` and `footprints`.
    pub restrict: BTreeSet<reda_model::Restriction>,
}

impl Default for KeepoutPreferences {
    fn default() -> KeepoutPreferences {
        KeepoutPreferences {
            shape: KeepoutToolShape::default(),
            all_layers: true,
            restrict: reda_model::Restriction::defaults(),
        }
    }
}

/// The arc Tool's options, as last set in Properties.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct ArcPreferences {
    /// How the arc Tool places: `edge` (both ends, then a point it passes through),
    /// `center` (the centre, the start, then the end's direction) or `circle` (the
    /// centre, then a point on it). All store arcs.
    pub mode: ArcToolMode,
}

/// How the arc Tool places.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ArcToolMode {
    /// Both ends, then a point the arc passes through.
    #[default]
    Edge,
    /// The centre, the start (which sets the radius), then the end's direction,
    /// counter-clockwise from the start.
    Center,
    /// The centre, then a point on the circle: a full circle, stored as two half arcs.
    Circle,
}

/// How the Keepout Tool draws.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum KeepoutToolShape {
    /// A click per corner; a click on the first, or Enter, closes it.
    #[default]
    Polygon,
    /// Two corners.
    Rectangle,
    /// The centre, then a point on the edge.
    Circle,
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
    /// The default object snap mode: on, new views snap to Hotspots (Pin ends, Wire
    /// ends and the like) within the grid preset's Electrical Range; off, they start
    /// with object snap Off.
    pub electrical_snap: bool,
    /// The Hotspot kinds new views leave out of the checklist: `pin_end`, `wire_end`,
    /// `wire_vertex`, `wire` (anywhere along a Wire), `port_anchor` and `label_anchor`.
    pub hotspots_off: BTreeSet<Hotspot>,
    /// The snap-grid preset new views start on, such as `50mil` or `0.5mm`; unset, or
    /// not one of the presets, is the schematic default.
    pub default_grid: Option<Length>,
    /// Single '\' Negation: a leading backslash draws an overbar over a whole Net Label,
    /// Port or Power Port name, as in `\RESET`. Off, a leading backslash has no effect and
    /// isn't drawn. Only how names are drawn changes, never the names.
    pub single_negation: bool,
    /// Display Cross-Overs: a Wire hops over each Wire it crosses without connecting, in
    /// a small bridge. Canvas only: it never reaches the PDF.
    pub display_cross_overs: bool,
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
    /// The Break Wire Tool's Cutting Length and Display Preferences. The Tool writes its
    /// changes straight back here.
    pub break_wire: BreakWirePreferences,
    /// Clipboard Reference: cut and copy ask for a click on the reference point a paste
    /// then holds. Off, it is the lower-left corner of what was copied.
    pub clipboard_reference: bool,
    /// In-Place Editing: F2 or a slow second click edits a name, Designator, value,
    /// visible field or free text on the canvas.
    pub in_place_editing: bool,
    /// Display When Dragging: a drag or move shows the Junctions it would make and a chip
    /// by the cursor saying whether Nets would change ("joins GND and VCC").
    pub display_when_dragging: bool,
}

/// The Break Wire Tool (docs/spec/v0.1/schematic.md#break-wire).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct BreakWirePreferences {
    /// Cutting Length (Space cycles it in the Tool): `segment` (Snap To Segment, the
    /// whole segment), `grid_multiple` (Snap Grid Size Multiple) or `fixed` (Fixed
    /// Length). The cutter shrinks to fit a shorter segment.
    pub cutting_length: CuttingLength,
    /// How many snap grids long the cut is with `grid_multiple`: 2 to 10.
    pub grid_multiple: u32,
    /// How long the cut is with `fixed`: 10 default units of 10 mil (100 mil) at first.
    pub fixed_length: Length,
    /// Show Cutter Box: `never`, `always` or `on_wire` (only while over a Wire).
    pub show_cutter_box: BreakWireShow,
    /// Show Extremity Markers at the cut's two ends: `never`, `always` or `on_wire`.
    /// With both Never, the part to be cut is highlighted instead.
    pub show_extremity_markers: BreakWireShow,
}

impl Default for BreakWirePreferences {
    fn default() -> BreakWirePreferences {
        BreakWirePreferences {
            cutting_length: CuttingLength::Segment,
            grid_multiple: 2,
            fixed_length: Length(reda_model::geom::Nm(254_000 * 10)),
            show_cutter_box: BreakWireShow::Always,
            show_extremity_markers: BreakWireShow::Always,
        }
    }
}

impl BreakWirePreferences {
    /// The fewest and most snap grids a `grid_multiple` cut is long.
    pub const GRID_MULTIPLES: std::ops::RangeInclusive<u32> = 2..=10;

    /// How long a gap the Tool cuts on a sheet whose snap grid is `grid`: `None` for the
    /// whole segment.
    pub fn length(&self, grid: reda_model::geom::Nm) -> Option<reda_model::geom::Nm> {
        match self.cutting_length {
            CuttingLength::Segment => None,
            CuttingLength::GridMultiple => Some(grid * i64::from(self.grid_multiple)),
            CuttingLength::Fixed => Some(self.fixed_length.0),
        }
    }
}

/// The Break Wire Tool's Cutting Length.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CuttingLength {
    /// Snap To Segment: the whole segment.
    #[default]
    Segment,
    /// Snap Grid Size Multiple.
    GridMultiple,
    /// Fixed Length.
    Fixed,
}

impl CuttingLength {
    /// The next one, as Space cycles them in the Tool.
    pub fn next(self) -> CuttingLength {
        match self {
            CuttingLength::Segment => CuttingLength::GridMultiple,
            CuttingLength::GridMultiple => CuttingLength::Fixed,
            CuttingLength::Fixed => CuttingLength::Segment,
        }
    }
}

/// When the Break Wire Tool draws its cutter box or extremity markers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BreakWireShow {
    Never,
    #[default]
    Always,
    /// Only while the cursor is over a Wire.
    OnWire,
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
            snap_distance: _,
            hotspots_off: _,
            default_grid,
            zone: _,
            keepout: _,
            arc: _,
            net_info: _,
            route: _,
            drag: _,
            ratsnest: _,
            view: _,
            place: _,
        } = Editing::default();
        SchematicEditing {
            default_units,
            repeat_placement,
            electrical_snap,
            hotspots_off: BTreeSet::new(),
            default_grid,
            single_negation: false,
            display_cross_overs: false,
            cross_reference_sheet_style: CrossReferenceStyles::DEFAULT.sheet,
            cross_reference_location_style: CrossReferenceStyles::DEFAULT.location,
            optimize_wires: true,
            components_cut_wires: true,
            break_wires_at_autojunctions: true,
            convert_cross_junctions: true,
            unit_suffix: reda_model::UnitSuffix::Alpha,
            auto_increment: AutoIncrement::default(),
            break_wire: BreakWirePreferences::default(),
            clipboard_reference: false,
            in_place_editing: true,
            display_when_dragging: true,
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
    /// How an overlay colours an offending object.
    pub overlay_style: OverlayStyle,
    /// What an overlay does when the object is too small on screen for its pattern.
    pub overlay_zoomed_out: OverlayZoomedOut,
    /// Per check: whether its Violations get a detail marker and an overlay. It changes
    /// only what is drawn, never what is detected.
    pub display: CheckDisplays,
}

impl Default for Checks {
    fn default() -> Checks {
        Checks {
            max_markers: 500,
            overlay_style: OverlayStyle::default(),
            overlay_zoomed_out: OverlayZoomedOut::default(),
            display: CheckDisplays::default(),
        }
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

/// Cross-select (docs/spec/v0.1/sync.md#cross-select-and-net-highlighting): what
/// selecting on one side does on the other.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct CrossSelect {
    /// Dim everything else on the side the cross-selected objects are on.
    pub dim: bool,
    /// Zoom the other side's canvas to the cross-selected objects.
    pub zoom: bool,
    /// Focus the other Document when its canvas is visible.
    pub focus: bool,
    /// Reposition: picking a Unit attaches its Footprint to the PCB cursor when the PCB
    /// canvas is visible. Locked Footprints are skipped.
    pub reposition: bool,
}

impl Default for CrossSelect {
    fn default() -> CrossSelect {
        CrossSelect {
            dim: true,
            zoom: false,
            focus: true,
            reposition: false,
        }
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
        if !UI_SCALES.iter().any(|s| (s - scale).abs() < 1e-9) {
            return Err((
                "general.ui_scale",
                format!("{scale} is not a UI scale step (0.75, 0.85, 1, 1.15, 1.25, 1.5 or 2)"),
            ));
        }
        let multiple = self.schematic.break_wire.grid_multiple;
        if !BreakWirePreferences::GRID_MULTIPLES.contains(&multiple) {
            return Err((
                "schematic.break_wire.grid_multiple",
                format!("{multiple} is not between 2 and 10"),
            ));
        }
        if self.schematic.break_wire.fixed_length.0 <= reda_model::geom::Nm(0) {
            return Err((
                "schematic.break_wire.fixed_length",
                "the Fixed Length must be positive".to_owned(),
            ));
        }
        grid::check(&self.editing.grid, &self.editing.display_precision)?;
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
