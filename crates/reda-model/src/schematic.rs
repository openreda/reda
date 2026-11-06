//! The objects on a Schematic Sheet: its frame, its part table, its placed Units and the
//! drawing that connects them (Wires, Net Labels and No-Connect Markers).
//!
//! Nets and Junctions are never stored: `reda-schematic` derives them from the drawing.
//!
//! Each object is stored as an [`InDocument`] row, which names the sheet it lives on, so
//! its scope follows from the object alone; the sheet's file leaves that field out.

use std::collections::{BTreeMap, BTreeSet};

use reda_geom::{Millideg, Nm, Point};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::pcb::InDocument;
use crate::{ComponentType, Fields, Footprint, Object, Part, Symbol, Uuid, Waiver};

/// A sheet's paper size. Sheets are landscape.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SheetSize {
    #[default]
    A4,
    A3,
    A2,
    A1,
    A0,
    Letter,
    Legal,
    Tabloid,
}

impl SheetSize {
    /// The landscape width and height.
    pub const fn dimensions(self) -> (Nm, Nm) {
        const MM: i64 = 1_000_000;
        const IN: i64 = 25_400_000;
        let (w, h) = match self {
            SheetSize::A4 => (297 * MM, 210 * MM),
            SheetSize::A3 => (420 * MM, 297 * MM),
            SheetSize::A2 => (594 * MM, 420 * MM),
            SheetSize::A1 => (841 * MM, 594 * MM),
            SheetSize::A0 => (1189 * MM, 841 * MM),
            SheetSize::Letter => (11 * IN, IN * 17 / 2),
            SheetSize::Legal => (14 * IN, IN * 17 / 2),
            SheetSize::Tabloid => (17 * IN, 11 * IN),
        };
        (Nm(w), Nm(h))
    }
}

/// A sheet's frame settings. A sheet has at most one; with none it uses the defaults.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SheetFrame {
    pub uuid: Uuid,
    pub size: SheetSize,
}

/// One entry of a sheet's part table: a literal copy of a Library Part with its Symbol,
/// placed once per sheet and shared by every Unit placed from it. It is keyed by the
/// Part's UUID ([`Part::uuid`]) plus [`PartEntry::hash`]: a sheet never holds two entries
/// with the same pair, and a changed Part gets an entry of its own.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PartEntry {
    /// The entry's own UUID, which Units point at.
    pub uuid: Uuid,
    /// The content hash of the Part with its Symbol and Footprints, as copied.
    pub hash: String,
    pub part: Part,
    pub symbol: Symbol,
    /// The sheet's [`FootprintCopy`] of each alternate, in the order of `part.footprints`.
    pub footprints: Vec<Uuid>,
}

/// A copy of a Library Footprint on a sheet, shared by every part-table entry that lists
/// it. Keyed by the Footprint's UUID plus [`FootprintCopy::hash`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FootprintCopy {
    pub uuid: Uuid,
    /// The content hash of the Footprint, as copied.
    pub hash: String,
    pub footprint: Footprint,
}

/// Which Component an object refers to: its UUID, after the Sheet Instance path that
/// leads to it. In v0.1 sheets are flat and the path is always empty.
#[derive(
    Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(deny_unknown_fields)]
pub struct ComponentRef {
    /// The Sheet Instances from the top sheet down. Empty in v0.1.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub path: Vec<Uuid>,
    pub uuid: Uuid,
}

impl ComponentRef {
    /// A reference with an empty Sheet Instance path.
    pub fn new(uuid: Uuid) -> ComponentRef {
        ComponentRef { path: vec![], uuid }
    }
}

