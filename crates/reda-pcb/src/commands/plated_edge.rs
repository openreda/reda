//! Marking Plated Edges on the Board Outline and Board Cutouts
//! (docs/spec/v0.2.5/plated-edges.md#marking): `pcb.contour.edge.set` for one edge, and
//! `pcb.contour.edge.set_all`, the "Plate all edges" and "Unplate all" shortcuts.
//!
//! Plating is not geometry, so a locked contour takes marks too.

use reda_command::{
    AvailabilityContext, Command, CommandError, Context, Kind, Meta, Registry, Unavailable,
};
use reda_model::{InDocument, Object, PcbDocument, PlatedEdge, PlatedEdges, Uuid};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::track::existing_net;
use super::{needs_pcb, pcb};

pub(super) fn register_all(registry: &mut Registry) {
    registry.add(EdgeSet);
    registry.add(EdgeSetAll);
}

/// What the marking Commands return.
#[derive(Debug, Serialize, JsonSchema)]
pub struct Marked {
    /// The Board Outline or Board Cutout.
    pub contour: Uuid,
    /// How many of its edges are plated now.
    pub plated_edges: usize,
}

/// The vertex count and marks of contour `uuid`, or why it is not one.
fn marks_of(pcb: &PcbDocument, uuid: Uuid) -> Result<(usize, PlatedEdges), CommandError> {
    if let Some(o) = pcb.outline().filter(|o| o.uuid == uuid) {
        return Ok((o.vertices.len(), o.plated.clone()));
    }
    if let Some(c) = pcb.cutout(uuid) {
        return Ok((c.vertices.len(), c.plated.clone()));
    }
    Err(CommandError::invalid_params(
        "/contour",
        format!("{uuid} is not the Board Outline or a Board Cutout on the board"),
    ))
}

/// Gives contour `uuid` the marks `plated`.
fn write(cx: &mut Context<'_>, uuid: Uuid, plated: PlatedEdges) -> Result<Marked, CommandError> {
    let pcb = pcb(cx)?;
    let document = pcb.uuid;
    let count = plated.len();
    let object = if let Some(o) = pcb.outline().filter(|o| o.uuid == uuid) {
        let mut o = o.clone();
        o.plated = plated;
        Object::BoardOutline(InDocument::new(document, o))
    } else {
        let mut c = pcb.cutout(uuid).expect("checked by marks_of").clone();
        c.plated = plated;
        Object::BoardCutout(InDocument::new(document, c))
    };
    cx.tx()?.replace(object)?;
    Ok(Marked {
        contour: uuid,
        plated_edges: count,
    })
}

pub struct EdgeSet;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EdgeSetParams {
    /// The Board Outline's or a Board Cutout's UUID.
    pub contour: Uuid,
    /// The index of the vertex that starts the edge.
    pub edge: usize,
    /// The mark, with an optional `net`; `null` clears it.
    pub plated: Option<PlatedEdge>,
}

impl Command for EdgeSet {
    const META: Meta = Meta {
        id: "pcb.contour.edge.set",
        title: "Set Plated Edge",
        description: "Marks one edge of the Board Outline or a Board Cutout as a Plated Edge, \
                      which the fab plates with copper, with an optional `net`; `plated: \
                      null` clears the mark. `edge` is the index of the vertex that starts \
                      the edge.",
        category: "pcb",
        icon: "board_outline",
        kind: Kind::Edit,
    };
    type Params = EdgeSetParams;
    type Output = Marked;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: EdgeSetParams) -> Result<Marked, CommandError> {
        let pcb = pcb(cx)?;
        let (len, mut marks) = marks_of(pcb, p.contour)?;
        if p.edge >= len {
            return Err(CommandError::invalid_params(
                "/edge",
                format!("the contour has {len} edges, numbered from 0"),
            ));
        }
        match p.plated {
            Some(mark) => {
                existing_net(pcb, mark.net, "/plated/net")?;
                marks.insert(p.edge, mark);
            }
            None => {
                marks.remove(&p.edge);
            }
        }
        write(cx, p.contour, marks)
    }
}

pub struct EdgeSetAll;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EdgeSetAllParams {
    /// The Board Outline's or a Board Cutout's UUID.
    pub contour: Uuid,
    /// The mark every edge takes, with an optional `net`; `null` clears every edge.
    pub plated: Option<PlatedEdge>,
}

impl Command for EdgeSetAll {
    const META: Meta = Meta {
        id: "pcb.contour.edge.set_all",
        title: "Plate All Edges",
        description: "Marks every edge of the Board Outline or a Board Cutout as a Plated \
                      Edge with the same optional `net` (Plate all edges); `plated: null` \
                      clears every edge (Unplate all).",
        category: "pcb",
        icon: "board_outline",
        kind: Kind::Edit,
    };
    type Params = EdgeSetAllParams;
    type Output = Marked;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: EdgeSetAllParams) -> Result<Marked, CommandError> {
        let pcb = pcb(cx)?;
        let (len, _) = marks_of(pcb, p.contour)?;
        let marks = match p.plated {
            Some(mark) => {
                existing_net(pcb, mark.net, "/plated/net")?;
                (0..len).map(|i| (i, mark)).collect()
            }
            None => PlatedEdges::new(),
        };
        write(cx, p.contour, marks)
    }
}
