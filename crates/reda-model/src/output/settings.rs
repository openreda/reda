//! The fab's file conventions and assembly table forms, shared by all its jobs.

use reda_geom::Nm;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{Column, ColumnSource};

/// Where Gerber, drill and CPL coordinates are measured from, for all three together.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FileOrigin {
    /// Design zero.
    Absolute,
    /// The Output Origin if one is placed on the PCB Document, otherwise the lower-left
    /// corner of the Board Outline's bounding box.
    Relative,
}

/// How the drill file writes a slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SlotCommand {
    /// Routed: `G00`, `M15`, `G01`, `M16`, `G05`.
    Route,
    /// The `G85` canned slot.
    G85,
}

/// A Layer whose Gerber file gets its own name.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LayerFileName {
    /// The Layer's name, so the override carries between boards and Projects.
    pub layer: String,
    /// The file name to write that Layer's Gerber as.
    pub name: String,
}

/// Suggestions never validate the board or seed its finish or colours.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StackupSuggestions {
    /// Board thicknesses the fab offers.
    pub thickness: Vec<Nm>,
    /// Outer copper thicknesses the fab offers.
    pub copper: Vec<Nm>,
    /// Surface finishes the fab offers.
    pub finish: Vec<String>,
    /// Solder Mask colours the fab offers.
    pub mask_color: Vec<String>,
    /// Silkscreen colours the fab offers.
    pub silk_color: Vec<String>,
}

/// One BOM row per group or per Component.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BomLayout {
    /// One row per group of Components sharing `group_by`.
    Grouped,
    /// One row per Component.
    Flat,
}

/// Sources shared by Components on a grouped BOM row.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BomGroup {
    /// The Component's Part.
    Part,
    /// The chosen Footprint.
    Footprint,
    /// All Component value fields, including manufacturer and supplier numbers.
    Fields,
    /// The Component's value.
    Value,
    /// The Component field with this name.
    Field(String),
}

/// Whether the BOM lists Not Fitted Components.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum NotFittedBom {
    /// Not Fitted Components are left out.
    Omit,
    /// Not Fitted Components get their own rows with quantity 0.
    List,
}

/// The BOM's line-number column.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LineNumbers {
    /// Whether the line-number column counts.
    pub enabled: bool,
    /// The first row's number.
    pub start: u32,
    /// The increase from one row to the next.
    pub step: u32,
}

/// The spellings of the mount column.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MountValues {
    /// The mount column for a Footprint with no plated through-hole Pad.
    pub smt: String,
    /// The mount column for a Footprint with any plated through-hole Pad.
    pub tht: String,
}

/// A Fab Preset's BOM: its columns and row conventions.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BomSettings {
    /// The columns, Source → Header, in file order.
    pub columns: Vec<Column>,
    /// Grouped rows or one row per Component.
    pub layout: BomLayout,
    /// What a grouped row's Components share.
    pub group_by: Vec<BomGroup>,
    /// The row order, by these columns in turn.
    pub sort: Vec<ColumnSource>,
    /// The line-number column's start and step.
    pub line_numbers: LineNumbers,
    /// What joins a row's Designators, which are never written as ranges.
    pub designator_separator: String,
    /// Whether Not Fitted Components are listed.
    pub not_fitted: NotFittedBom,
    /// Fields whose missing value warns; the row stays, and Mechanical rows are skipped.
    pub required_fields: Vec<String>,
    /// Whether footprintless Mechanical Components on a sheet are listed.
    pub list_unplaced: bool,
    /// The spellings of the mount column.
    pub mount_values: MountValues,
}

impl Default for BomSettings {
    fn default() -> Self {
        Self {
            columns: Vec::new(),
            layout: BomLayout::Grouped,
            group_by: vec![BomGroup::Part, BomGroup::Footprint, BomGroup::Fields],
            sort: vec![ColumnSource::Designator],
            line_numbers: LineNumbers {
                enabled: false,
                start: 1,
                step: 1,
            },
            designator_separator: ",".into(),
            not_fitted: NotFittedBom::Omit,
            required_fields: Vec::new(),
            list_unplaced: true,
            mount_values: MountValues {
                smt: "SMT".into(),
                tht: "THT".into(),
            },
        }
    }
}

/// The spellings of the CPL's side column.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SideValues {
    /// The side column for a part on top.
    pub top: String,
    /// The side column for a part on the bottom.
    pub bottom: String,
}

/// The unit of CPL positions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CplUnits {
    /// Millimetres.
    Mm,
    /// Thousandths of an inch.
    Mil,
    /// Inches.
    In,
}

/// How a bottom-side part's stored rotation θ is written.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BottomRotation {
    /// θ as stored: a part at 30° flipped to the bottom writes 330.
    Stored,
    /// θ + 180: writes 150.
    #[serde(rename = "plus_180")]
    Plus180,
    /// 360 − θ: writes 30.
    Negate,
}

/// How a bottom-side part's position is written.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BottomPosition {
    /// Positions as seen from the top.
    TopView,
    /// X negated.
    NegateX,
    /// Y negated.
    NegateY,
}

/// A Fab Preset's CPL: its columns and position conventions.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CplSettings {
    /// The columns, Source → Header, in file order.
    pub columns: Vec<Column>,
    /// The spellings of the side column.
    pub side_values: SideValues,
    /// The unit positions are written in.
    pub units: CplUnits,
    /// Whether the unit follows each position.
    pub unit_suffix: bool,
    /// How a bottom-side part's rotation is written.
    pub bottom_rotation: BottomRotation,
    /// How a bottom-side part's position is written.
    pub bottom_position: BottomPosition,
    /// Leaves out Footprints with a plated through-hole Pad.
    pub exclude_through_hole: bool,
}

impl Default for CplSettings {
    fn default() -> Self {
        Self {
            columns: Vec::new(),
            side_values: SideValues {
                top: "Top".into(),
                bottom: "Bottom".into(),
            },
            units: CplUnits::Mm,
            unit_suffix: false,
            bottom_rotation: BottomRotation::Plus180,
            bottom_position: BottomPosition::TopView,
            exclude_through_hole: false,
        }
    }
}
