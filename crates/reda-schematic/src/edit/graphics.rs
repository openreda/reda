//! Free text, lines and rectangles: `schematic.text.add` / `schematic.text.set` and
//! `schematic.graphic.add_line` / `add_rect` / `set`. Moving, turning and deleting them
//! goes through the shared editing Commands.

use reda_command::{Angle, Command, CommandError, Context, Kind, Length, Meta};
use reda_model::geom::{Millideg, Nm};
use reda_model::{InDocument, Justify, Object, SheetGraphic, SheetShape, SheetText, Uuid};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::component::At;
use crate::sheet;

/// The height of new text: 50 mil.
const DEFAULT_HEIGHT: Nm = Nm::from_mil(50);

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TextAddParams {
    /// The Schematic Sheet.
    pub document: Uuid,
    /// The text; `\n` starts a new line.
    pub text: String,
    /// The anchor point.
    pub at: At,
    /// The capital height; 50 mil by default.
    pub height: Option<Length>,
    /// Counter-clockwise about `at`; 0 by default.
    pub rotation: Option<Angle>,
    /// Which of the text block's nine points sits on `at`: `top_left`, `top`, `top_right`,
    /// `left`, `center`, `right`, `bottom_left` (the default), `bottom` or `bottom_right`.
    /// Top is the first line's capital height and bottom the last line's baseline.
    pub justify: Option<Justify>,
}

/// The text `schematic.text.add` made.
#[derive(Debug, Serialize, JsonSchema)]
pub struct TextResult {
    pub text: Uuid,
}

pub struct TextAdd;

impl Command for TextAdd {
    const META: Meta = Meta {
        id: "schematic.text.add",
        title: "Add Text",
        description: "Places free text on a Schematic Sheet: one or more lines, anchored \
                      at any of nine points of the block. It has no electrical meaning.",
        category: "schematic",
        icon: "text",
        kind: Kind::Edit,
    };
    type Params = TextAddParams;
    type Output = TextResult;

