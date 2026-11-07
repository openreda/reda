//! Library Status: how a placed Component's part-table copy compares with its Part in the
//! registered Libraries, aspect by aspect. Each aspect's hash is computed from the copy
//! itself, so no source hashes are stored and a partially updated (mixed) entry compares
//! correctly.

use reda_command::{Fingerprint, PartCopy};
use reda_model::{Footprint, Part, PartEntry, SchematicSheet, Symbol, Uuid};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// What an Update from Library can take, separately or together (Full Replace).
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Aspect {
    /// The Symbol: its graphics, Units and Pins.
    Symbol,
    /// The Part's name, default fields, Component Type, In BOM and In CPL.
    Fields,
    /// The alternate Footprints, their order and geometry, and the pin-map overrides.
    Footprints,
}

impl Aspect {
    /// Every aspect: Full Replace.
    pub const ALL: [Aspect; 3] = [Aspect::Symbol, Aspect::Fields, Aspect::Footprints];
}

/// How a placed Component compares with the Libraries.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LibraryStatus {
    /// Every aspect's content hash matches.
    UpToDate,
    /// At least one aspect differs.
    LibraryDiffers,
    /// No registered Library holds the Part, or a Symbol or Footprint it references.
    SourceMissing,
}

/// A Part with its Symbol and alternate Footprints, wherever it is held: a part-table
/// entry or a Library copy.
#[derive(Clone, Copy)]
pub(crate) struct Content<'a> {
    pub part: &'a Part,
    pub symbol: Option<&'a Symbol>,
    /// In the order of `part.footprints`.
    pub footprints: &'a [&'a Footprint],
}

/// The Footprints of a part-table entry, in its Part's order.
pub(crate) fn entry_footprints<'a>(
    sheet: &'a SchematicSheet,
    entry: &PartEntry,
) -> Vec<&'a Footprint> {
    entry
        .footprints
        .iter()
        .map(|c| {
            &sheet
                .footprint(*c)
                .expect("an entry's copies exist")
                .footprint
        })
        .collect()
}

/// The Footprints of a Library copy, in its Part's order.
pub(crate) fn copy_footprints(copy: &PartCopy) -> Vec<&Footprint> {
    copy.footprints.iter().map(|f| &f.footprint).collect()
}

impl Content<'_> {
    /// The content hash of one aspect.
    pub fn hash(&self, aspect: Aspect) -> String {
        let mut f = Fingerprint::new();
        let part = self.part;
        match aspect {
            Aspect::Symbol => f.add(&part.symbol).add(&self.symbol),
            Aspect::Fields => f
                .add(&part.name)
                .add(&part.component_type)
                .add(&part.in_bom)
                .add(&part.in_cpl)
                .add(&part.fields)
                .add(&part.provenance),
            Aspect::Footprints => f
                .add(&part.footprints)
                .add(&part.pin_map)
                .add(&self.footprints),
        };
        f.finish()
    }

    /// The aspects whose hashes differ from `other`'s, in [`Aspect::ALL`] order.
    pub fn differs(&self, other: &Content<'_>) -> Vec<Aspect> {
        Aspect::ALL
            .into_iter()
            .filter(|a| self.hash(*a) != other.hash(*a))
            .collect()
    }

    /// A hash of the whole content for a mixed part-table entry: one no Library holds as
    /// it is, because only some aspects were taken.
    pub fn mixed_hash(&self) -> String {
        let mut f = Fingerprint::new();
        for aspect in Aspect::ALL {
            f.add(&self.hash(aspect));
        }
        format!("mixed-{}", f.finish())
    }
}

/// One placed Component's Library Status.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ComponentStatus {
    pub component: Uuid,
    pub designator: String,
    /// The Library Part, by UUID.
    pub part: Uuid,
    /// The Part's name in the part table.
    pub part_name: String,
    pub status: LibraryStatus,
    /// For Library Differs: the aspects that differ.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aspects: Option<Vec<Aspect>>,
    /// For Source Missing: what is missing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}
