//! The Board Outline and Board Cutouts.

use reda_command::{
    Angle, AvailabilityContext, Command, CommandError, Context, Kind, Length, Meta, NoParams,
    Registry, Unavailable,
};
use reda_model::geom::{self, Millideg, Nm, Point, Segment, Vertex};
use reda_model::{BoardCutout, BoardOutline, GraphicShape, InDocument, Object, Uuid};
use schemars::JsonSchema;
use serde::Deserialize;

use super::{Created, needs_pcb, non_negative, pcb};

pub(super) fn register_all(registry: &mut Registry) {
    registry.add(OutlineAdd);
    registry.add(OutlineAddRect);
    registry.add(OutlineAddFromSegments);
    registry.add(OutlineAddFromGraphics);
    registry.add(OutlineDelete);
    registry.add(CutoutAdd);
    registry.add(CutoutDelete);
}

/// A point param.
#[derive(Clone, Copy, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PointParam {
    pub x: Length,
    pub y: Length,
}

impl PointParam {
    pub(super) fn point(self) -> Point {
        Point::new(self.x.nm(), self.y.nm())
    }
}

/// One vertex of a contour and the edge that leaves it towards the next.
#[derive(Clone, Copy, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VertexParam {
    pub x: Length,
    pub y: Length,
    /// The edge to the next vertex: left out for a straight edge, otherwise an arc sweeping
    /// this angle, counter-clockwise when positive.
    #[serde(default)]
    pub sweep: Option<Angle>,
}

/// Contour params as stored vertices, checked to be one closed contour.
pub(super) fn contour(vertices: &[VertexParam]) -> Result<Vec<Vertex>, CommandError> {
    let vertices: Vec<Vertex> = vertices
        .iter()
        .map(|v| {
            let sweep = v.sweep.map_or(Millideg::ZERO, Angle::millideg);
            Vertex::arc(Point::new(v.x.nm(), v.y.nm()), sweep)
        })
        .collect();
    geom::check_contour(&vertices)
        .map_err(|e| CommandError::invalid_params("/vertices", e.to_string()))?;
    Ok(vertices)
}

/// Replaces the Board Outline, if there is one, with a new one along `vertices`.
fn set_outline(cx: &mut Context<'_>, vertices: Vec<Vertex>) -> Result<Created, CommandError> {
    let pcb = pcb(cx)?;
    let (document, old) = (pcb.uuid, pcb.outline().map(|o| o.uuid));
    let uuid = cx.new_id();
    let tx = cx.tx()?;
    if let Some(old) = old {
        tx.remove(old)?;
    }
    let outline = BoardOutline { uuid, vertices };
    tx.insert(Object::BoardOutline(InDocument::new(document, outline)))?;
    Ok(Created { uuid })
}

pub struct OutlineAdd;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OutlineAddParams {
    /// The outline's vertices in order; the last joins the first.
    pub vertices: Vec<VertexParam>,
}

impl Command for OutlineAdd {
    const META: Meta = Meta {
        id: "pcb.board_outline.add",
        title: "Add Board Outline",
        description: "Sets the Board Outline to a polygon through `vertices`, whose edges are \
                      straight or, with a `sweep`, arcs. Replaces any Board Outline the board \
                      already has.",
        category: "pcb",
        icon: "board_outline",
        kind: Kind::Edit,
    };
    type Params = OutlineAddParams;
    type Output = Created;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: OutlineAddParams) -> Result<Created, CommandError> {
        let vertices = contour(&p.vertices)?;
        set_outline(cx, vertices)
    }
}

pub struct OutlineAddRect;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OutlineAddRectParams {
    pub width: Length,
    pub height: Length,
    /// Rounds each corner with this radius; left out for sharp corners.
    #[serde(default)]
    pub corner_radius: Option<Length>,
}

