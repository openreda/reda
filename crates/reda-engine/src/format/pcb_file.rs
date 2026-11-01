//! `<name>.redapcb`: the PCB Document.
//!
//! ```text
//! {
//!   "format": 1,
//!   "pcb": {"uuid":"…"},
//!   "layers": {"uuid":"…","layers":[
//!     {"uuid":"…","kind":"courtyard","side":"top"},
//!     …
//!   ]},
//!   "board": {"uuid":"…","default_net_class":"…"},
//!   "rules": {"clearance":160000,"track_width":200000,…},
//!   "severities": {"clearance":"error",…},
//!   "board_outline": {"uuid":"…","vertices":[
//!     {"x":0,"y":0},
//!     …
//!   ]},
//!   "board_cutouts": [],
//!   "net_classes": [
//!     {"uuid":"…","name":"Default"}
//!   ],
//!   "nets": [],
//!   "texts": [
//!     {"uuid":"…","layer":"…","text":"Demo","font":"Hershey Roman Simplex","position":{"x":0,"y":0},"height":1000000,"thickness":150000}
//!   ],
//!   "graphics": [
//!     {"uuid":"…","layer":"…","width":150000,"shape":{"kind":"line","from":{…},"to":{…}}}
//!   ]
//! }
//! ```
//!
//! The board setup is one object in the model; the file splits it over the `board`, `rules`
//! and `severities` lines so a rule change and a severity change touch different lines.
//! `board_outline` is left out while the board has none.

use std::path::Path;

use reda_model::{
    BoardCutout, BoardOutline, BoardSetup, DrcSeverities, InDocument, LayerSet, Net, NetClass,
    Object, PcbDocument, PcbGraphic, PcbText, Rules, Uuid,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{FORMAT, FileKind, LoadError, parse};
use crate::canonical::{CanonicalError, CanonicalWriter};

/// A REDA PCB Document file (`.redapcb`).
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(title = "REDA PCB Document file")]
pub(crate) struct PcbFile {
    /// The file-format version. REDA writes only the current one.
    #[allow(dead_code, reason = "checked by the migration chain")]
    #[schemars(schema_with = "super::schema::format_version")]
    format: u32,
    /// The PCB Document's identity; it matches its entry in the Project file.
    pcb: PcbRow,
    /// The Layers, top to bottom. Objects name a Layer by its UUID.
    layers: LayerSet,
    /// The board setup's identity and its Default Net Class.
    board: BoardRow,
    /// The board-wide design rules.
    rules: Rules,
    /// The severity of each DRC check.
    severities: DrcSeverities,
    /// The board's edge; left out while the board has none.
    #[serde(default)]
    board_outline: Option<BoardOutline>,
    /// Holes in the board material.
    board_cutouts: Vec<BoardCutout>,
    /// The Net Classes, the Default Net Class among them.
    net_classes: Vec<NetClass>,
    /// The PCB Nets.
    nets: Vec<Net>,
    /// Free text on copper, silkscreen, mechanical and documentation Layers.
    texts: Vec<PcbText>,
    /// Lines, arcs and rectangles on copper, silkscreen, mechanical and documentation
    /// Layers.
    graphics: Vec<PcbGraphic>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct PcbRow {
    #[schemars(schema_with = "super::schema::uuid")]
    uuid: Uuid,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct BoardRow {
    #[schemars(schema_with = "super::schema::uuid")]
    uuid: Uuid,
    /// The Net Class new Nets get.
    #[schemars(schema_with = "super::schema::uuid")]
    default_net_class: Uuid,
}

pub(crate) fn write_pcb_file(pcb: &PcbDocument) -> Result<Vec<u8>, CanonicalError> {
    let setup = pcb.setup();
    let mut w = CanonicalWriter::new(FORMAT);
    w.field("pcb", &PcbRow { uuid: pcb.uuid })?;
    w.container("layers", pcb.layers(), "layers")?;
    w.field(
        "board",
        &BoardRow {
            uuid: setup.uuid,
            default_net_class: setup.default_net_class,
        },
    )?;
    w.field("rules", &setup.rules)?;
    w.field("severities", &setup.severities)?;
    if let Some(outline) = pcb.outline() {
        w.container("board_outline", outline, "vertices")?;
    }
    w.containers("board_cutouts", pcb.cutouts(), |c| c.uuid, "vertices")?;
    w.table("net_classes", pcb.net_classes(), |c| c.uuid)?;
    w.table("nets", pcb.nets(), |n| n.uuid)?;
    w.table("texts", pcb.texts(), |t| t.uuid)?;
    w.table("graphics", pcb.graphics(), |g| g.uuid)?;
    Ok(w.finish())
}

/// Reads a PCB file, checks it is the Document the Project file says it is, and returns its
/// objects for the loader to insert.
pub(crate) fn read_pcb_file(
    path: &Path,
    bytes: &[u8],
    expected: Uuid,
) -> Result<Vec<Object>, LoadError> {
    let file: PcbFile = parse(FileKind::Pcb, path, bytes)?;
    if file.pcb.uuid != expected {
        return Err(LoadError::Invalid(
            path.to_owned(),
            format!(
                "holds PCB Document {} but the Project file lists it as {expected}",
                file.pcb.uuid
            ),
        ));
    }
    let doc = expected;
    let mut objects = vec![
        Object::LayerSet(InDocument::new(doc, file.layers)),
        Object::BoardSetup(InDocument::new(
            doc,
            BoardSetup {
                uuid: file.board.uuid,
                default_net_class: file.board.default_net_class,
                rules: file.rules,
                severities: file.severities,
            },
        )),
    ];
    objects.extend(
        file.board_outline
            .map(|o| Object::BoardOutline(InDocument::new(doc, o))),
    );
    objects.extend(
        file.board_cutouts
            .into_iter()
            .map(|c| Object::BoardCutout(InDocument::new(doc, c))),
    );
    objects.extend(
        file.net_classes
            .into_iter()
            .map(|c| Object::NetClass(InDocument::new(doc, c))),
    );
    objects.extend(
        file.nets
            .into_iter()
            .map(|n| Object::Net(InDocument::new(doc, n))),
    );
    objects.extend(
        file.texts
            .into_iter()
            .map(|t| Object::PcbText(InDocument::new(doc, t))),
    );
    objects.extend(
        file.graphics
            .into_iter()
            .map(|g| Object::PcbGraphic(InDocument::new(doc, g))),
    );
    Ok(objects)
}
