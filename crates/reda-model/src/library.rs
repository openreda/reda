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

/// The schematic representation of a part: its Units, each with its Pins and graphics.
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
    /// The body drawing of every Unit, sorted by UUID; [`SymbolGraphic::unit`] says which
    /// Unit it belongs to.
    pub graphics: Vec<SymbolGraphic>,
    /// Where an imported Symbol came from. Elements made in REDA have none.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provenance: Option<Box<Provenance>>,
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
    /// Whether the Pin's name is drawn.
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub name_visible: bool,
    /// Whether the Pin's number is drawn.
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub number_visible: bool,
}

fn yes() -> bool {
    true
}

fn is_true(b: &bool) -> bool {
    *b
}

fn is_false(b: &bool) -> bool {
    !*b
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

/// The physical land pattern of a part. Coordinates are relative to its origin, and its
/// zero orientation follows IPC-7351.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Footprint {
    pub uuid: Uuid,
    pub name: String,
    pub description: String,
    /// Sorted by UUID. Numbers may repeat: Pads that share a number belong to one Pin.
    pub pads: Vec<Pad>,
    /// Silkscreen, Courtyard and Assembly drawing, sorted by UUID.
    pub graphics: Vec<FootprintGraphic>,
    /// Where the Designator text goes.
    pub designator: FootprintText,
    /// Where the value text goes.
    pub value: FootprintText,
    /// Where an imported Footprint came from. Elements made in REDA have none.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provenance: Option<Box<Provenance>>,
}

/// The Layers a Footprint draws on besides copper.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FootprintLayer {
    SilkscreenTop,
    SilkscreenBottom,
    CourtyardTop,
    CourtyardBottom,
    AssemblyTop,
    AssemblyBottom,
}

/// The placement of a Footprint's Designator or value text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FootprintText {
    /// The text centre, relative to the Footprint origin.
    pub at: Point,
    pub rotation: Millideg,
    /// The text height.
    pub size: Nm,
    pub layer: FootprintLayer,
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub visible: bool,
}

impl FootprintText {
    /// The Designator text of a new Footprint: 1 mm high on the top Silkscreen, at the
    /// origin.
    pub const DESIGNATOR: FootprintText = FootprintText {
        at: Point::new(Nm(0), Nm(0)),
        rotation: Millideg(0),
        size: Nm::from_mm(1),
        layer: FootprintLayer::SilkscreenTop,
        visible: true,
    };

    /// The value text of a new Footprint: 1 mm high on the top Assembly Layer, at the
    /// origin.
    pub const VALUE: FootprintText = FootprintText {
        at: Point::new(Nm(0), Nm(0)),
        rotation: Millideg(0),
        size: Nm::from_mm(1),
        layer: FootprintLayer::AssemblyTop,
        visible: true,
    };
}

/// A drawn shape. Lines, arcs and outlines are stroked with the graphic's width.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Shape {
    Line {
        start: Point,
        end: Point,
    },
    /// An axis-aligned rectangle between two opposite corners.
    Rect {
        start: Point,
        end: Point,
    },
    /// The arc from `start` around `center`, sweeping `sweep` counter-clockwise (negative
    /// is clockwise).
    Arc {
        center: Point,
        start: Point,
        sweep: Millideg,
    },
    Circle {
        center: Point,
        radius: Nm,
    },
    /// A closed polygon through its corners.
    Polygon {
        points: Vec<Point>,
    },
    Text {
        /// The text's anchor.
        at: Point,
        text: String,
        /// The text height.
        size: Nm,
        rotation: Millideg,
    },
}

impl Shape {
    /// The shape moved by `by`.
    pub fn translated(&self, by: Point) -> Shape {
        let mut shape = self.clone();
        match &mut shape {
            Shape::Line { start, end } | Shape::Rect { start, end } => {
                *start = *start + by;
                *end = *end + by;
            }
            Shape::Arc { center, start, .. } => {
                *center = *center + by;
                *start = *start + by;
            }
            Shape::Circle { center, .. } => *center = *center + by,
            Shape::Polygon { points } => points.iter_mut().for_each(|p| *p = *p + by),
            Shape::Text { at, .. } => *at = *at + by,
        }
        shape
    }
}

/// One shape of a Symbol Unit's body.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SymbolGraphic {
    pub uuid: Uuid,
    /// The Unit it is drawn on.
    pub unit: Uuid,
    pub shape: Shape,
    /// The stroke width.
    pub width: Nm,
    /// Whether a closed shape is filled.
    #[serde(default, skip_serializing_if = "is_false")]
    pub fill: bool,
}

