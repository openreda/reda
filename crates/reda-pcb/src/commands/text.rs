//! Free text on the board.

use reda_command::{
    Angle, AvailabilityContext, Command, CommandError, Context, Kind, Length, Meta, Registry,
    Unavailable,
};
use reda_model::geom::{Millideg, Nm, Point};
use reda_model::{
    HERSHEY_FONT, InDocument, Layer, Object, PcbText, Side, TextFrame, TextJustify, TextKnockout,
    TextStyle, Uuid,
};
use schemars::JsonSchema;
use serde::Deserialize;

use super::layer::{LayerRef, drawing_layer};
use super::{Created, needs_pcb, pcb, positive};

pub(super) fn register_all(registry: &mut Registry) {
    registry.add(TextAdd);
    registry.add(TextSet);
    registry.add(TextDelete);
}

/// The capital height of new text when the params leave it out.
pub const DEFAULT_TEXT_HEIGHT: Nm = Nm::from_mm(1);
/// The stroke width of new text when the params leave it out.
pub const DEFAULT_TEXT_THICKNESS: Nm = Nm::from_um(150);

fn check_text(text: &str) -> Result<(), CommandError> {
    if text.trim().is_empty() {
        return Err(CommandError::invalid_params("/text", "the text is empty"));
    }
    CommandError::check_text("/text", text)
}

fn check_font(font: &str) -> Result<(), CommandError> {
    if font.trim().is_empty() {
        return Err(CommandError::invalid_params(
            "/font",
            "the font family is empty",
        ));
    }
    Ok(())
}

/// The mirroring for text on `layer`: `asked` if given, else mirrored on bottom Layers.
fn mirroring(layer: &Layer, asked: Option<bool>) -> Result<bool, CommandError> {
    let bottom = layer.side == Some(Side::Bottom);
    match asked {
        Some(true) if !bottom => Err(CommandError::invalid_params(
            "/mirrored",
            "only text on a bottom Layer can be mirrored",
        )),
        Some(mirrored) => Ok(mirrored),
        None => Ok(bottom),
    }
}

pub struct TextAdd;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TextAddParams {
    /// The Layer: copper, silkscreen, mechanical or documentation.
    pub layer: LayerRef,
    /// The text; `\n` starts a new line.
    pub text: String,
    /// The text block or frame anchor.
    pub x: Length,
    pub y: Length,
    /// The capital height; 1 mm when left out.
    #[serde(default)]
    pub height: Option<Length>,
    /// The pen width of stroke-font text; 0.15 mm when left out.
    #[serde(default)]
    pub thickness: Option<Length>,
    /// Counter-clockwise about the start point; 0 when left out.
    #[serde(default)]
    pub rotation: Option<Angle>,
    /// The font family; `Hershey Roman Simplex` when left out. `Noto Sans` is bundled;
    /// other families come from the fonts installed on this machine.
    #[serde(default)]
    pub font: Option<String>,
    /// Draw mirrored so it reads from below. Only on bottom Layers, where it is the
    /// default.
    #[serde(default)]
    pub mirrored: Option<bool>,
    #[serde(default)]
    pub justify: Option<TextJustify>,
    #[serde(default)]
    pub knockout: Option<KnockoutParam>,
    #[serde(default)]
    pub frame: Option<FrameParam>,
}