/// One placed Unit of a Component. Every Unit of a Component carries the same copy of the
/// Component's data; Commands keep the copies equal.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UnitInstance {
    pub uuid: Uuid,
    pub component: ComponentRef,
    /// The part-table entry on this sheet it was placed from.
    pub part: Uuid,
    /// The Symbol Unit it shows.
    pub unit: Uuid,
    /// Where the Symbol origin sits on the sheet.
    pub at: Point,
    pub rotation: Millideg,
    /// Mirrored left to right before rotating.
    #[serde(default, skip_serializing_if = "is_false")]
    pub mirror: bool,
    pub designator: String,
    /// Locked against Re-annotate, which then leaves the Designator as it is.
    #[serde(default, skip_serializing_if = "is_false")]
    pub designator_locked: bool,
    /// The value is `fields.value`.
    pub fields: Fields,
    /// The chosen Footprint: the UUID of one of the Part's alternates, if it has any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub footprint: Option<Uuid>,
    pub component_type: ComponentType,
    #[serde(default, skip_serializing_if = "is_false")]
    pub not_fitted: bool,
    /// The fields drawn on the sheet, by name: `designator`, a built-in field's stored name
    /// (such as `value`) or a free-form field's name. Designator and value by default.
    #[serde(
        default = "UnitInstance::default_visible_fields",
        skip_serializing_if = "UnitInstance::shows_default_fields"
    )]
    pub visible_fields: BTreeSet<String>,
}

impl UnitInstance {
    /// The name under which [`UnitInstance::visible_fields`] lists the Designator.
    pub const DESIGNATOR: &'static str = "designator";

    /// The fields a newly placed Unit shows: its Designator and value.
    pub fn default_visible_fields() -> BTreeSet<String> {
        [Self::DESIGNATOR, "value"]
            .into_iter()
            .map(str::to_owned)
            .collect()
    }

    fn shows_default_fields(fields: &BTreeSet<String>) -> bool {
        *fields == Self::default_visible_fields()
    }
}

/// A Wire: a polyline at any angle through its `vertices`, in order, at least two. It
/// connects where an end touches a Pin's connection point or any point of another Wire,
/// and anywhere along it that it passes over a Pin's connection point, a Port, a Power
/// Port or a Net Label. Two Wires crossing mid-span never connect.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Wire {
    pub uuid: Uuid,
    pub vertices: Vec<Point>,
}

impl Wire {
    /// The Wire through `vertices` with its zero-length segments dropped (repeated
    /// vertices merged). `None` when fewer than two distinct vertices are left.
    pub fn through(uuid: Uuid, mut vertices: Vec<Point>) -> Option<Wire> {
        vertices.dedup();
        (vertices.len() >= 2).then_some(Wire { uuid, vertices })
    }

    /// Its first and last vertex.
    pub fn ends(&self) -> [Point; 2] {
        [self.vertices[0], self.vertices[self.vertices.len() - 1]]
    }

    /// Its segments in order, as (from, to).
    pub fn segments(&self) -> impl Iterator<Item = (Point, Point)> + '_ {
        self.vertices.windows(2).map(|pair| (pair[0], pair[1]))
    }

    /// Whether `p` lies on the Wire, ends included.
    pub fn contains(&self, p: Point) -> bool {
        self.segments().any(|(a, b)| p.on_segment(a, b))
    }

    /// Whether the Wire passes over `p`: `p` lies on it but is not one of its ends.
    pub fn passes_over(&self, p: Point) -> bool {
        !self.ends().contains(&p) && self.contains(p)
    }

    /// Whether it has at least two vertices and some length. A zero-length segment
    /// inside it is harmless (edit Commands drop them, but a hand-edited file may keep one).
    pub fn has_length(&self) -> bool {
        self.segments().any(|(a, b)| a != b)
    }
}

/// A named marker that joins the Wire (or Pin) under `at` to every other Net Label with the
/// same name on its sheet.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NetLabel {
    pub uuid: Uuid,
    pub name: String,
    /// The connection point.
    pub at: Point,
    /// Counter-clockwise; only how the label is drawn.
    pub rotation: Millideg,
}

