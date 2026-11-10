//! `pcb.board.info`: the board's figures, as Properties shows them with nothing selected.

use reda_command::{
    AvailabilityContext, Command, CommandError, Context, Kind, Meta, NoParams, Registry,
    Unavailable,
};
use reda_model::geom::Nm;
use schemars::JsonSchema;
use serde::Serialize;

use super::{board_copper, needs_pcb, pcb};
use crate::Copper;
use crate::zone::poly::{contour, outline_area};
use reda_model::PcbDocument;

pub(super) fn register_all(registry: &mut Registry) {
    registry.add(BoardInfo);
}

/// Square nanometres in a square millimetre.
const NM2_PER_MM2: f64 = 1e12;

#[derive(Clone, Debug, PartialEq, Serialize, JsonSchema)]
pub struct BoardInfoResult {
    /// The Board Outline's width; left out without an outline.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<Nm>,
    /// The Board Outline's height; left out without an outline.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<Nm>,
    /// The board's area in mm²: inside the Board Outline, less the Board Cutouts.
    pub area: f64,
    /// The area the Components take in mm²: each Footprint's Courtyard box (its Pads' box
    /// without a Courtyard), added up.
    pub component_area: f64,
    /// Component area ÷ (2 × board area), both sides counting; left out without an area.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub density: Option<f64>,
    /// The share of connections copper has made, in percent; 100 with none to make.
    pub routed_percent: f64,
    /// Placed Footprints.
    pub components: usize,
    /// Copper Layers.
    pub layers: usize,
    pub nets: usize,
    /// Pads, Free Pads, Tracks, Vias, Zones, board graphics and board text.
    pub primitives: usize,
}

pub struct BoardInfo;

impl Command for BoardInfo {
    const META: Meta = Meta {
        id: "pcb.board.info",
        title: "Board Information",
        description: "The board's size, area, Component area, density (Component area ÷ \
                      (2 × board area)), routed percentage, and counts of Components, copper \
                      Layers, Nets and primitives.",
        category: "pcb",
        icon: "inspect",
        kind: Kind::Query,
    };
    type Params = NoParams;
    type Output = BoardInfoResult;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, _: NoParams) -> Result<BoardInfoResult, CommandError> {
        let copper = board_copper(cx)?;
        Ok(board_info(pcb(cx)?, &copper))
    }
}

/// The board's figures: what `pcb.board.info` returns, from `copper`, the board's copper
/// index.
pub fn board_info(pcb: &PcbDocument, copper: &Copper) -> BoardInfoResult {
    let outline = pcb.outline().filter(|o| !o.vertices.is_empty());
    let size = outline.and_then(|o| {
        let points = contour(&o.vertices);
        let xs = points.iter().map(|p| p.x);
        let ys = points.iter().map(|p| p.y);
        Some((
            Nm(xs.clone().max()? - xs.min()?),
            Nm(ys.clone().max()? - ys.min()?),
        ))
    });
    let area = outline.map_or(0, |o| {
        let cut: i128 = pcb.cutouts().map(|c| outline_area(&c.vertices)).sum();
        (outline_area(&o.vertices) - cut).max(0)
    }) as f64
        / NM2_PER_MM2;
    let component_area: f64 = pcb
        .footprints()
        .map(|f| {
            let b = crate::placed::courtyard_bounds(pcb, f);
            (b.max.x.0 - b.min.x.0) as f64 * (b.max.y.0 - b.min.y.0) as f64
        })
        .sum::<f64>()
        / NM2_PER_MM2;
    let pads: usize = pcb.footprints().map(|f| f.pads.len()).sum();
    let primitives = pads
        + pcb.free_pads().count()
        + pcb.tracks().count()
        + pcb.vias().count()
        + pcb.zones().count()
        + pcb.graphics().count()
        + pcb.texts().count();
    BoardInfoResult {
        width: size.map(|s| s.0),
        height: size.map(|s| s.1),
        area,
        component_area,
        density: (area > 0.0).then(|| component_area / (2.0 * area)),
        routed_percent: copper.all_connections().percent(),
        components: pcb.footprints().count(),
        layers: pcb.copper_layers().count(),
        nets: pcb.nets().count(),
        primitives,
    }
}
