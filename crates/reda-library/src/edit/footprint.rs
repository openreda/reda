//! The Footprint editor's Commands: the Footprint's own properties, its Pads, graphics,
//! Designator and value text, and its origin.

use reda_command::{
    Angle, AvailabilityContext, Command, CommandError, Context, HoleParam, Kind, Length, Meta,
    RatioParam, Registry, Unavailable, ZoneConnectionParam, zone_connection,
};
use reda_model::geom::{Millideg, Nm, Point};
use reda_model::{
    Corners, Footprint, FootprintGraphic, FootprintLayer, Pad, PadKind, PadShape, Ratio, Side,
    Uuid, ZoneConnection,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{
    GraphicResult, PointParam, ShapeParam, check_element_name, edit_footprint, find_mut, invalid,
    positive, present, remove,
};

pub(super) fn register_all(registry: &mut Registry) {
    registry.add(FootprintEdit);
    registry.add(PadAdd);
    registry.add(PadEdit);
    registry.add(PadRemove);
    registry.add(FootprintGraphicAdd);
    registry.add(FootprintGraphicEdit);
    registry.add(FootprintGraphicRemove);
    registry.add(FootprintTextSet);
    registry.add(FootprintOriginSet);
    registry.add(FootprintMove);
    registry.add(FootprintDelete);
}

/// What the Footprint editor (and [`FootprintMove`] / [`FootprintDelete`]) calls the
/// Designator text of a Footprint, which has no UUID of its own.
pub const DESIGNATOR_TEXT: Uuid = Uuid::from_u128(0xfd00_0000_0000_4000_8000_0000_0000_0001);
/// What the Footprint editor calls the value text of a Footprint.
pub const VALUE_TEXT: Uuid = Uuid::from_u128(0xfd00_0000_0000_4000_8000_0000_0000_0002);

const fn meta(id: &'static str, title: &'static str, description: &'static str) -> Meta {
    Meta {
        id,
        title,
        description,
        category: "library",
        icon: "footprint_edit",
        kind: Kind::Edit,
    }
}

/// The default stroke width on a Layer: 0.12 mm Silkscreen, 0.05 mm Courtyard, 0.1 mm
/// Assembly.
fn default_stroke(layer: FootprintLayer) -> Nm {
    match layer {
        FootprintLayer::SilkscreenTop | FootprintLayer::SilkscreenBottom => Nm::from_um(120),
        FootprintLayer::CourtyardTop | FootprintLayer::CourtyardBottom => Nm::from_um(50),
        FootprintLayer::AssemblyTop | FootprintLayer::AssemblyBottom => Nm::from_um(100),
    }
}

pub struct FootprintEdit;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FootprintEditParams {
    /// The open Footprint.
    pub footprint: Uuid,
    pub name: Option<String>,
    pub description: Option<String>,
}

impl Command for FootprintEdit {
    const META: Meta = meta(
        "library.footprint.edit",
        "Edit Footprint",
        "Changes an open Footprint's name or description. Omitted fields keep their value.",
    );
    type Params = FootprintEditParams;
    type Output = ();

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        cx.elements()
    }

    fn run(&self, cx: &mut Context<'_>, p: FootprintEditParams) -> Result<(), CommandError> {
        if let Some(name) = &p.name {
            check_element_name(name).map_err(invalid("/name"))?;
        }
        edit_footprint(cx, p.footprint, |f| {
            if let Some(name) = p.name {
                f.name = name;
            }
            if let Some(description) = p.description {
                f.description = description;
            }
            Ok(())
        })
    }
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct PadResult {
    /// The new Pad's UUID.
    pub pad: Uuid,
}

pub struct PadAdd;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PadAddParams {
    /// The open Footprint.
    pub footprint: Uuid,
    /// Pads that share a number belong to the same Pin. May be empty for a non-plated
    /// hole.
    pub number: String,
    pub kind: PadKind,
    pub shape: PadShape,
    /// The Pad's anchor, relative to the Footprint origin: the hole sits there and Tracks
    /// connect there.
    pub at: PointParam,
    pub rotation: Option<Angle>,
    pub width: Length,
    pub height: Length,
    /// For a `rounded_rect` (50% when left out) and the uncut corners of a
    /// `chamfered_rect`.
    pub corner_ratio: Option<RatioParam>,
    /// For a `chamfered_rect`, which needs it.
    pub chamfer_ratio: Option<RatioParam>,
    /// The corners a `rounded_rect` rounds or a `chamfered_rect` chamfers. All four when
    /// left out.
    pub corners: Option<Corners>,
    /// Moves the copper away from `at`, in the Pad's frame.
    pub offset: Option<PointParam>,
    /// Required for plated and non-plated through-hole Pads; SMD Pads have none.
    pub hole: Option<HoleParam>,
    /// An SMD Pad's side in the Footprint's frame; top when left out.
    pub side: Option<Side>,
    /// Overrides the board's solder-mask margin for this Pad.
    pub mask_margin: Option<Length>,
    /// Overrides the board's solder-paste margin for this Pad.
    pub paste_margin: Option<Length>,
    /// How Zone fills of its Net join it; each value left out inherits.
    pub zone_connection: Option<ZoneConnectionParam>,
}

impl Command for PadAdd {
    const META: Meta = meta(
        "library.footprint.pad.add",
        "Add Pad",
        "Adds an SMD, plated through-hole or non-plated Pad, shaped as a circle, \
         rectangle, rounded or chamfered rectangle or oval, with its copper optionally \
         offset from its anchor, an optional round hole or slot, its side, per-Pad mask and \
         paste margins and its Zone connection.",
    );
    type Params = PadAddParams;
    type Output = PadResult;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        cx.elements()
    }

    fn run(&self, cx: &mut Context<'_>, p: PadAddParams) -> Result<PadResult, CommandError> {
        let uuid = cx.new_id();
        let explicit = Explicit {
            corner_ratio: p.corner_ratio.is_some(),
            chamfer_ratio: p.chamfer_ratio.is_some(),
            corners: p.corners.is_some(),
        };
        let pad = Pad {
            uuid,
            number: p.number,
            kind: p.kind,
            shape: p.shape,
            at: p.at.point(),
            rotation: p.rotation.map_or(Millideg(0), Angle::millideg),
            width: p.width.nm(),
            height: p.height.nm(),
            corner_ratio: p.corner_ratio.map(RatioParam::ratio),
            chamfer_ratio: p.chamfer_ratio.map(RatioParam::ratio),
            corners: p.corners,
            offset: p.offset.map(PointParam::point),
            hole: p.hole.map(HoleParam::hole),
            side: p.side,
            mask_margin: p.mask_margin.map(Length::nm),
            paste_margin: p.paste_margin.map(Length::nm),
            zone_connection: zone_connection(p.zone_connection),
        };
        let pad = finish_pad(pad, explicit)?;
        edit_footprint(cx, p.footprint, |f| {
            f.pads.push(pad);
            f.pads.sort_by_key(|pad| pad.uuid);
            Ok(PadResult { pad: uuid })
        })
    }
}

