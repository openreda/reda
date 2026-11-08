//! Zone Cutouts: `pcb.zone_cutout.add`, `add_from_graphics` and `delete`. A Zone Cutout
//! removes the fill of every Zone on its Layer inside its outline, exactly to its edge.

use reda_command::{AvailabilityContext, Command, CommandError, Context, Kind, Meta, Unavailable};
use reda_model::geom::Vertex;
use reda_model::{InDocument, Object, Uuid, ZoneCutout};
use schemars::JsonSchema;
use serde::Deserialize;

use super::super::from_graphics::{FromGraphics, LoopMode};
use super::super::layer::LayerRef;
use super::super::outline::{VertexParam, contour};
use super::super::track::copper_layer;
use super::super::{Created, needs_pcb, pcb};
use super::{Deleted, delete};

pub struct ZoneCutoutAdd;
pub struct ZoneCutoutAddFromGraphics;
pub struct ZoneCutoutDelete;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ZoneCutoutAddParams {
    /// The copper Layer whose Zones it cuts.
    pub layer: LayerRef,
    /// The outline's vertices in order; the last joins the first.
    pub vertices: Vec<VertexParam>,
}

fn insert(
    cx: &mut Context<'_>,
    layer: Uuid,
    vertices: Vec<Vertex>,
) -> Result<Created, CommandError> {
    let document = pcb(cx)?.uuid;
    let uuid = cx.new_id();
    let cutout = ZoneCutout {
        uuid,
        layer,
        vertices,
    };
    cx.tx()?
        .insert(Object::ZoneCutout(InDocument::new(document, cutout)))?;
    Ok(Created { uuid })
}

impl Command for ZoneCutoutAdd {
    const META: Meta = Meta {
        id: "pcb.zone_cutout.add",
        title: "Add Zone Cutout",
        description: "Adds a Zone Cutout on copper `layer`: the fill of every Zone there \
                      is removed inside `vertices`, exactly to its edge. It is never \
                      plotted.",
        category: "pcb",
        icon: "keepout",
        kind: Kind::Edit,
    };
    type Params = ZoneCutoutAddParams;
    type Output = Created;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: ZoneCutoutAddParams) -> Result<Created, CommandError> {
        let layer = copper_layer(pcb(cx)?, p.layer, "/layer")?;
        let vertices = contour(&p.vertices)?;
        insert(cx, layer, vertices)
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ZoneCutoutFromGraphicsParams {
    /// Free lines, arcs and outlined rectangles forming one closed loop.
    pub graphics: Vec<Uuid>,
    /// `centerline` (default) or `outer_edge`.
    #[serde(default)]
    pub mode: LoopMode,
    /// Keep the graphics as well; by default they are removed.
    #[serde(default)]
    pub keep: bool,
    /// The copper Layer whose Zones it cuts; the GUI passes the active one.
    pub layer: LayerRef,
}

impl Command for ZoneCutoutAddFromGraphics {
    const META: Meta = Meta {
        id: "pcb.zone_cutout.add_from_graphics",
        title: "Zone Cutout from Selection",
        description: "Adds a Zone Cutout on copper `layer` along the closed loop that the \
                      free lines, arcs and rectangles `graphics` form, and removes them \
                      unless `keep`. `mode` `centerline` (default) needs their ends to meet \
                      within 0.001 mm and refuses a gap with its location and size; \
                      `outer_edge` follows the outside of the strokes.",
        category: "pcb",
        icon: "keepout",
        kind: Kind::Edit,
    };
    type Params = ZoneCutoutFromGraphicsParams;
    type Output = Created;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: ZoneCutoutFromGraphicsParams,
    ) -> Result<Created, CommandError> {
        let pcb = pcb(cx)?;
        let layer = copper_layer(pcb, p.layer, "/layer")?;
        let from = FromGraphics {
            graphics: p.graphics,
            mode: p.mode,
            keep: p.keep,
        };
        let vertices = from.contour(pcb)?;
        from.consume(cx)?;
        insert(cx, layer, vertices)
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ZoneCutoutDeleteParams {
    /// The Zone Cutouts' UUIDs.
    pub cutouts: Vec<Uuid>,
}

impl Command for ZoneCutoutDelete {
    const META: Meta = Meta {
        id: "pcb.zone_cutout.delete",
        title: "Delete Zone Cutout",
        description: "Deletes Zone Cutouts.",
        category: "pcb",
        icon: "delete",
        kind: Kind::Edit,
    };
    type Params = ZoneCutoutDeleteParams;
    type Output = Deleted;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: ZoneCutoutDeleteParams,
    ) -> Result<Deleted, CommandError> {
        delete(
            cx,
            p.cutouts,
            "cutouts",
            |pcb, u| pcb.zone_cutout(u).is_some(),
            "Zone Cutout",
        )
    }
}
