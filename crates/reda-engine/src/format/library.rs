//! Library files: `<name>.redalib` and one file per Symbol, Footprint and Part.
//!
//! ```text
//! {
//!   "format": 1,
//!   "symbol": {"uuid":"…","name":"OpAmp_Dual","designator_prefix":"U"},
//!   "units": [
//!     {"uuid":"…","name":"A"}
//!   ],
//!   "pins": [
//!     {"uuid":"…","unit":"…","name":"OUT","number":"1","electrical_type":"output",…}
//!   ]
//! }
//! ```
//!
//! Units are a list (their order is A, B, C…); Pins and Pads are tables sorted by UUID.
//! A Part lists its alternate Footprints in order, the default first. Pin-map overrides
//! are written sorted. Where the files live in a Library folder is `reda-library`'s
//! business; this module maps one file's bytes to and from its element.

use std::collections::BTreeSet;
use std::path::Path;

use reda_model::{
    ComponentType, Fields, Footprint, LibraryInfo, Pad, Part, Pin, PinMap, Symbol, Unit, Uuid,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{FORMAT, FileKind, LoadError, parse};
use crate::canonical::{CanonicalError, CanonicalWriter};

/// A REDA Library file (`.redalib`), which marks its folder as a Library.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(title = "REDA Library file")]
pub(crate) struct LibraryFile {
    /// The file-format version. REDA writes only the current one.
    #[allow(dead_code, reason = "checked by the migration chain")]
    #[schemars(schema_with = "super::schema::format_version")]
    format: u32,
    /// The Library's identity and description.
    library: LibraryInfo,
}

/// A REDA Symbol file (`.redasym`).
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(title = "REDA Symbol file")]
pub(crate) struct SymbolFile {
    /// The file-format version. REDA writes only the current one.
    #[allow(dead_code, reason = "checked by the migration chain")]
    #[schemars(schema_with = "super::schema::format_version")]
    format: u32,
    symbol: SymbolRow,
    /// The Units in order (A, B, C…). A single-unit Symbol has exactly one.
    units: Vec<Unit>,
    /// Every Pin of every Unit, sorted by UUID.
    pins: Vec<Pin>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SymbolRow {
    uuid: Uuid,
    name: String,
    /// The Designator prefix of Components placed from it, such as `R` or `U`.
    designator_prefix: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    description: String,
}

/// A REDA Footprint file (`.redafp`).
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(title = "REDA Footprint file")]
pub(crate) struct FootprintFile {
    /// The file-format version. REDA writes only the current one.
    #[allow(dead_code, reason = "checked by the migration chain")]
    #[schemars(schema_with = "super::schema::format_version")]
    format: u32,
    footprint: FootprintRow,
    /// The Pads, sorted by UUID. Pads that share a number belong to one Pin.
    pads: Vec<Pad>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct FootprintRow {
    uuid: Uuid,
    name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    description: String,
}

/// A REDA Part file (`.redapart`).
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(title = "REDA Part file")]
pub(crate) struct PartFile {
    /// The file-format version. REDA writes only the current one.
    #[allow(dead_code, reason = "checked by the migration chain")]
    #[schemars(schema_with = "super::schema::format_version")]
    format: u32,
    part: PartRow,
    /// The alternate Footprints by UUID, in order; the first is the default.
    footprints: Vec<Uuid>,
    /// The default fields, copied onto each Component placed from the Part.
    fields: Fields,
    /// Exceptions to the implicit map, where a Pin connects to the Pads with its number.
    pin_map: PinMap,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct PartRow {
    uuid: Uuid,
    name: String,
    component_type: ComponentType,
    /// The Symbol by UUID. Mechanical Parts may have none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    symbol: Option<Uuid>,
}

pub fn write_library_info(info: &LibraryInfo) -> Result<Vec<u8>, CanonicalError> {
    let mut w = CanonicalWriter::new(FORMAT);
    w.field("library", info)?;
    Ok(w.finish())
}

pub fn read_library_info(path: &Path, bytes: &[u8]) -> Result<LibraryInfo, LoadError> {
    let file: LibraryFile = parse(FileKind::Library, path, bytes)?;
    Ok(file.library)
}

pub fn write_symbol(symbol: &Symbol) -> Result<Vec<u8>, CanonicalError> {
    let mut w = CanonicalWriter::new(FORMAT);
    w.field(
        "symbol",
        &SymbolRow {
            uuid: symbol.uuid,
            name: symbol.name.clone(),
            designator_prefix: symbol.designator_prefix.clone(),
            description: symbol.description.clone(),
        },
    )?;
    w.list("units", &symbol.units)?;
    w.table("pins", &symbol.pins, |p| p.uuid)?;
    Ok(w.finish())
}

pub fn read_symbol(path: &Path, bytes: &[u8]) -> Result<Symbol, LoadError> {
    let invalid = |why: String| LoadError::Invalid(path.to_owned(), why);
    let file: SymbolFile = parse(FileKind::Symbol, path, bytes)?;
    if file.units.is_empty() {
        return Err(invalid("a Symbol needs at least one Unit".into()));
    }
    let units = unique(file.units.iter().map(|u| u.uuid), "Unit").map_err(invalid)?;
    unique(file.pins.iter().map(|p| p.uuid), "Pin").map_err(invalid)?;
    if let Some(pin) = file.pins.iter().find(|p| !units.contains(&p.unit)) {
        return Err(invalid(format!(
            "Pin {} is on unit {}, which the Symbol does not have",
            pin.uuid, pin.unit
        )));
    }
    let mut pins = file.pins;
    pins.sort_by_key(|p| p.uuid);
    Ok(Symbol {
        uuid: file.symbol.uuid,
        name: file.symbol.name,
        description: file.symbol.description,
        designator_prefix: file.symbol.designator_prefix,
        units: file.units,
        pins,
    })
}

pub fn write_footprint(footprint: &Footprint) -> Result<Vec<u8>, CanonicalError> {
    let mut w = CanonicalWriter::new(FORMAT);
    w.field(
        "footprint",
        &FootprintRow {
            uuid: footprint.uuid,
            name: footprint.name.clone(),
            description: footprint.description.clone(),
        },
    )?;
    w.table("pads", &footprint.pads, |p| p.uuid)?;
    Ok(w.finish())
}

pub fn read_footprint(path: &Path, bytes: &[u8]) -> Result<Footprint, LoadError> {
    let invalid = |why: String| LoadError::Invalid(path.to_owned(), why);
    let file: FootprintFile = parse(FileKind::Footprint, path, bytes)?;
    unique(file.pads.iter().map(|p| p.uuid), "Pad").map_err(invalid)?;
    let mut pads = file.pads;
    pads.sort_by_key(|p| p.uuid);
    Ok(Footprint {
        uuid: file.footprint.uuid,
        name: file.footprint.name,
        description: file.footprint.description,
        pads,
    })
}

pub fn write_part(part: &Part) -> Result<Vec<u8>, CanonicalError> {
    let mut w = CanonicalWriter::new(FORMAT);
    w.field(
        "part",
        &PartRow {
            uuid: part.uuid,
            name: part.name.clone(),
            component_type: part.component_type,
            symbol: part.symbol,
        },
    )?;
    w.list("footprints", &part.footprints)?;
    w.field("fields", &part.fields)?;
    w.field("pin_map", &sorted(&part.pin_map))?;
    Ok(w.finish())
}

pub fn read_part(path: &Path, bytes: &[u8]) -> Result<Part, LoadError> {
    let file: PartFile = parse(FileKind::Part, path, bytes)?;
    Ok(Part {
        uuid: file.part.uuid,
        name: file.part.name,
        component_type: file.part.component_type,
        symbol: file.part.symbol,
        footprints: file.footprints,
        fields: file.fields,
        pin_map: sorted(&file.pin_map),
    })
}

/// The pin map in its canonical order: overrides by Pin number, unconnected Pads sorted.
fn sorted(map: &PinMap) -> PinMap {
    let mut map = map.clone();
    map.pins.sort_by(|a, b| a.pin.cmp(&b.pin));
    map.unconnected.sort();
    map
}

/// The UUIDs as a set, or an error naming the first that repeats.
fn unique(uuids: impl Iterator<Item = Uuid>, what: &str) -> Result<BTreeSet<Uuid>, String> {
    let mut seen = BTreeSet::new();
    for uuid in uuids {
        if !seen.insert(uuid) {
            return Err(format!("{what} {uuid} appears twice"));
        }
    }
    Ok(seen)
}