/// The corner ratio a `rounded_rect` gets when none is given: a radius of a quarter of the
/// shorter side.
pub const DEFAULT_CORNER_RATIO: Ratio = Ratio::percent(50);

/// Which shape fields a Command gave explicitly.
#[derive(Clone, Copy, Default)]
struct Explicit {
    corner_ratio: bool,
    chamfer_ratio: bool,
    corners: bool,
}

/// Checks a Pad and fills in what follows from its shape: a rounded rectangle without a
/// ratio gets [`DEFAULT_CORNER_RATIO`], shape fields the shape can't use are dropped unless
/// given explicitly, and stored defaults are left out (all four corners, no offset, the
/// top side).
fn finish_pad(mut pad: Pad, explicit: Explicit) -> Result<Pad, CommandError> {
    let cornered = matches!(pad.shape, PadShape::RoundedRect | PadShape::ChamferedRect);
    if !cornered && !explicit.corner_ratio {
        pad.corner_ratio = None;
    }
    if pad.shape != PadShape::ChamferedRect && !explicit.chamfer_ratio {
        pad.chamfer_ratio = None;
    }
    if !cornered && !explicit.corners {
        pad.corners = None;
    }
    if pad.shape == PadShape::RoundedRect && pad.corner_ratio.is_none() {
        pad.corner_ratio = Some(DEFAULT_CORNER_RATIO);
    }
    if pad.corners == Some(Corners::ALL) {
        pad.corners = None;
    }
    if pad.offset == Some(Point::default()) {
        pad.offset = None;
    }
    if pad.kind == PadKind::Smd && pad.side == Some(Side::Top) {
        pad.side = None;
    }
    if pad.zone_connection == Some(ZoneConnection::default()) {
        pad.zone_connection = None;
    }
    if let Some((pointer, message)) = pad.form().problem() {
        return Err(CommandError::invalid_params(&pointer, message));
    }
    Ok(pad)
}