/// Which way a Port's signal flows. It only changes how the Port is drawn and checked;
/// every direction connects the same way.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PortDirection {
    In,
    Out,
    #[default]
    Bidirectional,
}

/// A named, directional marker that joins the Wire (or Pin) under `at` to every Port of
/// the same name on any Schematic Sheet of the Project.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Port {
    pub uuid: Uuid,
    pub name: String,
    pub direction: PortDirection,
    /// The connection point.
    pub at: Point,
    /// Counter-clockwise; only how the Port is drawn.
    pub rotation: Millideg,
}

/// The fixed, built-in look of a Power Port.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PowerPortStyle {
    /// A horizontal bar, for positive rails.
    #[default]
    Bar,
    Arrow,
    Circle,
    Wave,
    /// Three shrinking bars.
    Ground,
    /// A downward triangle.
    SignalGround,
    /// Ground bars with diagonal strokes.
    Earth,
}

/// A named symbol, such as GND or 3V3, that joins the Wire (or Pin) under `at` to the
/// Project-wide Net of that name on every Schematic Sheet.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PowerPort {
    pub uuid: Uuid,
    pub name: String,
    pub style: PowerPortStyle,
    /// The connection point.
    pub at: Point,
    /// Counter-clockwise; only how the symbol is drawn.
    pub rotation: Millideg,
}

/// Free text drawn on a sheet. It has no electrical meaning.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SheetText {
    pub uuid: Uuid,
    /// The text; `\n` starts a new line.
    pub text: String,
    /// The left end of the first baseline.
    pub at: Point,
    /// The capital height.
    pub height: Nm,
    /// Counter-clockwise about `at`.
    pub rotation: Millideg,
}

/// A free line or rectangle drawn on a sheet. It has no electrical meaning.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SheetGraphic {
    pub uuid: Uuid,
    pub shape: SheetShape,
}

/// What a [`SheetGraphic`] draws.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SheetShape {
    /// A straight line, at any angle.
    Line { from: Point, to: Point },
    /// An axis-aligned rectangle between two opposite corners.
    Rect {
        from: Point,
        to: Point,
        #[serde(default, skip_serializing_if = "is_false")]
        filled: bool,
    },
}

impl SheetShape {
    /// Its two defining points: a line's ends or a rectangle's opposite corners.
    pub fn ends(&self) -> (Point, Point) {
        match self {
            SheetShape::Line { from, to } | SheetShape::Rect { from, to, .. } => (*from, *to),
        }
    }

    /// Whether it draws nothing: a line with both ends together, or a rectangle with no
    /// width or no height.
    pub fn is_empty(&self) -> bool {
        let (a, b) = self.ends();
        match self {
            SheetShape::Line { .. } => a == b,
            SheetShape::Rect { .. } => a.x == b.x || a.y == b.y,
        }
    }

    /// The same shape through two other defining points.
    pub fn with_ends(&self, a: Point, b: Point) -> SheetShape {
        match self {
            SheetShape::Line { .. } => SheetShape::Line { from: a, to: b },
            SheetShape::Rect { filled, .. } => SheetShape::Rect {
                from: a,
                to: b,
                filled: *filled,
            },
        }
    }
}

/// A mark on the Pin under `at` stating that it is deliberately left unconnected.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NoConnect {
    pub uuid: Uuid,
    pub at: Point,
}

fn is_false(b: &bool) -> bool {
    !*b
}

/// The body of a Schematic Sheet: one UUID-keyed table per object kind.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SchematicSheet {
    pub uuid: Uuid,
    pub(crate) frames: BTreeMap<Uuid, SheetFrame>,
    pub(crate) parts: BTreeMap<Uuid, PartEntry>,
    pub(crate) footprints: BTreeMap<Uuid, FootprintCopy>,
    pub(crate) units: BTreeMap<Uuid, UnitInstance>,
    pub(crate) wires: BTreeMap<Uuid, Wire>,
    pub(crate) net_labels: BTreeMap<Uuid, NetLabel>,
    pub(crate) no_connects: BTreeMap<Uuid, NoConnect>,
    pub(crate) ports: BTreeMap<Uuid, Port>,
    pub(crate) power_ports: BTreeMap<Uuid, PowerPort>,
    pub(crate) texts: BTreeMap<Uuid, SheetText>,
    pub(crate) graphics: BTreeMap<Uuid, SheetGraphic>,
    pub(crate) waivers: BTreeMap<Uuid, Waiver>,
}

