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
//!   "board": {"uuid":"…"},
//!   "rules": [
//!     {"uuid":"…","kind":"clearance","scope":{},"values":{"value":160000}},
//!     {"uuid":"…","kind":"track_width","scope":{"net_class":"…"},"values":{"preferred":500000}},
//!     …
//!   ],
//!   "severities": {"clearance":"error",…},
//!   "board_outline": {"uuid":"…","vertices":[
//!     {"x":0,"y":0},
//!     …
//!   ]},
//!   "board_cutouts": [],
//!   "net_classes": [
//!     {"uuid":"…","name":"Power","rank":1}
//!   ],
//!   "nets": [
//!     {"uuid":"…","name":"GND","classes":["…"]}
//!   ],
//!   "texts": [
//!     {"uuid":"…","layer":"…","text":"Demo","font":"Hershey Roman Simplex","position":{"x":0,"y":0},"height":1000000,"thickness":150000}
//!   ],
//!   "graphics": [
//!     {"uuid":"…","layer":"…","width":150000,"shape":{"kind":"line","from":{…},"to":{…}}}
//!   ],
//!   "footprint_copies": [
//!     {"uuid":"…","hash":"…","footprint":{…},"text":{…},"pads":[
//!       {"uuid":"…","number":"1",…}
//!     ],"graphics":[]}
//!   ],
//!   "footprints": [
//!     {"uuid":"…","component":{"uuid":"…"},"footprint":"…","at":{…},"rotation":0,"side":"top","designator":"R1","fields":{…},"pads":[
//!       {"pad":"…","net":"…"}
//!     ]}
//!   ],
//!   "tracks": [
//!     {"uuid":"…","layer":"…","net":"…","width":200000,"geometry":{"kind":"line","from":{…},"to":{…}}}
//!   ],
//!   "vias": [
//!     {"uuid":"…","at":{…},"net":"…","diameter":600000,"hole":300000,"span":{"from":"…","to":"…"}}
//!   ],
//!   "free_pads": [
//!     {"uuid":"…","kind":"plated_through_hole","shape":"circle","at":{…},"rotation":0,"width":6000000,"height":6000000,"hole":{…},"net":"…"}
//!   ],
//!   "placement_origin": {"uuid":"…","at":{"x":0,"y":0}},
//!   "zones": [
//!     {"uuid":"…","layer":"…","net":"…","vertices":[
//!       {"x":0,"y":0},
//!       …
//!     ],"clearance":160000,"min_width":200000,"pad_connection":"thermal",…}
//!   ],
//!   "keepouts": [
//!     {"uuid":"…","layers":["…"],"vertices":[
//!       {"x":0,"y":0},
//!       …
//!     ],"copper":true,"vias":true}
//!   ],
//!   "waivers": [
//!     {"uuid":"…","check":"clearance","objects":["…","…"],"comment":"…"}
//!   ]
//! }
//! ```
//!
//! `rules` holds the design rule records, one per line: exactly one board-wide record per
//! kind, and the Net Class records. The Default Net Class is not stored: its values are
//! the board-wide records, and a Net lists only its other classes.
//! `board_outline` is left out while the board has none. `footprint_copies` (the part
//! table, rows shaped like a sheet file's `footprints`) and `footprints` (the Footprint
//! instances, each Pad's Net on its own line) are left out while the board has no
//! Footprints. `tracks`, `vias`, `free_pads`, `zones`, `keepouts` and `waivers` (of DRC
//! Violations) are left out while the board has none, `placement_origin` while it is not
//! set. Zone fills are never stored here: they live in the `.reda/` cache.

use std::path::Path;

use reda_model::{
    BoardCutout, BoardOutline, BoardSetup, DesignRule, DrcSeverities, FootprintInstance, FreePad,
    InDocument, Keepout, LayerSet, Net, NetClass, Object, PcbDocument, PcbGraphic, PcbText,
    PlacementOrigin, Track, Uuid, Via, Waiver, Zone,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::sheet_file::FootprintCopyRow;
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
    /// The board setup's identity.
    board: BoardRow,
    /// The design rule records: one board-wide record per kind, and Net Class records.
    rules: Vec<DesignRule>,
    /// The severity of each DRC check.
    severities: DrcSeverities,
    /// The board's edge; left out while the board has none.
    #[serde(default)]
    board_outline: Option<BoardOutline>,
    /// Holes in the board material.
    board_cutouts: Vec<BoardCutout>,
    /// The Net Classes besides the Default Net Class, which is not stored.
    net_classes: Vec<NetClass>,
    /// The PCB Nets.
    nets: Vec<Net>,
    /// Free text on copper, silkscreen, mechanical and documentation Layers.
    texts: Vec<PcbText>,
    /// Lines, arcs and rectangles on copper, silkscreen, mechanical and documentation
    /// Layers.
    graphics: Vec<PcbGraphic>,
    /// The part table: a copy of each Footprint in use, sorted by UUID.
    #[serde(default)]
    footprint_copies: Vec<FootprintCopyRow>,
    /// The placed Footprint instances, sorted by UUID.
    #[serde(default)]
    footprints: Vec<FootprintInstance>,
    /// The copper Track segments; left out while the board has none.
    #[serde(default)]
    tracks: Vec<Track>,
    /// The Vias; left out while the board has none.
    #[serde(default)]
    vias: Vec<Via>,
    /// Pads that belong to no Footprint, sorted by UUID; left out while there are none.
    #[serde(default)]
    free_pads: Vec<FreePad>,
    /// The point the CPL measures from; left out while it is not set.
    #[serde(default)]
    placement_origin: Option<PlacementOrigin>,
    /// The copper Zones (outlines and settings, never fills); left out while the board
    /// has none.
    #[serde(default)]
    zones: Vec<Zone>,
    /// The Keepouts; left out while the board has none.
    #[serde(default)]
    keepouts: Vec<Keepout>,
    /// The Waivers of DRC Violations, sorted by UUID; left out while the board has none.
    #[serde(default)]
    waivers: Vec<Waiver>,
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
}

