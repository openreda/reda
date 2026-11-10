//! Footprint instances: the Footprints placed on the board, each with its own copy of its
//! Component's data and a Net per Pad.

use reda_geom::{Millideg, Point};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::Side;
use crate::{ComponentRef, ComponentType, Fields, TextStyle, Uuid};

/// One Footprint placed on the board.
///
/// A Footprint instance synced from the schematic is linked to its Component by
/// [`FootprintInstance::component`] and carries its own copy of the Component's
/// Designator, fields, Component Type, In BOM, In CPL and Not Fitted, so outputs read only
/// the PCB Document. A Mechanical
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
    /// Its Footprint's graphics can be edited on the board, each edit making a Locally
    /// Edited Footprint (ADR-0013).
    #[serde(default, skip_serializing_if = "is_false")]
    pub graphics_unlocked: bool,
    pub designator: String,
    /// The value is `fields.value`.
    pub fields: Fields,
    pub component_type: ComponentType,
    /// Whether it goes into the BOM.
    pub in_bom: bool,
    /// Whether it goes into the CPL.
    pub in_cpl: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub not_fitted: bool,
    /// Sparse placement/style overrides, sorted by the UUID of the Footprint text entry.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub text_overrides: Vec<TextOverride>,
    /// One entry per Pad of the Footprint copy, sorted by Pad UUID.
    pub pads: Vec<PadNet>,
}

impl FootprintInstance {
    /// The Net of Pad `pad`, if it has one.
    pub fn pad_net(&self, pad: Uuid) -> Option<Uuid> {
        self.pads.iter().find(|p| p.pad == pad).and_then(|p| p.net)
    }

    /// The override of Footprint text entry `text`, if this instance has one.
    pub fn text_override(&self, text: Uuid) -> Option<&TextOverride> {
        self.text_overrides.iter().find(|o| o.text == text)
    }
}

/// One field text on one placed Footprint, independent of other instances of its copy.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(deny_unknown_fields)]
pub struct FootprintTextRef {
    pub footprint: Uuid,
    pub text: Uuid,
}

/// An instance changes only the properties present here; the Footprint supplies the rest.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TextOverride {
    pub text: Uuid,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<Point>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotation: Option<Millideg>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<TextStyle>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visible: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub autoposition: Option<TextAutoposition>,
}

impl TextOverride {
    pub fn new(text: Uuid) -> Self {
        Self {
            text,
            at: None,
            rotation: None,
            style: None,
            visible: None,
            autoposition: None,
        }
    }
    pub fn is_empty(&self) -> bool {
        self.at.is_none()
            && self.rotation.is_none()
            && self.style.is_none()
            && self.visible.is_none()
            && self.autoposition.is_none()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TextAutoposition {
    #[default]
    Manual,
    LeftAbove,
    CenterAbove,
    RightAbove,
    LeftCenter,
    Center,
    RightCenter,
    LeftBelow,
    CenterBelow,
    RightBelow,
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

/// A Pad of a Footprint instance on the PCB. Such a Pad has no UUID of its own: it is
/// named by its instance and the Pad of the instance's Footprint copy.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(deny_unknown_fields)]
pub struct PadRef {
    /// The Footprint instance.
    pub footprint: Uuid,
    /// The Pad of its Footprint copy.
    pub pad: Uuid,
}

fn is_false(b: &bool) -> bool {
    !*b
}