pub struct PadEdit;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PadEditParams {
    /// The open Footprint.
    pub footprint: Uuid,
    pub pad: Uuid,
    pub number: Option<String>,
    pub kind: Option<PadKind>,
    pub shape: Option<PadShape>,
    pub at: Option<PointParam>,
    pub rotation: Option<Angle>,
    pub width: Option<Length>,
    pub height: Option<Length>,
    /// `null` returns a rounded rectangle to the default ratio.
    #[serde(default, deserialize_with = "present")]
    pub corner_ratio: Option<Option<RatioParam>>,
    /// `null` removes it.
    #[serde(default, deserialize_with = "present")]
    pub chamfer_ratio: Option<Option<RatioParam>>,
    /// `null` returns to all four corners.
    #[serde(default, deserialize_with = "present")]
    pub corners: Option<Option<Corners>>,
    /// `null` puts the copper back on the anchor.
    #[serde(default, deserialize_with = "present")]
    pub offset: Option<Option<PointParam>>,
    /// `null` removes the hole.
    #[serde(default, deserialize_with = "present")]
    pub hole: Option<Option<HoleParam>>,
    /// `null` returns an SMD Pad to the top.
    #[serde(default, deserialize_with = "present")]
    pub side: Option<Option<Side>>,
    /// `null` returns to the board's margin.
    #[serde(default, deserialize_with = "present")]
    pub mask_margin: Option<Option<Length>>,
    /// `null` returns to the board's margin.
    #[serde(default, deserialize_with = "present")]
    pub paste_margin: Option<Option<Length>>,
    /// Replaces the whole Zone connection; `null` inherits every value.
    #[serde(default, deserialize_with = "present")]
    pub zone_connection: Option<Option<ZoneConnectionParam>>,
}

impl Command for PadEdit {
    const META: Meta = meta(
        "library.footprint.pad.edit",
        "Edit Pad",
        "Changes any of a Pad's number, kind, shape, position, rotation, size, corner and \
         chamfer ratios and corners, offset, hole, side, mask and paste margins and Zone \
         connection. Omitted fields keep their value; `null` clears an optional one. A \
         resized Pad keeps its ratios, and so its look.",
    );
    type Params = PadEditParams;
    type Output = ();

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        cx.elements()
    }

    fn run(&self, cx: &mut Context<'_>, p: PadEditParams) -> Result<(), CommandError> {
        edit_footprint(cx, p.footprint, |f| {
            let mut pad = find_mut(&mut f.pads, p.pad, |pad| pad.uuid, "Pad")?.clone();
            if let Some(number) = p.number {
                pad.number = number;
            }
            if let Some(kind) = p.kind {
                pad.kind = kind;
            }
            if let Some(shape) = p.shape {
                pad.shape = shape;
            }
            if let Some(at) = p.at {
                pad.at = at.point();
            }
            if let Some(rotation) = p.rotation {
                pad.rotation = rotation.millideg();
            }
            if let Some(width) = p.width {
                pad.width = width.nm();
            }
            if let Some(height) = p.height {
                pad.height = height.nm();
            }
            let explicit = Explicit {
                corner_ratio: matches!(p.corner_ratio, Some(Some(_))),
                chamfer_ratio: matches!(p.chamfer_ratio, Some(Some(_))),
                corners: matches!(p.corners, Some(Some(_))),
            };
            if let Some(ratio) = p.corner_ratio {
                pad.corner_ratio = ratio.map(RatioParam::ratio);
            }
            if let Some(ratio) = p.chamfer_ratio {
                pad.chamfer_ratio = ratio.map(RatioParam::ratio);
            }
            if let Some(corners) = p.corners {
                pad.corners = corners;
            }
            if let Some(offset) = p.offset {
                pad.offset = offset.map(PointParam::point);
            }
            if let Some(hole) = p.hole {
                pad.hole = hole.map(HoleParam::hole);
            }
            if let Some(side) = p.side {
                pad.side = side;
            }
            if let Some(margin) = p.mask_margin {
                pad.mask_margin = margin.map(Length::nm);
            }
            if let Some(margin) = p.paste_margin {
                pad.paste_margin = margin.map(Length::nm);
            }
            if let Some(connection) = p.zone_connection {
                pad.zone_connection = zone_connection(connection);
            }
            let pad = finish_pad(pad, explicit)?;
            *find_mut(&mut f.pads, p.pad, |pad| pad.uuid, "Pad")? = pad;
            Ok(())
        })
    }
}