impl Command for TextAdd {
    const META: Meta = Meta {
        id: "pcb.text.add",
        title: "Add Text",
        description: "Adds free text on a copper, silkscreen, mechanical or documentation \
                      Layer, anchored at (`x`, `y`), left/bottom by default. It uses \
                      the Hershey stroke font unless `font` names another family. Text on a \
                      bottom Layer is mirrored unless `mirrored` is false.",
        category: "pcb",
        icon: "text",
        kind: Kind::Edit,
    };
    type Params = TextAddParams;
    type Output = Created;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: TextAddParams) -> Result<Created, CommandError> {
        check_text(&p.text)?;
        let font = p.font.unwrap_or_else(|| HERSHEY_FONT.into());
        check_font(&font)?;
        let height = p
            .height
            .map_or(Ok(DEFAULT_TEXT_HEIGHT), |h| positive("/height", h))?;
        let thickness = p
            .thickness
            .map_or(Ok(DEFAULT_TEXT_THICKNESS), |t| positive("/thickness", t))?;
        let pcb = pcb(cx)?;
        let layer = drawing_layer(pcb, p.layer, "/layer")?;
        let mirrored = mirroring(layer, p.mirrored)?;
        let (document, layer) = (pcb.uuid, layer.uuid);
        let uuid = cx.new_id();
        let text = PcbText {
            uuid,
            layer,
            text: p.text,
            position: Point::new(p.x.nm(), p.y.nm()),
            style: TextStyle {
                font,
                height,
                thickness,
                justify: p.justify.unwrap_or_default(),
                knockout: p.knockout.map(KnockoutParam::resolve).transpose()?,
            },
            frame: p.frame.map(FrameParam::resolve).transpose()?,
            rotation: p
                .rotation
                .map_or(Millideg::ZERO, |r| r.millideg().normalized()),
            mirrored,
        };
        cx.tx()?
            .insert(Object::PcbText(InDocument::new(document, text)))?;
        Ok(Created { uuid })
    }
}

pub struct TextSet;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TextSetParams {
    /// The text object's UUID.
    pub uuid: Uuid,
    /// A new Layer: copper, silkscreen, mechanical or documentation.
    #[serde(default)]
    pub layer: Option<LayerRef>,
    /// New text; `\n` starts a new line.
    #[serde(default)]
    pub text: Option<String>,
    /// A new anchor; give both coordinates.
    #[serde(default)]
    pub x: Option<Length>,
    #[serde(default)]
    pub y: Option<Length>,
    #[serde(default)]
    pub height: Option<Length>,
    #[serde(default)]
    pub thickness: Option<Length>,
    #[serde(default)]
    pub rotation: Option<Angle>,
    #[serde(default)]
    pub font: Option<String>,
    /// Mirroring; only on bottom Layers. Moving the text to another Layer without it
    /// mirrors on bottom Layers and unmirrors elsewhere.
    #[serde(default)]
    pub mirrored: Option<bool>,
    #[serde(default)]
    pub justify: Option<TextJustify>,
    #[serde(default)]
    pub knockout: Option<KnockoutParam>,
    #[serde(default)]
    pub frame: Option<FrameParam>,
    #[serde(default)]
    pub clear_knockout: bool,
    #[serde(default)]
    pub clear_frame: bool,
}

impl Command for TextSet {
    const META: Meta = Meta {
        id: "pcb.text.set",
        title: "Edit Text",
        description: "Changes the given properties of one text object and keeps the rest. \
                      Moving it to another Layer without `mirrored` mirrors it on a bottom \
                      Layer and unmirrors it elsewhere.",
        category: "pcb",
        icon: "text_edit",
        kind: Kind::Edit,
    };
    type Params = TextSetParams;
    type Output = ();

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: TextSetParams) -> Result<(), CommandError> {
        let pcb = pcb(cx)?;
        let mut text = pcb
            .text(p.uuid)
            .cloned()
            .ok_or_else(|| CommandError::not_found(format!("no text has UUID {}", p.uuid)))?;
        if let Some(value) = p.text {
            check_text(&value)?;
            text.text = value;
        }
        if let Some(font) = p.font {
            check_font(&font)?;
            text.font = font;
        }
        match (p.x, p.y) {
            (Some(x), Some(y)) => text.position = Point::new(x.nm(), y.nm()),
            (None, None) => {}
            (Some(_), None) => return Err(CommandError::invalid_params("/y", "give y with x")),
            (None, Some(_)) => return Err(CommandError::invalid_params("/x", "give x with y")),
        }
        if let Some(height) = p.height {
            text.height = positive("/height", height)?;
        }
        if let Some(thickness) = p.thickness {
            text.thickness = positive("/thickness", thickness)?;
        }
        if let Some(rotation) = p.rotation {
            text.rotation = rotation.millideg().normalized();
        }
        if let Some(justify) = p.justify {
            text.justify = justify;
        }
        if let Some(knockout) = p.knockout {
            text.knockout = Some(knockout.resolve()?);
        }
        if let Some(frame) = p.frame {
            text.frame = Some(frame.resolve()?);
        }
        if p.clear_knockout {
            text.knockout = None;
        }
        if p.clear_frame {
            text.frame = None;
        }
        let moved = p.layer.is_some();
        let layer = match p.layer {
            Some(layer) => drawing_layer(pcb, layer, "/layer")?,
            None => pcb.layers().layer(text.layer).expect("checked on commit"),
        };
        text.layer = layer.uuid;
        text.mirrored = match (p.mirrored, moved) {
            (None, false) => text.mirrored,
            (asked, _) => mirroring(layer, asked)?,
        };
        let document = pcb.uuid;
        cx.tx()?
            .replace(Object::PcbText(InDocument::new(document, text)))?;
        Ok(())
    }
}

