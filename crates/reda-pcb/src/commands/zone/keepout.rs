//! Keepouts: `pcb.keepout.add`, `set`, `delete` and `add_from_graphics`. A Keepout is on
//! one copper Layer or all of them, is a polygon or a line, and restricts any of Tracks,
//! Vias, Zones, SMD Pads, through-hole Pads and Footprints.

use std::collections::BTreeSet;

use reda_command::{
    Angle, AvailabilityContext, Command, CommandError, Context, Kind, Length, Meta, Unavailable,
};
use reda_model::geom::{self, Millideg, Nm, Point, Vertex};
use reda_model::{
    InDocument, Keepout, KeepoutShape, KeepoutShapeKind, Object, PcbDocument, Restriction, Uuid,
};
use schemars::JsonSchema;
use serde::Deserialize;

use super::super::from_graphics::{FromGraphics, LoopMode};
use super::super::layer::LayerRef;
use super::super::outline::VertexParam;
use super::super::track::copper_layer;
use super::super::{Created, needs_pcb, nullable, pcb};
use super::{Deleted, delete};

pub struct KeepoutAdd;
pub struct KeepoutSet;
pub struct KeepoutDelete;
pub struct KeepoutAddFromGraphics;

/// The vertices as stored, unchecked.
fn vertices(params: &[VertexParam]) -> Vec<Vertex> {
    params
        .iter()
        .map(|v| {
            let sweep = v.sweep.map_or(Millideg::ZERO, Angle::millideg);
            Vertex::arc(Point::new(v.x.nm(), v.y.nm()), sweep)
        })
        .collect()
}

/// A checked shape: a polygon's vertices form one closed contour; a line has two or more
/// vertices and a positive width.
fn shape(
    kind: KeepoutShapeKind,
    vertices: Vec<Vertex>,
    width: Option<Nm>,
) -> Result<KeepoutShape, CommandError> {
    match kind {
        KeepoutShapeKind::Polygon => {
            if width.is_some() {
                return Err(CommandError::invalid_params(
                    "/width",
                    "a polygon Keepout has no width",
                ));
            }
            geom::check_contour(&vertices)
                .map_err(|e| CommandError::invalid_params("/vertices", e.to_string()))?;
            Ok(KeepoutShape::Polygon { vertices })
        }
        KeepoutShapeKind::Line => {
            let width = width.ok_or_else(|| {
                CommandError::invalid_params("/width", "a line Keepout needs a width")
            })?;
            if width <= Nm::ZERO {
                return Err(CommandError::invalid_params(
                    "/width",
                    "a line Keepout needs a positive width",
                ));
            }
            if vertices.len() < 2 {
                return Err(CommandError::invalid_params(
                    "/vertices",
                    "a line Keepout needs two or more vertices",
                ));
            }
            Ok(KeepoutShape::Line { vertices, width })
        }
    }
}

/// The copper Layer a param names; `None` (every copper Layer) when left out.
fn layer(pcb: &PcbDocument, layer: Option<LayerRef>) -> Result<Option<Uuid>, CommandError> {
    layer.map(|l| copper_layer(pcb, l, "/layer")).transpose()
}

/// What a Keepout restricts: the given kinds, or every kind but `footprints`. Empty is
/// refused.
fn restrict(given: Option<BTreeSet<Restriction>>) -> Result<BTreeSet<Restriction>, CommandError> {
    let restrict = given.unwrap_or_else(Restriction::defaults);
    if restrict.is_empty() {
        return Err(CommandError::invalid_params(
            "/restrict",
            "a Keepout must restrict at least one kind of object",
        ));
    }
    Ok(restrict)
}

