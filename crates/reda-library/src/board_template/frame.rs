//! The evaluated frame: what a Board Template gives at one set of values, in integer
//! nanometres and millidegrees, with the problems the board's own checks find in it.
//!
//! Every object the gates let through is here. Objects the caller excluded, and those
//! that go with them (a Keepout that protects an excluded hole, a Dimension that follows
//! one, the drawings of an excluded group), stay in the frame marked `excluded`, so the
//! preview can draw them faint; creation skips them.

use std::collections::{BTreeMap, BTreeSet};

use reda_model::geom::{Millideg, Nm, Point, Vertex};
use reda_model::{
    DrawingKind, DrawingLayer, FreePad, KeepoutShapeKind, Restriction, Side, TemplateCopperLayer,
    TemplateFollow, Uuid,
};
use schemars::JsonSchema;
use serde::Serialize;

/// A parameter's resolved value: an option key for a choice, nanometres for a length.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(untagged)]
pub enum ResolvedValue {
    Choice(String),
    Length(Nm),
}

/// One evaluated Board Template.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Frame {
    /// The template's Library element UUID.
    pub element: Uuid,
    pub name: String,
    /// Every parameter's value, defaults filled in. A parameter hidden by its `when`
    /// takes its default.
    pub values: BTreeMap<String, ResolvedValue>,
    /// The copper Layer count the template suggests at these values.
    pub suggested_layers: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub board: Option<FrameBoard>,
    /// The Board Outline. Zero-length edges are dropped.
    pub outline: FrameContour,
    pub cutouts: Vec<FrameContour>,
    pub keepouts: Vec<FrameKeepout>,
    pub connectors: Vec<FramePlacement>,
    pub holes: Vec<FrameHole>,
    pub drawing_groups: Vec<FrameDrawingGroup>,
    /// What the Board Outline, Board Cutout and Keepout checks find. Any problem makes the
    /// frame invalid: creation refuses it.
    pub problems: Vec<FrameProblem>,
}

impl Frame {
    pub fn is_valid(&self) -> bool {
        self.problems.is_empty()
    }
}

/// Board properties the frame sets.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct FrameBoard {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edge_connector: Option<FrameEdgeConnector>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct FrameEdgeConnector {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bevel: Option<FrameBevel>,
    pub hard_gold: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct FrameBevel {
    pub angle: Millideg,
    pub depth: Nm,
}

/// The Board Outline or a Board Cutout.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct FrameContour {
    /// The template row's UUID.
    pub uuid: Uuid,
    pub vertices: Vec<FrameVertex>,
    pub locked: bool,
}

impl FrameContour {
    /// The vertices as a board contour stores them.
    pub fn contour(&self) -> Vec<Vertex> {
        self.vertices.iter().map(FrameVertex::vertex).collect()
    }
}

/// One contour vertex and the edge that leaves it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct FrameVertex {
    pub x: Nm,
    pub y: Nm,
    #[serde(skip_serializing_if = "is_zero_angle")]
    pub sweep: Millideg,
    /// The edge this vertex starts is a Plated Edge, with no Net.
    #[serde(skip_serializing_if = "is_false")]
    pub plated: bool,
}

impl FrameVertex {
    pub fn vertex(&self) -> Vertex {
        Vertex::arc(Point::new(self.x, self.y), self.sweep)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct FrameKeepout {
    pub uuid: Uuid,
    pub layer: TemplateCopperLayer,
    pub restrict: BTreeSet<Restriction>,
    pub shape: KeepoutShapeKind,
    pub vertices: Vec<Vertex>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<Nm>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub protects: Option<Uuid>,
    pub locked: bool,
    /// It goes with the excluded object it protects.
    #[serde(skip_serializing_if = "is_false")]
    pub excluded: bool,
}

/// A connector as placed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct FramePlacement {
    pub uuid: Uuid,
    /// The template's part-table entry.
    pub part: Uuid,
    /// The template's Footprint copy.
    pub footprint: Uuid,
    pub at: Point,
    pub rotation: Millideg,
    pub side: Side,
    pub locked: bool,
    #[serde(skip_serializing_if = "is_false")]
    pub excluded: bool,
    /// Its Footprint's Pads on the board, for drawing and measuring.
    pub pads: Vec<FramePad>,
}

/// A hole: a Mechanical Part (`part`, `footprint`, `side`) or a Free Pad (`free_pad`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct FrameHole {
    pub uuid: Uuid,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub part: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub footprint: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub side: Option<Side>,
    pub at: Point,
    pub rotation: Millideg,
    /// The Free Pad as it would be created, its UUID the template row's.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub free_pad: Option<FreePad>,
    pub locked: bool,
    #[serde(skip_serializing_if = "is_false")]
    pub excluded: bool,
    pub pads: Vec<FramePad>,
}

/// One Pad on the board.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct FramePad {
    /// The Pad number; empty for a Free Pad.
    pub number: String,
    /// Its anchor on the board.
    pub at: Point,
    /// Its copper outline on the board (its land on the Footprint's side).
    pub outline: Vec<Vertex>,
    /// Its drilled hole's size, if it has one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hole: Option<FrameDrill>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct FrameDrill {
    pub width: Nm,
    pub height: Nm,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct FrameDrawingGroup {
    pub uuid: Uuid,
    pub name: String,
    pub layer: DrawingLayer,
    #[serde(skip_serializing_if = "is_false")]
    pub excluded: bool,
    pub drawings: Vec<FrameDrawing>,
}

/// One drawing, with only the fields its kind uses.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct FrameDrawing {
    pub uuid: Uuid,
    pub kind: DrawingKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<Nm>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<Point>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to: Option<Point>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sweep: Option<Millideg>,
    #[serde(skip_serializing_if = "is_false")]
    pub filled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<Point>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<Nm>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rotation: Option<Millideg>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub a: Option<FrameDimensionEnd>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub b: Option<FrameDimensionEnd>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<Nm>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prefix: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suffix: Option<String>,
    pub locked: bool,
    /// Its group is excluded, or a Dimension end follows an excluded object.
    #[serde(skip_serializing_if = "is_false")]
    pub excluded: bool,
}

/// A Dimension end: where it is, and the template object it follows, if any.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct FrameDimensionEnd {
    pub point: Point,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub follow: Option<TemplateFollow>,
}

/// Which kind of frame object a problem is on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProblemObject {
    Outline,
    Cutout,
    Keepout,
}

/// What a board check found in the frame, and where, so the preview can mark it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct FrameProblem {
    /// The template row it is on.
    pub object: Uuid,
    pub kind: ProblemObject,
    pub message: String,
    /// Where it is, when the check names a point.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub at: Option<Point>,
}

fn is_false(value: &bool) -> bool {
    !*value
}

fn is_zero_angle(angle: &Millideg) -> bool {
    *angle == Millideg::ZERO
}
