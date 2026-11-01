//! `<name>.redasch`: one Schematic Sheet.
//!
//! ```text
//! {
//!   "format": 1,
//!   "sheet": {"uuid":"…"},
//!   "frame": {"uuid":"…","size":"a4"},
//!   "parts": [
//!     {"uuid":"…","hash":"…","part":{…},"footprints":[…],"copies":[…],"fields":{…},"pin_map":{…},"symbol":{…},"units":[
//!       {"uuid":"…","name":"A"}
//!     ],"pins":[
//!       {"uuid":"…","unit":"…","name":"+","number":"3",…}
//!     ]}
//!   ],
//!   "footprints": [
//!     {"uuid":"…","hash":"…","footprint":{…},"pads":[
//!       {"uuid":"…","number":"1",…}
//!     ]}
//!   ],
//!   "units": [
//!     {"uuid":"…","component":{"uuid":"…"},"part":"…","unit":"…","at":{…},"rotation":0,"designator":"U1",…}
//!   ]
//! }
//! ```
//!
//! `frame` is written only when the sheet has one, and the three tables only when the sheet
//! holds any Component. A part-table entry (`parts`) holds the
//! keys of a `.redapart` file (`part`, `footprints`, `fields`, `pin_map`) and of a
//! `.redasym` file (`symbol`, `units`, `pins`) with the same shapes, plus its own `uuid`,
//! its content `hash` and `copies`: the sheet's Footprint copy of each alternate. A
//! Footprint copy (`footprints`) holds the keys of a `.redafp` file. Rows leave out the
//! sheet their model objects name: the file is the sheet.

use std::path::Path;

use reda_model::{
    Fields, FootprintCopy, InDocument, Object, Pad, PartEntry, Pin, PinMap, SchematicSheet,
    SheetFrame, Unit, UnitInstance, Uuid,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::library::{FootprintRow, PartRow, SymbolRow, sorted};
use super::{FORMAT, FileKind, LoadError, parse};
use crate::canonical::{CanonicalError, CanonicalWriter};

/// A REDA Schematic Sheet file (`.redasch`).
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(title = "REDA Schematic Sheet file")]
pub(crate) struct SheetFile {
    /// The file-format version. REDA writes only the current one.
    #[allow(dead_code, reason = "checked by the migration chain")]
    #[schemars(schema_with = "super::schema::format_version")]
    format: u32,
    /// The sheet's identity; it matches the sheet's entry in the Project file.
    sheet: SheetRow,
    /// The sheet's frame settings. Without it the sheet is A4.
    #[serde(default)]
    frame: Option<SheetFrame>,
    /// The part table: one copied Part with its Symbol per entry, sorted by UUID.
    #[serde(default)]
    parts: Vec<PartEntryRow>,
    /// The copied Footprints the part table's entries share, sorted by UUID.
    #[serde(default)]
    footprints: Vec<FootprintCopyRow>,
    /// The placed Units, sorted by UUID.
    #[serde(default)]
    units: Vec<UnitInstance>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SheetRow {
    #[schemars(schema_with = "super::schema::uuid")]
    uuid: Uuid,
}

/// One part-table entry: a `.redapart` and a `.redasym` file's keys in one row.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct PartEntryRow {
    /// The entry's own UUID, which Units point at.
    uuid: Uuid,
    /// The content hash of the Part with its Symbol and Footprints, as copied.
    hash: String,
    part: PartRow,
    /// The alternate Footprints by Library UUID, the default first.
    footprints: Vec<Uuid>,
    /// The sheet's Footprint copy of each alternate, in the same order.
    copies: Vec<Uuid>,
    fields: Fields,
    pin_map: PinMap,
    symbol: SymbolRow,
    units: Vec<Unit>,
    pins: Vec<Pin>,
}

/// One copied Footprint: a `.redafp` file's keys in one row.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct FootprintCopyRow {
    uuid: Uuid,
    /// The content hash of the Footprint, as copied.
    hash: String,
    footprint: FootprintRow,
    pads: Vec<Pad>,
}

pub(crate) fn write_sheet_file(sheet: &SchematicSheet) -> Result<Vec<u8>, CanonicalError> {
    let mut w = CanonicalWriter::new(FORMAT);
    w.field("sheet", &SheetRow { uuid: sheet.uuid })?;
    if let Some(frame) = sheet.frame() {
        w.field("frame", frame)?;
    }
    let parts = sheet.parts().map(|e| PartEntryRow {
        uuid: e.uuid,
        hash: e.hash.clone(),
        part: PartRow::of(&e.part),
        footprints: e.part.footprints.clone(),
        copies: e.footprints.clone(),
        fields: e.part.fields.clone(),
        pin_map: sorted(&e.part.pin_map),
        symbol: SymbolRow::of(&e.symbol),
        units: e.symbol.units.clone(),
        pins: e.symbol.pins.clone(),
    });
    let footprints = sheet.footprints().map(|c| FootprintCopyRow {
        uuid: c.uuid,
        hash: c.hash.clone(),
        footprint: FootprintRow::of(&c.footprint),
        pads: c.footprint.pads.clone(),
    });
    // A sheet with no Components keeps the short form of an empty sheet.
    if sheet.parts().next().is_some() || sheet.units().next().is_some() {
        w.containers_spread("parts", parts, |r| r.uuid, &["units", "pins"])?;
        w.containers("footprints", footprints, |r| r.uuid, "pads")?;
        w.table("units", sheet.units(), |u| u.uuid)?;
    }
    Ok(w.finish())
}

/// Reads a sheet file, checks it is the Document the Project file says it is, and returns
/// the objects on it, for the loader to insert.
pub(crate) fn read_sheet_file(
    path: &Path,
    bytes: &[u8],
    expected: Uuid,
) -> Result<Vec<Object>, LoadError> {
    let invalid = |why: String| LoadError::Invalid(path.to_owned(), why);
    let file: SheetFile = parse(FileKind::Sheet, path, bytes)?;
    let sheet = file.sheet.uuid;
    if sheet != expected {
        return Err(invalid(format!(
            "holds sheet {sheet} but the Project file lists it as {expected}"
        )));
    }
    let mut objects = vec![];
    if let Some(frame) = file.frame {
        objects.push(Object::SheetFrame(InDocument::new(sheet, frame)));
    }
    for row in file.footprints {
        let footprint = row
            .footprint
            .into_footprint(row.pads)
            .map_err(|e| invalid(format!("Footprint copy {}: {e}", row.uuid)))?;
        objects.push(Object::FootprintCopy(InDocument::new(
            sheet,
            FootprintCopy {
                uuid: row.uuid,
                hash: row.hash,
                footprint,
            },
        )));
    }
    for row in file.parts {
        let symbol = row
            .symbol
            .into_symbol(row.units, row.pins)
            .map_err(|e| invalid(format!("part entry {}: {e}", row.uuid)))?;
        let part = row.part.into_part(row.footprints, row.fields, &row.pin_map);
        objects.push(Object::PartEntry(InDocument::new(
            sheet,
            PartEntry {
                uuid: row.uuid,
                hash: row.hash,
                part,
                symbol,
                footprints: row.copies,
            },
        )));
    }
    for unit in file.units {
        objects.push(Object::Unit(InDocument::new(sheet, unit)));
    }
    Ok(objects)
}
