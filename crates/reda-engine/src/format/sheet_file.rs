//! `<name>.redasch`: one Schematic Sheet.
//!
//! ```text
//! {
//!   "format": 1,
//!   "sheet": {"uuid":"…"}
//! }
//! ```
//!
//! The sheet's tables (Wires, Net Labels, Units…) are added after `sheet`, one
//! [`CanonicalWriter::table`] each, as their object kinds arrive.

use std::path::Path;

use reda_model::{SchematicSheet, Uuid};
use serde::{Deserialize, Serialize};

use super::{FORMAT, FileKind, LoadError, parse};
use crate::canonical::{CanonicalError, CanonicalWriter};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SheetFile {
    #[allow(dead_code, reason = "checked by the migration chain")]
    format: u32,
    sheet: SheetRow,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SheetRow {
    uuid: Uuid,
}

pub(crate) fn write_sheet_file(sheet: &SchematicSheet) -> Result<Vec<u8>, CanonicalError> {
    let mut w = CanonicalWriter::new(FORMAT);
    w.field("sheet", &SheetRow { uuid: sheet.uuid })?;
    Ok(w.finish())
}

/// Reads a sheet file and checks it is the Document the Project file says it is.
pub(crate) fn read_sheet_file(
    path: &Path,
    bytes: &[u8],
    expected: Uuid,
) -> Result<SchematicSheet, LoadError> {
    let file: SheetFile = parse(FileKind::Sheet, path, bytes)?;
    if file.sheet.uuid != expected {
        return Err(LoadError::Invalid(
            path.to_owned(),
            format!(
                "holds sheet {} but the Project file lists it as {expected}",
                file.sheet.uuid
            ),
        ));
    }
    Ok(SchematicSheet {
        uuid: file.sheet.uuid,
    })
}
