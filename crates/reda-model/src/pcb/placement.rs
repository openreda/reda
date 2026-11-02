//! Placement objects that are not Footprints: Free Pads and the Placement Origin.

use reda_geom::{Millideg, Nm, Point};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::Side;
use crate::{Hole, PadKind, PadShape, Uuid};

/// A Pad placed directly on the board, belonging to no Footprint: a mounting hole, a
/// fiducial or a test point.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FreePad {
    pub uuid: Uuid,
    /// SMD (plated, no hole), plated through-hole, or non-plated (a bare hole).
    pub kind: PadKind,
    pub shape: PadShape,
    /// The Pad centre.
    pub at: Point,
    /// Counter-clockwise.
    pub rotation: Millideg,
    pub width: Nm,
    pub height: Nm,
    /// The corner radius of a rounded-rectangle Pad.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub corner_radius: Option<Nm>,
    /// The drilled hole; SMD Pads have none, the others must.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hole: Option<Hole>,
    /// The copper side of an SMD Pad; left out for Pads with a hole.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub side: Option<Side>,
    /// Its PCB Net; left out for a Pad on no Net. Non-plated Pads have no copper and no
    /// Net.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub net: Option<Uuid>,
}

impl FreePad {
    /// What is wrong with this Pad's shape, if anything.
    pub fn problem(&self) -> Option<String> {
        if self.width <= Nm::ZERO || self.height <= Nm::ZERO {
            return Some("the Pad size must be positive".into());
        }
        match (self.kind, self.hole, self.side) {
            (PadKind::Smd, Some(_), _) => Some("an SMD Pad has no hole".into()),
            (PadKind::Smd, None, None) => Some("an SMD Pad needs a side".into()),
            (PadKind::Smd, None, Some(_)) => None,
            (_, None, _) => Some("a plated through-hole or non-plated Pad needs a hole".into()),
            (_, Some(_), Some(_)) => Some("a Pad with a hole is on both sides".into()),
            (kind, Some(hole), None) => {
                if hole.width <= Nm::ZERO || hole.height <= Nm::ZERO {
                    Some("the hole size must be positive".into())
                } else if hole.width > self.width || hole.height > self.height {
                    Some("the hole is larger than the Pad".into())
                } else if kind == PadKind::NonPlated && self.net.is_some() {
                    Some("a non-plated Pad has no copper and cannot be on a Net".into())
                } else {
                    None
                }
            }
        }
    }
}

/// The point from which Footprint positions are measured in the manufacturing outputs
/// (CPL). A PCB Document has at most one; without it the outputs measure from the
/// lower-left corner of the Board Outline's bounding box.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlacementOrigin {
    pub uuid: Uuid,
    pub at: Point,
}
