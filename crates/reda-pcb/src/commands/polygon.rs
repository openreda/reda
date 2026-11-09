//! The polygon editor's Commands: `pcb.polygon.set_vertices`, `fillet` and `chamfer`, on
//! the Board Outline, Board Cutouts, Zones, Zone Cutouts and polygon Keepouts. Every one
//! refuses a shape that crosses itself.

use reda_command::{
    AvailabilityContext, Command, CommandError, Context, Kind, Length, Meta, Registry, Unavailable,
};
use reda_model::geom::{self, CornerError, Nm, Vertex};
use reda_model::{InDocument, KeepoutShape, Object, PcbDocument, Uuid};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::outline::{VertexParam, contour};
use super::{needs_pcb, pcb};

pub(super) fn register_all(registry: &mut Registry) {
    registry.add(SetVertices);
    registry.add(Fillet);
    registry.add(Chamfer);
}

/// `rejected` when `vertices` cross or touch themselves, with the point in
/// `details.crossing`.
pub(super) fn refuse_crossing(vertices: &[Vertex]) -> Result<(), CommandError> {
    let Some(at) = geom::crossing(vertices) else {
        return Ok(());
    };
    let mut err = CommandError::rejected(format!(
        "the shape crosses itself at ({} nm, {} nm)",
        at.x.0, at.y.0
    ));
    err.details = Some(json!({ "crossing": { "x": at.x.0, "y": at.y.0 } }));
    Err(err)
}

/// What the polygon Commands return.
#[derive(Debug, Serialize, JsonSchema)]
pub struct Reshaped {
    /// The object changed.
    pub uuid: Uuid,
    /// How many vertices it has now.
    pub vertices: usize,
}

/// The vertices of polygon object `uuid`, or why it has none to edit.
fn vertices_of(pcb: &PcbDocument, uuid: Uuid) -> Result<Vec<Vertex>, CommandError> {
    if let Some(o) = pcb.outline().filter(|o| o.uuid == uuid) {
        return Ok(o.vertices.clone());
    }
    if let Some(c) = pcb.cutout(uuid) {
        return Ok(c.vertices.clone());
    }
    if let Some(z) = pcb.zone(uuid) {
        if z.follows_outline {
            return Err(CommandError::rejected(
                "the Zone follows the Board Outline: edit the Board Outline, or detach the \
                 Zone first",
            ));
        }
        return Ok(z.vertices.clone());
    }
    if let Some(c) = pcb.zone_cutout(uuid) {
        return Ok(c.vertices.clone());
    }
    if let Some(k) = pcb.keepout(uuid) {
        return match &k.shape {
            KeepoutShape::Polygon { vertices } => Ok(vertices.clone()),
            KeepoutShape::Line { .. } => Err(CommandError::rejected(
                "a line Keepout has no corners to edit; only polygon Keepouts do",
            )),
        };
    }
    Err(CommandError::invalid_params(
        "/object",
        format!(
            "{uuid} is not a Board Outline, Board Cutout, Zone, Zone Cutout or Keepout on \
             the board"
        ),
    ))
}

/// Gives polygon object `uuid` the new `vertices`, refusing a self-crossing shape.
fn reshape(
    cx: &mut Context<'_>,
    uuid: Uuid,
    vertices: Vec<Vertex>,
) -> Result<Reshaped, CommandError> {
    refuse_crossing(&vertices)?;
    let pcb = pcb(cx)?;
    let document = pcb.uuid;
    let count = vertices.len();
    let object = if let Some(o) = pcb.outline().filter(|o| o.uuid == uuid) {
        let mut o = o.clone();
        o.vertices = vertices;
        Object::BoardOutline(InDocument::new(document, o))
    } else if let Some(c) = pcb.cutout(uuid) {
        let mut c = c.clone();
        c.vertices = vertices;
        Object::BoardCutout(InDocument::new(document, c))
    } else if let Some(z) = pcb.zone(uuid) {
        let mut z = z.clone();
        z.vertices = vertices;
        Object::Zone(InDocument::new(document, z))
    } else if let Some(c) = pcb.zone_cutout(uuid) {
        let mut c = c.clone();
        c.vertices = vertices;
        Object::ZoneCutout(InDocument::new(document, c))
    } else {
        let mut k = pcb.keepout(uuid).expect("checked by vertices_of").clone();
        k.shape = KeepoutShape::Polygon { vertices };
        Object::Keepout(InDocument::new(document, k))
    };
    cx.tx()?.replace(object)?;
    Ok(Reshaped {
        uuid,
        vertices: count,
    })
}

