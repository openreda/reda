//! `<name>.redafab`: one Fab Preset, as the user keeps it in the config folder.
//!
//! ```text
//! {
//!   "format": 1,
//!   "name": "My fab",
//!   "rules": {"clearance":100000,…},
//!   "stackup": {"thickness":1600000,"copper":35000},
//!   "gerber": {"x2_attributes":true,"naming":"protel"},
//!   "bom": [
//!     {"header":"Comment","source":"value"},
//!     {"header":"LCSC Part #","source":{"field":"LCSC"}}
//!   ],
//!   "cpl": [
//!     {"header":"Designator","source":"designator"},
//!     …
//!   ]
//! }
//! ```
//!
//! The fields are a [`FabPreset`]'s, the same as a shipped preset's and a Project copy's.

use std::path::Path;

use reda_model::{Column, FabPreset, GerberSettings, Rules, StackupSeeds};
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
    /// The design rules a new board is seeded with.
    rules: Rules,
    /// The thickness and copper a new board's Stackup starts with; left out when the
    /// preset seeds neither, and then a new board gets 1.6 mm and 1 oz.
    #[serde(default)]
    stackup: StackupSeeds,
    /// The Gerber and drill switches.
    gerber: GerberSettings,
    /// The BOM columns, in order.
    bom: Vec<Column>,
    /// The CPL columns, in order.
    cpl: Vec<Column>,
}

pub(crate) fn read_fab_file(path: &Path, bytes: &[u8]) -> Result<FabPreset, LoadError> {
    let file: FabPresetFile = parse(FileKind::FabPreset, path, bytes)?;
    Ok(FabPreset {
        name: file.name,
        rules: file.rules,
        stackup: file.stackup,
        gerber: file.gerber,
        bom: file.bom,
        cpl: file.cpl,
    })
}

pub(crate) fn write_fab_file(preset: &FabPreset) -> Result<Vec<u8>, CanonicalError> {
    let mut w = CanonicalWriter::new(FORMAT);
    w.field("name", &preset.name)?;
    w.field("rules", &preset.rules)?;
    if !preset.stackup.is_empty() {
        w.field("stackup", &preset.stackup)?;
    }
    w.field("gerber", &preset.gerber)?;
    w.list("bom", &preset.bom)?;
    w.list("cpl", &preset.cpl)?;
    Ok(w.finish())
}
