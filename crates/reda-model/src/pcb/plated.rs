//! Plated Edges: edges of the Board Outline or a Board Cutout that the fab plates with
//! copper (docs/spec/v0.2.5/plated-edges.md).
//!
//! A stored vertex holds the edge that leaves it, so the mark is stored there as
//! `plated: {net?}`. In memory a contour keeps its geometry as plain [`Vertex`]es and its
//! marks beside them, keyed by the index of the vertex that starts the edge
//! ([`PlatedEdges`]); [`ContourRow`] is the file's form that joins the two.
//!
//! This is data only (ADR 0009): how the marks follow the contour edits, and the board's
//! list of Plated Edges, live in `reda_pcb::plated`.

use std::collections::BTreeMap;

use reda_geom::{Millideg, Nm, Point, Vertex};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::Uuid;

/// The mark on a Plated Edge: the Net its plating carries, if any.
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
)]
#[serde(deny_unknown_fields)]
pub struct PlatedEdge {
    /// The Net the plating carries; left out for none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub net: Option<Uuid>,
}

/// The Plated Edges of one contour, by the index of the vertex that starts each.
pub type PlatedEdges = BTreeMap<usize, PlatedEdge>;

/// One vertex of a Board Outline or Board Cutout as the file stores it: a [`Vertex`] and
/// the mark of the edge that leaves it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BoardVertex {
    pub x: Nm,
    pub y: Nm,
    /// The edge to the next vertex: left out (or 0) for a straight edge, otherwise an arc
    /// sweeping this angle, counter-clockwise when positive.
    #[serde(default, skip_serializing_if = "is_straight")]
    pub sweep: Millideg,
    /// Set when the edge to the next vertex is a Plated Edge; left out otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plated: Option<PlatedEdge>,
}

fn is_straight(sweep: &Millideg) -> bool {
    *sweep == Millideg::ZERO
}

fn is_false(value: &bool) -> bool {
    !*value
}

/// The file row of a Board Outline or a Board Cutout.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ContourRow {
    pub uuid: Uuid,
    /// Locked against geometry changes; left out when false.
    #[serde(default, skip_serializing_if = "is_false")]
    pub locked: bool,
    /// The contour; the last vertex joins the first.
    pub vertices: Vec<BoardVertex>,
}

/// The file's vertices of a contour with `marks`.
pub fn join(vertices: &[Vertex], marks: &PlatedEdges) -> Vec<BoardVertex> {
    vertices
        .iter()
        .enumerate()
        .map(|(i, v)| BoardVertex {
            x: v.x,
            y: v.y,
            sweep: v.sweep,
            plated: marks.get(&i).copied(),
        })
        .collect()
}

/// The geometry and the marks of the file's `vertices`.
pub fn split(vertices: &[BoardVertex]) -> (Vec<Vertex>, PlatedEdges) {
    let geometry = vertices
        .iter()
        .map(|v| Vertex::arc(Point::new(v.x, v.y), v.sweep))
        .collect();
    let marks = vertices
        .iter()
        .enumerate()
        .filter_map(|(i, v)| Some((i, v.plated?)))
        .collect();
    (geometry, marks)
}