pub struct PadRemove;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PadParams {
    /// The open Footprint.
    pub footprint: Uuid,
    pub pad: Uuid,
}

impl Command for PadRemove {
    const META: Meta = meta(
        "library.footprint.pad.remove",
        "Remove Pad",
        "Removes a Pad from an open Footprint.",
    );
    type Params = PadParams;
    type Output = ();

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        cx.elements()
    }

    fn run(&self, cx: &mut Context<'_>, p: PadParams) -> Result<(), CommandError> {
        edit_footprint(cx, p.footprint, |f| {
            remove(&mut f.pads, p.pad, |pad| pad.uuid, "Pad").map(drop)
        })
    }
}

pub struct FootprintGraphicAdd;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FootprintGraphicAddParams {
    /// The open Footprint.
    pub footprint: Uuid,
    pub layer: FootprintLayer,
    pub shape: ShapeParam,
    /// The stroke width. Omitted, 0.12 mm on Silkscreen, 0.05 mm on Courtyard and 0.1 mm
    /// on Assembly.
    pub width: Option<Length>,
    /// Whether a closed shape is filled. Omitted, not.
    pub fill: Option<bool>,
}

impl Command for FootprintGraphicAdd {
    const META: Meta = meta(
        "library.footprint.graphic.add",
        "Add Footprint Graphic",
        "Draws a line, rectangle, arc, circle, polygon or text on the Silkscreen, \
         Courtyard or Assembly Layer, top or bottom, of an open Footprint.",
    );
    type Params = FootprintGraphicAddParams;
    type Output = GraphicResult;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        cx.elements()
    }

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: FootprintGraphicAddParams,
    ) -> Result<GraphicResult, CommandError> {
        let uuid = cx.new_id();
        let graphic = FootprintGraphic {
            uuid,
            layer: p.layer,
            shape: p.shape.shape().map_err(invalid("/shape"))?,
            width: p.width.map_or(default_stroke(p.layer), Length::nm),
            fill: p.fill.unwrap_or(false),
        };
        check_width(graphic.width)?;
        edit_footprint(cx, p.footprint, |f| {
            f.graphics.push(graphic);
            f.graphics.sort_by_key(|g| g.uuid);
            Ok(GraphicResult { graphic: uuid })
        })
    }
}

fn check_width(width: Nm) -> Result<(), CommandError> {
    if width.0 < 0 {
        return Err(CommandError::invalid_params(
            "/width",
            "the stroke width cannot be negative",
        ));
    }
    Ok(())
}

pub struct FootprintGraphicEdit;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FootprintGraphicEditParams {
    /// The open Footprint.
    pub footprint: Uuid,
    pub graphic: Uuid,
    pub layer: Option<FootprintLayer>,
    /// Replaces the whole shape.
    pub shape: Option<ShapeParam>,
    pub width: Option<Length>,
    pub fill: Option<bool>,
}

