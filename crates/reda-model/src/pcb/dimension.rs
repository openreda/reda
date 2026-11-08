//! Dimensions: a measured distance drawn on a Layer that takes graphics. v0.1 has one
//! linear type, at any angle.
//!
//! The value is derived, never stored: each end holds a `point`, kept current while it
//! follows a `ref` to a Hotspot of another object, so the Dimension survives losing its
//! reference. `reda-pcb` lays a Dimension out as lines and text.

use reda_geom::{Millideg, Nm, Point};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{TextStyle, Uuid};

/// A linear Dimension: the distance from `a` to `b` measured along `rotation`.
///
/// The dimension line runs along `rotation`, `offset` to the left of `a` (seen looking
/// along `rotation`). An extension line runs from `gap` off each end's point to `gap`
/// beyond the dimension line.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Dimension {
    pub uuid: Uuid,
    /// The Layer it is drawn on.
    pub layer: Uuid,
    /// The first measured end.
    pub a: DimensionEnd,
    /// The second measured end.
    pub b: DimensionEnd,
    /// The direction measured along, counter-clockwise from +x.
    pub rotation: Millideg,
    /// The pen width of the dimension line and arrows.
    pub width: Nm,
    /// The pen width of the extension lines.
    pub extension_width: Nm,
    /// How far each extension line stops short of its point, and runs past the
    /// dimension line.
    pub gap: Nm,
    /// The signed distance of the dimension line from `a`, to the left of `rotation`.
    pub offset: Nm,
    /// The space between the dimension line and the text.
    pub text_gap: Nm,
    /// The length of each side of an arrowhead.
    pub arrow_size: Nm,
    /// With `outside` arrows, how far the dimension line runs past each extension line.
    pub arrow_length: Nm,
    pub arrow_position: ArrowPosition,
    pub text_position: DimensionTextPosition,
    /// The text style, flattened in the file. `justify` applies only to `manual` text;
    /// the other positions set their own.
    #[serde(flatten)]
    pub style: TextStyle,
    pub units: DimensionUnits,
    /// Digits after the decimal point.
    pub precision: u8,
    pub format: DimensionFormat,
    /// Text before the value; left out when empty.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub prefix: String,
    /// Text after the value; left out when empty.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub suffix: String,
}

impl std::ops::Deref for Dimension {
    type Target = TextStyle;
    fn deref(&self) -> &TextStyle {
        &self.style
    }
}

impl std::ops::DerefMut for Dimension {
    fn deref_mut(&mut self) -> &mut TextStyle {
        &mut self.style
    }
}

impl Dimension {
    /// The objects its ends follow, each once.
    pub fn references(&self) -> Vec<Uuid> {
        let mut out: Vec<Uuid> = [&self.a, &self.b]
            .iter()
            .filter_map(|end| end.reference.map(|r| r.object))
            .collect();
        out.dedup();
        out
    }

    /// Centred Hershey text 1 mm high, the style of a new Dimension.
    pub fn default_style() -> TextStyle {
        TextStyle::of_height(Nm::from_mm(1))
    }
}

/// One measured end of a [`Dimension`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DimensionEnd {
    /// Where the end is. While it has a `ref`, this is kept on the Hotspot.
    pub point: Point,
    /// The Hotspot it follows; left out for an end in free space.
    #[serde(default, rename = "ref", skip_serializing_if = "Option::is_none")]
    pub reference: Option<HotspotRef>,
}

impl DimensionEnd {
    /// An end in free space.
    pub fn free(point: Point) -> DimensionEnd {
        DimensionEnd {
            point,
            reference: None,
        }
    }
}

/// A Hotspot of one object on the board.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HotspotRef {
    /// The object: a Footprint, Free Pad, Via, Track, graphic, text, Board Outline, Board
    /// Cutout, Zone or Keepout.
    pub object: Uuid,
    pub hotspot: Hotspot,
}

/// Which point of an object a [`HotspotRef`] names.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Hotspot {
    /// A Footprint's origin.
    Origin,
    /// The centre of a Free Pad or Via, or of an arc Track or arc graphic.
    Center,
    /// The start of a Track or line or arc graphic.
    Start,
    /// The end of a Track or line or arc graphic.
    End,
    /// A free text's anchor.
    Anchor,
    /// The centre of the Pad with this UUID in a Footprint's copy.
    Pad(Uuid),
    /// A vertex, counted from 0, of an outline, cutout, Zone, Keepout or rectangle graphic
    /// (corners `from`, (`to.x`, `from.y`), `to`, (`from.x`, `to.y`)).
    Vertex(usize),
}

/// Where a Dimension's arrows sit.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ArrowPosition {
    /// Between the extension lines, pointing out at them.
    #[default]
    Inside,
    /// Outside the extension lines, pointing in at them.
    Outside,
}

/// Where a Dimension's text sits.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DimensionTextPosition {
    /// `top` when the text fits between the extension lines, else `right`.
    #[default]
    Automatic,
    /// Centred on the dimension line.
    Center,
    /// Centred, above the dimension line as the text reads.
    Top,
    /// Centred, below the dimension line as the text reads.
    Bottom,
    /// On the dimension line, outside the left extension line as the text reads.
    Left,
    /// On the dimension line, outside the right extension line as the text reads.
    Right,
    /// Above the line, inside against the left extension line.
    InsideLeft,
    /// Above the line, inside against the right extension line.
    InsideRight,
    /// Centred above the line and always horizontal, whatever the Dimension's angle.
    Unidirectional,
    /// At this anchor, along the dimension line, with the style's `justify`.
    Manual(Point),
}

/// The units a Dimension's value is shown in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DimensionUnits {
    Mm,
    Mil,
    In,
    Cm,
    /// Millimetres: a board stores no display units in v0.1.
    #[default]
    Automatic,
}

/// How a Dimension's value and unit are written.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DimensionFormat {
    /// No value: only the prefix and suffix.
    None,
    /// `12.70`
    Value,
    /// `12.70mm`
    #[default]
    ValueUnit,
    /// `12.70 (mm)`
    ValueBracketedUnit,
}
