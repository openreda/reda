//! Manufacturing set-up stored in the Project file: the Project's copies of Fab Presets and
//! its Output Jobs.
//!
//! A [`FabPreset`] is also the shape of a user's `.redafab` file and of the presets REDA
//! ships, so a Project copy, a user preset and a shipped preset compare field by field.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{Rules, StackupSeeds, Uuid};

/// How the Gerber and drill files of a job are named.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FileNaming {
    /// Protel extensions: `.GTL`, `.GBL`, `.GTS`, … and `.GKO` for the Board Outline.
    Protel,
}

/// The Gerber and drill switches of a Fab Preset.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GerberSettings {
    /// Write Gerber X2 attributes (`.FileFunction` and friends); off writes plain X1.
    pub x2_attributes: bool,
    /// How the files are named.
    pub naming: FileNaming,
}

/// Where a BOM or CPL column takes its values from.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ColumnSource {
    /// The row's number, from 1.
    Line,
    /// The Designators of the row, in natural order (CPL: the one Designator).
    Designator,
    /// How many Components the BOM row groups.
    Quantity,
    /// The Component's value, such as `10k`.
    Value,
    /// The chosen Footprint's name.
    Footprint,
    /// `SMT` or `THT`.
    Mount,
    /// The Footprint's centre, X, in mm from the CPL origin.
    MidX,
    /// The Footprint's centre, Y, in mm from the CPL origin.
    MidY,
    /// `Top` or `Bottom`.
    Layer,
    /// The Footprint's rotation in degrees, counter-clockwise.
    Rotation,
    /// The Component field with this name, such as `LCSC` or `MPN`.
    Field(String),
}

/// One column of a BOM or CPL file.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Column {
    /// The column header, such as "LCSC Part #".
    pub header: String,
    pub source: ColumnSource,
}

impl Column {
    pub fn new(header: &str, source: ColumnSource) -> Column {
        Column {
            header: header.into(),
            source,
        }
    }
}

/// What a fab wants: seed design rules, the Gerber and drill switches, and the BOM and CPL
/// columns. The same shape is a shipped preset, a user's `.redafab` file and the body of a
/// Project copy.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FabPreset {
    /// The preset's name, such as "JLCPCB". It names the preset a Project copy came from.
    pub name: String,
    /// The design rules a new board is seeded with, and the minimums a job checks against.
    pub rules: Rules,
    /// The thickness and copper a new board's Stackup starts with; left out when the
    /// preset seeds neither.
    #[serde(default, skip_serializing_if = "StackupSeeds::is_empty")]
    pub stackup: StackupSeeds,
    pub gerber: GerberSettings,
    /// The BOM columns, in order.
    pub bom: Vec<Column>,
    /// The CPL columns, in order.
    pub cpl: Vec<Column>,
}

/// The Project's own copy of a Fab Preset, so outputs come out the same on any machine.
/// Its fields are a [`FabPreset`]'s, after the UUID.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProjectFabPreset {
    pub uuid: Uuid,
    /// The name of the preset it was copied from.
    pub name: String,
    pub rules: Rules,
    #[serde(default, skip_serializing_if = "StackupSeeds::is_empty")]
    pub stackup: StackupSeeds,
    pub gerber: GerberSettings,
    pub bom: Vec<Column>,
    pub cpl: Vec<Column>,
}

impl ProjectFabPreset {
    /// A copy of `preset` under `uuid`.
    pub fn new(uuid: Uuid, preset: FabPreset) -> ProjectFabPreset {
        let FabPreset {
            name,
            rules,
            stackup,
            gerber,
            bom,
            cpl,
        } = preset;
        ProjectFabPreset {
            uuid,
            name,
            rules,
            stackup,
            gerber,
            bom,
            cpl,
        }
    }

    /// The preset this copy holds.
    pub fn preset(&self) -> FabPreset {
        FabPreset {
            name: self.name.clone(),
            rules: self.rules,
            stackup: self.stackup,
            gerber: self.gerber,
            bom: self.bom.clone(),
            cpl: self.cpl.clone(),
        }
    }
}

/// Which files an Output Job writes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Outputs {
    /// Gerber files for copper, mask, paste, silkscreen and the Board Outline.
    pub gerber: bool,
    /// Excellon drill files, plated and non-plated.
    pub drill: bool,
    /// The CSV bill of materials.
    pub bom: bool,
    /// The CSV pick-and-place file.
    pub cpl: bool,
}

impl Outputs {
    /// Every output.
    pub const ALL: Outputs = Outputs {
        gerber: true,
        drill: true,
        bom: true,
        cpl: true,
    };
}

/// A named recipe for manufacturing files, stored in the Project file.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OutputJob {
    pub uuid: Uuid,
    pub name: String,
    /// The PCB Document it plots. A job whose PCB Document is gone is broken until it is
    /// pointed at another.
    pub pcb: Uuid,
    /// The Project copy of the Fab Preset it uses.
    pub fab_preset: Uuid,
    pub outputs: Outputs,
    /// The output folder relative to the Project folder, `/`-separated; left out for
    /// `outputs/<job name>`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub destination: Option<String>,
}

impl OutputJob {
    /// The output folder relative to the Project folder.
    pub fn destination(&self) -> String {
        match &self.destination {
            Some(destination) => destination.clone(),
            None => format!("outputs/{}", self.name),
        }
    }
}