impl Command for FootprintGraphicEdit {
    const META: Meta = meta(
        "library.footprint.graphic.edit",
        "Edit Footprint Graphic",
        "Changes a Footprint graphic's Layer, shape, stroke width or fill. Omitted fields \
         keep their value.",
    );
    type Params = FootprintGraphicEditParams;
    type Output = ();

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        cx.elements()
    }

    fn run(&self, cx: &mut Context<'_>, p: FootprintGraphicEditParams) -> Result<(), CommandError> {
        let shape = p
            .shape
            .map(ShapeParam::shape)
            .transpose()
            .map_err(invalid("/shape"))?;
        if let Some(width) = p.width {
            check_width(width.nm())?;
        }
        edit_footprint(cx, p.footprint, |f| {
            let graphic = find_mut(&mut f.graphics, p.graphic, |g| g.uuid, "graphic")?;
            if let Some(layer) = p.layer {
                graphic.layer = layer;
            }
            if let Some(shape) = shape {
                graphic.shape = shape;
            }
            if let Some(width) = p.width {
                graphic.width = width.nm();
            }
            if let Some(fill) = p.fill {
                graphic.fill = fill;
            }
            Ok(())
        })
    }
}

pub struct FootprintGraphicRemove;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FootprintGraphicParams {
    /// The open Footprint.
    pub footprint: Uuid,
    pub graphic: Uuid,
}

impl Command for FootprintGraphicRemove {
    const META: Meta = meta(
        "library.footprint.graphic.remove",
        "Remove Footprint Graphic",
        "Removes a graphic from an open Footprint.",
    );
    type Params = FootprintGraphicParams;
    type Output = ();

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        cx.elements()
    }

    fn run(&self, cx: &mut Context<'_>, p: FootprintGraphicParams) -> Result<(), CommandError> {
        edit_footprint(cx, p.footprint, |f| {
            remove(&mut f.graphics, p.graphic, |g| g.uuid, "graphic").map(drop)
        })
    }
}

/// Which of a Footprint's two texts.
#[derive(Clone, Copy, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TextWhich {
    Designator,
    Value,
}

pub struct FootprintTextSet;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FootprintTextSetParams {
    /// The open Footprint.
    pub footprint: Uuid,
    pub text: TextWhich,
    /// The text centre, relative to the Footprint origin.
    pub at: Option<PointParam>,
    pub rotation: Option<Angle>,
    /// The text height.
    pub size: Option<Length>,
    pub layer: Option<FootprintLayer>,
    pub visible: Option<bool>,
}

impl Command for FootprintTextSet {
    const META: Meta = meta(
        "library.footprint.text.set",
        "Place Footprint Text",
        "Places the Designator or value text of an open Footprint: its position, rotation, \
         size, Layer and visibility. Omitted fields keep their value.",
    );
    type Params = FootprintTextSetParams;
    type Output = ();

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        cx.elements()
    }

    fn run(&self, cx: &mut Context<'_>, p: FootprintTextSetParams) -> Result<(), CommandError> {
        if let Some(size) = p.size {
            positive(size.nm(), "the text size").map_err(invalid("/size"))?;
        }
        edit_footprint(cx, p.footprint, |f| {
            let text = match p.text {
                TextWhich::Designator => &mut f.designator,
                TextWhich::Value => &mut f.value,
            };
            if let Some(at) = p.at {
                text.at = at.point();
            }
            if let Some(rotation) = p.rotation {
                text.rotation = rotation.millideg();
            }
            if let Some(size) = p.size {
                text.size = size.nm();
            }
            if let Some(layer) = p.layer {
                text.layer = layer;
            }
            if let Some(visible) = p.visible {
                text.visible = visible;
            }
            Ok(())
        })
    }
}

pub struct FootprintOriginSet;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FootprintOriginSetParams {
    /// The open Footprint.
    pub footprint: Uuid,
    /// The new origin, in the current coordinates.
    pub at: PointParam,
}

