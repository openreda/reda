//! Dimensions: `pcb.dimension.add`, `set`, `delete` and `explode`.
//!
//! Ends that follow a Hotspot are kept on it by [`settle`], which runs after every edit:
//! the Dimension re-measures when its reference moves, and keeps its point without a
//! `ref` when the reference is deleted.

use reda_command::{
    Angle, AvailabilityContext, Command, CommandError, Context, Kind, Length, Meta, Registry,
    Unavailable,
};
use reda_model::geom::{Millideg, Nm, Point};
use reda_model::{
    ArrowPosition, Dimension, DimensionEnd, DimensionFormat, DimensionTextPosition, DimensionUnits,
    GraphicShape, HotspotRef, InDocument, Object, PcbDocument, PcbGraphic, PcbText, Transaction,
    Uuid,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::layer::{LayerRef, drawing_layer};
use super::outline::PointParam;
use super::text::StyleParam;
use super::{Created, needs_pcb, non_negative, pcb, positive};
use crate::dimension::{self, hotspot_point};

pub(super) fn register_all(registry: &mut Registry) {
    registry.add(DimensionAdd);
    registry.add(DimensionSet);
    registry.add(DimensionDelete);
    registry.add(DimensionExplode);
    registry.add_settle(settle);
}

/// Brings every Dimension that follows a Hotspot up to date, after an edit and before it
/// commits: see [`dimension::follow`].
pub fn settle(tx: &mut Transaction<'_>) -> Result<(), CommandError> {
    let Some(pcb) = tx.project().pcb() else {
        return Ok(());
    };
    let document = pcb.uuid;
    for next in dimension::follow(pcb) {
        tx.replace(Object::Dimension(InDocument::new(document, next)))?;
    }
    Ok(())
}

/// The pen width of a new Dimension's lines.
pub const DEFAULT_DIMENSION_WIDTH: Nm = Nm::from_um(150);
/// How far a new Dimension's line sits from its first end.
pub const DEFAULT_DIMENSION_OFFSET: Nm = Nm::from_mm(2);
/// A new Dimension's extension-line gap.
pub const DEFAULT_DIMENSION_GAP: Nm = Nm::from_um(500);
/// A new Dimension's text gap.
pub const DEFAULT_TEXT_GAP: Nm = Nm::from_um(250);
/// A new Dimension's arrowhead side.
pub const DEFAULT_ARROW_SIZE: Nm = Nm::from_mm(1);
/// A new Dimension's outside arrow length.
pub const DEFAULT_ARROW_LENGTH: Nm = Nm::from_mm(2);
/// A new Dimension's digits after the point.
pub const DEFAULT_PRECISION: u8 = 2;
/// The most digits after the point.
pub const MAX_PRECISION: u8 = 6;

/// One end of a Dimension: a point, or a Hotspot to follow.
#[derive(Clone, Copy, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EndParam {
    /// Where the end is in free space. With `ref`, the end is on the Hotspot and this is
    /// left out.
    #[serde(default)]
    pub point: Option<PointParam>,
    /// The Hotspot to follow: `{object, hotspot}`, where `hotspot` is `origin`, `center`,
    /// `start`, `end`, `anchor`, `{"pad": <Pad UUID>}` or `{"vertex": <index>}`.
    #[serde(default, rename = "ref")]
    pub reference: Option<HotspotRef>,
}

impl EndParam {
    fn resolve(self, pcb: &PcbDocument, at: &str) -> Result<DimensionEnd, CommandError> {
        match (self.point, self.reference) {
            (Some(point), None) => Ok(DimensionEnd::free(point.point())),
            (None, Some(reference)) => {
                let point = hotspot_point(pcb, &reference).ok_or_else(|| {
                    CommandError::invalid_params(
                        &format!("{at}/ref"),
                        format!("{} has no such Hotspot on the board", reference.object),
                    )
                })?;
                Ok(DimensionEnd {
                    point,
                    reference: Some(reference),
                })
            }
            (Some(_), Some(_)) => Err(CommandError::invalid_params(
                at,
                "give `point` or `ref`, not both: a followed end sits on its Hotspot",
            )),
            (None, None) => Err(CommandError::invalid_params(
                at,
                "give the end's `point` or a `ref` to follow",
            )),
        }
    }
}

