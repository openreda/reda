//! The Footprint editor's Commands: the Footprint's own properties, its Pads, graphics,
//! Designator and value text, and its origin.

use reda_command::{Angle, Command, CommandError, Context, Kind, Length, Meta, Registry};
use reda_model::geom::{Millideg, Nm, Point};
use reda_model::{Footprint, FootprintGraphic, FootprintLayer, Hole, Pad, PadKind, PadShape, Uuid};
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
}

impl Command for FootprintEdit {
    const META: Meta = meta(
        "library.footprint.edit",
        "Edit Footprint",
        "Changes an open Footprint's name or description. Omitted fields keep their value.",
    );
    type Params = FootprintEditParams;
    type Output = ();

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

/// A drilled hole.
#[derive(Clone, Copy, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HoleParam {
    pub width: Length,
    /// Omitted, the same as `width`: a round hole. Otherwise a slot along the longer side.
    pub height: Option<Length>,
}

impl HoleParam {
    fn hole(self) -> Hole {
        Hole {
            width: self.width.nm(),
            height: self.height.unwrap_or(self.width).nm(),
        }
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
    /// The Pad centre, relative to the Footprint origin.
    pub at: PointParam,
    pub rotation: Option<Angle>,
    pub width: Length,
    pub height: Length,
    /// Only for `rounded_rect`. Omitted, a quarter of the shorter side.
    pub corner_radius: Option<Length>,
    /// Required for plated and non-plated through-hole Pads; SMD Pads have none.
    pub hole: Option<HoleParam>,
    /// Overrides the board's solder-mask margin for this Pad.
    pub mask_margin: Option<Length>,
    /// Overrides the board's solder-paste margin for this Pad.
    pub paste_margin: Option<Length>,
}

impl Command for PadAdd {
    const META: Meta = meta(
        "library.footprint.pad.add",
        "Add Pad",
        "Adds an SMD, plated through-hole or non-plated Pad, shaped as a circle, \
         rectangle, rounded rectangle or oval, with an optional round hole or slot and \
         per-Pad mask and paste margins.",
    );
    type Params = PadAddParams;
    type Output = PadResult;

    fn run(&self, cx: &mut Context<'_>, p: PadAddParams) -> Result<PadResult, CommandError> {
        let uuid = cx.new_id();
        let pad = Pad {
            uuid,
            number: p.number,
            kind: p.kind,
            shape: p.shape,
            at: p.at.point(),
            rotation: p.rotation.map_or(Millideg(0), Angle::millideg),
            width: p.width.nm(),
            height: p.height.nm(),
            corner_radius: p.corner_radius.map(Length::nm),
            hole: p.hole.map(HoleParam::hole),
            mask_margin: p.mask_margin.map(Length::nm),
            paste_margin: p.paste_margin.map(Length::nm),
        };
        let pad = finish_pad(pad, p.corner_radius.is_some())?;
        edit_footprint(cx, p.footprint, |f| {
            f.pads.push(pad);
            f.pads.sort_by_key(|pad| pad.uuid);
            Ok(PadResult { pad: uuid })
        })
    }
}

/// Checks a Pad and fills in what follows from its shape: a rounded rectangle without a
/// radius gets a quarter of its shorter side, and other shapes drop a radius not given
/// explicitly (`explicit_radius`), which they cannot use.
fn finish_pad(mut pad: Pad, explicit_radius: bool) -> Result<Pad, CommandError> {
    positive(pad.width, "the Pad width").map_err(invalid("/width"))?;
    positive(pad.height, "the Pad height").map_err(invalid("/height"))?;
    let shorter = Nm(pad.width.0.min(pad.height.0));
    match (pad.shape, pad.corner_radius) {
        (PadShape::RoundedRect, None) => pad.corner_radius = Some(Nm(shorter.0 / 4)),
        (PadShape::RoundedRect, Some(r)) if r.0 < 0 || r.0 * 2 > shorter.0 => {
            return Err(CommandError::invalid_params(
                "/corner_radius",
                "the corner radius must be between zero and half the shorter side",
            ));
        }
        (PadShape::RoundedRect, Some(_)) => {}
        (_, Some(_)) if explicit_radius => {
            return Err(CommandError::invalid_params(
                "/corner_radius",
                "only a rounded_rect Pad has a corner radius",
            ));
        }
        (_, _) => pad.corner_radius = None,
    }
    match (pad.kind, pad.hole) {
        (PadKind::Smd, Some(_)) => {
            return Err(CommandError::invalid_params(
                "/hole",
                "an SMD Pad has no hole",
            ));
        }
        (PadKind::Smd, None) => {}
        (_, None) => {
            return Err(CommandError::invalid_params(
                "/hole",
                "a through-hole Pad needs a hole",
            ));
        }
        (kind, Some(hole)) => {
            positive(hole.width, "the hole width").map_err(invalid("/hole/width"))?;
            positive(hole.height, "the hole height").map_err(invalid("/hole/height"))?;
            if kind == PadKind::PlatedThroughHole
                && (hole.width > pad.width || hole.height > pad.height)
            {
                return Err(CommandError::invalid_params(
                    "/hole",
                    "a plated hole must fit inside its Pad",
                ));
            }
        }
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
    /// `null` returns a rounded rectangle to the default radius.
    #[serde(default, deserialize_with = "present")]
    pub corner_radius: Option<Option<Length>>,
    /// `null` removes the hole.
    #[serde(default, deserialize_with = "present")]
    pub hole: Option<Option<HoleParam>>,
    /// `null` returns to the board's margin.
    #[serde(default, deserialize_with = "present")]
    pub mask_margin: Option<Option<Length>>,
    /// `null` returns to the board's margin.
    #[serde(default, deserialize_with = "present")]
    pub paste_margin: Option<Option<Length>>,
}

impl Command for PadEdit {
    const META: Meta = meta(
        "library.footprint.pad.edit",
        "Edit Pad",
        "Changes any of a Pad's number, kind, shape, position, rotation, size, corner \
         radius, hole and mask and paste margins. Omitted fields keep their value; `null` \
         clears an optional one.",
    );
    type Params = PadEditParams;
    type Output = ();

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
            let explicit_radius = matches!(p.corner_radius, Some(Some(_)));
            if let Some(radius) = p.corner_radius {
                pad.corner_radius = radius.map(Length::nm);
            } else if p.width.is_some() || p.height.is_some() {
                // A resized rounded rectangle keeps its proportions.
                pad.corner_radius = None;
            }
            if let Some(hole) = p.hole {
                pad.hole = hole.map(HoleParam::hole);
            }
            if let Some(margin) = p.mask_margin {
                pad.mask_margin = margin.map(Length::nm);
            }
            if let Some(margin) = p.paste_margin {
                pad.paste_margin = margin.map(Length::nm);
            }
            let pad = finish_pad(pad, explicit_radius)?;
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