impl Command for FootprintOriginSet {
    const META: Meta = meta(
        "library.footprint.origin.set",
        "Set Footprint Origin",
        "Moves an open Footprint's origin to `at`: every Pad, graphic and text keeps its \
         place while its coordinates shift by `-at`.",
    );
    type Params = FootprintOriginSetParams;
    type Output = ();

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        cx.elements()
    }

    fn run(&self, cx: &mut Context<'_>, p: FootprintOriginSetParams) -> Result<(), CommandError> {
        let at = p.at.point();
        edit_footprint(cx, p.footprint, |f| {
            shift(f, Point::new(-at.x, -at.y));
            Ok(())
        })
    }
}

fn shift(f: &mut Footprint, by: Point) {
    for pad in &mut f.pads {
        pad.at = pad.at + by;
    }
    for graphic in &mut f.graphics {
        graphic.shape = graphic.shape.translated(by);
    }
    f.designator.at = f.designator.at + by;
    f.value.at = f.value.at + by;
}

pub struct FootprintMove;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FootprintMoveParams {
    /// The open Footprint.
    pub footprint: Uuid,
    /// Pads and graphics by UUID, and the Designator and value text as
    /// `fd000000-0000-4000-8000-000000000001` and `…0002`.
    pub objects: Vec<Uuid>,
    pub by: PointParam,
}

impl Command for FootprintMove {
    const META: Meta = meta(
        "library.footprint.move",
        "Move in Footprint",
        "Moves any mix of an open Footprint's Pads, graphics and Designator and value text \
         by `by`, in one step.",
    );
    type Params = FootprintMoveParams;
    type Output = ();

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        cx.elements()
    }

    fn run(&self, cx: &mut Context<'_>, p: FootprintMoveParams) -> Result<(), CommandError> {
        let by = p.by.point();
        edit_footprint(cx, p.footprint, |f| {
            for (i, uuid) in p.objects.iter().enumerate() {
                if *uuid == DESIGNATOR_TEXT {
                    f.designator.at = f.designator.at + by;
                } else if *uuid == VALUE_TEXT {
                    f.value.at = f.value.at + by;
                } else if let Some(pad) = f.pads.iter_mut().find(|pad| pad.uuid == *uuid) {
                    pad.at = pad.at + by;
                } else if let Some(g) = f.graphics.iter_mut().find(|g| g.uuid == *uuid) {
                    g.shape = g.shape.translated(by);
                } else {
                    return Err(not_in_footprint(i, *uuid));
                }
            }
            Ok(())
        })
    }
}

fn not_in_footprint(index: usize, uuid: Uuid) -> CommandError {
    CommandError::not_found(format!(
        "objects/{index}: the Footprint has no Pad or graphic {uuid}"
    ))
}

pub struct FootprintDelete;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FootprintDeleteParams {
    /// The open Footprint.
    pub footprint: Uuid,
    /// Pads and graphics by UUID, and the Designator and value text (as for
    /// `library.footprint.move`), which are hidden rather than removed.
    pub objects: Vec<Uuid>,
}

impl Command for FootprintDelete {
    const META: Meta = meta(
        "library.footprint.delete",
        "Delete in Footprint",
        "Removes any mix of an open Footprint's Pads and graphics in one step. The \
         Designator and value text cannot be removed, so they are hidden.",
    );
    type Params = FootprintDeleteParams;
    type Output = ();

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        cx.elements()
    }

    fn run(&self, cx: &mut Context<'_>, p: FootprintDeleteParams) -> Result<(), CommandError> {
        edit_footprint(cx, p.footprint, |f| {
            for (i, uuid) in p.objects.iter().enumerate() {
                if *uuid == DESIGNATOR_TEXT {
                    f.designator.visible = false;
                } else if *uuid == VALUE_TEXT {
                    f.value.visible = false;
                } else if f.pads.iter().any(|pad| pad.uuid == *uuid) {
                    f.pads.retain(|pad| pad.uuid != *uuid);
                } else if f.graphics.iter().any(|g| g.uuid == *uuid) {
                    f.graphics.retain(|g| g.uuid != *uuid);
                } else {
                    return Err(not_in_footprint(i, *uuid));
                }
            }
            Ok(())
        })
    }
}
