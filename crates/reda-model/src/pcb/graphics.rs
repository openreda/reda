//! Free text and graphics drawn on a PCB Layer: text, lines, arcs and rectangles.

use reda_geom::{Millideg, Nm, Point};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{TextFrame, TextStyle, Uuid};

/// The family of the vendored Hershey stroke font, the default for PCB text.
pub const HERSHEY_FONT: &str = "Hershey Roman Simplex";

/// The family of the bundled outline font, which also stands in for missing fonts.
pub const NOTO_SANS_FONT: &str = "Noto Sans";

/// Free text on a PCB Layer.
///
/// `position` is its justification anchor. Mirroring and rotation apply about it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PcbText {
    pub uuid: Uuid,
    /// The Layer it is drawn on.
    pub layer: Uuid,
    /// The text; `\n` starts a new line.
    pub text: String,
    /// The anchor of the text block or Text Frame.
    pub position: Point,
    /// The same style as a Footprint field text, flattened in the file.
    #[serde(flatten)]
    pub style: TextStyle,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frame: Option<TextFrame>,
    /// Counter-clockwise about `position`; left out when 0.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub rotation: Millideg,
    /// Drawn mirrored, so it reads from below; only on bottom Layers. Left out when false.
    #[serde(default, skip_serializing_if = "is_false")]
    pub mirrored: bool,
}

impl std::ops::Deref for PcbText {
    type Target = TextStyle;
    fn deref(&self) -> &TextStyle {
        &self.style
    }
}

impl std::ops::DerefMut for PcbText {
    fn deref_mut(&mut self) -> &mut TextStyle {
        &mut self.style
    }
}

/// A line, arc or rectangle drawn on a PCB Layer with a round pen.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PcbGraphic {
    pub uuid: Uuid,
    /// The Layer it is drawn on.
    pub layer: Uuid,
    /// The pen width.
    pub width: Nm,
    pub shape: GraphicShape,
}

/// The shape of a [`PcbGraphic`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum GraphicShape {
    /// A straight line.
    Line { from: Point, to: Point },
    /// A circular arc from `from` to `to`, sweeping `sweep` (counter-clockwise when
    /// positive).
    Arc {
        from: Point,
        to: Point,
        sweep: Millideg,
    },
    /// An axis-aligned rectangle with opposite corners `from` and `to`, outlined, or
    /// filled as well when `filled`.
    Rect {
        from: Point,
        to: Point,
        #[serde(default, skip_serializing_if = "is_false")]
        filled: bool,
    },
}

fn is_zero(angle: &Millideg) -> bool {
    *angle == Millideg::ZERO
}

fn is_false(value: &bool) -> bool {
    !*value
}
