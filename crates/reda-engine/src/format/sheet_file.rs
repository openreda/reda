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
//!   ],
//!   "wires": [
//!     {"uuid":"…","vertices":[{"x":0,"y":0},{"x":2540000,"y":0},{"x":2540000,"y":2540000}]}
//!   ],
//!   "net_labels": [
//!     {"uuid":"…","name":"SDA","at":{…},"rotation":0}
//!   ],
//!   "no_connects": [
//!     {"uuid":"…","at":{…}}
//!   ],
//!   "ports": [
//!     {"uuid":"…","name":"SDA","direction":"bidirectional","at":{…},"rotation":0}
//!   ],
//!   "power_ports": [
//!     {"uuid":"…","name":"GND","style":"ground","at":{…},"rotation":0}
//!   ],
//!   "texts": [
//!     {"uuid":"…","text":"Power","at":{…},"height":1270000,"rotation":0}
//!   ],
//!   "graphics": [
//!     {"uuid":"…","shape":{"kind":"rect","from":{…},"to":{…}}}
//!   ],
//!   "waivers": [
//!     {"uuid":"…","check":"unconnected_pin","objects":["…","…"],"author":"Ada","created":1767225600000,"reason":"test point"}
//!   ]
//! }
//! ```
//!
//! `frame` is written only when the sheet has one, and the three tables only when the sheet
//! holds any Component; `wires`, `net_labels`, `no_connects`, `ports`, `power_ports`, `texts`,
//! `graphics` and `waivers` only when not empty. Nets
//! and Junctions are derived from the drawing, never written. A part-table entry (`parts`) holds the
//! keys of a `.redapart` file (`part`, `footprints`, `fields`, `pin_map`) and of a
//! `.redasym` file (`symbol`, `units`, `pins`, `graphics`) with the same shapes, plus its own `uuid`,
//! its content `hash` and `copies`: the sheet's Footprint copy of each alternate. An
//! imported Part's `provenance` and its Symbol's `symbol_provenance` are kept too. A
//! Footprint copy (`footprints`) holds the keys of a `.redafp` file. Rows leave out the
//! sheet their model objects name: the file is the sheet.

use std::path::Path;

use reda_model::{
    EditedFrom, Fields, FootprintCopy, FootprintGraphic, FootprintText, InDocument, NetLabel,
    NoConnect, Object, Pad, PartEntry, Pin, PinMap, Port, PowerPort, Provenance, SchematicSheet,
    SheetFrame, SheetGraphic, SheetText, SymbolGraphic, Unit, UnitInstance, Uuid, Waiver, Wire,
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
    /// The Wires: one orthogonal segment each, sorted by UUID.
    #[serde(default)]
    wires: Vec<Wire>,
    /// The Net Labels, sorted by UUID.
    #[serde(default)]
    net_labels: Vec<NetLabel>,
    /// The No-Connect Markers, sorted by UUID.
    #[serde(default)]
    no_connects: Vec<NoConnect>,
    /// The Ports, which join Nets across sheets, sorted by UUID.
    #[serde(default)]
    ports: Vec<Port>,
    /// The Power Ports, which join Nets across the Project, sorted by UUID.
    #[serde(default)]
    power_ports: Vec<PowerPort>,
    /// The free texts, sorted by UUID.
    #[serde(default)]
    texts: Vec<SheetText>,
    /// The free lines and rectangles, sorted by UUID.
    #[serde(default)]
    graphics: Vec<SheetGraphic>,
    /// The Waivers of ERC Violations whose objects are on this sheet, sorted by UUID.
    #[serde(default)]
    waivers: Vec<Waiver>,
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
    /// Where the Part was imported from, as in a `.redapart` file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    provenance: Option<Box<Provenance>>,
    /// The alternate Footprints by Library UUID, the default first.
    footprints: Vec<Uuid>,
    /// The sheet's Footprint copy of each alternate, in the same order.
    copies: Vec<Uuid>,
    fields: Fields,
    pin_map: PinMap,
    symbol: SymbolRow,
    /// Where the Symbol was imported from, as in a `.redasym` file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    symbol_provenance: Option<Box<Provenance>>,
    units: Vec<Unit>,
    pins: Vec<Pin>,
    graphics: Vec<SymbolGraphic>,
}

/// One copied Footprint: a `.redafp` file's keys in one row. The PCB file's part table
/// uses the same rows.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct FootprintCopyRow {
    uuid: Uuid,
    /// The content hash of the Footprint, as copied.
    hash: String,
    /// On a PCB's Locally Edited Footprint: the copy it was edited from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    edited_from: Option<EditedFrom>,
    footprint: FootprintRow,
    /// Where the Footprint was imported from, as in a `.redafp` file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    provenance: Option<Box<Provenance>>,
    pads: Vec<Pad>,
    graphics: Vec<FootprintGraphic>,
    texts: Vec<FootprintText>,
}

