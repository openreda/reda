//! `<name>.redaboard`: one Board Template, in a Library's `board-templates/` folder
//! (ADR-0016).
//!
//! ```text
//! {
//!   "format": 2,
//!   "uuid": "…",
//!   "name": "Custom rectangle",
//!   "description": "A plain rectangle…",
//!   "category": "Generic",
//!   "suggested_layers": 2,
//!   "parameters": [
//!     {"kind":"length","name":"width","label":"Width","default":"100","min":"1","max":"1000","unit":"mm"}
//!   ],
//!   "outline": [
//!     {"uuid":"…","vertices":[
//!       {"x":"corner_radius","y":0},
//!       {"x":"width - corner_radius","y":0,"sweep":"90"},
//!       …
//!     ],"locked":true}
//!   ]
//! }
//! ```
//!
//! The one exception to integers-only (ADR-0005): a numeric geometry field may be an
//! expression string over the template's parameter names. Tables are sorted by UUID;
//! `parameters` keeps the order the dialog shows. `parts` uses the sheet part table's
//! rows, except that a Mechanical Part may have no Symbol (its `units`, `pins` and
//! `graphics` are then empty); `footprint_copies` uses the Footprint copy rows unchanged.

use std::path::Path;

use reda_model::{
    BoardTemplate, Fields, PinMap, Provenance, SymbolGraphic, TemplateBoard, TemplateContour,
    TemplateDrawing, TemplateDrawingGroup, TemplateHole, TemplateKeepout, TemplateParameter,
    TemplatePart, TemplatePlacement, Unit, Uuid,
};
use reda_model::{Pin, text};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::library::{PartRow, SymbolRow, sorted};
use super::sheet_file::FootprintCopyRow;
use super::{FORMAT, FileKind, LoadError, parse};
use crate::canonical::{CanonicalError, CanonicalWriter};

/// A REDA Board Template file (`.redaboard`).
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(title = "REDA Board Template file")]
pub(crate) struct BoardTemplateFile {
    /// The file-format version. REDA writes only the current one.
    #[allow(dead_code, reason = "checked by the migration chain")]
    #[schemars(schema_with = "super::schema::format_version")]
    format: u32,
    /// The template's identity as a Library element.
    #[schemars(schema_with = "super::schema::uuid")]
    uuid: Uuid,
    /// Unique within its Library. Built-in names refer to the host only.
    name: String,
    /// Compatibility wording, trademark acknowledgements and sources.
    #[serde(default)]
    description: String,
    /// Groups the picker; left out, the template is listed under "Other".
    #[serde(default)]
    category: Option<String>,
    /// A plain copper Layer count, 2 to 32.
    #[schemars(range(min = 2, max = 32))]
    suggested_layers: u8,
    /// Board properties the template sets.
    #[serde(default)]
    board: Option<TemplateBoard>,
    /// The choice and length parameters, in the order the dialog shows them.
    #[serde(default)]
    parameters: Vec<TemplateParameter>,
    /// The Board Outline rows; exactly one survives every choice combination.
    outline: Vec<TemplateContour>,
    /// The Board Cutouts.
    #[serde(default)]
    cutouts: Vec<TemplateContour>,
    /// The Keepouts.
    #[serde(default)]
    keepouts: Vec<TemplateKeepout>,
    /// The part table: one copied Part, with its Symbol if it has one, per entry.
    #[serde(default)]
    parts: Vec<TemplatePartRow>,
    /// The copied Footprints the part table's entries share.
    #[serde(default)]
    footprint_copies: Vec<FootprintCopyRow>,
    /// The connectors: Parts placed as linked Components.
    #[serde(default)]
    connectors: Vec<TemplatePlacement>,
    /// The holes: Mechanical Parts or Free Pads.
    #[serde(default)]
    holes: Vec<TemplateHole>,
    /// The named drawing groups, one Includes tick each.
    #[serde(default)]
    drawing_groups: Vec<TemplateDrawingGroup>,
    /// The drawings, each in one group.
    #[serde(default)]
    drawings: Vec<TemplateDrawing>,
}

/// One part-table entry: a sheet's part-table row whose Symbol may be left out.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct TemplatePartRow {
    /// The entry's own UUID, which connector and hole rows name.
    uuid: Uuid,
    /// The content hash of the Part with its Symbol and Footprints, as copied.
    hash: String,
    part: PartRow,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    provenance: Option<Box<Provenance>>,
    /// The alternate Footprints by Library UUID, the default first.
    footprints: Vec<Uuid>,
    /// The template's Footprint copy of each alternate, in the same order.
    copies: Vec<Uuid>,
    fields: Fields,
    pin_map: PinMap,
    /// Left out for a Mechanical Part without a Symbol.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    symbol: Option<SymbolRow>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    symbol_provenance: Option<Box<Provenance>>,
    #[serde(default)]
    units: Vec<Unit>,
    #[serde(default)]
    pins: Vec<Pin>,
    #[serde(default)]
    graphics: Vec<SymbolGraphic>,
}

