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
//! Units are a list (their order is A, B, C…); Pins, Pads and graphics are tables sorted
//! by UUID.
//! A Part lists its alternate Footprints in order, the default first. Pin-map overrides
//! are written sorted. Where the files live in a Library folder is `reda-library`'s
//! business; this module maps one file's bytes to and from its element.

use std::collections::BTreeSet;
use std::path::Path;

use reda_model::{
    ComponentType, Fields, Footprint, FootprintGraphic, FootprintText, LibraryInfo, Pad, Part, Pin,
    PinMap, Provenance, Symbol, SymbolGraphic, Unit, Uuid,
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
    /// Where an imported element came from: the service, its terms and any copyright
    /// lines. Elements made in REDA have none.
    #[serde(default)]
    provenance: Option<Box<Provenance>>,
    /// The Units in order (A, B, C…). A single-unit Symbol has exactly one.
    units: Vec<Unit>,
    /// Every Pin of every Unit, sorted by UUID.
    pins: Vec<Pin>,
    /// The body drawing of every Unit, sorted by UUID.
    #[serde(default)]
    graphics: Vec<SymbolGraphic>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct SymbolRow {
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
    /// Where an imported element came from: the service, its terms and any copyright
    /// lines. Elements made in REDA have none.
    #[serde(default)]
    provenance: Option<Box<Provenance>>,
    /// Where the Designator and value text go.
    #[serde(default)]
    text: FootprintTexts,
    /// The Pads, sorted by UUID. Pads that share a number belong to one Pin.
    pads: Vec<Pad>,
    /// The Silkscreen, Courtyard and Assembly drawing, sorted by UUID.
    #[serde(default)]
    graphics: Vec<FootprintGraphic>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct FootprintTexts {
    designator: FootprintText,
    value: FootprintText,
}

impl FootprintTexts {
    pub(crate) fn of(footprint: &Footprint) -> FootprintTexts {
        FootprintTexts {
            designator: footprint.designator,
            value: footprint.value,
        }
    }
}

impl Default for FootprintTexts {
    fn default() -> FootprintTexts {
        FootprintTexts {
            designator: FootprintText::DESIGNATOR,
            value: FootprintText::VALUE,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct FootprintRow {
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
    /// Where an imported element came from: the service, its terms and any copyright
    /// lines. Elements made in REDA have none.
    #[serde(default)]
    provenance: Option<Box<Provenance>>,
    /// The alternate Footprints by UUID, in order; the first is the default.
    footprints: Vec<Uuid>,
    /// The default fields, copied onto each Component placed from the Part.
    fields: Fields,
    /// Exceptions to the implicit map, where a Pin connects to the Pads with its number.
    pin_map: PinMap,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct PartRow {
    uuid: Uuid,
    name: String,
    component_type: ComponentType,
    /// Whether its Components go into the BOM, by default.
    in_bom: bool,
    /// Whether its Components go into the CPL, by default.
    in_cpl: bool,
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

impl SymbolRow {
    pub(crate) fn of(symbol: &Symbol) -> SymbolRow {
        SymbolRow {
            uuid: symbol.uuid,
            name: symbol.name.clone(),
            designator_prefix: symbol.designator_prefix.clone(),
            description: symbol.description.clone(),
        }
    }

    /// The Symbol with these Units, Pins and graphics, checked: at least one Unit, unique
    /// UUIDs and every Pin and graphic on one of the Units. Pins and graphics come back
    /// sorted by UUID.
    pub(crate) fn into_symbol(
        self,
        units: Vec<Unit>,
        mut pins: Vec<Pin>,
        mut graphics: Vec<SymbolGraphic>,
        provenance: Option<Box<Provenance>>,
    ) -> Result<Symbol, String> {
        if units.is_empty() {
            return Err("a Symbol needs at least one Unit".into());
        }
        let unit_set = unique(units.iter().map(|u| u.uuid), "Unit")?;
        unique(pins.iter().map(|p| p.uuid), "Pin")?;
        if let Some(pin) = pins.iter().find(|p| !unit_set.contains(&p.unit)) {
            return Err(format!(
                "Pin {} is on unit {}, which the Symbol does not have",
                pin.uuid, pin.unit
            ));
        }
        unique(graphics.iter().map(|g| g.uuid), "graphic")?;
        if let Some(g) = graphics.iter().find(|g| !unit_set.contains(&g.unit)) {
            return Err(format!(
                "graphic {} is on unit {}, which the Symbol does not have",
                g.uuid, g.unit
            ));
        }
        pins.sort_by_key(|p| p.uuid);
        graphics.sort_by_key(|g| g.uuid);
        Ok(Symbol {
            uuid: self.uuid,
            name: self.name,
            description: self.description,
            designator_prefix: self.designator_prefix,
            units,
            pins,
            graphics,
            provenance,
        })
    }
}

pub fn write_symbol(symbol: &Symbol) -> Result<Vec<u8>, CanonicalError> {
    let mut w = CanonicalWriter::new(FORMAT);
    w.field("symbol", &SymbolRow::of(symbol))?;
    write_provenance(&mut w, &symbol.provenance)?;
    w.list("units", &symbol.units)?;
    w.table("pins", &symbol.pins, |p| p.uuid)?;
    w.table("graphics", &symbol.graphics, |g| g.uuid)?;
    Ok(w.finish())
}

pub fn read_symbol(path: &Path, bytes: &[u8]) -> Result<Symbol, LoadError> {
    let invalid = |why: String| LoadError::Invalid(path.to_owned(), why);
    let file: SymbolFile = parse(FileKind::Symbol, path, bytes)?;
    file.symbol
        .into_symbol(file.units, file.pins, file.graphics, file.provenance)
        .map_err(invalid)
}

impl FootprintRow {
    pub(crate) fn of(footprint: &Footprint) -> FootprintRow {
        FootprintRow {
            uuid: footprint.uuid,
            name: footprint.name.clone(),
            description: footprint.description.clone(),
        }
    }

    /// The Footprint with this text, these Pads and graphics, checked for unique UUIDs;
    /// Pads and graphics come back sorted.
    pub(crate) fn into_footprint(
        self,
        text: FootprintTexts,
        mut pads: Vec<Pad>,
        mut graphics: Vec<FootprintGraphic>,
        provenance: Option<Box<Provenance>>,
    ) -> Result<Footprint, String> {
        unique(pads.iter().map(|p| p.uuid), "Pad")?;
        unique(graphics.iter().map(|g| g.uuid), "graphic")?;
        pads.sort_by_key(|p| p.uuid);
        graphics.sort_by_key(|g| g.uuid);
        Ok(Footprint {
            uuid: self.uuid,
            name: self.name,
            description: self.description,
            pads,
            graphics,
            designator: text.designator,
            value: text.value,
            provenance,
        })
    }
}

pub fn write_footprint(footprint: &Footprint) -> Result<Vec<u8>, CanonicalError> {
    let mut w = CanonicalWriter::new(FORMAT);
    w.field("footprint", &FootprintRow::of(footprint))?;
    write_provenance(&mut w, &footprint.provenance)?;
    w.field("text", &FootprintTexts::of(footprint))?;
    w.table("pads", &footprint.pads, |p| p.uuid)?;
    w.table("graphics", &footprint.graphics, |g| g.uuid)?;
    Ok(w.finish())
}

pub fn read_footprint(path: &Path, bytes: &[u8]) -> Result<Footprint, LoadError> {
    let invalid = |why: String| LoadError::Invalid(path.to_owned(), why);
    let file: FootprintFile = parse(FileKind::Footprint, path, bytes)?;
    file.footprint
        .into_footprint(file.text, file.pads, file.graphics, file.provenance)
        .map_err(invalid)
}

impl PartRow {
    pub(crate) fn of(part: &Part) -> PartRow {
        PartRow {
            uuid: part.uuid,
            name: part.name.clone(),
            component_type: part.component_type,
            in_bom: part.in_bom,
            in_cpl: part.in_cpl,
            symbol: part.symbol,
        }
    }

    /// The Part with these alternates, fields and pin map, the map in canonical order.
    pub(crate) fn into_part(
        self,
        footprints: Vec<Uuid>,
        fields: Fields,
        pin_map: &PinMap,
        provenance: Option<Box<Provenance>>,
    ) -> Part {
        Part {
            uuid: self.uuid,
            name: self.name,
            component_type: self.component_type,
            in_bom: self.in_bom,
            in_cpl: self.in_cpl,
            symbol: self.symbol,
            footprints,
            fields,
            pin_map: sorted(pin_map),
            provenance,
        }
    }
}

pub fn write_part(part: &Part) -> Result<Vec<u8>, CanonicalError> {
    let mut w = CanonicalWriter::new(FORMAT);
    w.field("part", &PartRow::of(part))?;
    write_provenance(&mut w, &part.provenance)?;
    w.list("footprints", &part.footprints)?;
    w.field("fields", &part.fields)?;
    w.field("pin_map", &sorted(&part.pin_map))?;
    Ok(w.finish())
}

pub fn read_part(path: &Path, bytes: &[u8]) -> Result<Part, LoadError> {
    let file: PartFile = parse(FileKind::Part, path, bytes)?;
    Ok(file
        .part
        .into_part(file.footprints, file.fields, &file.pin_map, file.provenance))
}

/// The `provenance` line, written only for imported elements.
fn write_provenance(
    w: &mut CanonicalWriter,
    provenance: &Option<Box<Provenance>>,
) -> Result<(), CanonicalError> {
    match provenance {
        Some(p) => w.field("provenance", p),
        None => Ok(()),
    }
}

/// The pin map in its canonical order: overrides by Pin number, unconnected Pads sorted.
pub(crate) fn sorted(map: &PinMap) -> PinMap {
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