impl SchematicSheet {
    pub(crate) fn new(uuid: Uuid) -> SchematicSheet {
        SchematicSheet {
            uuid,
            frames: BTreeMap::new(),
            parts: BTreeMap::new(),
            footprints: BTreeMap::new(),
            units: BTreeMap::new(),
            wires: BTreeMap::new(),
            net_labels: BTreeMap::new(),
            no_connects: BTreeMap::new(),
            ports: BTreeMap::new(),
            power_ports: BTreeMap::new(),
            texts: BTreeMap::new(),
            graphics: BTreeMap::new(),
            waivers: BTreeMap::new(),
        }
    }

    /// The sheet's frame settings, if it has any.
    pub fn frame(&self) -> Option<&SheetFrame> {
        self.frames.values().next()
    }

    /// The sheet's paper size.
    pub fn size(&self) -> SheetSize {
        self.frame().map(|f| f.size).unwrap_or_default()
    }

    /// The part table, sorted by entry UUID.
    pub fn parts(&self) -> impl Iterator<Item = &PartEntry> {
        self.parts.values()
    }

    pub fn part(&self, uuid: Uuid) -> Option<&PartEntry> {
        self.parts.get(&uuid)
    }

    /// The part-table entry copied from Library Part `part` with content hash `hash`.
    pub fn part_entry_for(&self, part: Uuid, hash: &str) -> Option<&PartEntry> {
        self.parts
            .values()
            .find(|e| e.part.uuid == part && e.hash == hash)
    }

    /// The Footprint copies, sorted by UUID.
    pub fn footprints(&self) -> impl Iterator<Item = &FootprintCopy> {
        self.footprints.values()
    }

    pub fn footprint(&self, uuid: Uuid) -> Option<&FootprintCopy> {
        self.footprints.get(&uuid)
    }

    /// The copy of Library Footprint `footprint` with content hash `hash`.
    pub fn footprint_copy_for(&self, footprint: Uuid, hash: &str) -> Option<&FootprintCopy> {
        self.footprints
            .values()
            .find(|c| c.footprint.uuid == footprint && c.hash == hash)
    }

    /// The placed Units, sorted by UUID.
    pub fn units(&self) -> impl Iterator<Item = &UnitInstance> {
        self.units.values()
    }

    pub fn unit(&self, uuid: Uuid) -> Option<&UnitInstance> {
        self.units.get(&uuid)
    }

    /// The Wires, sorted by UUID.
    pub fn wires(&self) -> impl Iterator<Item = &Wire> {
        self.wires.values()
    }

    pub fn wire(&self, uuid: Uuid) -> Option<&Wire> {
        self.wires.get(&uuid)
    }

    /// The Net Labels, sorted by UUID.
    pub fn net_labels(&self) -> impl Iterator<Item = &NetLabel> {
        self.net_labels.values()
    }

    pub fn net_label(&self, uuid: Uuid) -> Option<&NetLabel> {
        self.net_labels.get(&uuid)
    }

    /// The No-Connect Markers, sorted by UUID.
    pub fn no_connects(&self) -> impl Iterator<Item = &NoConnect> {
        self.no_connects.values()
    }

    pub fn no_connect(&self, uuid: Uuid) -> Option<&NoConnect> {
        self.no_connects.get(&uuid)
    }

    /// The Ports, sorted by UUID.
    pub fn ports(&self) -> impl Iterator<Item = &Port> {
        self.ports.values()
    }

