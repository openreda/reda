//! Manufacturing set-up stored in the Project file: the Project's copies of Fab Presets and
//! its Output Jobs.
//!
//! A [`FabPreset`] is also the shape of a user's `.redafab` file and of the presets REDA
//! ships, so a Project copy, a user preset and a shipped preset compare field by field.

use reda_geom::Nm;

mod settings;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
pub use settings::*;

use crate::{StackupSeeds, Uuid};

/// A Fab Preset's design rules: flat limits with no scope, which a job compares the board
/// with, and the preferred values a new board's routing starts from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FabRules {
    /// Minimum clearance between copper of different Nets.
    pub clearance: Nm,
    /// Minimum Track width.
    pub track_width: Nm,
    /// Minimum Via diameter.
    pub via_diameter: Nm,
    /// Minimum Via hole.
    pub via_hole: Nm,
    /// Minimum Pad, Free Pad and non-plated hole.
    pub hole_min: Nm,
    /// Maximum Pad, Free Pad and non-plated hole.
    pub hole_max: Nm,
    /// Minimum annular ring of Vias and plated Pads.
    pub annular_ring: Nm,
    /// Minimum spacing between hole edges.
    pub hole_to_hole: Nm,
    /// Minimum clearance from copper to the Board Outline and Board Cutouts.
    pub edge_clearance: Nm,
    /// The preferred sizes a new board is seeded with. They are never compared with the
    /// board.
    pub preferred: PreferredSeeds,
}

/// The preferred Track and Via sizes a Fab Preset seeds a new board with.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PreferredSeeds {
    pub track_width: Nm,
    pub via_diameter: Nm,
    pub via_hole: Nm,
}

/// How the Gerber and drill files of a job are named.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FileNaming {
    /// Classic extensions: `.gtl`, `.gbl`, `.gts`, … and `.gm1` for the Profile.
    Classic,
    /// The project and Layer name, ending in `.gbr`.
    Descriptive,
}

/// The Gerber and drill switches of a Fab Preset.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GerberSettings {
    /// Write Gerber X2 attributes (`.FileFunction` and friends); off writes plain X1.
    pub x2_attributes: bool,
    /// How the files are named.
    pub naming: FileNaming,
    /// Shared by Gerber, drill and CPL.
    pub file_origin: FileOrigin,
    pub slot_command: SlotCommand,
    pub merge_drill: bool,
    pub outline_on_every_plot: bool,
    /// Layer name to output file name; portable between boards and Projects.
    pub file_names: Vec<LayerFileName>,
}

impl Default for GerberSettings {
    fn default() -> Self {
        Self {
            x2_attributes: true,
            naming: FileNaming::Classic,
            file_origin: FileOrigin::Absolute,
            slot_command: SlotCommand::Route,
            merge_drill: false,
            outline_on_every_plot: false,
            file_names: Vec::new(),
        }
    }
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
    /// Footprint copper-Pad bounding box centre, X, from the output origin.
    CentreX,
    /// Footprint copper-Pad bounding box centre, Y.
    CentreY,
    /// Footprint origin, X.
    OriginX,
    /// Footprint origin, Y.
    OriginY,
    /// Pad 1, X.
    Pad1X,
    /// Pad 1, Y.
    Pad1Y,
    /// Whether the Component is fitted.
    Fitted,
    /// `Top` or `Bottom`, or the spellings from `side_values`.
    Side,
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
    /// The limits a job checks the board against, and the seeds of a new board's rules.
    pub rules: FabRules,
    /// The thickness and copper a new board's Stackup starts with; left out when the
    /// preset seeds neither.
    #[serde(default, skip_serializing_if = "StackupSeeds::is_empty")]
    pub stackup: StackupSeeds,
    pub gerber: GerberSettings,
    pub stackup_suggestions: StackupSuggestions,
    pub zip_assembly: bool,
    /// The BOM table form, columns and row conventions.
    pub bom: BomSettings,
    /// The CPL table form, columns and position conventions.
    pub cpl: CplSettings,
}

/// The Project's own copy of a Fab Preset, so outputs come out the same on any machine.
/// Its fields are a [`FabPreset`]'s, after the UUID.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProjectFabPreset {
    pub uuid: Uuid,
    /// The name of the preset it was copied from.
    pub name: String,
    pub rules: FabRules,
    #[serde(default, skip_serializing_if = "StackupSeeds::is_empty")]
    pub stackup: StackupSeeds,
    pub gerber: GerberSettings,
    pub stackup_suggestions: StackupSuggestions,
    pub zip_assembly: bool,
    pub bom: BomSettings,
    pub cpl: CplSettings,
}

impl ProjectFabPreset {
    /// A copy of `preset` under `uuid`.
    pub fn new(uuid: Uuid, preset: FabPreset) -> ProjectFabPreset {
        let FabPreset {
            name,
            rules,
            stackup,
            gerber,
            stackup_suggestions,
            zip_assembly,
            bom,
            cpl,
        } = preset;
        ProjectFabPreset {
            uuid,
            name,
            rules,
            stackup,
            gerber,
            stackup_suggestions,
            zip_assembly,
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
            gerber: self.gerber.clone(),
            stackup_suggestions: self.stackup_suggestions.clone(),
            zip_assembly: self.zip_assembly,
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
    pub fab_notes: bool,
    pub drill_report: bool,
    pub gerber_job: bool,
    pub reports: bool,
    pub schematic_pdf: bool,
}

impl Outputs {
    /// Every output.
    pub const ALL: Outputs = Outputs {
        gerber: true,
        drill: true,
        bom: true,
        cpl: true,
        fab_notes: true,
        drill_report: true,
        gerber_job: true,
        reports: true,
        schematic_pdf: true,
    };
}

/// Whether this build retains Not Fitted Components' paste openings.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum NotFittedPaste {
    #[default]
    Keep,
    Remove,
}

/// The schematic PDF's paper colour mode.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PdfColour {
    #[default]
    Colour,
    Mono,
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
    /// The zip and assembly files' stem, resolved with the shared text resolver.
    pub name_template: String,
    pub not_fitted_paste: NotFittedPaste,
    pub schematic_pdf_colour: PdfColour,
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
