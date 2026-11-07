//! The Footprint editor's Commands: the Footprint's own properties, its Pads, graphics,
//! field texts, and its origin.

use reda_command::{
    Angle, AvailabilityContext, Command, CommandError, Context, HoleParam, Kind, Length, Meta,
    RatioParam, Registry, Unavailable, ZoneConnectionParam, zone_connection,
};
use reda_model::geom::{Millideg, Nm, Point};
use reda_model::{
    Corners, Footprint, FootprintGraphic, FootprintLayer, FootprintText, Pad, PadKind, PadShape,
    Ratio, Shape, Side, TextJustify, TextKnockout, TextStyle, Uuid, ZoneConnection,
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
    registry.add(FootprintTextAdd);
    registry.add(FootprintTextEdit);
    registry.add(FootprintTextRemove);
    registry.add(FootprintOriginSet);
    registry.add(FootprintMove);
    registry.add(FootprintDelete);
}

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
    /// The part's greatest height above the board; `null` clears it.
    #[serde(default, deserialize_with = "present")]
    pub height: Option<Option<Length>>,
}

impl Command for FootprintEdit {
    const META: Meta = meta(
        "library.footprint.edit",
        "Edit Footprint",
        "Changes an open Footprint's name, description or height. Omitted fields keep \
         their value; a `null` height clears it.",
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
        if let Some(Some(height)) = p.height {
            positive(height.nm(), "the height").map_err(invalid("/height"))?;
        }
        edit_footprint(cx, p.footprint, |f| {
            if let Some(name) = p.name {
                f.name = name;
            }
            if let Some(description) = p.description {
                f.description = description;
            }
            if let Some(height) = p.height {
                f.height = height.map(Length::nm);
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
    /// Per-side mask expansion or tenting; omitted sides inherit.
    pub mask: Option<reda_command::MaskParam>,
    /// SMD paste expansion, signed area percent, none or windows; omitted, inherit.
    pub paste: Option<reda_command::PasteParam>,
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
         paste overrides and its Zone connection.",
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
            mask: reda_command::mask(p.mask),
            paste: reda_command::paste(p.paste),
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
    if let Some((pointer, message)) = pad.problem() {
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
    /// `null` inherits the board's rule.
    #[serde(default, deserialize_with = "present")]
    pub mask: Option<Option<reda_command::MaskParam>>,
    /// `null` inherits the board's rule.
    #[serde(default, deserialize_with = "present")]
    pub paste: Option<Option<reda_command::PasteParam>>,
    /// Replaces the whole Zone connection; `null` inherits every value.
    #[serde(default, deserialize_with = "present")]
    pub zone_connection: Option<Option<ZoneConnectionParam>>,
}

impl Command for PadEdit {
    const META: Meta = meta(
        "library.footprint.pad.edit",
        "Edit Pad",
        "Changes any of a Pad's number, kind, shape, position, rotation, size, corner and \
         chamfer ratios and corners, offset, hole, side, mask and paste overrides and Zone \
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
            if let Some(margin) = p.mask {
                pad.mask = reda_command::mask(margin);
            }
            if let Some(margin) = p.paste {
                pad.paste = reda_command::paste(margin);
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
         Courtyard or Assembly Layer, top or bottom, of an open Footprint. A Courtyard \
         Layer takes only closed shapes: rectangles, circles and polygons.",
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
        check_closed(graphic.layer, &graphic.shape)?;
        edit_footprint(cx, p.footprint, |f| {
            f.graphics.push(graphic);
            f.graphics.sort_by_key(|g| g.uuid);
            Ok(GraphicResult { graphic: uuid })
        })
    }
}

/// Refuses an open shape (a line, arc or text) on a Courtyard Layer, which takes only
/// closed outlines so a courtyard can never be left open.
fn check_closed(layer: FootprintLayer, shape: &Shape) -> Result<(), CommandError> {
    let closed = matches!(
        shape,
        Shape::Rect { .. } | Shape::Circle { .. } | Shape::Polygon { .. }
    );
    if layer.is_courtyard() && !closed {
        return Err(CommandError::invalid_params(
            "/shape",
            "a Courtyard Layer takes only closed shapes: a rectangle, circle or polygon",
        ));
    }
    Ok(())
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
         keep their value. A Courtyard Layer takes only closed shapes.",
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
            check_closed(graphic.layer, &graphic.shape)
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

#[derive(Debug, Serialize, JsonSchema)]
pub struct FootprintTextResult {
    /// The new text's UUID.
    pub text: Uuid,
}

pub struct FootprintTextAdd;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FootprintTextAddParams {
    /// The open Footprint.
    pub footprint: Uuid,
    /// The field it shows: `Designator`, `Value` or any field name.
    pub field: String,
    /// The text anchor, relative to the Footprint origin; `justify` says how the text sits
    /// on it.
    pub at: PointParam,
    pub rotation: Option<Angle>,
    /// A Silkscreen or Assembly Layer. Omitted, the top Silkscreen.
    pub layer: Option<FootprintLayer>,
    /// The capital height. Omitted, 1 mm.
    pub height: Option<Length>,
    /// The pen width. Omitted, 15% of the height.
    pub thickness: Option<Length>,
    /// The font family. Omitted, Hershey Roman Simplex.
    pub font: Option<String>,
    /// Omitted, centred on the anchor.
    pub justify: Option<TextJustify>,
    /// A box around the text with the glyphs removed. Omitted, none.
    pub knockout: Option<KnockoutParam>,
    /// Omitted, shown.
    pub visible: Option<bool>,
}

/// The knockout box around a text, grown by `margin`.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct KnockoutParam {
    pub margin: Length,
}

impl Command for FootprintTextAdd {
    const META: Meta = meta(
        "library.footprint.text.add",
        "Add Footprint Text",
        "Adds a text showing a field (`Designator`, `Value` or any field name) to an open \
         Footprint, on a Silkscreen or Assembly Layer. A Component without that field draws \
         nothing there.",
    );
    type Params = FootprintTextAddParams;
    type Output = FootprintTextResult;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        cx.elements()
    }

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: FootprintTextAddParams,
    ) -> Result<FootprintTextResult, CommandError> {
        let uuid = cx.new_id();
        let mut text = FootprintText::designator(uuid);
        text.field = p.field;
        text.at = p.at.point();
        if let Some(height) = p.height {
            text.style = TextStyle::of_height(height.nm());
        }
        let change = TextChange {
            field: None,
            at: None,
            rotation: p.rotation,
            layer: p.layer,
            height: None,
            thickness: p.thickness,
            font: p.font,
            justify: p.justify,
            knockout: p.knockout,
            clear_knockout: false,
            visible: p.visible,
        };
        change.apply(&mut text)?;
        edit_footprint(cx, p.footprint, |f| {
            f.texts.push(text);
            f.texts.sort_by_key(|t| t.uuid);
            Ok(FootprintTextResult { text: uuid })
        })
    }
}

pub struct FootprintTextEdit;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FootprintTextEditParams {
    /// The open Footprint.
    pub footprint: Uuid,
    pub text: Uuid,
    /// The field it shows: `Designator`, `Value` or any field name.
    pub field: Option<String>,
    /// The text anchor, relative to the Footprint origin.
    pub at: Option<PointParam>,
    pub rotation: Option<Angle>,
    /// A Silkscreen or Assembly Layer.
    pub layer: Option<FootprintLayer>,
    /// The capital height. Without a new `thickness`, the pen width keeps its proportion
    /// to the height.
    pub height: Option<Length>,
    /// The pen width.
    pub thickness: Option<Length>,
    /// The font family.
    pub font: Option<String>,
    /// How the text sits on its anchor.
    pub justify: Option<TextJustify>,
    /// Adds or changes the knockout box.
    pub knockout: Option<KnockoutParam>,
    /// Removes the knockout box.
    #[serde(default)]
    pub clear_knockout: bool,
    pub visible: Option<bool>,
}

/// The parts of a Footprint text to change; `None` keeps the value.
struct TextChange {
    field: Option<String>,
    at: Option<PointParam>,
    rotation: Option<Angle>,
    layer: Option<FootprintLayer>,
    height: Option<Length>,
    thickness: Option<Length>,
    font: Option<String>,
    justify: Option<TextJustify>,
    knockout: Option<KnockoutParam>,
    clear_knockout: bool,
    visible: Option<bool>,
}

impl TextChange {
    /// Applies the change to `text` and checks the result.
    fn apply(self, text: &mut FootprintText) -> Result<(), CommandError> {
        if let Some(field) = self.field {
            text.field = field;
        }
        if let Some(at) = self.at {
            text.at = at.point();
        }
        if let Some(rotation) = self.rotation {
            text.rotation = rotation.millideg();
        }
        if let Some(layer) = self.layer {
            text.layer = layer;
        }
        if let Some(height) = self.height {
            // Without a new pen width, the pen keeps its proportion to the height.
            let (old, new) = (text.style.height.0, height.nm().0);
            if self.thickness.is_none() && old > 0 {
                let scaled = i128::from(text.style.thickness.0) * i128::from(new) / i128::from(old);
                text.style.thickness = Nm(scaled as i64);
            }
            text.style.height = height.nm();
        }
        if let Some(thickness) = self.thickness {
            text.style.thickness = thickness.nm();
        }
        if let Some(font) = self.font {
            text.style.font = font;
        }
        if let Some(justify) = self.justify {
            text.style.justify = justify;
        }
        if self.clear_knockout && self.knockout.is_some() {
            return Err(CommandError::invalid_params(
                "/clear_knockout",
                "give knockout or clear_knockout, not both",
            ));
        }
        if let Some(knockout) = self.knockout {
            let margin = knockout.margin.nm();
            if margin < Nm(0) {
                return Err(CommandError::invalid_params(
                    "/knockout/margin",
                    "the knockout margin is negative",
                ));
            }
            text.style.knockout = Some(TextKnockout { margin });
        }
        if self.clear_knockout {
            text.style.knockout = None;
        }
        if let Some(visible) = self.visible {
            text.visible = visible;
        }
        if text.field.trim().is_empty() {
            return Err(CommandError::invalid_params(
                "/field",
                "the field name is empty",
            ));
        }
        if !text.layer.is_silkscreen() && !text.layer.is_assembly() {
            return Err(CommandError::invalid_params(
                "/layer",
                "Footprint text goes on a Silkscreen or Assembly Layer",
            ));
        }
        positive(text.style.height, "the text height").map_err(invalid("/height"))?;
        positive(text.style.thickness, "the pen width").map_err(invalid("/thickness"))?;
        if text.style.font.trim().is_empty() {
            return Err(CommandError::invalid_params(
                "/font",
                "the font family is empty",
            ));
        }
        Ok(())
    }
}

impl Command for FootprintTextEdit {
    const META: Meta = meta(
        "library.footprint.text.edit",
        "Edit Footprint Text",
        "Changes a Footprint text's field, anchor, rotation, Layer, height, pen width, \
         font, justification, knockout or visibility. Omitted fields keep their value.",
    );
    type Params = FootprintTextEditParams;
    type Output = ();

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        cx.elements()
    }

    fn run(&self, cx: &mut Context<'_>, p: FootprintTextEditParams) -> Result<(), CommandError> {
        edit_footprint(cx, p.footprint, |f| {
            let text = find_mut(&mut f.texts, p.text, |t| t.uuid, "text")?;
            TextChange {
                field: p.field,
                at: p.at,
                rotation: p.rotation,
                layer: p.layer,
                height: p.height,
                thickness: p.thickness,
                font: p.font,
                justify: p.justify,
                knockout: p.knockout,
                clear_knockout: p.clear_knockout,
                visible: p.visible,
            }
            .apply(text)
        })
    }
}

pub struct FootprintTextRemove;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FootprintTextParams {
    /// The open Footprint.
    pub footprint: Uuid,
    pub text: Uuid,
}

impl Command for FootprintTextRemove {
    const META: Meta = meta(
        "library.footprint.text.remove",
        "Remove Footprint Text",
        "Removes a field text from an open Footprint.",
    );
    type Params = FootprintTextParams;
    type Output = ();

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        cx.elements()
    }

    fn run(&self, cx: &mut Context<'_>, p: FootprintTextParams) -> Result<(), CommandError> {
        edit_footprint(cx, p.footprint, |f| {
            remove(&mut f.texts, p.text, |t| t.uuid, "text").map(drop)
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
    for text in &mut f.texts {
        text.at = text.at + by;
    }
}

pub struct FootprintMove;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FootprintMoveParams {
    /// The open Footprint.
    pub footprint: Uuid,
    /// Pads, graphics and texts by UUID.
    pub objects: Vec<Uuid>,
    pub by: PointParam,
}

impl Command for FootprintMove {
    const META: Meta = meta(
        "library.footprint.move",
        "Move in Footprint",
        "Moves any mix of an open Footprint's Pads, graphics and texts by `by`, in one step.",
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
                if let Some(text) = f.texts.iter_mut().find(|t| t.uuid == *uuid) {
                    text.at = text.at + by;
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
        "objects/{index}: the Footprint has no Pad, graphic or text {uuid}"
    ))
}

pub struct FootprintDelete;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FootprintDeleteParams {
    /// The open Footprint.
    pub footprint: Uuid,
    /// Pads, graphics and texts by UUID.
    pub objects: Vec<Uuid>,
}

impl Command for FootprintDelete {
    const META: Meta = meta(
        "library.footprint.delete",
        "Delete in Footprint",
        "Removes any mix of an open Footprint's Pads, graphics and texts in one step.",
    );
    type Params = FootprintDeleteParams;
    type Output = ();

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        cx.elements()
    }

    fn run(&self, cx: &mut Context<'_>, p: FootprintDeleteParams) -> Result<(), CommandError> {
        edit_footprint(cx, p.footprint, |f| {
            for (i, uuid) in p.objects.iter().enumerate() {
                if f.texts.iter().any(|t| t.uuid == *uuid) {
                    f.texts.retain(|t| t.uuid != *uuid);
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
