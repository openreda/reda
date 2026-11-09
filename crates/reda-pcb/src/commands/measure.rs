//! Measuring: the clearance between two primitives and the length of selected Tracks.

use reda_command::{
    AvailabilityContext, Command, CommandError, Context, Kind, Meta, Registry, Unavailable,
};
use reda_model::geom::grown::{Core, Prim, Shape, arc_points};
use reda_model::geom::{Nm, Point, Vertex};
use reda_model::{GraphicShape, LayerKind, PadRef, PcbDocument, Uuid};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{board_copper, needs_pcb, pcb};
use crate::copper::{Copper, Item};

pub(super) fn register_all(registry: &mut Registry) {
    registry.add(MeasureClearance);
    registry.add(MeasureLength);
}

/// A primitive to measure: a UUID, or a Pad of a Footprint instance.
#[derive(Clone, Copy, Debug, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum Primitive {
    Object(Uuid),
    Pad(PadRef),
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MeasureClearanceParams {
    /// A Track, Via, Free Pad, Zone (its fill), board graphic, the Board Outline or a
    /// Board Cutout by UUID, or a Footprint Pad as `{footprint, pad}`.
    pub a: Primitive,
    /// The other primitive, as `a`.
    pub b: Primitive,
}

/// One end of a measured gap.
#[derive(Debug, Serialize, JsonSchema)]
pub struct GapEnd {
    /// The Layer the primitive is on there.
    pub layer: Uuid,
    /// The point on the primitive nearest the other.
    pub at: Point,
}

/// The gap on one Layer both primitives are on.
#[derive(Debug, Serialize, JsonSchema)]
pub struct SameLayerGap {
    pub distance: Nm,
    pub layer: Uuid,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct MeasureClearanceResult {
    /// The smallest gap, wherever it falls, with the Layers flattened: 0 where the
    /// primitives overlap seen from above.
    pub xy: Nm,
    /// The smallest gap on a Layer both are on; left out when they share none.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub same_layer: Option<SameLayerGap>,
    /// Where the smallest gap falls on `a`.
    pub a: GapEnd,
    /// Where the smallest gap falls on `b`.
    pub b: GapEnd,
}

pub struct MeasureClearance;

impl Command for MeasureClearance {
    const META: Meta = Meta {
        id: "pcb.measure.clearance",
        title: "Measure Clearance",
        description: "The smallest gap between two primitives (copper, board graphics or the \
                      board edge), wherever it falls: with the Layers flattened and on a Layer \
                      both are on, with each one's Layer and nearest point. Group objects such \
                      as Footprints and Dimensions can't be measured; measure a Pad instead.",
        category: "pcb",
        icon: "measure",
        kind: Kind::Query,
    };
    type Params = MeasureClearanceParams;
    type Output = MeasureClearanceResult;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: MeasureClearanceParams,
    ) -> Result<MeasureClearanceResult, CommandError> {
        let pcb = pcb(cx)?;
        let copper = board_copper(cx)?;
        let a = pieces(pcb, &copper, p.a, "/a")?;
        let b = pieces(pcb, &copper, p.b, "/b")?;
        let mut flat: Option<(f64, (Uuid, &Shape), (Uuid, &Shape))> = None;
        let mut same: Option<(f64, Uuid)> = None;
        for (la, sa) in &a {
            for (lb, sb) in &b {
                let d = sa.distance(sb).max(0.0);
                if flat.as_ref().is_none_or(|f| d < f.0) {
                    flat = Some((d, (*la, sa), (*lb, sb)));
                }
                if la == lb && same.is_none_or(|s| d < s.0) {
                    same = Some((d, *la));
                }
            }
        }
        let (d, (la, sa), (lb, sb)) = flat.expect("both primitives have shapes");
        let [pa, pb] = sa.closest_points(sb);
        Ok(MeasureClearanceResult {
            xy: Nm(d.round() as i64),
            same_layer: same.map(|(d, layer)| SameLayerGap {
                distance: Nm(d.round() as i64),
                layer,
            }),
            a: GapEnd { layer: la, at: pa },
            b: GapEnd { layer: lb, at: pb },
        })
    }
}

