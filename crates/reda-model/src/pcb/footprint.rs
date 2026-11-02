//! Footprint instances: the Footprints placed on the board, each with its own copy of its
//! Component's data and a Net per Pad.

use reda_geom::{Millideg, Point};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::Side;
use crate::{ComponentRef, Fields, FootprintText, Uuid};

/// One Footprint placed on the board.
///
/// A Footprint instance synced from the schematic is linked to its Component by
/// [`FootprintInstance::component`] and carries its own copy of the Component's
/// Designator, fields and Not Fitted, so outputs read only the PCB Document. A Mechanical
/// Part placed directly on the PCB has no Component link.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FootprintInstance {
    pub uuid: Uuid,
    /// The Component it shows (Sheet Instance path + Component UUID); left out for a
    /// Mechanical Part that exists only on the PCB.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub component: Option<ComponentRef>,
    /// The PCB Document's Footprint copy it is placed from.
    pub footprint: Uuid,
    /// Where the Footprint origin sits on the board.
    pub at: Point,
    /// Counter-clockwise, applied after flipping to the bottom side.
    pub rotation: Millideg,
    pub side: Side,
    /// Locked against moving.
    #[serde(default, skip_serializing_if = "is_false")]
    pub locked: bool,
    pub designator: String,
    /// The value is `fields.value`.
    pub fields: Fields,
    #[serde(default, skip_serializing_if = "is_false")]
    pub not_fitted: bool,
    /// Where this instance draws its Designator text, when moved from the Footprint's own
    /// placement.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub designator_text: Option<FootprintText>,
    /// One entry per Pad of the Footprint copy, sorted by Pad UUID.
    pub pads: Vec<PadNet>,
}

impl FootprintInstance {
    /// The Net of Pad `pad`, if it has one.
    pub fn pad_net(&self, pad: Uuid) -> Option<Uuid> {
        self.pads.iter().find(|p| p.pad == pad).and_then(|p| p.net)
    }
}

/// The Net a Pad of a Footprint instance is on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PadNet {
    /// The Pad of the Footprint copy.
    pub pad: Uuid,
    /// Its PCB Net; left out for a Pad on no Net.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub net: Option<Uuid>,
}

fn is_false(b: &bool) -> bool {
    !*b
}
