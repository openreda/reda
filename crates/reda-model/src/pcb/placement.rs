//! Placement objects that are not Footprints: Free Pads and the Output Origin.

use reda_geom::{CornerRect, Millideg, Nm, Point};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::Side;
use crate::{Corners, Hole, PadForm, PadKind, PadShape, Ratio, Uuid, ZoneConnection};

/// A Pad placed directly on the board, belonging to no Footprint: a mounting hole, a
/// fiducial or a test point. It has the whole Pad model, in board coordinates.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FreePad {
    pub uuid: Uuid,
    /// SMD (plated, no hole), plated through-hole, or non-plated (a bare hole).
    pub kind: PadKind,
    pub shape: PadShape,
    /// The Pad's anchor: the hole sits there, Tracks connect there and it turns about it.
    pub at: Point,
    /// Counter-clockwise.
    pub rotation: Millideg,
    pub width: Nm,
    pub height: Nm,
    /// How round the corners of a `rounded_rect`, or the uncut corners of a
    /// `chamfered_rect`, are.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub corner_ratio: Option<Ratio>,
    /// How deep the chamfers of a `chamfered_rect` are.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chamfer_ratio: Option<Ratio>,
    /// Which corners are rounded or chamfered, in the Pad's frame. Left out, all four.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub corners: Option<Corners>,
    /// Where the copper sits relative to `at`, in the Pad's frame. Left out, on it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offset: Option<Point>,
    /// The drilled hole; SMD Pads have none, the others must.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hole: Option<Hole>,
    /// The board side of an SMD Pad. Left out, the top. Pads with a hole have none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub side: Option<Side>,
    /// Its PCB Net; left out for a Pad on no Net. Non-plated Pads have no copper and no
    /// Net.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub net: Option<Uuid>,
    /// How Zone fills of its Net join it. Each value left out inherits.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub zone_connection: Option<ZoneConnection>,
}

impl FreePad {
    /// Its shape fields, for the geometry and the checks every Pad shares.
    pub fn form(&self) -> PadForm {
        PadForm {
            kind: self.kind,
            shape: self.shape,
            width: self.width,
            height: self.height,
            corner_ratio: self.corner_ratio,
            chamfer_ratio: self.chamfer_ratio,
            corners: self.corners,
            offset: self.offset,
            hole: self.hole,
            side: self.side,
            zone_connection: self.zone_connection,
        }
    }

    /// Its copper outline in its own frame, the anchor at the origin.
    pub fn outline(&self) -> CornerRect {
        self.form().outline()
    }

    /// What is wrong with this Pad, if anything: a JSON pointer to the field and a message.
    pub fn problem(&self) -> Option<(String, String)> {
        if let Some(problem) = self.form().problem() {
            return Some(problem);
        }
        if self.kind == PadKind::NonPlated && self.net.is_some() {
            return Some((
                "/net".into(),
                "a non-plated Pad has no copper and cannot be on a Net".into(),
            ));
        }
        None
    }
}

/// The point the editor measures coordinates and grid from, and manufacturing files use
/// when a Fab Preset requests a relative origin. A PCB Document has at most one.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OutputOrigin {
    pub uuid: Uuid,
    pub at: Point,
}
