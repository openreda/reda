//! Library elements: Symbols, Footprints and Parts (ADR-0008), and the Library that holds
//! them.
//!
//! These are the shapes REDA stores in a Library folder and copies into a Project when a
//! Component is placed. Elements reference each other by UUID, across Libraries too, so a
//! Part names its Symbol and Footprints by UUID rather than by Library.

use reda_geom::{Millideg, Nm, Point};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::Uuid;

/// A Library's own description, stored in `<name>.redalib` at the root of its folder.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LibraryInfo {
    pub uuid: Uuid,
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
}

/// The schematic representation of a part: its Units, each with its Pins.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Symbol {
    pub uuid: Uuid,
    pub name: String,
    pub description: String,
    /// The Designator prefix of Components placed from it, such as `R` or `U`.
    pub designator_prefix: String,
    /// In order: A, B, C… A single-unit Symbol has exactly one.
    pub units: Vec<Unit>,
    /// Every Pin of every Unit, sorted by UUID; [`Pin::unit`] says which Unit it is on.
    pub pins: Vec<Pin>,
}

impl Symbol {
    /// The Pins of one Unit, sorted by UUID.
    pub fn unit_pins(&self, unit: Uuid) -> impl Iterator<Item = &Pin> {
        self.pins.iter().filter(move |p| p.unit == unit)
    }
}

/// One independently placeable section of a Symbol.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Unit {
    pub uuid: Uuid,
    /// Such as `A`, or `Power` for a power section.
    pub name: String,
}

/// A connection point of a Symbol.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Pin {
    pub uuid: Uuid,
    /// The Unit it belongs to.
    pub unit: Uuid,
    pub name: String,
    /// Unique across the Symbol; Pins map to the Pads with the same number unless the
    /// Part overrides it.
    pub number: String,
    pub electrical_type: PinType,
    /// The connection end of the Pin, relative to the Symbol origin.
    pub at: Point,
    /// The direction from the connection end towards the body, counter-clockwise from +x.
    pub rotation: Millideg,
    pub length: Nm,
}

/// A Pin's electrical type, which ERC checks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PinType {
    Input,
    Output,
    Bidirectional,
    TriState,
    Passive,
    PowerIn,
    PowerOut,
    OpenCollector,
    OpenEmitter,
    NotConnected,
}

/// The physical land pattern of a part.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Footprint {
    pub uuid: Uuid,
    pub name: String,
    pub description: String,
    /// Sorted by UUID. Numbers may repeat: Pads that share a number belong to one Pin.
    pub pads: Vec<Pad>,
}

/// A copper area, with or without a hole, of a Footprint.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Pad {
    pub uuid: Uuid,
    pub number: String,
    pub kind: PadKind,
    pub shape: PadShape,
    /// The Pad centre, relative to the Footprint origin.
    pub at: Point,
    pub rotation: Millideg,
    pub width: Nm,
    pub height: Nm,
    /// A round hole when width equals height, otherwise a slot. SMD Pads have none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hole: Option<Hole>,
    /// Overrides the board's solder-mask margin for this Pad.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mask_margin: Option<Nm>,
    /// Overrides the board's solder-paste margin for this Pad.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paste_margin: Option<Nm>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PadKind {
    Smd,
    PlatedThroughHole,
    NonPlated,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PadShape {
    Circle,
    Rect,
    RoundedRect,
    Oval,
}

/// A drilled hole: round when `width == height`, otherwise a slot along the longer side.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Hole {
    pub width: Nm,
    pub height: Nm,
}

/// A Library definition that binds a Symbol to its alternate Footprints, with default
/// fields and a Component Type.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Part {
    pub uuid: Uuid,
    pub name: String,
    pub component_type: ComponentType,
    /// Mechanical Parts may have none; Standard Parts need one.
    pub symbol: Option<Uuid>,
    /// The alternate Footprints, in order; the first is the default.
    pub footprints: Vec<Uuid>,
    pub fields: Fields,
    pub pin_map: PinMap,
}

/// Whether a part's Components go into the BOM and may exist only on the board.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ComponentType {
    Standard,
    Mechanical,
}

/// A Part's default fields, copied onto each Component placed from it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Fields {
    #[serde(skip_serializing_if = "String::is_empty")]
    pub value: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub datasheet: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub manufacturer: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub mpn: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub keywords: String,
    /// Free-form fields, such as `LCSC` or `DigiKey`, in the user's order.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub custom: Vec<Field>,
}

impl Fields {
    /// Every non-empty field value, built-in fields first.
    pub fn values(&self) -> impl Iterator<Item = &str> {
        [
            &self.value,
            &self.description,
            &self.datasheet,
            &self.manufacturer,
            &self.mpn,
            &self.keywords,
        ]
        .into_iter()
        .map(String::as_str)
        .chain(self.custom.iter().map(|f| f.value.as_str()))
        .filter(|v| !v.is_empty())
    }
}

/// One free-form field.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Field {
    pub name: String,
    pub value: String,
}

/// The Part's exceptions to the implicit map, where a Pin connects to the Pads with its
/// own number.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct PinMap {
    /// Pins mapped to other Pads, or to several, sorted by Pin number.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub pins: Vec<PinPads>,
    /// Pad numbers deliberately left unconnected, sorted.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub unconnected: Vec<String>,
}

impl PinMap {
    pub fn is_empty(&self) -> bool {
        self.pins.is_empty() && self.unconnected.is_empty()
    }
}

/// One Pin mapped to an explicit set of Pads.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PinPads {
    pub pin: String,
    pub pads: Vec<String>,
}