/// Where the text sits: a [`DimensionTextPosition`], with `{"manual": {x, y}}` taking
/// lengths.
#[derive(Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TextPositionParam {
    Automatic,
    Center,
    Top,
    Bottom,
    Left,
    Right,
    InsideLeft,
    InsideRight,
    Unidirectional,
    /// At this anchor, along the dimension line, with the style's `justify`.
    Manual(PointParam),
}

impl TextPositionParam {
    fn resolve(self) -> DimensionTextPosition {
        use DimensionTextPosition as P;
        match self {
            TextPositionParam::Automatic => P::Automatic,
            TextPositionParam::Center => P::Center,
            TextPositionParam::Top => P::Top,
            TextPositionParam::Bottom => P::Bottom,
            TextPositionParam::Left => P::Left,
            TextPositionParam::Right => P::Right,
            TextPositionParam::InsideLeft => P::InsideLeft,
            TextPositionParam::InsideRight => P::InsideRight,
            TextPositionParam::Unidirectional => P::Unidirectional,
            TextPositionParam::Manual(at) => P::Manual(at.point()),
        }
    }
}

/// The Dimension fields `add` and `set` share; each is kept (or defaulted) when left out.
#[derive(Default, Deserialize, JsonSchema)]
pub struct FieldsParam {
    /// The direction measured along, counter-clockwise from +x. A new Dimension measures
    /// along the line from `a` to `b` when it is left out.
    #[serde(default)]
    pub rotation: Option<Angle>,
    /// The pen width of the dimension line and arrows; 0.15 mm by default.
    #[serde(default)]
    pub width: Option<Length>,
    /// The pen width of the extension lines; 0.15 mm by default.
    #[serde(default)]
    pub extension_width: Option<Length>,
    /// How far each extension line stops short of its point and runs past the dimension
    /// line; 0.5 mm by default.
    #[serde(default)]
    pub gap: Option<Length>,
    /// The signed distance of the dimension line from `a`, to the left of `rotation`;
    /// 2 mm by default.
    #[serde(default)]
    pub offset: Option<Length>,
    /// The space between the dimension line and the text; 0.25 mm by default.
    #[serde(default)]
    pub text_gap: Option<Length>,
    /// The length of each arrowhead side; 1 mm by default.
    #[serde(default)]
    pub arrow_size: Option<Length>,
    /// With `outside` arrows, how far the line runs past each extension line; 2 mm by
    /// default.
    #[serde(default)]
    pub arrow_length: Option<Length>,
    /// `inside` (default) or `outside`.
    #[serde(default)]
    pub arrow_position: Option<ArrowPosition>,
    /// `automatic` (default), `center`, `top`, `bottom`, `left`, `right`, `inside_left`,
    /// `inside_right`, `unidirectional` or `{"manual": {x, y}}`.
    #[serde(default)]
    pub text_position: Option<TextPositionParam>,
    /// The text style: Hershey, 1 mm high, 0.15 mm pen by default.
    #[serde(default)]
    pub style: Option<StyleParam>,
    /// `mm`, `mil`, `in`, `cm` or `automatic` (default, millimetres).
    #[serde(default)]
    pub units: Option<DimensionUnits>,
    /// Digits after the decimal point, 0 to 6; 2 by default.
    #[serde(default)]
    pub precision: Option<u8>,
    /// `none`, `value`, `value_unit` (default) or `value_bracketed_unit`.
    #[serde(default)]
    pub format: Option<DimensionFormat>,
    #[serde(default)]
    pub prefix: Option<String>,
    #[serde(default)]
    pub suffix: Option<String>,
}

