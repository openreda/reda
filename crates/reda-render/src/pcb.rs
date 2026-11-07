//! Building the Scene of a PCB Document.
//!
//! What needs PCB behaviour (placed Footprint geometry, laid-out text, the Ratsnest, Zone
//! fill staleness) comes in as a [`BoardGeometry`], because this crate does not depend on
//! `reda-pcb`: the engine builds it. Everything else is read from the Document itself.

mod copper;
mod drawing;
mod labels;
mod zones;

use std::collections::BTreeMap;

use reda_model::geom::{CornerRect, Millideg, Nm, Point};
use reda_model::{Hole, Layer, LayerKind, PadKind, PcbDocument, Project, Shape, Side, Uuid};
use reda_text::{Mesh, Segment};

use crate::color::{BoardColor, ColorKey};
use crate::scene::{Primitive, Scene, SceneLayer, SceneSource};

/// What a PCB Scene needs from `reda-pcb`, already worked out.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BoardGeometry {
    /// Every Pad on the board: Footprint Pads and Free Pads.
    pub pads: Vec<BoardPad>,
    /// Every Footprint's drawing, on board Layers.
    pub graphics: Vec<BoardGraphic>,
    /// Every text on the board, laid out: free text, Designators and Footprint text.
    pub texts: Vec<BoardText>,
    /// The Ratsnest.
    pub ratsnest: Vec<RatsnestLine>,
    /// Each Zone's fill state, by Zone UUID; a Zone left out counts as never filled.
    pub zones: BTreeMap<Uuid, ZoneFillState>,
}

/// A Pad placed on the board.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoardPad {
    pub child: Option<crate::Child>,
    /// What selecting the Pad selects: its Footprint instance, or the Free Pad.
    pub object: Uuid,
    pub number: String,
    pub kind: PadKind,
    /// The anchor, where the hole is.
    pub at: Point,
    pub rotation: Millideg,
    /// The copper outline in the Pad's frame on the board, about `at`.
    pub outline: CornerRect,
    /// The hole, its slot turned in the Pad's frame.
    pub hole: Option<Hole>,
    /// The copper side of an SMD Pad; `None` for a Pad with a hole.
    pub side: Option<Side>,
    pub net: Option<Uuid>,
}

impl BoardPad {
    /// The centre of the copper on the board.
    pub fn copper_center(&self) -> Point {
        (self.outline.center + self.at).rotated_about(self.at, self.rotation)
    }
}

/// A Footprint's drawn shape, on a board Layer. Text shapes come as [`BoardText`]s.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoardGraphic {
    /// The Footprint instance.
    pub object: Uuid,
    pub layer: Uuid,
    pub shape: Shape,
    pub width: Nm,
    pub fill: bool,
}

/// Text laid out on the board.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoardText {
    pub align: crate::Align,
    /// Optional independent field text identity beneath its owning Footprint.
    pub child: Option<crate::Child>,
    /// The text object, or the Footprint instance for its Designator and text.
    pub object: Uuid,
    pub layer: Uuid,
    pub text: String,
    /// Where the text is anchored.
    pub at: Point,
    /// Capital height.
    pub height: Nm,
    pub rotation: Millideg,
    /// Stroke-font pen strokes, drawn `stroke_width` wide.
    pub strokes: Vec<Segment>,
    pub stroke_width: Nm,
    /// Outline-font glyphs, filled.
    pub fill: Mesh,
}

/// One line of the Ratsnest: copper of `net` still to be joined.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RatsnestLine {
    pub net: Uuid,
    pub from: Point,
    pub to: Point,
}

/// Whether a Zone's stored fill matches the board.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ZoneFillState {
    /// Filled from the board as it is: drawn in the Layer's colour.
    Filled,
    /// Filled, but the board changed since: drawn dimmed.
    Stale,
    /// Never filled: only the outline is drawn.
    #[default]
    Unfilled,
}

/// The Scene of PCB Document `document`, or `None` when the Project has no such PCB.
///
/// Each Layer's items go in this order, so later kinds draw over earlier ones: Zone fills,
/// Zone outlines, Tracks, Pads, Vias, Footprint and free drawing, text, the Board Outline
/// and cutouts, then the view-only labels (Pad numbers, Net names on Pads and Tracks).
/// Holes go on [`SceneLayer::Drill`] and the Ratsnest on [`SceneLayer::Ratsnest`].
pub fn pcb_scene(project: &Project, document: Uuid, geometry: &BoardGeometry) -> Option<Scene> {
    let pcb = project.pcb_document(document)?;
    let mut b = Builder {
        scene: Scene::new(SceneSource::Pcb(document)),
        pcb,
        layers: pcb.layers().layers.iter().map(|l| (l.uuid, l)).collect(),
    };
    zones::build(&mut b, project.zone_fills(document), &geometry.zones);
    copper::tracks(&mut b);
    copper::pads(&mut b, &geometry.pads);
    copper::vias(&mut b);
    drawing::graphics(&mut b, &geometry.graphics);
    drawing::texts(&mut b, &geometry.texts);
    drawing::outline(&mut b);
    labels::pads(&mut b, &geometry.pads);
    labels::tracks(&mut b);
    copper::holes(&mut b, &geometry.pads);
    labels::drilled_pads(&mut b, &geometry.pads);
    drawing::ratsnest(&mut b, &geometry.ratsnest);
    Some(b.scene)
}

pub(crate) struct Builder<'a> {
    scene: Scene,
    pcb: &'a PcbDocument,
    layers: BTreeMap<Uuid, &'a Layer>,
}

impl Builder<'_> {
    /// Pushes onto a board Layer in that Layer's colour; nothing for an unknown Layer.
    fn on_layer(&mut self, object: Uuid, layer: Uuid, primitive: Primitive) {
        if let Some(l) = self.layers.get(&layer) {
            let color = BoardColor::layer(l.kind, l.side);
            self.push(object, SceneLayer::Board(layer), color, primitive);
        }
    }

    fn push(&mut self, object: Uuid, layer: SceneLayer, color: BoardColor, primitive: Primitive) {
        self.scene
            .push(object, layer, ColorKey::Board(color), primitive);
    }

    /// The Layer of `kind` on `side`, if the board has one.
    fn find(&self, kind: LayerKind, side: Option<Side>) -> Option<Uuid> {
        self.pcb.layers().find(kind, side).map(|l| l.uuid)
    }

    fn layer(&self, uuid: Uuid) -> Option<&Layer> {
        self.layers.get(&uuid).copied()
    }

    /// A stroke along `points`, closing back to the first when `closed`.
    fn polyline(&mut self, object: Uuid, layer: Uuid, points: &[Point], width: Nm, closed: bool) {
        for w in points.windows(2) {
            self.on_layer(object, layer, capsule(w[0], w[1], width));
        }
        if closed && points.len() > 2 {
            let (first, last) = (points[0], points[points.len() - 1]);
            self.on_layer(object, layer, capsule(last, first, width));
        }
    }
}

fn capsule(a: Point, b: Point, width: Nm) -> Primitive {
    Primitive::Capsule { a, b, width }
}
