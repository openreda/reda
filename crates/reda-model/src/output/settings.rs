//! The fab's file conventions and assembly table forms, shared by all its jobs.

use reda_geom::Nm;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{Column, ColumnSource};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FileOrigin {
    Absolute,
    Relative,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SlotCommand {
    Route,
    G85,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LayerFileName {
    pub layer: String,
    pub name: String,
}

/// Suggestions never validate the board or seed its finish or colours.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StackupSuggestions {
    pub thickness: Vec<Nm>,
    pub copper: Vec<Nm>,
    pub finish: Vec<String>,
    pub mask_color: Vec<String>,
    pub silk_color: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BomLayout {
    Grouped,
    Flat,
}

/// Sources shared by Components on a grouped BOM row.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BomGroup {
    Part,
    Footprint,
    /// All Component value fields, including manufacturer and supplier numbers.
    Fields,
    Value,
    Field(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum NotFittedBom {
    Omit,
    List,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LineNumbers {
    pub enabled: bool,
    pub start: u32,
    pub step: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MountValues {
    pub smt: String,
    pub tht: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BomSettings {
    pub columns: Vec<Column>,
    pub layout: BomLayout,
    pub group_by: Vec<BomGroup>,
    pub sort: Vec<ColumnSource>,
    pub line_numbers: LineNumbers,
    pub designator_separator: String,
    pub not_fitted: NotFittedBom,
    pub required_fields: Vec<String>,
    pub list_unplaced: bool,
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SideValues {
    pub top: String,
    pub bottom: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CplUnits {
    Mm,
    Mil,
    In,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BottomRotation {
    Stored,
    #[serde(rename = "plus_180")]
    Plus180,
    Negate,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BottomPosition {
    TopView,
    NegateX,
    NegateY,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CplSettings {
    pub columns: Vec<Column>,
    pub side_values: SideValues,
    pub units: CplUnits,
    pub unit_suffix: bool,
    pub bottom_rotation: BottomRotation,
    pub bottom_position: BottomPosition,
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