/// One shape a Footprint draws on a non-copper Layer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FootprintGraphic {
    pub uuid: Uuid,
    pub layer: FootprintLayer,
    pub shape: Shape,
    /// The stroke width.
    pub width: Nm,
    /// Whether a closed shape is filled.
    #[serde(default, skip_serializing_if = "is_false")]
    pub fill: bool,
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
    /// The corner radius of a rounded-rectangle Pad.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub corner_radius: Option<Nm>,
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
    /// Where an imported Part came from. Elements made in REDA have none.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provenance: Option<Box<Provenance>>,
}

/// Where an imported Library element came from, kept with the element so its terms travel
/// with it: vendor parts stay under the vendor's terms after import.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Provenance {
    /// The service the files came from, such as `SnapMagic`, or `Unknown`.
    pub service: String,
    /// The service's terms of use, when known.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub terms_url: String,
    /// Copyright lines found in the imported files, kept verbatim.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub copyright: Vec<String>,
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
    /// The names of the built-in fields, as stored.
    pub const BUILT_IN: [&'static str; 6] = [
        "value",
        "description",
        "datasheet",
        "manufacturer",
        "mpn",
        "keywords",
    ];

    fn built_in(&self, name: &str) -> Option<&String> {
        Some(match name {
            "value" => &self.value,
            "description" => &self.description,
            "datasheet" => &self.datasheet,
            "manufacturer" => &self.manufacturer,
            "mpn" => &self.mpn,
            "keywords" => &self.keywords,
            _ => return None,
        })
    }

    fn built_in_mut(&mut self, name: &str) -> Option<&mut String> {
        Some(match name {
            "value" => &mut self.value,
            "description" => &mut self.description,
            "datasheet" => &mut self.datasheet,
            "manufacturer" => &mut self.manufacturer,
            "mpn" => &mut self.mpn,
            "keywords" => &mut self.keywords,
            _ => return None,
        })
    }

    /// The field called `name`: a built-in one by its stored name (see
    /// [`Fields::BUILT_IN`]), otherwise a free-form one. Empty when it is not set.
    pub fn get(&self, name: &str) -> &str {
        match self.built_in(name) {
            Some(value) => value,
            None => self
                .custom
                .iter()
                .find(|f| f.name == name)
                .map_or("", |f| f.value.as_str()),
        }
    }

    /// Sets the field called `name`. An empty value clears a built-in field and removes a
    /// free-form one; a changed free-form field keeps its place, and a new one goes last.
    pub fn set(&mut self, name: &str, value: &str) {
        if let Some(field) = self.built_in_mut(name) {
            value.clone_into(field);
            return;
        }
        match self.custom.iter().position(|f| f.name == name) {
            Some(i) if value.is_empty() => {
                self.custom.remove(i);
            }
            Some(i) => value.clone_into(&mut self.custom[i].value),
            None if value.is_empty() => {}
            None => self.custom.push(Field {
                name: name.to_owned(),
                value: value.to_owned(),
            }),
        }
    }

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

    /// The Pad numbers the Pin numbered `pin` reaches: its override's, or else its own.
    pub fn pads_of<'a>(&'a self, pin: &'a str) -> Vec<&'a str> {
        match self.pins.iter().find(|o| o.pin == pin) {
            Some(over) => over.pads.iter().map(String::as_str).collect(),
            None => vec![pin],
        }
    }
}

/// One Pin mapped to an explicit set of Pads.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PinPads {
    pub pin: String,
    pub pads: Vec<String>,
}

/// One Library element of any kind.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LibraryElement {
    Symbol(Symbol),
    Footprint(Footprint),
    Part(Part),
}

impl LibraryElement {
    pub fn uuid(&self) -> Uuid {
        match self {
            LibraryElement::Symbol(s) => s.uuid,
            LibraryElement::Footprint(f) => f.uuid,
            LibraryElement::Part(p) => p.uuid,
        }
    }

    pub fn name(&self) -> &str {
        match self {
            LibraryElement::Symbol(s) => &s.name,
            LibraryElement::Footprint(f) => &f.name,
            LibraryElement::Part(p) => &p.name,
        }
    }
}

/// Where an open Library element came from: its Library, and whether that Library may be
/// changed (the starter Library may not).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ElementSource {
    pub library: Uuid,
    pub read_only: bool,
}
