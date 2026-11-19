//! `<name>.redafab`: one Fab Preset, as the user keeps it in the config folder.
//!
//! ```text
//! {
//!   "format": 2,
//!   "name": "My fab",
//!   "rules": {"clearance":100000,…},
//!   "constructions": [{"name":"2 Layer","default":true,"thickness":1600000,"entries":[{"copper":35000},{"kind":"core"},{"copper":35000}]}],
//!   "gerber": {"x2_attributes":true,"naming":"classic","file_origin":"absolute",…},
//!   "stackup_suggestions": {"thickness":[1600000],"copper":[35000],…},
//!   "zip_assembly": false,
//!   "bom": {"columns":[{"header":"Comment","source":"value"}],"layout":"grouped",…},
//!   "cpl": {"columns":[{"header":"Designator","source":"designator"}],"units":"mm",…},
//!   "placement_corrections": [{"footprint":"SOT-23*","rotation":90000}]
//! }
//! ```
//!
//! The fields are a [`FabPreset`]'s, the same as a shipped preset's and a Project copy's.

use std::path::Path;

use reda_model::{
    BomSettings, Construction, CplSettings, FabPreset, FabRules, GerberSettings,
    PlacementCorrection, StackupSuggestions,
};
use schemars::JsonSchema;
use serde::Deserialize;

use super::{FORMAT, FileKind, LoadError, parse};
use crate::canonical::{CanonicalError, CanonicalWriter};

/// A REDA Fab Preset file (`.redafab`).
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(title = "REDA Fab Preset file")]
pub(crate) struct FabPresetFile {
    /// The file-format version. REDA writes only the current one.
    #[allow(dead_code, reason = "checked by the migration chain")]
    #[schemars(schema_with = "super::schema::format_version")]
    format: u32,
    /// The preset's name, as the New Project dialog lists it.
    name: String,
    /// The flat limits a job checks the board against, and the preferred seeds of a new
    /// board's rules.
    rules: FabRules,
    /// Named physical builds offered by the preset.
    constructions: Vec<Construction>,
    /// The Gerber and drill switches.
    gerber: GerberSettings,
    /// The BOM columns, in order.
    bom: BomSettings,
    /// The CPL columns, in order.
    cpl: CplSettings,
    #[serde(default)]
    placement_corrections: Vec<PlacementCorrection>,
    zip_assembly: bool,
    stackup_suggestions: StackupSuggestions,
}

pub(crate) fn read_fab_file(path: &Path, bytes: &[u8]) -> Result<FabPreset, LoadError> {
    let file: FabPresetFile = parse(FileKind::FabPreset, path, bytes)?;
    Ok(FabPreset {
        name: file.name,
        rules: file.rules,
        constructions: file.constructions,
        gerber: file.gerber,
        zip_assembly: file.zip_assembly,
        stackup_suggestions: file.stackup_suggestions,
        bom: file.bom,
        cpl: file.cpl,
        placement_corrections: file.placement_corrections,
    })
}

pub(crate) fn write_fab_file(preset: &FabPreset) -> Result<Vec<u8>, CanonicalError> {
    let mut w = CanonicalWriter::new(FORMAT);
    w.field("name", &preset.name)?;
    w.field("rules", &preset.rules)?;
    w.list("constructions", &preset.constructions)?;
    w.field("gerber", &preset.gerber)?;
    w.field("stackup_suggestions", &preset.stackup_suggestions)?;
    w.field("zip_assembly", &preset.zip_assembly)?;
    w.field("bom", &preset.bom)?;
    w.field("cpl", &preset.cpl)?;
    w.list("placement_corrections", &preset.placement_corrections)?;
    Ok(w.finish())
}