impl FootprintCopyRow {
    pub(crate) fn of(c: &FootprintCopy) -> FootprintCopyRow {
        FootprintCopyRow {
            uuid: c.uuid,
            hash: c.hash.clone(),
            edited_from: c.edited_from.clone(),
            footprint: FootprintRow::of(&c.footprint),
            provenance: c.footprint.provenance.clone(),
            pads: c.footprint.pads.clone(),
            graphics: c.footprint.graphics.clone(),
            texts: c.footprint.texts.clone(),
        }
    }

    pub(crate) fn uuid(&self) -> Uuid {
        self.uuid
    }

    /// The copy this row holds, or why its Footprint is malformed.
    pub(crate) fn into_copy(self) -> Result<FootprintCopy, String> {
        let uuid = self.uuid;
        let footprint = self
            .footprint
            .into_footprint(self.pads, self.graphics, self.texts, self.provenance)
            .map_err(|e| format!("Footprint copy {uuid}: {e}"))?;
        Ok(FootprintCopy {
            uuid,
            hash: self.hash,
            edited_from: self.edited_from,
            footprint,
        })
    }
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
        provenance: e.part.provenance.clone(),
        footprints: e.part.footprints.clone(),
        copies: e.footprints.clone(),
        fields: e.part.fields.clone(),
        pin_map: sorted(&e.part.pin_map),
        symbol: SymbolRow::of(&e.symbol),
        symbol_provenance: e.symbol.provenance.clone(),
        units: e.symbol.units.clone(),
        pins: e.symbol.pins.clone(),
        graphics: e.symbol.graphics.clone(),
    });
    let footprints = sheet.footprints().map(FootprintCopyRow::of);
    // A sheet with no Components keeps the short form of an empty sheet.
    if sheet.parts().next().is_some() || sheet.units().next().is_some() {
        w.containers_spread("parts", parts, |r| r.uuid, &["units", "pins", "graphics"])?;
        w.containers_spread(
            "footprints",
            footprints,
            |r| r.uuid,
            &["pads", "graphics", "texts"],
        )?;
        w.table("units", sheet.units(), |u| u.uuid)?;
    }
    if sheet.wires().next().is_some() {
        w.table("wires", sheet.wires(), |r| r.uuid)?;
    }
    if sheet.net_labels().next().is_some() {
        w.table("net_labels", sheet.net_labels(), |r| r.uuid)?;
    }
    if sheet.no_connects().next().is_some() {
        w.table("no_connects", sheet.no_connects(), |r| r.uuid)?;
    }
    if sheet.ports().next().is_some() {
        w.table("ports", sheet.ports(), |r| r.uuid)?;
    }
    if sheet.power_ports().next().is_some() {
        w.table("power_ports", sheet.power_ports(), |r| r.uuid)?;
    }
    if sheet.texts().next().is_some() {
        w.table("texts", sheet.texts(), |r| r.uuid)?;
    }
    if sheet.graphics().next().is_some() {
        w.table("graphics", sheet.graphics(), |r| r.uuid)?;
    }
    if sheet.waivers().next().is_some() {
        w.table("waivers", sheet.waivers(), |r| r.uuid)?;
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
        let copy = row.into_copy().map_err(invalid)?;
        objects.push(Object::FootprintCopy(InDocument::new(sheet, copy)));
    }
    for row in file.parts {
        let symbol = row
            .symbol
            .into_symbol(row.units, row.pins, row.graphics, row.symbol_provenance)
            .map_err(|e| invalid(format!("part entry {}: {e}", row.uuid)))?;
        let part = row
            .part
            .into_part(row.footprints, row.fields, &row.pin_map, row.provenance);
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
    for wire in file.wires {
        objects.push(Object::Wire(InDocument::new(sheet, wire)));
    }
    for label in file.net_labels {
        objects.push(Object::NetLabel(InDocument::new(sheet, label)));
    }
    for marker in file.no_connects {
        objects.push(Object::NoConnect(InDocument::new(sheet, marker)));
    }
    for port in file.ports {
        objects.push(Object::Port(InDocument::new(sheet, port)));
    }
    for port in file.power_ports {
        objects.push(Object::PowerPort(InDocument::new(sheet, port)));
    }
    for text in file.texts {
        objects.push(Object::SheetText(InDocument::new(sheet, text)));
    }
    for graphic in file.graphics {
        objects.push(Object::SheetGraphic(InDocument::new(sheet, graphic)));
    }
    for waiver in file.waivers {
        objects.push(Object::Waiver(InDocument::new(sheet, waiver)));
    }
    for object in &objects {
        reda_model::text::check_object(object).map_err(invalid)?;
    }
    Ok(objects)
}