/// A primitive's shapes, one per Layer it is on; `pointer` names the param in errors.
fn pieces(
    pcb: &PcbDocument,
    copper: &Copper,
    primitive: Primitive,
    pointer: &str,
) -> Result<Vec<(Uuid, Shape)>, CommandError> {
    let refuse = |message: String| CommandError::invalid_params(pointer, message);
    let copper_of = |items: Vec<Item>| -> Vec<(Uuid, Shape)> {
        items
            .into_iter()
            .flat_map(|item| copper.item_pieces(item))
            .map(|(layer, shape)| (layer, shape.clone()))
            .collect()
    };
    let uuid = match primitive {
        Primitive::Pad(pad) => {
            let found = copper_of(vec![Item::Pad {
                footprint: pad.footprint,
                pad: pad.pad,
            }]);
            if found.is_empty() {
                return Err(refuse(format!(
                    "no Pad {} with copper on Footprint {}",
                    pad.pad, pad.footprint
                )));
            }
            return Ok(found);
        }
        Primitive::Object(uuid) => uuid,
    };
    let found = if pcb.track(uuid).is_some() {
        copper_of(vec![Item::Track { uuid }])
    } else if pcb.via(uuid).is_some() {
        copper_of(vec![Item::Via { uuid }])
    } else if pcb.free_pad(uuid).is_some() {
        copper_of(vec![Item::FreePad { uuid }])
    } else if pcb.zone(uuid).is_some() {
        let islands: Vec<Item> = copper
            .items()
            .filter(|i| matches!(i, Item::Zone { zone, .. } if *zone == uuid))
            .collect();
        if islands.is_empty() {
            return Err(refuse(format!(
                "the Zone {uuid} has no fill to measure; fill it first"
            )));
        }
        copper_of(islands)
    } else if let Some(graphic) = pcb.graphics().find(|g| g.uuid == uuid) {
        vec![(graphic.layer, graphic_shape(&graphic.shape, graphic.width))]
    } else if let Some(outline) = pcb.outline().filter(|o| o.uuid == uuid) {
        vec![(edge_layer(pcb, pointer)?, edge(&outline.vertices))]
    } else if let Some(cutout) = pcb.cutouts().find(|c| c.uuid == uuid) {
        vec![(edge_layer(pcb, pointer)?, edge(&cutout.vertices))]
    } else if pcb.footprint(uuid).is_some() {
        return Err(refuse(format!(
            "{uuid} is a Footprint, a group of objects: measure one of its Pads instead"
        )));
    } else if pcb.dimension(uuid).is_some() {
        return Err(refuse(format!(
            "{uuid} is a Dimension, a group of objects, and can't be measured"
        )));
    } else {
        return Err(refuse(format!(
            "{uuid} is not a primitive on the board: a Track, Via, Pad, Free Pad, Zone, \
             board graphic, the Board Outline or a Board Cutout"
        )));
    };
    if found.is_empty() {
        return Err(refuse(format!("{uuid} has no copper to measure")));
    }
    Ok(found)
}

fn p(point: Point) -> [i64; 2] {
    [point.x.0, point.y.0]
}

/// A board graphic drawn with a round pen of `width`.
fn graphic_shape(shape: &GraphicShape, width: Nm) -> Shape {
    let r = width.0 / 2;
    match *shape {
        GraphicShape::Line { from, to } => Shape::chain(&[from, to], r),
        GraphicShape::Arc { from, to, sweep } => Shape::chain(&arc_points(from, to, sweep), r),
        GraphicShape::Rect { from, to, filled } => {
            let corners = [
                from,
                Point::new(to.x, from.y),
                to,
                Point::new(from.x, to.y),
                from,
            ];
            let mut shape = Shape::chain(&corners, r);
            if filled && from.x != to.x && from.y != to.y {
                shape.prims.push(Prim {
                    core: Core::Poly(corners[..4].iter().map(|c| p(*c)).collect()),
                    radius: r,
                });
            }
            shape
        }
    }
}

/// The edge of a closed outline, as a line of no width.
fn edge(vertices: &[Vertex]) -> Shape {
    let mut points: Vec<Point> = crate::zone::poly::contour(vertices)
        .into_iter()
        .map(crate::zone::poly::point)
        .collect();
    if let Some(first) = points.first().copied() {
        points.push(first);
    }
    Shape::chain(&points, 0)
}

/// The Board Outline Layer, where the board edge is.
fn edge_layer(pcb: &PcbDocument, pointer: &str) -> Result<Uuid, CommandError> {
    pcb.layers()
        .find(LayerKind::BoardOutline, None)
        .map(|l| l.uuid)
        .ok_or_else(|| CommandError::invalid_params(pointer, "the board has no outline Layer"))
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MeasureLengthParams {
    /// The selected objects; only Tracks count.
    pub objects: Vec<Uuid>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct MeasureLengthResult {
    /// The Tracks' total centreline length, arcs along their curve.
    pub length: Nm,
    /// How many Tracks were counted.
    pub tracks: usize,
}

pub struct MeasureLength;

impl Command for MeasureLength {
    const META: Meta = Meta {
        id: "pcb.measure.length",
        title: "Measure Selection Length",
        description: "The total length of the Tracks among `objects`, arcs along their curve. \
                      Vias, Pads, fills and other objects don't count.",
        category: "pcb",
        icon: "measure",
        kind: Kind::Query,
    };
    type Params = MeasureLengthParams;
    type Output = MeasureLengthResult;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: MeasureLengthParams,
    ) -> Result<MeasureLengthResult, CommandError> {
        let pcb = pcb(cx)?;
        let project = cx.project()?;
        let mut objects = p.objects.clone();
        objects.sort();
        objects.dedup();
        let mut length = 0.0;
        let mut tracks = 0;
        for uuid in &objects {
            if let Some(track) = pcb.track(*uuid) {
                length += track.geometry.length();
                tracks += 1;
            } else if project.object(*uuid).is_none() {
                let i = p.objects.iter().position(|o| o == uuid).unwrap_or(0);
                return Err(CommandError::invalid_params(
                    &format!("/objects/{i}"),
                    format!("no object has UUID {uuid}"),
                ));
            }
        }
        Ok(MeasureLengthResult {
            length: Nm(length.round() as i64),
            tracks,
        })
    }
}