    pub fn port(&self, uuid: Uuid) -> Option<&Port> {
        self.ports.get(&uuid)
    }

    /// The Power Ports, sorted by UUID.
    pub fn power_ports(&self) -> impl Iterator<Item = &PowerPort> {
        self.power_ports.values()
    }

    pub fn power_port(&self, uuid: Uuid) -> Option<&PowerPort> {
        self.power_ports.get(&uuid)
    }

    /// The free texts, sorted by UUID.
    pub fn texts(&self) -> impl Iterator<Item = &SheetText> {
        self.texts.values()
    }

    pub fn text(&self, uuid: Uuid) -> Option<&SheetText> {
        self.texts.get(&uuid)
    }

    /// The free lines and rectangles, sorted by UUID.
    pub fn graphics(&self) -> impl Iterator<Item = &SheetGraphic> {
        self.graphics.values()
    }

    pub fn graphic(&self, uuid: Uuid) -> Option<&SheetGraphic> {
        self.graphics.get(&uuid)
    }

    /// The Waivers of Violations whose objects this sheet holds, sorted by UUID.
    pub fn waivers(&self) -> impl Iterator<Item = &Waiver> {
        self.waivers.values()
    }

    pub fn waiver(&self, uuid: Uuid) -> Option<&Waiver> {
        self.waivers.get(&uuid)
    }

    /// The object with this UUID, if this sheet holds it.
    pub(crate) fn object(&self, uuid: Uuid) -> Option<Object> {
        let doc = self.uuid;
        macro_rules! find {
            ($($table:ident => $variant:ident),*) => {
                None$(.or_else(|| {
                    let row = self.$table.get(&uuid)?.clone();
                    Some(Object::$variant(InDocument::new(doc, row)))
                }))*
            };
        }
        find!(
            frames => SheetFrame,
            parts => PartEntry,
            footprints => FootprintCopy,
            units => Unit,
            wires => Wire,
            net_labels => NetLabel,
            no_connects => NoConnect,
            ports => Port,
            power_ports => PowerPort,
            texts => SheetText,
            graphics => SheetGraphic,
            waivers => Waiver
        )
    }

    /// Every object this sheet holds, cloned.
    pub(crate) fn objects(&self) -> Vec<Object> {
        let doc = self.uuid;
        let mut out = Vec::new();
        macro_rules! all {
            ($($table:ident => $variant:ident),*) => {
                $(out.extend(
                    self.$table
                        .values()
                        .map(|row| Object::$variant(InDocument::new(doc, row.clone()))),
                );)*
            };
        }
        all!(
            frames => SheetFrame,
            parts => PartEntry,
            footprints => FootprintCopy,
            units => Unit,
            wires => Wire,
            net_labels => NetLabel,
            no_connects => NoConnect,
            ports => Port,
            power_ports => PowerPort,
            texts => SheetText,
            graphics => SheetGraphic,
            waivers => Waiver
        );
        out
    }

    /// Removes the object with this UUID from whichever table holds it.
    pub(crate) fn remove(&mut self, uuid: Uuid) {
        self.frames.remove(&uuid);
        self.parts.remove(&uuid);
        self.footprints.remove(&uuid);
        self.units.remove(&uuid);
        self.wires.remove(&uuid);
        self.net_labels.remove(&uuid);
        self.no_connects.remove(&uuid);
        self.ports.remove(&uuid);
        self.power_ports.remove(&uuid);
        self.texts.remove(&uuid);
        self.graphics.remove(&uuid);
        self.waivers.remove(&uuid);
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.frames.is_empty()
            && self.parts.is_empty()
            && self.footprints.is_empty()
            && self.units.is_empty()
            && self.wires.is_empty()
            && self.net_labels.is_empty()
            && self.no_connects.is_empty()
            && self.ports.is_empty()
            && self.power_ports.is_empty()
            && self.texts.is_empty()
            && self.graphics.is_empty()
            && self.waivers.is_empty()
    }