impl Command for OutlineAddRect {
    const META: Meta = Meta {
        id: "pcb.board_outline.add_rect",
        title: "Create Rectangular Board Outline",
        description: "Sets the Board Outline to a `width` × `height` rectangle with its lower \
                      left corner at the origin, optionally with rounded corners. Replaces \
                      any Board Outline the board already has; the result is an ordinary \
                      polygon outline.",
        category: "pcb",
        icon: "board_outline_rect",
        kind: Kind::Edit,
    };
    type Params = OutlineAddRectParams;
    type Output = Created;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: OutlineAddRectParams) -> Result<Created, CommandError> {
        let vertices = rect_outline("", p.width, p.height, p.corner_radius)?;
        set_outline(cx, vertices)
    }
}

/// The contour of a `width` × `height` rectangle with its lower left corner at the origin
/// and corners rounded by `corner_radius`, checked as params under `prefix` (such as
/// `/board`). `pcb.board_outline.add_rect` and `project.new` both build with it.
pub fn rect_outline(
    prefix: &str,
    width: Length,
    height: Length,
    corner_radius: Option<Length>,
) -> Result<Vec<Vertex>, CommandError> {
    let (width, height) = (width.nm(), height.nm());
    for (field, value) in [("width", width), ("height", height)] {
        if value <= Nm::ZERO {
            return Err(CommandError::invalid_params(
                &format!("{prefix}/{field}"),
                format!("the {field} must be positive"),
            ));
        }
    }
    let pointer = format!("{prefix}/corner_radius");
    let radius = match corner_radius {
        Some(r) => non_negative(&pointer, r)?,
        None => Nm::ZERO,
    };
    if radius.0 * 2 > width.0.min(height.0) {
        return Err(CommandError::invalid_params(
            &pointer,
            "the corner radius is more than half the shorter side",
        ));
    }
    Ok(geom::rounded_rect(width, height, radius))
}

pub struct OutlineAddFromSegments;

/// A loose line or arc.
#[derive(Clone, Copy, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SegmentParam {
    pub from: PointParam,
    pub to: PointParam,
    /// Left out for a straight line, otherwise the arc's sweep from `from` to `to`.
    #[serde(default)]
    pub sweep: Option<Angle>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OutlineAddFromSegmentsParams {
    /// Lines and arcs in any order and direction whose ends meet exactly, forming one loop.
    pub segments: Vec<SegmentParam>,
}

impl Command for OutlineAddFromSegments {
    const META: Meta = Meta {
        id: "pcb.board_outline.add_from_segments",
        title: "Board Outline from Lines and Arcs",
        description: "Sets the Board Outline to the closed loop that `segments`, lines and \
                      arcs in any order and direction, form. Their ends must meet exactly, \
                      with no branches and no second loop. Replaces any Board Outline the \
                      board already has.",
        category: "pcb",
        icon: "board_outline_from_lines",
        kind: Kind::Edit,
    };
    type Params = OutlineAddFromSegmentsParams;
    type Output = Created;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: OutlineAddFromSegmentsParams,
    ) -> Result<Created, CommandError> {
        let segments: Vec<Segment> = p
            .segments
            .iter()
            .map(|s| {
                let sweep = s.sweep.map_or(Millideg::ZERO, Angle::millideg);
                Segment::arc(s.from.point(), s.to.point(), sweep)
            })
            .collect();
        let vertices = geom::chain(&segments)
            .map_err(|e| CommandError::invalid_params("/segments", e.to_string()))?;
        set_outline(cx, vertices)
    }
}

pub struct OutlineAddFromGraphics;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OutlineAddFromGraphicsParams {
    /// Free lines, arcs and outlined rectangles whose ends meet exactly, forming one loop,
    /// such as the selection.
    pub graphics: Vec<Uuid>,
    /// Keep the graphics as well; by default they become the outline and are removed.
    #[serde(default)]
    pub keep: bool,
}