pub(crate) fn write_pcb_file(pcb: &PcbDocument) -> Result<Vec<u8>, CanonicalError> {
    let setup = pcb.setup();
    let mut w = CanonicalWriter::new(FORMAT);
    w.field("pcb", &PcbRow { uuid: pcb.uuid })?;
    w.container("layers", pcb.layers(), "layers")?;
    w.field("board", &BoardRow { uuid: setup.uuid })?;
    w.table("rules", pcb.rules(), |r| r.uuid)?;
    w.field("severities", &setup.severities)?;
    if let Some(outline) = pcb.outline() {
        w.container("board_outline", outline, "vertices")?;
    }
    w.containers("board_cutouts", pcb.cutouts(), |c| c.uuid, "vertices")?;
    w.table("net_classes", pcb.net_classes(), |c| c.uuid)?;
    w.table("nets", pcb.nets(), |n| n.uuid)?;
    w.table("texts", pcb.texts(), |t| t.uuid)?;
    w.table("graphics", pcb.graphics(), |g| g.uuid)?;
    if pcb.footprint_copies().next().is_some() || pcb.footprints().next().is_some() {
        let copies = pcb.footprint_copies().map(FootprintCopyRow::of);
        w.containers_spread(
            "footprint_copies",
            copies,
            FootprintCopyRow::uuid,
            &["pads", "graphics"],
        )?;
        w.containers("footprints", pcb.footprints(), |f| f.uuid, "pads")?;
    }
    if pcb.tracks().next().is_some() {
        w.table("tracks", pcb.tracks(), |t| t.uuid)?;
    }
    if pcb.vias().next().is_some() {
        w.table("vias", pcb.vias(), |v| v.uuid)?;
    }
    if pcb.free_pads().next().is_some() {
        w.table("free_pads", pcb.free_pads(), |p| p.uuid)?;
    }
    if let Some(origin) = pcb.placement_origin() {
        w.field("placement_origin", origin)?;
    }
    if pcb.zones().next().is_some() {
        w.containers("zones", pcb.zones(), |z| z.uuid, "vertices")?;
    }
    if pcb.keepouts().next().is_some() {
        w.containers("keepouts", pcb.keepouts(), |k| k.uuid, "vertices")?;
    }
    if pcb.waivers().next().is_some() {
        w.table("waivers", pcb.waivers(), |r| r.uuid)?;
    }
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
                severities: file.severities,
            },
        )),
    ];
    objects.extend(
        file.rules
            .into_iter()
            .map(|r| Object::DesignRule(InDocument::new(doc, r))),
    );
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
    for row in file.footprint_copies {
        let copy = row
            .into_copy()
            .map_err(|why| LoadError::Invalid(path.to_owned(), why))?;
        objects.push(Object::FootprintCopy(InDocument::new(doc, copy)));
    }
    objects.extend(
        file.footprints
            .into_iter()
            .map(|f| Object::FootprintInstance(InDocument::new(doc, f))),
    );
    objects.extend(
        file.tracks
            .into_iter()
            .map(|t| Object::Track(InDocument::new(doc, t))),
    );
    objects.extend(
        file.vias
            .into_iter()
            .map(|v| Object::Via(InDocument::new(doc, v))),
    );
    objects.extend(
        file.free_pads
            .into_iter()
            .map(|p| Object::FreePad(InDocument::new(doc, p))),
    );
    objects.extend(
        file.placement_origin
            .map(|o| Object::PlacementOrigin(InDocument::new(doc, o))),
    );
    objects.extend(
        file.zones
            .into_iter()
            .map(|z| Object::Zone(InDocument::new(doc, z))),
    );
    objects.extend(
        file.keepouts
            .into_iter()
            .map(|k| Object::Keepout(InDocument::new(doc, k))),
    );
    objects.extend(
        file.waivers
            .into_iter()
            .map(|w| Object::Waiver(InDocument::new(doc, w))),
    );
    Ok(objects)
}