    fn run(&self, cx: &mut Context<'_>, p: TextAddParams) -> Result<TextResult, CommandError> {
        sheet(cx.project()?, p.document)?;
        let text = SheetText {
            uuid: cx.new_id(),
            text: check_text(p.text)?,
            at: p.at.point(),
            height: check_height(p.height.map_or(DEFAULT_HEIGHT, Length::nm))?,
            rotation: p
                .rotation
                .map_or(Millideg::ZERO, |a| a.millideg().normalized()),
            justify: p.justify.unwrap_or_default(),
        };
        let uuid = text.uuid;
        cx.tx()?
            .insert(Object::SheetText(InDocument::new(p.document, text)))?;
        Ok(TextResult { text: uuid })
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TextSetParams {
    /// The text object's UUID.
    pub uuid: Uuid,
    /// New text; `\n` starts a new line.
    pub text: Option<String>,
    /// A new baseline start.
    pub at: Option<At>,
    /// A new capital height.
    pub height: Option<Length>,
    /// A new rotation, counter-clockwise.
    pub rotation: Option<Angle>,
    /// A new anchor point on the text block.
    pub justify: Option<Justify>,
}

pub struct TextSet;

impl Command for TextSet {
    const META: Meta = Meta {
        id: "schematic.text.set",
        title: "Edit Text",
        description: "Changes a free text's words, position, height, rotation or \
                      justification; what is left out stays.",
        category: "schematic",
        icon: "text",
        kind: Kind::Edit,
    };
    type Params = TextSetParams;
    type Output = TextResult;

    fn run(&self, cx: &mut Context<'_>, p: TextSetParams) -> Result<TextResult, CommandError> {
        let Some(Object::SheetText(mut text)) = cx.project()?.object(p.uuid) else {
            return Err(CommandError::not_found(format!(
                "no sheet text has UUID {}",
                p.uuid
            )));
        };
        if let Some(words) = p.text {
            text.object.text = check_text(words)?;
        }
        if let Some(at) = p.at {
            text.object.at = at.point();
        }
        if let Some(height) = p.height {
            text.object.height = check_height(height.nm())?;
        }
        if let Some(rotation) = p.rotation {
            text.object.rotation = rotation.millideg().normalized();
        }
        if let Some(justify) = p.justify {
            text.object.justify = justify;
        }
        cx.tx()?.replace(Object::SheetText(text))?;
        Ok(TextResult { text: p.uuid })
    }
}

fn check_text(text: String) -> Result<String, CommandError> {
    match text.trim().is_empty() {
        true => Err(CommandError::invalid_params("/text", "the text is empty")),
        false => Ok(text),
    }
}

fn check_height(height: Nm) -> Result<Nm, CommandError> {
    match height > Nm::ZERO {
        true => Ok(height),
        false => Err(CommandError::invalid_params(
            "/height",
            "the height must be positive",
        )),
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LineAddParams {
    /// The Schematic Sheet.
    pub document: Uuid,
    pub from: At,
    pub to: At,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RectAddParams {
    /// The Schematic Sheet.
    pub document: Uuid,
    /// One corner.
    pub from: At,
    /// The opposite corner.
    pub to: At,
    /// Filled rather than outlined.
    #[serde(default)]
    pub filled: bool,
}

/// The graphic a Command made or changed.
#[derive(Debug, Serialize, JsonSchema)]
pub struct GraphicResult {
    pub graphic: Uuid,
}

pub struct LineAdd;

impl Command for LineAdd {
    const META: Meta = Meta {
        id: "schematic.graphic.add_line",
        title: "Add Line",
        description: "Draws a free straight line, at any angle, on a Schematic Sheet. It has \
                      no electrical meaning.",
        category: "schematic",
        icon: "line",
        kind: Kind::Edit,
    };
    type Params = LineAddParams;
    type Output = GraphicResult;

    fn run(&self, cx: &mut Context<'_>, p: LineAddParams) -> Result<GraphicResult, CommandError> {
        let shape = SheetShape::Line {
            from: p.from.point(),
            to: p.to.point(),
        };
        add_graphic(cx, p.document, shape)
    }
}

pub struct RectAdd;

impl Command for RectAdd {
    const META: Meta = Meta {
        id: "schematic.graphic.add_rect",
        title: "Add Rectangle",
        description: "Draws a free axis-aligned rectangle between two opposite corners on a \
                      Schematic Sheet. It has no electrical meaning.",
        category: "schematic",
        icon: "rectangle",
        kind: Kind::Edit,
    };
    type Params = RectAddParams;
    type Output = GraphicResult;

    fn run(&self, cx: &mut Context<'_>, p: RectAddParams) -> Result<GraphicResult, CommandError> {
        let shape = SheetShape::Rect {
            from: p.from.point(),
            to: p.to.point(),
            filled: p.filled,
        };
        add_graphic(cx, p.document, shape)
    }
}

fn add_graphic(
    cx: &mut Context<'_>,
    document: Uuid,
    shape: SheetShape,
) -> Result<GraphicResult, CommandError> {
    sheet(cx.project()?, document)?;
    let graphic = SheetGraphic {
        uuid: cx.new_id(),
        shape: check_shape(shape)?,
    };
    let uuid = graphic.uuid;
    cx.tx()?
        .insert(Object::SheetGraphic(InDocument::new(document, graphic)))?;
    Ok(GraphicResult { graphic: uuid })
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GraphicSetParams {
    /// The line or rectangle, by UUID.
    pub uuid: Uuid,
    /// A new start (line) or first corner (rectangle).
    pub from: Option<At>,
    /// A new end (line) or opposite corner (rectangle).
    pub to: Option<At>,
    /// Fill a rectangle, or not. Refused for a line.
    pub filled: Option<bool>,
}

pub struct GraphicSet;

impl Command for GraphicSet {
    const META: Meta = Meta {
        id: "schematic.graphic.set",
        title: "Edit Graphic",
        description: "Changes a free line's ends, or a rectangle's corners or fill; what is \
                      left out stays.",
        category: "schematic",
        icon: "line",
        kind: Kind::Edit,
    };
    type Params = GraphicSetParams;
    type Output = GraphicResult;

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: GraphicSetParams,
    ) -> Result<GraphicResult, CommandError> {
        let Some(Object::SheetGraphic(mut graphic)) = cx.project()?.object(p.uuid) else {
            return Err(CommandError::not_found(format!(
                "no sheet graphic has UUID {}",
                p.uuid
            )));
        };
        let (from, to) = graphic.shape.ends();
        let mut shape = graphic
            .shape
            .with_ends(p.from.map_or(from, At::point), p.to.map_or(to, At::point));
        if let Some(fill) = p.filled {
            match &mut shape {
                SheetShape::Rect { filled, .. } => *filled = fill,
                SheetShape::Line { .. } => {
                    return Err(CommandError::invalid_params(
                        "/filled",
                        "a line cannot be filled",
                    ));
                }
            }
        }
        graphic.object.shape = check_shape(shape)?;
        cx.tx()?.replace(Object::SheetGraphic(graphic))?;
        Ok(GraphicResult { graphic: p.uuid })
    }
}

fn check_shape(shape: SheetShape) -> Result<SheetShape, CommandError> {
    match shape.is_empty() {
        true => Err(CommandError::invalid_params(
            "/to",
            "the shape would be empty: give two distinct points (a rectangle needs both \
             width and height)",
        )),
        false => Ok(shape),
    }
}