fn insert(cx: &mut Context<'_>, mut keepout: Keepout) -> Result<Created, CommandError> {
    let document = pcb(cx)?.uuid;
    let uuid = cx.new_id();
    keepout.uuid = uuid;
    cx.tx()?
        .insert(Object::Keepout(InDocument::new(document, keepout)))?;
    Ok(Created { uuid })
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct KeepoutAddParams {
    /// One copper Layer; left out, every copper Layer, including any added later.
    #[serde(default)]
    pub layer: Option<LayerRef>,
    /// `polygon` (default) or `line`.
    #[serde(default)]
    pub shape: Option<KeepoutShapeKind>,
    /// A polygon's outline, the last vertex joining the first; or a line's points.
    pub vertices: Vec<VertexParam>,
    /// A line's width; only for `line`.
    #[serde(default)]
    pub width: Option<Length>,
    /// Any of `tracks`, `vias`, `zones`, `smd_pads`, `tht_pads` and `footprints`.
    /// Default: all but `footprints`.
    #[serde(default)]
    pub restrict: Option<BTreeSet<Restriction>>,
}

impl Command for KeepoutAdd {
    const META: Meta = Meta {
        id: "pcb.keepout.add",
        title: "Add Keepout",
        description: "Adds a Keepout on copper `layer`, or on every copper Layer if left \
                      out: a `polygon` (default) inside `vertices`, or a `line` through \
                      them `width` wide. It keeps out the kinds in `restrict` (default: \
                      Tracks, Vias, Zones, SMD and through-hole Pads; `footprints` is \
                      checked by Courtyard). Copper keeps the normal clearance from it \
                      unless `zero_keepout_clearance` is set. It is never plotted.",
        category: "pcb",
        icon: "keepout",
        kind: Kind::Edit,
    };
    type Params = KeepoutAddParams;
    type Output = Created;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: KeepoutAddParams) -> Result<Created, CommandError> {
        let pcb = pcb(cx)?;
        let keepout = Keepout {
            uuid: Uuid::NIL,
            layer: layer(pcb, p.layer)?,
            shape: shape(
                p.shape.unwrap_or(KeepoutShapeKind::Polygon),
                vertices(&p.vertices),
                p.width.map(Length::nm),
            )?,
            restrict: restrict(p.restrict)?,
        };
        insert(cx, keepout)
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct KeepoutFromGraphicsParams {
    /// Free lines, arcs and outlined rectangles forming one closed loop.
    pub graphics: Vec<Uuid>,
    /// `centerline` (default) or `outer_edge`.
    #[serde(default)]
    pub mode: LoopMode,
    /// Keep the graphics as well; by default they are removed.
    #[serde(default)]
    pub keep: bool,
    /// One copper Layer; left out, every copper Layer.
    #[serde(default)]
    pub layer: Option<LayerRef>,
    /// What it keeps out. Default: all but `footprints`.
    #[serde(default)]
    pub restrict: Option<BTreeSet<Restriction>>,
}

impl Command for KeepoutAddFromGraphics {
    const META: Meta = Meta {
        id: "pcb.keepout.add_from_graphics",
        title: "Keepout from Selection",
        description: "Adds a polygon Keepout along the closed loop that the free lines, \
                      arcs and rectangles `graphics` form, and removes them unless `keep`. \
                      `mode` `centerline` (default) needs their ends to meet within \
                      0.001 mm and refuses a gap with its location and size; `outer_edge` \
                      follows the outside of the strokes.",
        category: "pcb",
        icon: "keepout",
        kind: Kind::Edit,
    };
    type Params = KeepoutFromGraphicsParams;
    type Output = Created;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: KeepoutFromGraphicsParams,
    ) -> Result<Created, CommandError> {
        let pcb = pcb(cx)?;
        let layer = layer(pcb, p.layer)?;
        let restrict = restrict(p.restrict)?;
        let from = FromGraphics {
            graphics: p.graphics,
            mode: p.mode,
            keep: p.keep,
        };
        let vertices = from.contour(pcb)?;
        from.consume(cx)?;
        let keepout = Keepout {
            uuid: Uuid::NIL,
            layer,
            shape: KeepoutShape::Polygon { vertices },
            restrict,
        };
        insert(cx, keepout)
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct KeepoutSetParams {
    /// The Keepout's UUID.
    pub keepout: Uuid,
    /// One copper Layer; `null` for every copper Layer.
    #[serde(default, deserialize_with = "nullable")]
    #[schemars(with = "Option<LayerRef>")]
    pub layer: Option<Option<LayerRef>>,
    /// `polygon` or `line`.
    #[serde(default)]
    pub shape: Option<KeepoutShapeKind>,
    #[serde(default)]
    pub vertices: Option<Vec<VertexParam>>,
    /// A line's width.
    #[serde(default)]
    pub width: Option<Length>,
    #[serde(default)]
    pub restrict: Option<BTreeSet<Restriction>>,
}

impl Command for KeepoutSet {
    const META: Meta = Meta {
        id: "pcb.keepout.set",
        title: "Change Keepout",
        description: "Changes a Keepout's Layer (`null` for every copper Layer), shape, \
                      vertices, line width or what it restricts; what is left out stays as \
                      it is.",
        category: "pcb",
        icon: "keepout",
        kind: Kind::Edit,
    };
    type Params = KeepoutSetParams;
    type Output = ();

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: KeepoutSetParams) -> Result<(), CommandError> {
        let pcb = pcb(cx)?;
        let document = pcb.uuid;
        let mut keepout = pcb
            .keepout(p.keepout)
            .ok_or_else(|| {
                CommandError::invalid_params(
                    "/keepout",
                    format!("no Keepout has UUID {}", p.keepout),
                )
            })?
            .clone();
        if let Some(given) = p.layer {
            keepout.layer = layer(pcb, given)?;
        }
        let (kind, old_width) = match &keepout.shape {
            KeepoutShape::Polygon { .. } => (KeepoutShapeKind::Polygon, None),
            KeepoutShape::Line { width, .. } => (KeepoutShapeKind::Line, Some(*width)),
        };
        let kind = p.shape.unwrap_or(kind);
        let width = match kind {
            KeepoutShapeKind::Polygon => p.width.map(Length::nm),
            KeepoutShapeKind::Line => p.width.map(Length::nm).or(old_width),
        };
        let points = p
            .vertices
            .as_deref()
            .map_or_else(|| keepout.vertices().to_vec(), vertices);
        keepout.shape = shape(kind, points, width)?;
        if p.restrict.is_some() {
            keepout.restrict = restrict(p.restrict)?;
        }
        cx.tx()?
            .replace(Object::Keepout(InDocument::new(document, keepout)))?;
        Ok(())
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct KeepoutDeleteParams {
    /// The Keepouts' UUIDs.
    pub keepouts: Vec<Uuid>,
}

impl Command for KeepoutDelete {
    const META: Meta = Meta {
        id: "pcb.keepout.delete",
        title: "Delete Keepout",
        description: "Deletes Keepouts.",
        category: "pcb",
        icon: "delete",
        kind: Kind::Edit,
    };
    type Params = KeepoutDeleteParams;
    type Output = Deleted;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: KeepoutDeleteParams) -> Result<Deleted, CommandError> {
        delete(
            cx,
            p.keepouts,
            "keepouts",
            |pcb, u| pcb.keepout(u).is_some(),
            "Keepout",
        )
    }
}