pub struct TextDelete;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TextDeleteParams {
    /// The text object's UUID.
    pub uuid: Uuid,
}

impl Command for TextDelete {
    const META: Meta = Meta {
        id: "pcb.text.delete",
        title: "Delete Text",
        description: "Deletes one text object.",
        category: "pcb",
        icon: "delete",
        kind: Kind::Edit,
    };
    type Params = TextDeleteParams;
    type Output = ();

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: TextDeleteParams) -> Result<(), CommandError> {
        if pcb(cx)?.text(p.uuid).is_none() {
            return Err(CommandError::not_found(format!(
                "no text has UUID {}",
                p.uuid
            )));
        }
        cx.tx()?.remove(p.uuid)?;
        Ok(())
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct KnockoutParam {
    pub margin: Length,
}
impl KnockoutParam {
    pub(super) fn resolve(self) -> Result<TextKnockout, CommandError> {
        let margin = self.margin.nm();
        if margin < Nm::ZERO {
            return Err(CommandError::invalid_params(
                "/knockout/margin",
                "margin must be nonnegative",
            ));
        }
        Ok(TextKnockout { margin })
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FrameParam {
    pub width: Length,
    pub height: Length,
}
impl FrameParam {
    fn resolve(self) -> Result<TextFrame, CommandError> {
        Ok(TextFrame {
            width: positive("/frame/width", self.width)?,
            height: positive("/frame/height", self.height)?,
        })
    }
}

/// Sparse shared style patch, used when changing Footprint text across instances.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StyleParam {
    pub font: Option<String>,
    pub height: Option<Length>,
    pub thickness: Option<Length>,
    pub justify: Option<TextJustify>,
    pub knockout: Option<KnockoutParam>,
    #[serde(default)]
    pub clear_knockout: bool,
}
impl StyleParam {
    pub(super) fn apply(&self, style: &mut TextStyle) -> Result<(), CommandError> {
        if let Some(font) = &self.font {
            check_font(font)?;
            style.font = font.clone();
        }
        if let Some(height) = self.height {
            style.height = positive("/style/height", height)?;
        }
        if let Some(thickness) = self.thickness {
            style.thickness = positive("/style/thickness", thickness)?;
        }
        if let Some(justify) = self.justify {
            style.justify = justify;
        }
        if let Some(knockout) = &self.knockout {
            let margin = knockout.margin.nm();
            if margin < Nm::ZERO {
                return Err(CommandError::invalid_params(
                    "/style/knockout/margin",
                    "margin must be nonnegative",
                ));
            }
            style.knockout = Some(TextKnockout { margin });
        }
        if self.clear_knockout {
            style.knockout = None;
        }
        Ok(())
    }
}