impl Command for OutlineAddFromGraphics {
    const META: Meta = Meta {
        id: "pcb.board_outline.add_from_graphics",
        title: "Board Outline from Selection",
        description: "Sets the Board Outline to the closed loop that the free lines, arcs \
                      and rectangles `graphics` form, and removes them unless `keep`. \
                      Their ends must meet exactly. Replaces any Board Outline the board \
                      already has.",
        category: "pcb",
        icon: "board_outline_from_lines",
        kind: Kind::Edit,
    };
    type Params = OutlineAddFromGraphicsParams;
    type Output = Created;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: OutlineAddFromGraphicsParams,
    ) -> Result<Created, CommandError> {
        let pcb = pcb(cx)?;
        let mut segments = Vec::new();
        for (i, uuid) in p.graphics.iter().enumerate() {
            let refuse = || {
                CommandError::invalid_params(
                    &format!("/graphics/{i}"),
                    format!("{uuid} is not a free line, arc or rectangle outline"),
                )
            };
            let graphic = pcb.graphic(*uuid).ok_or_else(refuse)?;
            match graphic.shape {
                GraphicShape::Line { from, to } => segments.push(Segment::line(from, to)),
                GraphicShape::Arc { from, to, sweep } => {
                    segments.push(Segment::arc(from, to, sweep))
                }
                GraphicShape::Rect {
                    from,
                    to,
                    filled: false,
                } => {
                    let (b, d) = (Point::new(to.x, from.y), Point::new(from.x, to.y));
                    for (a, z) in [(from, b), (b, to), (to, d), (d, from)] {
                        segments.push(Segment::line(a, z));
                    }
                }
                GraphicShape::Rect { .. } => return Err(refuse()),
            }
        }
        let vertices = geom::chain(&segments)
            .map_err(|e| CommandError::invalid_params("/graphics", e.to_string()))?;
        if !p.keep {
            let tx = cx.tx()?;
            for uuid in &p.graphics {
                tx.remove(*uuid)?;
            }
        }
        set_outline(cx, vertices)
    }
}

pub struct OutlineDelete;

impl Command for OutlineDelete {
    const META: Meta = Meta {
        id: "pcb.board_outline.delete",
        title: "Delete Board Outline",
        description: "Deletes the Board Outline.",
        category: "pcb",
        icon: "delete",
        kind: Kind::Edit,
    };
    type Params = NoParams;
    type Output = ();

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, _: NoParams) -> Result<(), CommandError> {
        let outline = pcb(cx)?
            .outline()
            .map(|o| o.uuid)
            .ok_or_else(|| CommandError::not_found("the board has no Board Outline"))?;
        cx.tx()?.remove(outline)?;
        Ok(())
    }
}

pub struct CutoutAdd;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CutoutAddParams {
    /// The cutout's vertices in order; the last joins the first.
    pub vertices: Vec<VertexParam>,
}

impl Command for CutoutAdd {
    const META: Meta = Meta {
        id: "pcb.board_cutout.add",
        title: "Add Board Cutout",
        description: "Adds a Board Cutout: a polygon through `vertices`, whose edges are \
                      straight or, with a `sweep`, arcs, where board material is removed.",
        category: "pcb",
        icon: "board_cutout",
        kind: Kind::Edit,
    };
    type Params = CutoutAddParams;
    type Output = Created;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: CutoutAddParams) -> Result<Created, CommandError> {
        let vertices = contour(&p.vertices)?;
        let document = pcb(cx)?.uuid;
        let uuid = cx.new_id();
        let cutout = BoardCutout { uuid, vertices };
        cx.tx()?
            .insert(Object::BoardCutout(InDocument::new(document, cutout)))?;
        Ok(Created { uuid })
    }
}

pub struct CutoutDelete;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CutoutDeleteParams {
    /// The Board Cutout's UUID.
    pub cutout: Uuid,
}

impl Command for CutoutDelete {
    const META: Meta = Meta {
        id: "pcb.board_cutout.delete",
        title: "Delete Board Cutout",
        description: "Deletes one Board Cutout.",
        category: "pcb",
        icon: "delete",
        kind: Kind::Edit,
    };
    type Params = CutoutDeleteParams;
    type Output = ();

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: CutoutDeleteParams) -> Result<(), CommandError> {
        if pcb(cx)?.cutout(p.cutout).is_none() {
            return Err(CommandError::not_found(format!(
                "no Board Cutout has UUID {}",
                p.cutout
            )));
        }
        cx.tx()?.remove(p.cutout)?;
        Ok(())
    }
}