impl TemplatePartRow {
    fn of(entry: &TemplatePart) -> TemplatePartRow {
        let symbol = entry.symbol.as_ref();
        TemplatePartRow {
            uuid: entry.uuid,
            hash: entry.hash.clone(),
            part: PartRow::of(&entry.part),
            provenance: entry.part.provenance.clone(),
            footprints: entry.part.footprints.clone(),
            copies: entry.footprints.clone(),
            fields: entry.part.fields.clone(),
            pin_map: sorted(&entry.part.pin_map),
            symbol: symbol.map(SymbolRow::of),
            symbol_provenance: symbol.and_then(|s| s.provenance.clone()),
            units: symbol.map(|s| s.units.clone()).unwrap_or_default(),
            pins: symbol.map(|s| s.pins.clone()).unwrap_or_default(),
            graphics: symbol.map(|s| s.graphics.clone()).unwrap_or_default(),
        }
    }

    fn into_entry(self) -> Result<TemplatePart, String> {
        let uuid = self.uuid;
        let symbol = match self.symbol {
            Some(row) => Some(
                row.into_symbol(self.units, self.pins, self.graphics, self.symbol_provenance)
                    .map_err(|e| format!("part entry {uuid}: {e}"))?,
            ),
            None if self.units.is_empty() && self.pins.is_empty() && self.graphics.is_empty() => {
                None
            }
            None => {
                return Err(format!(
                    "part entry {uuid} has Units, Pins or graphics but no Symbol"
                ));
            }
        };
        let part =
            self.part
                .into_part(self.footprints, self.fields, &self.pin_map, self.provenance);
        Ok(TemplatePart {
            uuid,
            hash: self.hash,
            part,
            symbol,
            footprints: self.copies,
        })
    }
}

/// Reads a `.redaboard` file. Only the file shape is checked here; the template's own
/// load checks are `reda_library::board_template::check`.
pub fn read_board_template(path: &Path, bytes: &[u8]) -> Result<BoardTemplate, LoadError> {
    let invalid = |why: String| LoadError::Invalid(path.to_owned(), why);
    let file: BoardTemplateFile = parse(FileKind::BoardTemplate, path, bytes)?;
    let parts = file
        .parts
        .into_iter()
        .map(TemplatePartRow::into_entry)
        .collect::<Result<Vec<_>, _>>()
        .map_err(invalid)?;
    let footprint_copies = file
        .footprint_copies
        .into_iter()
        .map(FootprintCopyRow::into_copy)
        .collect::<Result<Vec<_>, _>>()
        .map_err(invalid)?;
    let template = BoardTemplate {
        uuid: file.uuid,
        name: file.name,
        description: file.description,
        category: file.category,
        suggested_layers: file.suggested_layers,
        board: file.board,
        parameters: file.parameters,
        outline: file.outline,
        cutouts: file.cutouts,
        keepouts: file.keepouts,
        parts,
        footprint_copies,
        connectors: file.connectors,
        holes: file.holes,
        drawing_groups: file.drawing_groups,
        drawings: file.drawings,
    };
    let mut texts = vec![];
    for entry in &template.parts {
        text::part_texts(&entry.part, &mut texts);
    }
    text::check_texts(&format!("board template {}", template.uuid), &texts).map_err(invalid)?;
    Ok(template)
}

/// Writes a Board Template as its canonical `.redaboard` bytes.
pub fn write_board_template(template: &BoardTemplate) -> Result<Vec<u8>, CanonicalError> {
    let mut w = CanonicalWriter::new(FORMAT);
    w.field("uuid", &template.uuid)?;
    w.field("name", &template.name)?;
    w.field("description", &template.description)?;
    if let Some(category) = &template.category {
        w.field("category", category)?;
    }
    w.field("suggested_layers", &template.suggested_layers)?;
    if let Some(board) = &template.board {
        w.field("board", board)?;
    }
    if !template.parameters.is_empty() {
        w.list("parameters", &template.parameters)?;
    }
    w.containers("outline", &template.outline, |r| r.uuid, "vertices")?;
    if !template.cutouts.is_empty() {
        w.containers("cutouts", &template.cutouts, |r| r.uuid, "vertices")?;
    }
    if !template.keepouts.is_empty() {
        w.containers("keepouts", &template.keepouts, |r| r.uuid, "vertices")?;
    }
    if !template.parts.is_empty() {
        let rows = template.parts.iter().map(TemplatePartRow::of);
        w.containers_spread("parts", rows, |r| r.uuid, &["units", "pins", "graphics"])?;
    }
    if !template.footprint_copies.is_empty() {
        let rows = template.footprint_copies.iter().map(FootprintCopyRow::of);
        w.containers_spread(
            "footprint_copies",
            rows,
            FootprintCopyRow::uuid,
            &["pads", "graphics", "texts"],
        )?;
    }
    if !template.connectors.is_empty() {
        w.table("connectors", &template.connectors, |r| r.uuid)?;
    }
    if !template.holes.is_empty() {
        w.table("holes", &template.holes, |r| r.uuid)?;
    }
    if !template.drawing_groups.is_empty() {
        w.table("drawing_groups", &template.drawing_groups, |r| r.uuid)?;
    }
    if !template.drawings.is_empty() {
        w.table("drawings", &template.drawings, |r| r.uuid)?;
    }
    Ok(w.finish())
}