impl FieldsParam {
    /// Writes the given fields onto `d`, checked.
    fn apply(self, d: &mut Dimension) -> Result<(), CommandError> {
        if let Some(r) = self.rotation {
            d.rotation = r.millideg().normalized();
        }
        let set = |slot: &mut Nm, value: Option<Length>, check: fn(&str, Length) -> _, at| {
            if let Some(value) = value {
                *slot = check(at, value)?;
            }
            Ok::<_, CommandError>(())
        };
        set(&mut d.width, self.width, positive, "/width")?;
        set(
            &mut d.extension_width,
            self.extension_width,
            positive,
            "/extension_width",
        )?;
        set(&mut d.gap, self.gap, non_negative, "/gap")?;
        set(&mut d.text_gap, self.text_gap, non_negative, "/text_gap")?;
        set(&mut d.arrow_size, self.arrow_size, positive, "/arrow_size")?;
        set(
            &mut d.arrow_length,
            self.arrow_length,
            non_negative,
            "/arrow_length",
        )?;
        if let Some(offset) = self.offset {
            d.offset = offset.nm();
        }
        if let Some(p) = self.arrow_position {
            d.arrow_position = p;
        }
        if let Some(p) = self.text_position {
            d.text_position = p.resolve();
        }
        if let Some(style) = &self.style {
            style.apply(&mut d.style)?;
        }
        if let Some(units) = self.units {
            d.units = units;
        }
        if let Some(precision) = self.precision {
            if precision > MAX_PRECISION {
                return Err(CommandError::invalid_params(
                    "/precision",
                    format!("the precision is at most {MAX_PRECISION} digits"),
                ));
            }
            d.precision = precision;
        }
        if let Some(format) = self.format {
            d.format = format;
        }
        if let Some(prefix) = self.prefix {
            CommandError::check_text("/prefix", &prefix)?;
            d.prefix = prefix;
        }
        if let Some(suffix) = self.suffix {
            CommandError::check_text("/suffix", &suffix)?;
            d.suffix = suffix;
        }
        Ok(())
    }
}

/// Refuses a Dimension that measures nothing.
fn check_measures(d: &Dimension) -> Result<(), CommandError> {
    if d.a.point == d.b.point {
        return Err(CommandError::invalid_params(
            "/b",
            "the Dimension's ends coincide",
        ));
    }
    if dimension::value(d) == Nm::ZERO {
        return Err(CommandError::invalid_params(
            "/rotation",
            "the ends are level across the rotation, so it measures nothing",
        ));
    }
    Ok(())
}

/// The angle of the line from `a` to `b`, to the millidegree.
fn angle_of(a: Point, b: Point) -> Millideg {
    let d = b - a;
    let deg = (d.y.0 as f64).atan2(d.x.0 as f64).to_degrees();
    Millideg((deg * 1000.0).round() as i64).normalized()
}

pub struct DimensionAdd;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DimensionAddParams {
    /// The Layer: copper, silkscreen, mechanical or documentation.
    pub layer: LayerRef,
    pub a: EndParam,
    pub b: EndParam,
    #[serde(flatten)]
    pub fields: FieldsParam,
}

impl Command for DimensionAdd {
    const META: Meta = Meta {
        id: "pcb.dimension.add",
        title: "Add Dimension",
        description: "Adds a linear Dimension from `a` to `b` on a copper, silkscreen, \
                      mechanical or documentation Layer, measuring along `rotation` (the \
                      line from `a` to `b` by default). Each end is a free `point` or a \
                      `ref` to a Hotspot it follows: the value is derived and updates when \
                      the referenced object moves.",
        category: "pcb",
        icon: "measure",
        kind: Kind::Edit,
    };
    type Params = DimensionAddParams;
    type Output = Created;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: DimensionAddParams) -> Result<Created, CommandError> {
        let pcb = pcb(cx)?;
        let layer = drawing_layer(pcb, p.layer, "/layer")?.uuid;
        let a = p.a.resolve(pcb, "/a")?;
        let b = p.b.resolve(pcb, "/b")?;
        let document = pcb.uuid;
        let uuid = cx.new_id();
        let mut d = Dimension {
            uuid,
            layer,
            a,
            b,
            rotation: angle_of(a.point, b.point),
            width: DEFAULT_DIMENSION_WIDTH,
            extension_width: DEFAULT_DIMENSION_WIDTH,
            gap: DEFAULT_DIMENSION_GAP,
            offset: DEFAULT_DIMENSION_OFFSET,
            text_gap: DEFAULT_TEXT_GAP,
            arrow_size: DEFAULT_ARROW_SIZE,
            arrow_length: DEFAULT_ARROW_LENGTH,
            arrow_position: ArrowPosition::default(),
            text_position: DimensionTextPosition::default(),
            style: Dimension::default_style(),
            units: DimensionUnits::default(),
            precision: DEFAULT_PRECISION,
            format: DimensionFormat::default(),
            prefix: String::new(),
            suffix: String::new(),
        };
        p.fields.apply(&mut d)?;
        check_measures(&d)?;
        cx.tx()?
            .insert(Object::Dimension(InDocument::new(document, d)))?;
        Ok(Created { uuid })
    }
}

/// The Dimension `uuid` on the board, or `not_found`.
fn find(pcb: &PcbDocument, uuid: Uuid) -> Result<&Dimension, CommandError> {
    pcb.dimension(uuid)
        .ok_or_else(|| CommandError::not_found(format!("no Dimension has UUID {uuid}")))
}