fn corner_error(e: CornerError, explicit: bool) -> CommandError {
    match e {
        CornerError::NoVertex(i) | CornerError::NotACorner(i) | CornerError::ArcEdge(i)
            if explicit =>
        {
            let pointer = "/vertices";
            CommandError::invalid_params(pointer, format!("{e} (index {i})"))
        }
        other => {
            let mut err = CommandError::rejected(other.to_string());
            if let CornerError::TooLarge(i) = other {
                err.details = Some(json!({ "vertex": i }));
            }
            err
        }
    }
}

fn positive(pointer: &str, value: Length) -> Result<Nm, CommandError> {
    let nm = value.nm();
    if nm <= Nm::ZERO {
        return Err(CommandError::invalid_params(pointer, "must be positive"));
    }
    Ok(nm)
}

pub struct SetVertices;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetVerticesParams {
    /// The Board Outline, Board Cutout, Zone, Zone Cutout or polygon Keepout.
    pub object: Uuid,
    /// Its new vertices in order; the last joins the first.
    pub vertices: Vec<VertexParam>,
}

impl Command for SetVertices {
    const META: Meta = Meta {
        id: "pcb.polygon.set_vertices",
        title: "Set Polygon Vertices",
        description: "Replaces the outline of a Board Outline, Board Cutout, Zone, Zone \
                      Cutout or polygon Keepout with `vertices`, whose edges are straight or, \
                      with a `sweep`, arcs. A shape that crosses itself is refused with the \
                      crossing point.",
        category: "pcb",
        icon: "board_outline",
        kind: Kind::Edit,
    };
    type Params = SetVerticesParams;
    type Output = Reshaped;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: SetVerticesParams) -> Result<Reshaped, CommandError> {
        vertices_of(pcb(cx)?, p.object)?;
        let vertices = contour(&p.vertices)?;
        reshape(cx, p.object, vertices)
    }
}

pub struct Fillet;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FilletParams {
    /// The Board Outline, Board Cutout, Zone, Zone Cutout or polygon Keepout.
    pub object: Uuid,
    /// The corners to round, by vertex index; every corner between straight edges when
    /// left out.
    #[serde(default)]
    pub vertices: Option<Vec<usize>>,
    /// The arc's radius.
    pub radius: Length,
}

impl Command for Fillet {
    const META: Meta = Meta {
        id: "pcb.polygon.fillet",
        title: "Fillet Corners",
        description: "Rounds corners of a Board Outline, Board Cutout, Zone, Zone Cutout or \
                      polygon Keepout with arcs of `radius` tangent to both edges: the \
                      corners at `vertices`, or every corner between straight edges when \
                      left out. Refused when an edge is too short for the radius, or the \
                      result crosses itself.",
        category: "pcb",
        icon: "board_outline",
        kind: Kind::Edit,
    };
    type Params = FilletParams;
    type Output = Reshaped;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: FilletParams) -> Result<Reshaped, CommandError> {
        let radius = positive("/radius", p.radius)?;
        let old = vertices_of(pcb(cx)?, p.object)?;
        let explicit = p.vertices.is_some();
        let new = geom::fillet(&old, p.vertices.as_deref(), radius)
            .map_err(|e| corner_error(e, explicit))?;
        reshape(cx, p.object, new)
    }
}

pub struct Chamfer;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ChamferParams {
    /// The Board Outline, Board Cutout, Zone, Zone Cutout or polygon Keepout.
    pub object: Uuid,
    /// The corners to bevel, by vertex index; every corner between straight edges when
    /// left out.
    #[serde(default)]
    pub vertices: Option<Vec<usize>>,
    /// How far back along each edge the bevel starts.
    pub distance: Length,
}

impl Command for Chamfer {
    const META: Meta = Meta {
        id: "pcb.polygon.chamfer",
        title: "Chamfer Corners",
        description: "Bevels corners of a Board Outline, Board Cutout, Zone, Zone Cutout or \
                      polygon Keepout with a straight edge `distance` back along each edge: \
                      the corners at `vertices`, or every corner between straight edges when \
                      left out. Refused when an edge is too short for the distance, or the \
                      result crosses itself.",
        category: "pcb",
        icon: "board_outline",
        kind: Kind::Edit,
    };
    type Params = ChamferParams;
    type Output = Reshaped;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: ChamferParams) -> Result<Reshaped, CommandError> {
        let distance = positive("/distance", p.distance)?;
        let old = vertices_of(pcb(cx)?, p.object)?;
        let explicit = p.vertices.is_some();
        let new = geom::chamfer(&old, p.vertices.as_deref(), distance)
            .map_err(|e| corner_error(e, explicit))?;
        reshape(cx, p.object, new)
    }
}