    /// Checks the invariants a committed transaction must leave behind.
    pub(crate) fn check(&self) -> Result<(), String> {
        let sheet = self.uuid;
        if self.frames.len() > 1 {
            return Err(format!("sheet {sheet} has more than one frame"));
        }
        let mut keys = BTreeSet::new();
        for copy in self.footprints.values() {
            if !keys.insert((copy.footprint.uuid, copy.hash.as_str())) {
                return Err(format!(
                    "sheet {sheet} holds Footprint {} with hash {} twice",
                    copy.footprint.uuid, copy.hash
                ));
            }
        }
        let mut keys = BTreeSet::new();
        for entry in self.parts.values() {
            if !keys.insert((entry.part.uuid, entry.hash.as_str())) {
                return Err(format!(
                    "sheet {sheet} holds Part {} with hash {} twice",
                    entry.part.uuid, entry.hash
                ));
            }
            if entry.part.symbol != Some(entry.symbol.uuid) {
                return Err(format!(
                    "part entry {} copies Symbol {}, which its Part does not name",
                    entry.uuid, entry.symbol.uuid
                ));
            }
            if entry.footprints.len() != entry.part.footprints.len() {
                return Err(format!(
                    "part entry {} has {} Footprint copies for {} alternates",
                    entry.uuid,
                    entry.footprints.len(),
                    entry.part.footprints.len()
                ));
            }
            for (copy, alternate) in entry.footprints.iter().zip(&entry.part.footprints) {
                match self.footprints.get(copy) {
                    Some(c) if c.footprint.uuid == *alternate => {}
                    _ => {
                        return Err(format!(
                            "part entry {} names Footprint copy {copy}, which is not a copy \
                             of Footprint {alternate} on its sheet",
                            entry.uuid
                        ));
                    }
                }
            }
        }
        for unit in self.units.values() {
            let Some(entry) = self.parts.get(&unit.part) else {
                return Err(format!(
                    "Unit {} names part entry {}, which is not on its sheet",
                    unit.uuid, unit.part
                ));
            };
            if !entry.symbol.units.iter().any(|u| u.uuid == unit.unit) {
                return Err(format!(
                    "Unit {} shows Symbol Unit {}, which its Symbol does not have",
                    unit.uuid, unit.unit
                ));
            }
            if let Some(footprint) = unit.footprint
                && !entry.part.footprints.contains(&footprint)
            {
                return Err(format!(
                    "Unit {} chooses Footprint {footprint}, which is not one of its Part's \
                     alternates",
                    unit.uuid
                ));
            }
        }
        for wire in self.wires.values() {
            if !wire.has_length() {
                return Err(format!(
                    "Wire {} needs at least two vertices, not all at one point",
                    wire.uuid
                ));
            }
        }
        for label in self.net_labels.values() {
            if label.name.trim().is_empty() {
                return Err(format!("Net Label {} has no name", label.uuid));
            }
        }
        for port in self.ports.values() {
            if port.name.trim().is_empty() {
                return Err(format!("Port {} has no name", port.uuid));
            }
        }
        for port in self.power_ports.values() {
            if port.name.trim().is_empty() {
                return Err(format!("Power Port {} has no name", port.uuid));
            }
        }
        for text in self.texts.values() {
            if text.text.trim().is_empty() || text.height <= Nm::ZERO {
                return Err(format!(
                    "text {} needs some text and a positive height",
                    text.uuid
                ));
            }
        }
        for graphic in self.graphics.values() {
            if graphic.shape.is_empty() {
                return Err(format!("graphic {} is empty", graphic.uuid));
            }
        }
        Ok(())
    }
}

/// Serialised as its objects in UUID order, for fingerprinting what a check reads.
impl serde::Serialize for SchematicSheet {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(self.objects())
    }
}