pub struct DimensionSet;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DimensionSetParams {
    /// The Dimension's UUID.
    pub uuid: Uuid,
    /// A new Layer: copper, silkscreen, mechanical or documentation.
    #[serde(default)]
    pub layer: Option<LayerRef>,
    /// A new first end: a free `point` or a `ref` to follow.
    #[serde(default)]
    pub a: Option<EndParam>,
    /// A new second end.
    #[serde(default)]
    pub b: Option<EndParam>,
    #[serde(flatten)]
    pub fields: FieldsParam,
}

impl Command for DimensionSet {
    const META: Meta = Meta {
        id: "pcb.dimension.set",
        title: "Edit Dimension",
        description: "Changes the given fields of one Dimension and keeps the rest. A new \
                      end replaces the old one, with its `ref` or without.",
        category: "pcb",
        icon: "measure",
        kind: Kind::Edit,
    };
    type Params = DimensionSetParams;
    type Output = ();

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: DimensionSetParams) -> Result<(), CommandError> {
        let pcb = pcb(cx)?;
        let mut d = find(pcb, p.uuid)?.clone();
        if let Some(layer) = p.layer {
            d.layer = drawing_layer(pcb, layer, "/layer")?.uuid;
        }
        if let Some(a) = p.a {
            d.a = a.resolve(pcb, "/a")?;
        }
        if let Some(b) = p.b {
            d.b = b.resolve(pcb, "/b")?;
        }
        let document = pcb.uuid;
        p.fields.apply(&mut d)?;
        check_measures(&d)?;
        cx.tx()?
            .replace(Object::Dimension(InDocument::new(document, d)))?;
        Ok(())
    }
}

pub struct DimensionDelete;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DimensionUuidParams {
    /// The Dimension's UUID.
    pub uuid: Uuid,
}

impl Command for DimensionDelete {
    const META: Meta = Meta {
        id: "pcb.dimension.delete",
        title: "Delete Dimension",
        description: "Deletes one Dimension. Its referenced objects stay.",
        category: "pcb",
        icon: "delete",
        kind: Kind::Edit,
    };
    type Params = DimensionUuidParams;
    type Output = ();

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: DimensionUuidParams) -> Result<(), CommandError> {
        find(pcb(cx)?, p.uuid)?;
        cx.tx()?.remove(p.uuid)?;
        Ok(())
    }
}

pub struct DimensionExplode;

/// What `pcb.dimension.explode` made.
#[derive(Debug, Serialize, JsonSchema)]
pub struct Exploded {
    /// The new free lines, in drawing order: extension lines, arrowheads, dimension line.
    pub graphics: Vec<Uuid>,
    /// The new free text; `None` when the Dimension showed none.
    pub text: Option<Uuid>,
}

impl Command for DimensionExplode {
    const META: Meta = Meta {
        id: "pcb.dimension.explode",
        title: "Explode Dimension",
        description: "Replaces one Dimension with the free lines and text it is drawn as, \
                      on its Layer. They no longer follow anything; only undo brings the \
                      Dimension back.",
        category: "pcb",
        icon: "split",
        kind: Kind::Edit,
    };
    type Params = DimensionUuidParams;
    type Output = Exploded;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: DimensionUuidParams) -> Result<Exploded, CommandError> {
        let pcb = pcb(cx)?;
        let d = find(pcb, p.uuid)?;
        let document = pcb.uuid;
        let layer = d.layer;
        let drawn = dimension::drawing(d, cx.fonts());
        let mut objects = Vec::new();
        let mut graphics = Vec::new();
        for line in drawn.lines {
            let uuid = cx.new_id();
            graphics.push(uuid);
            objects.push(Object::PcbGraphic(InDocument::new(
                document,
                PcbGraphic {
                    uuid,
                    layer,
                    width: line.width,
                    shape: GraphicShape::Line {
                        from: line.from,
                        to: line.to,
                    },
                },
            )));
        }
        let text = drawn.text.map(|t| {
            let uuid = cx.new_id();
            objects.push(Object::PcbText(InDocument::new(
                document,
                PcbText { uuid, ..t },
            )));
            uuid
        });
        let tx = cx.tx()?;
        tx.remove(p.uuid)?;
        for object in objects {
            tx.insert(object)?;
        }
        Ok(Exploded { graphics, text })
    }
}
