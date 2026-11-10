//! The Scene the Footprint editor draws: one Footprint, unplaced, on the fixed v0.1 board
//! Layers, through the same PCB Scene builder as a board, so Pads, mask, paste, holes,
//! drawing and text look exactly as they will once placed.
//!
//! Every item is tagged with what the editor selects: a Pad, graphic or field text with its
//! own UUID.

use reda_command::Fonts;
use reda_model::{
    DocumentEntry, DocumentKind, Footprint, Layer, Project, ProjectProps, Shape, Uuid,
};
use reda_pcb::{board_layer, element_texts, new_board, text_geometry};
use reda_render::{
    BoardGeometry, BoardGraphic, BoardOpening, BoardPad, BoardText, Mesh, Scene, Segment,
};

use crate::builders::add_document;

/// What the Designator shows in the Footprint editor.
pub const DESIGNATOR_PLACEHOLDER: &str = "REF**";

/// A Footprint drawn for its editor: the Scene and the board Layers it is drawn on, for
/// compositing.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FootprintScene {
    pub scene: Scene,
    pub layers: Vec<Layer>,
}

/// The Scene of `footprint` at its own origin, seen from the top: its Pads (copper, holes,
/// numbered, and the mask and paste openings the Generic Fab Preset's seeds give), drawing
/// and text, with the Designator as
/// [`DESIGNATOR_PLACEHOLDER`] and the value as the Footprint's name.
pub fn footprint_scene(footprint: &Footprint, fonts: &Fonts) -> FootprintScene {
    let (project, document) = board_for(footprint.uuid);
    let pcb = project.pcb_document(document).expect("just added");
    let mut geometry = BoardGeometry::default();
    for pad in &footprint.pads {
        geometry.pads.push(BoardPad {
            child: None,
            object: pad.uuid,
            number: pad.number.clone(),
            kind: pad.kind,
            at: pad.at,
            rotation: pad.rotation,
            outline: pad.outline(),
            hole: pad.hole,
            side: pad.copper_side(),
            net: None,
        });
    }
    for g in &footprint.graphics {
        if matches!(g.shape, Shape::Text { .. }) {
            continue;
        }
        if let Some(layer) = board_layer(pcb, g.layer) {
            geometry.graphics.push(BoardGraphic {
                object: g.uuid,
                layer,
                shape: g.shape.clone(),
                width: g.width,
                fill: g.fill,
            });
        }
    }
    let shown = (DESIGNATOR_PLACEHOLDER, footprint.name.as_str());
    for (object, text) in element_texts(pcb, footprint, shown) {
        let laid = text_geometry(&text, fonts);
        geometry.texts.push(BoardText {
            align: text.justify.into(),
            child: None,
            object,
            layer: text.layer,
            text: text.text.clone(),
            at: text.position,
            height: text.height,
            rotation: text.rotation,
            strokes: laid
                .strokes
                .iter()
                .map(|s| Segment::new(s.a, s.b))
                .collect(),
            stroke_width: laid.stroke_width,
            fill: Mesh {
                points: laid.fill.points,
                indices: laid.fill.indices,
            },
        });
    }
    geometry.openings = Some(
        reda_pcb::openings::element_openings(pcb, footprint)
            .into_iter()
            .filter_map(|o| {
                let g = o.geometry?;
                Some(BoardOpening {
                    object: g.pad,
                    side: o.side,
                    paste: o.kind == reda_pcb::openings::Kind::Paste,
                    at: g.at,
                    rotation: g.rotation,
                    outline: g.outline,
                })
            })
            .collect(),
    );
    let scene = reda_render::pcb_scene(&project, document, &geometry).unwrap_or_default();
    FootprintScene {
        scene,
        layers: pcb.layers().layers.clone(),
    }
}

/// A blank Project holding one empty board with the v0.1 Layers, whose UUIDs follow from
/// `seed` so a Footprint's Scene is the same on every build.
fn board_for(seed: Uuid) -> (Project, Uuid) {
    let mut next = seed.as_u128();
    let mut new_id = move || {
        next = next.wrapping_add(1);
        Uuid::from_u128(next)
    };
    let mut project = Project::new(ProjectProps {
        uuid: new_id(),
        name: String::new(),
        documents: vec![],
        libraries: vec![],
        properties: Default::default(),
        erc: Default::default(),
        schematic: Default::default(),
        reannotate: None,
        library_update: None,
        missing: vec![],
    });
    let document = new_id();
    // A Library element has no board: the preview opens mask and paste from the Generic
    // Fab Preset's seeds (library.md#footprint-editor).
    let rules = reda_output::generic().rules;
    let mut tx = project.transaction();
    let entry = DocumentEntry {
        uuid: document,
        kind: DocumentKind::Pcb,
        name: String::new(),
        title: String::new(),
    };
    add_document(&mut tx, entry).expect("a blank Project takes a board");
    for object in new_board(document, &rules, Default::default(), &mut new_id) {
        tx.insert(object).expect("a new board's objects fit");
    }
    tx.commit().expect("a new board commits");
    (project, document)
}
