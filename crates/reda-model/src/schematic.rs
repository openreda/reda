//! The objects on a Schematic Sheet: its frame, its part table and its placed Units.
//!
//! Each object is stored as an [`InDocument`] row, which names the sheet it lives on, so
//! its scope follows from the object alone; the sheet's file leaves that field out.

use std::collections::{BTreeMap, BTreeSet};

use reda_geom::{Millideg, Nm, Point};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::pcb::InDocument;
use crate::{ComponentType, Fields, Footprint, Object, Part, Symbol, Uuid};

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
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
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
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
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
}

impl SchematicSheet {
    pub(crate) fn new(uuid: Uuid) -> SchematicSheet {
        SchematicSheet {
            uuid,
            frames: BTreeMap::new(),
            parts: BTreeMap::new(),
            footprints: BTreeMap::new(),
            units: BTreeMap::new(),
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
            units => Unit
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
            units => Unit
        );
        out
    }

    /// Removes the object with this UUID from whichever table holds it.
    pub(crate) fn remove(&mut self, uuid: Uuid) {
        self.frames.remove(&uuid);
        self.parts.remove(&uuid);
        self.footprints.remove(&uuid);
        self.units.remove(&uuid);
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.frames.is_empty()
            && self.parts.is_empty()
            && self.footprints.is_empty()
            && self.units.is_empty()
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
        Ok(())
    }
}
