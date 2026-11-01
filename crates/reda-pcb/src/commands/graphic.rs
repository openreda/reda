//! Lines, arcs and rectangles drawn on the board.

use reda_command::{
    Angle, AvailabilityContext, Command, CommandError, Context, Kind, Length, Meta, Registry,
    Unavailable,
};
use reda_model::geom::{Millideg, Nm};
use reda_model::{GraphicShape, InDocument, Object, PcbGraphic, Uuid};
use schemars::JsonSchema;
use serde::Deserialize;

use super::layer::{LayerRef, drawing_layer};
use super::outline::PointParam;
use super::{Created, needs_pcb, non_negative, pcb, positive};

pub(super) fn register_all(registry: &mut Registry) {
    registry.add(LineAdd);
    registry.add(ArcAdd);
    registry.add(RectAdd);
    registry.add(GraphicDelete);
}

/// The pen width of new graphics when the params leave it out.
pub const DEFAULT_GRAPHIC_WIDTH: Nm = Nm::from_um(150);

/// Adds a graphic of `shape` on `layer` drawn with a pen of `width`.
fn add(
    cx: &mut Context<'_>,
    layer: LayerRef,
    width: Nm,
    shape: GraphicShape,
) -> Result<Created, CommandError> {
    let pcb = pcb(cx)?;
    let layer = drawing_layer(pcb, layer, "/layer")?.uuid;
    let document = pcb.uuid;
    let uuid = cx.new_id();
    let graphic = PcbGraphic {
        uuid,
        layer,
        width,
        shape,
    };
    cx.tx()?
        .insert(Object::PcbGraphic(InDocument::new(document, graphic)))?;
    Ok(Created { uuid })
}

fn pen(width: Option<Length>) -> Result<Nm, CommandError> {
    width.map_or(Ok(DEFAULT_GRAPHIC_WIDTH), |w| positive("/width", w))
}

pub struct LineAdd;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LineAddParams {
    /// The Layer: copper, silkscreen, mechanical or documentation.
    pub layer: LayerRef,
    pub from: PointParam,
    pub to: PointParam,
    /// The pen width; 0.15 mm when left out.
    #[serde(default)]
    pub width: Option<Length>,
}

impl Command for LineAdd {
    const META: Meta = Meta {
        id: "pcb.graphic.add_line",
        title: "Add Line",
        description: "Draws a straight line from `from` to `to` with a round pen on a \
                      copper, silkscreen, mechanical or documentation Layer.",
        category: "pcb",
        icon: "line",
        kind: Kind::Edit,
    };
    type Params = LineAddParams;
    type Output = Created;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: LineAddParams) -> Result<Created, CommandError> {
        let (from, to) = (p.from.point(), p.to.point());
        if from == to {
            return Err(CommandError::invalid_params(
                "/to",
                "the line has no length",
            ));
        }
        let width = pen(p.width)?;
        add(cx, p.layer, width, GraphicShape::Line { from, to })
    }
}

pub struct ArcAdd;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ArcAddParams {
    /// The Layer: copper, silkscreen, mechanical or documentation.
    pub layer: LayerRef,
    pub from: PointParam,
    pub to: PointParam,
    /// The arc's sweep from `from` to `to`, counter-clockwise when positive; at most one
    /// turn either way.
    pub sweep: Angle,
    /// The pen width; 0.15 mm when left out.
    #[serde(default)]
    pub width: Option<Length>,
}

impl Command for ArcAdd {
    const META: Meta = Meta {
        id: "pcb.graphic.add_arc",
        title: "Add Arc",
        description: "Draws a circular arc from `from` to `to` sweeping `sweep` \
                      (counter-clockwise when positive) with a round pen on a copper, \
                      silkscreen, mechanical or documentation Layer.",
        category: "pcb",
        icon: "arc",
        kind: Kind::Edit,
    };
    type Params = ArcAddParams;
    type Output = Created;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: ArcAddParams) -> Result<Created, CommandError> {
        let (from, to) = (p.from.point(), p.to.point());
        let sweep = p.sweep.millideg();
        if sweep == Millideg::ZERO || sweep.0.abs() >= Millideg::TURN.0 {
            return Err(CommandError::invalid_params(
                "/sweep",
                "the sweep must be non-zero and less than a full turn",
            ));
        }
        if from == to {
            return Err(CommandError::invalid_params(
                "/to",
                "the arc's ends coincide",
            ));
        }
        let width = pen(p.width)?;
        add(cx, p.layer, width, GraphicShape::Arc { from, to, sweep })
    }
}

pub struct RectAdd;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RectAddParams {
    /// The Layer: copper, silkscreen, mechanical or documentation.
    pub layer: LayerRef,
    /// One corner.
    pub from: PointParam,
    /// The opposite corner.
    pub to: PointParam,
    /// The outline's pen width; 0.15 mm when left out. A filled rectangle may use 0.
    #[serde(default)]
    pub width: Option<Length>,
    /// Fill the rectangle as well as outlining it.
    #[serde(default)]
    pub filled: bool,
}

impl Command for RectAdd {
    const META: Meta = Meta {
        id: "pcb.graphic.add_rect",
        title: "Add Rectangle",
        description: "Draws an axis-aligned rectangle with opposite corners `from` and `to` \
                      on a copper, silkscreen, mechanical or documentation Layer, outlined \
                      with a round pen and optionally filled.",
        category: "pcb",
        icon: "rectangle",
        kind: Kind::Edit,
    };
    type Params = RectAddParams;
    type Output = Created;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: RectAddParams) -> Result<Created, CommandError> {
        let (from, to) = (p.from.point(), p.to.point());
        if from.x == to.x || from.y == to.y {
            return Err(CommandError::invalid_params(
                "/to",
                "the rectangle has no area: its corners share an x or a y",
            ));
        }
        let width = match (p.width, p.filled) {
            (Some(w), true) => non_negative("/width", w)?,
            (w, _) => pen(w)?,
        };
        let filled = p.filled;
        add(cx, p.layer, width, GraphicShape::Rect { from, to, filled })
    }
}

pub struct GraphicDelete;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GraphicDeleteParams {
    /// The graphic's UUID.
    pub uuid: Uuid,
}

impl Command for GraphicDelete {
    const META: Meta = Meta {
        id: "pcb.graphic.delete",
        title: "Delete Graphic",
        description: "Deletes one line, arc or rectangle.",
        category: "pcb",
        icon: "delete",
        kind: Kind::Edit,
    };
    type Params = GraphicDeleteParams;
    type Output = ();

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: GraphicDeleteParams) -> Result<(), CommandError> {
        if pcb(cx)?.graphic(p.uuid).is_none() {
            return Err(CommandError::not_found(format!(
                "no graphic has UUID {}",
                p.uuid
            )));
        }
        cx.tx()?.remove(p.uuid)?;
        Ok(())
    }
}
