//! The PCB Document: its Layers, Stackup ([`stackup`]), board setup (rules and DRC severities), Board Outline,
//! Board Cutouts, Net Classes, Nets, free text and graphics ([`graphics`]), and the
//! placed Footprints: a table of Footprint copies and the Footprint instances
//! ([`footprint`]) placed from them.
//!
//! Every type here is a row of the `.redapcb` file as well as an object kind; field order
//! is the on-disk key order. In the [`crate::Object`] enum they are wrapped in
//! [`InDocument`], which names the PCB Document they live in.

use std::collections::BTreeMap;

use reda_geom::{Nm, Vertex};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{FootprintCopy, Object, Severity, Uuid, Waiver};

mod copper;
mod footprint;
mod graphics;
mod placement;
mod rules;
mod stackup;
mod zone;

pub use copper::{LayerSpan, Track, TrackGeometry, Via};

pub use footprint::{
    FootprintInstance, FootprintTextRef, PadNet, PadRef, TextAutoposition, TextOverride,
};
pub use placement::{FreePad, OutputOrigin};
pub use rules::{
    Bounds, DesignRule, PortableClass, PortableRule, RuleField, RuleKind, RuleScope, RuleSet,
    RuleValue, RuleValues,
};
pub use stackup::{
    CopperEntry, DEFAULT_BOARD_THICKNESS, DEFAULT_COPPER, DielectricEntry, Stackup, StackupEntry,
    StackupSeeds, StackupTotals,
};

pub use graphics::{GraphicShape, HERSHEY_FONT, NOTO_SANS_FONT, PcbGraphic, PcbText};

pub use zone::{FillShape, Keepout, PadConnection, Zone, ZoneFills};

/// Which face of the board a sided Layer is on.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    Top,
    Bottom,
}

/// What a Layer is for. Copper and the sided kinds come with a [`Side`].
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum LayerKind {
    Copper,
    Silkscreen,
    SolderMask,
    Paste,
    Courtyard,
    Assembly,
    BoardOutline,
    Mechanical,
    Documentation,
}

/// One Layer. Objects name their Layer by UUID, never by position.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Layer {
    pub uuid: Uuid,
    pub kind: LayerKind,
    /// The board face, for sided Layers; left out for the others.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub side: Option<Side>,
}

/// The PCB Document's Layers as one ordered list, top to bottom. A generic list, so more
/// copper Layers need no format change.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LayerSet {
    pub uuid: Uuid,
    pub layers: Vec<Layer>,
}

impl LayerSet {
    pub fn layer(&self, uuid: Uuid) -> Option<&Layer> {
        self.layers.iter().find(|l| l.uuid == uuid)
    }

    /// The first Layer of this kind and side.
    pub fn find(&self, kind: LayerKind, side: Option<Side>) -> Option<&Layer> {
        self.layers
            .iter()
            .find(|l| l.kind == kind && l.side == side)
    }
}

crate::check::severities! {
    /// One DRC check, named as in [`DrcSeverities`].
    enum DrcCheck;
    /// The severity of every DRC check, stored next to the rules. A check left out has its
    /// default.
    #[serde(default)]
    struct DrcSeverities;
    /// Clearance between copper of different Nets.
    clearance => Clearance = Error,
    /// Different Nets touching.
    short_circuit => ShortCircuit = Error,
    /// Pads with different numbers touching on a Part that isn't a Net Tie.
    shorted_copper => ShortedCopper = Error,
    /// A Net Tie's Footprint that isn't a valid tie.
    net_tie_not_joined => NetTieNotJoined = Error,
    /// A Track narrower than the minimum or wider than the maximum.
    track_width => TrackWidth = Error,
    /// A Via diameter or hole outside the `via_size` range.
    via_size => ViaSize = Error,
    /// A Pad, Free Pad or non-plated hole outside the `hole_size` range.
    hole_size => HoleSize = Error,
    /// An annular ring below the minimum.
    annular_ring => AnnularRing = Error,
    /// Holes closer than the minimum spacing.
    hole_to_hole => HoleToHole = Error,
    /// Copper too close to the Board Outline or a Board Cutout.
    edge_clearance => EdgeClearance = Error,
    /// An object inside a Keepout that forbids it.
    keepout => Keepout = Error,
    /// A Footprint outside the Board Outline.
    footprint_outside_board => FootprintOutsideBoard = Error,
    /// Connections the Ratsnest still shows.
    unrouted => Unrouted = Error,
    /// Copper with no Net.
    copper_without_net => CopperWithoutNet = Warning,
    /// Tracks and Vias that end unconnected.
    dangling => Dangling = Warning,
    /// Overlapping courtyards.
    courtyard_overlap => CourtyardOverlap = Warning,
    /// Silkscreen over exposed Pads.
    silkscreen_over_pads => SilkscreenOverPads = Warning,
    /// The PCB out of sync with the schematic.
    out_of_sync => OutOfSync = Warning,
    /// A negative mask or paste expansion leaves no opening.
    opening_collapsed => OpeningCollapsed = Warning,
    /// A rule record with a blocking problem, which only a hand edit or a merge can cause.
    rule_invalid => RuleInvalid = Error,
    /// A rule that allows less than a Fab Preset limit.
    rule_outside_preset => RuleOutsidePreset = Warning,
}

/// The board's own settings: its DRC severities. The design rules are records of their
/// own ([`DesignRule`]).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BoardSetup {
    pub uuid: Uuid,
    pub severities: DrcSeverities,
}

/// The closed shape of the board's physical edge.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BoardOutline {
    pub uuid: Uuid,
    /// The contour, counter-clockwise or not; the last vertex joins the first.
    pub vertices: Vec<Vertex>,
}

/// A closed shape inside the Board Outline where board material is removed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BoardCutout {
    pub uuid: Uuid,
    /// The contour; the last vertex joins the first.
    pub vertices: Vec<Vertex>,
}

/// A named, ranked group of Nets that share design rules. Its values are the rule records
/// scoped to it. The Default Net Class is not stored: it is the board-wide records.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NetClass {
    pub uuid: Uuid,
    pub name: String,
    /// Its rank, unique on the board: 1 ranks highest. Where a Net's classes set the same
    /// value, the highest-ranked class wins; the Default Net Class ranks below them all.
    pub rank: u32,
}

/// A PCB Net: Pads, Tracks, Vias and Zones name it by UUID.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Net {
    pub uuid: Uuid,
    pub name: String,
    /// The Net Classes it is in besides the Default Net Class, which every Net is in;
    /// left out when there are none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub classes: Vec<Uuid>,
}

/// An object stored in a Document, with the Document it lives in. The Document is not
/// serialised: object views and files say where an object is in their own way.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct InDocument<T> {
    #[serde(skip)]
    pub document: Uuid,
    #[serde(flatten)]
    pub object: T,
}

impl<T> InDocument<T> {
    pub fn new(document: Uuid, object: T) -> InDocument<T> {
        InDocument { document, object }
    }
}

impl<T> std::ops::Deref for InDocument<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.object
    }
}

/// The body of a PCB Document: one UUID-keyed table per object kind.
///
/// A committed Project has exactly one [`LayerSet`], one [`Stackup`] and one [`BoardSetup`] here and at most
/// one [`BoardOutline`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PcbDocument {
    pub uuid: Uuid,
    pub(crate) layer_sets: BTreeMap<Uuid, LayerSet>,
    pub(crate) setups: BTreeMap<Uuid, BoardSetup>,
    pub(crate) stackups: BTreeMap<Uuid, Stackup>,
    pub(crate) rules: BTreeMap<Uuid, DesignRule>,
    pub(crate) outlines: BTreeMap<Uuid, BoardOutline>,
    pub(crate) cutouts: BTreeMap<Uuid, BoardCutout>,
    pub(crate) net_classes: BTreeMap<Uuid, NetClass>,
    pub(crate) nets: BTreeMap<Uuid, Net>,
    pub(crate) texts: BTreeMap<Uuid, PcbText>,
    pub(crate) graphics: BTreeMap<Uuid, PcbGraphic>,
    pub(crate) footprint_copies: BTreeMap<Uuid, FootprintCopy>,
    pub(crate) footprints: BTreeMap<Uuid, FootprintInstance>,
    pub(crate) tracks: BTreeMap<Uuid, Track>,
    pub(crate) vias: BTreeMap<Uuid, Via>,
    pub(crate) zones: BTreeMap<Uuid, Zone>,
    pub(crate) keepouts: BTreeMap<Uuid, Keepout>,
    pub(crate) free_pads: BTreeMap<Uuid, FreePad>,
    pub(crate) output_origins: BTreeMap<Uuid, OutputOrigin>,
    pub(crate) waivers: BTreeMap<Uuid, Waiver>,
}

impl PcbDocument {
    pub(crate) fn new(uuid: Uuid) -> PcbDocument {
        PcbDocument {
            uuid,
            layer_sets: BTreeMap::new(),
            setups: BTreeMap::new(),
            stackups: BTreeMap::new(),
            rules: BTreeMap::new(),
            outlines: BTreeMap::new(),
            cutouts: BTreeMap::new(),
            net_classes: BTreeMap::new(),
            nets: BTreeMap::new(),
            texts: BTreeMap::new(),
            graphics: BTreeMap::new(),
            footprint_copies: BTreeMap::new(),
            footprints: BTreeMap::new(),
            tracks: BTreeMap::new(),
            vias: BTreeMap::new(),
            zones: BTreeMap::new(),
            keepouts: BTreeMap::new(),
            free_pads: BTreeMap::new(),
            output_origins: BTreeMap::new(),
            waivers: BTreeMap::new(),
        }
    }

    /// The Layers. Every committed PCB Document has them.
    pub fn layers(&self) -> &LayerSet {
        self.layer_sets
            .values()
            .next()
            .expect("a PCB Document has its Layers")
    }

    /// The DRC severities. Every committed PCB Document has them.
    pub fn setup(&self) -> &BoardSetup {
        self.setups
            .values()
            .next()
            .expect("a PCB Document has its board setup")
    }

    /// The Stackups: one in v0.1.
    pub fn stackups(&self) -> impl Iterator<Item = &Stackup> {
        self.stackups.values()
    }

    /// The Stackup. Every committed PCB Document has one.
    pub fn stackup(&self) -> &Stackup {
        self.stackups
            .values()
            .next()
            .expect("a PCB Document has its Stackup")
    }

    /// The design rule records, sorted by UUID.
    pub fn rules(&self) -> impl Iterator<Item = &DesignRule> {
        self.rules.values()
    }

    pub fn rule(&self, uuid: Uuid) -> Option<&DesignRule> {
        self.rules.get(&uuid)
    }

    /// The board-wide record of `kind`. Every committed PCB Document has one per kind.
    pub fn board_rule(&self, kind: RuleKind) -> &DesignRule {
        self.rules
            .values()
            .find(|r| r.kind == kind && r.scope.is_board_wide())
            .expect("a PCB Document has a board-wide record of every rule kind")
    }

    /// Net Class `class`'s record of `kind`, if it has one.
    pub fn class_rule(&self, kind: RuleKind, class: Uuid) -> Option<&DesignRule> {
        self.rules
            .values()
            .find(|r| r.kind == kind && r.scope.net_class == Some(class))
    }

    /// The record of `kind` in `scope`, if there is one.
    pub fn scoped_rule(&self, kind: RuleKind, scope: RuleScope) -> Option<&DesignRule> {
        self.rules
            .values()
            .find(|r| r.kind == kind && r.scope == scope)
    }

    pub fn outline(&self) -> Option<&BoardOutline> {
        self.outlines.values().next()
    }

    pub fn cutouts(&self) -> impl Iterator<Item = &BoardCutout> {
        self.cutouts.values()
    }

    pub fn cutout(&self, uuid: Uuid) -> Option<&BoardCutout> {
        self.cutouts.get(&uuid)
    }

    pub fn net_classes(&self) -> impl Iterator<Item = &NetClass> {
        self.net_classes.values()
    }

    pub fn net_class(&self, uuid: Uuid) -> Option<&NetClass> {
        self.net_classes.get(&uuid)
    }

    /// The Net Class with this name, compared case-insensitively.
    pub fn net_class_named(&self, name: &str) -> Option<&NetClass> {
        let name = name.to_lowercase();
        self.net_classes
            .values()
            .find(|c| c.name.to_lowercase() == name)
    }

    pub fn nets(&self) -> impl Iterator<Item = &Net> {
        self.nets.values()
    }

    pub fn net(&self, uuid: Uuid) -> Option<&Net> {
        self.nets.get(&uuid)
    }

    pub fn texts(&self) -> impl Iterator<Item = &PcbText> {
        self.texts.values()
    }

    pub fn text(&self, uuid: Uuid) -> Option<&PcbText> {
        self.texts.get(&uuid)
    }

    pub fn graphics(&self) -> impl Iterator<Item = &PcbGraphic> {
        self.graphics.values()
    }

    pub fn graphic(&self, uuid: Uuid) -> Option<&PcbGraphic> {
        self.graphics.get(&uuid)
    }

    /// The part table: the copies of the Footprints in use, sorted by UUID.
    pub fn footprint_copies(&self) -> impl Iterator<Item = &FootprintCopy> {
        self.footprint_copies.values()
    }

    pub fn footprint_copy(&self, uuid: Uuid) -> Option<&FootprintCopy> {
        self.footprint_copies.get(&uuid)
    }

    /// The copy of Library Footprint `footprint` with content hash `hash`.
    pub fn footprint_copy_for(&self, footprint: Uuid, hash: &str) -> Option<&FootprintCopy> {
        self.footprint_copies
            .values()
            .find(|c| c.footprint.uuid == footprint && c.hash == hash)
    }

    /// The placed Footprint instances, sorted by UUID.
    pub fn footprints(&self) -> impl Iterator<Item = &FootprintInstance> {
        self.footprints.values()
    }

    pub fn footprint(&self, uuid: Uuid) -> Option<&FootprintInstance> {
        self.footprints.get(&uuid)
    }

    /// The Tracks, sorted by UUID.
    pub fn tracks(&self) -> impl Iterator<Item = &Track> {
        self.tracks.values()
    }

    pub fn track(&self, uuid: Uuid) -> Option<&Track> {
        self.tracks.get(&uuid)
    }

    /// The Vias, sorted by UUID.
    pub fn vias(&self) -> impl Iterator<Item = &Via> {
        self.vias.values()
    }

    pub fn via(&self, uuid: Uuid) -> Option<&Via> {
        self.vias.get(&uuid)
    }

    /// The Zones, sorted by UUID.
    pub fn zones(&self) -> impl Iterator<Item = &Zone> {
        self.zones.values()
    }

    pub fn zone(&self, uuid: Uuid) -> Option<&Zone> {
        self.zones.get(&uuid)
    }

    /// The Keepouts, sorted by UUID.
    pub fn keepouts(&self) -> impl Iterator<Item = &Keepout> {
        self.keepouts.values()
    }

    pub fn keepout(&self, uuid: Uuid) -> Option<&Keepout> {
        self.keepouts.get(&uuid)
    }

    /// The copper Layers, top to bottom.
    pub fn copper_layers(&self) -> impl Iterator<Item = &Layer> {
        self.layers()
            .layers
            .iter()
            .filter(|l| l.kind == LayerKind::Copper)
    }

    /// The copper number of Layer `layer`: 1 for the top copper, counting down. Copper
    /// numbers (`L1`, `L2`, …) come from position and are never stored. `None` for a
    /// Layer that is not copper.
    pub fn copper_number(&self, layer: Uuid) -> Option<usize> {
        self.copper_layers()
            .position(|l| l.uuid == layer)
            .map(|i| i + 1)
    }

    /// The copper Layers a Via's span joins, top to bottom; empty when the span names a
    /// Layer that is not copper.
    pub fn span_layers(&self, span: LayerSpan) -> Vec<Uuid> {
        let copper: Vec<Uuid> = self.copper_layers().map(|l| l.uuid).collect();
        let (Some(a), Some(b)) = (
            copper.iter().position(|l| *l == span.from),
            copper.iter().position(|l| *l == span.to),
        ) else {
            return Vec::new();
        };
        copper[a.min(b)..=a.max(b)].to_vec()
    }

    /// The Free Pads, sorted by UUID.
    pub fn free_pads(&self) -> impl Iterator<Item = &FreePad> {
        self.free_pads.values()
    }

    pub fn free_pad(&self, uuid: Uuid) -> Option<&FreePad> {
        self.free_pads.get(&uuid)
    }

    /// The Output Origin, if the user has set one.
    pub fn output_origin(&self) -> Option<&OutputOrigin> {
        self.output_origins.values().next()
    }

    /// The Waivers of DRC Violations, sorted by UUID.
    pub fn waivers(&self) -> impl Iterator<Item = &Waiver> {
        self.waivers.values()
    }

    pub fn waiver(&self, uuid: Uuid) -> Option<&Waiver> {
        self.waivers.get(&uuid)
    }

    /// The object with this UUID, if this Document holds it.
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
            layer_sets => LayerSet,
            setups => BoardSetup,
            stackups => Stackup,
            rules => DesignRule,
            outlines => BoardOutline,
            cutouts => BoardCutout,
            net_classes => NetClass,
            nets => Net,
            texts => PcbText,
            graphics => PcbGraphic,
            footprint_copies => FootprintCopy,
            footprints => FootprintInstance,
            tracks => Track,
            vias => Via,
            zones => Zone,
            keepouts => Keepout,
            free_pads => FreePad,
            output_origins => OutputOrigin,
            waivers => Waiver
        )
    }

    /// Every object this Document holds, cloned.
    pub fn objects(&self) -> Vec<Object> {
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
            layer_sets => LayerSet,
            setups => BoardSetup,
            stackups => Stackup,
            rules => DesignRule,
            outlines => BoardOutline,
            cutouts => BoardCutout,
            net_classes => NetClass,
            nets => Net,
            texts => PcbText,
            graphics => PcbGraphic,
            footprint_copies => FootprintCopy,
            footprints => FootprintInstance,
            tracks => Track,
            vias => Via,
            zones => Zone,
            keepouts => Keepout,
            free_pads => FreePad,
            output_origins => OutputOrigin,
            waivers => Waiver
        );
        out
    }

    /// Removes the object with this UUID from whichever table holds it.
    pub(crate) fn remove(&mut self, uuid: Uuid) {
        self.layer_sets.remove(&uuid);
        self.setups.remove(&uuid);
        self.stackups.remove(&uuid);
        self.rules.remove(&uuid);
        self.outlines.remove(&uuid);
        self.cutouts.remove(&uuid);
        self.net_classes.remove(&uuid);
        self.nets.remove(&uuid);
        self.texts.remove(&uuid);
        self.graphics.remove(&uuid);
        self.footprint_copies.remove(&uuid);
        self.footprints.remove(&uuid);
        self.tracks.remove(&uuid);
        self.vias.remove(&uuid);
        self.zones.remove(&uuid);
        self.keepouts.remove(&uuid);
        self.free_pads.remove(&uuid);
        self.output_origins.remove(&uuid);
        self.waivers.remove(&uuid);
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.layer_sets.is_empty()
            && self.setups.is_empty()
            && self.stackups.is_empty()
            && self.rules.is_empty()
            && self.outlines.is_empty()
            && self.cutouts.is_empty()
            && self.net_classes.is_empty()
            && self.nets.is_empty()
            && self.texts.is_empty()
            && self.graphics.is_empty()
            && self.footprint_copies.is_empty()
            && self.footprints.is_empty()
            && self.tracks.is_empty()
            && self.vias.is_empty()
            && self.zones.is_empty()
            && self.keepouts.is_empty()
            && self.free_pads.is_empty()
            && self.output_origins.is_empty()
            && self.waivers.is_empty()
    }

    /// Checks the invariants a committed transaction must leave behind.
    pub(crate) fn check(&self) -> Result<(), String> {
        let doc = self.uuid;
        if self.layer_sets.len() != 1 {
            return Err(format!(
                "PCB Document {doc} must have exactly one Layer set"
            ));
        }
        if self.setups.len() != 1 {
            return Err(format!(
                "PCB Document {doc} must have exactly one board setup"
            ));
        }
        if self.outlines.len() > 1 {
            return Err(format!(
                "PCB Document {doc} has more than one Board Outline"
            ));
        }
        if self.output_origins.len() > 1 {
            return Err(format!(
                "PCB Document {doc} has more than one Output Origin"
            ));
        }
        if self.stackups.len() != 1 {
            return Err(format!("PCB Document {doc} must have exactly one Stackup"));
        }
        self.check_layers()?;
        let copper: Vec<Uuid> = self.copper_layers().map(|l| l.uuid).collect();
        if let Some(problem) = self.stackup().problem(&copper) {
            return Err(problem);
        }
        let mut names = std::collections::BTreeSet::new();
        let mut ranks = std::collections::BTreeSet::new();
        for class in self.net_classes.values() {
            if class.name.trim().is_empty() {
                return Err(format!("Net Class {} has no name", class.uuid));
            }
            if !names.insert(class.name.to_lowercase()) {
                return Err(format!("two Net Classes are named {:?}", class.name));
            }
            if class.rank == 0 || !ranks.insert(class.rank) {
                return Err(format!(
                    "Net Class {:?} needs a rank of 1 or more that no other class has",
                    class.name
                ));
            }
        }
        for net in self.nets.values() {
            if let Some(class) = net
                .classes
                .iter()
                .find(|c| !self.net_classes.contains_key(c))
            {
                return Err(format!(
                    "Net {:?} names Net Class {class}, which does not exist",
                    net.name
                ));
            }
            let mut seen = std::collections::BTreeSet::new();
            if !net.classes.iter().all(|c| seen.insert(*c)) {
                return Err(format!("Net {:?} lists a Net Class twice", net.name));
            }
        }
        self.check_rules()?;
        let drawn = self
            .texts
            .values()
            .map(|t| (t.uuid, t.layer))
            .chain(self.graphics.values().map(|g| (g.uuid, g.layer)));
        for (uuid, layer) in drawn {
            if self.layers().layer(layer).is_none() {
                return Err(format!(
                    "{uuid} is drawn on Layer {layer}, which does not exist"
                ));
            }
        }
        self.check_footprints()?;
        self.check_copper()?;
        self.check_free_pads()
    }

    /// Checks the Layer list: no Layer twice, and every side where it belongs. Copper runs
    /// from the top copper (`side: top`) to the bottom copper (`side: bottom`), and copper
    /// in between is inner copper, with no side. The other sided Layers lie outside the
    /// copper on their own face, and the rest have no side.
    fn check_layers(&self) -> Result<(), String> {
        let layers = &self.layers().layers;
        let mut seen = std::collections::BTreeSet::new();
        if let Some(dup) = layers.iter().find(|l| !seen.insert(l.uuid)) {
            return Err(format!("Layer {} appears twice", dup.uuid));
        }
        let copper: Vec<usize> = layers
            .iter()
            .enumerate()
            .filter(|(_, l)| l.kind == LayerKind::Copper)
            .map(|(i, _)| i)
            .collect();
        let (Some(&first), Some(&last)) = (copper.first(), copper.last()) else {
            return Err("the board has no copper Layers".into());
        };
        if first == last {
            return Err("the board needs a top and a bottom copper Layer".into());
        }
        for (i, layer) in layers.iter().enumerate() {
            let uuid = layer.uuid;
            let expected = match layer.kind {
                LayerKind::Copper if i == first => Some(Some(Side::Top)),
                LayerKind::Copper if i == last => Some(Some(Side::Bottom)),
                LayerKind::Copper => Some(None),
                LayerKind::BoardOutline | LayerKind::Mechanical | LayerKind::Documentation => {
                    Some(None)
                }
                _ => match layer.side {
                    Some(Side::Top) if i < first => None,
                    Some(Side::Bottom) if i > last => None,
                    Some(side) => {
                        return Err(format!(
                            "Layer {uuid} is on the {side:?} side but lies on the other side \
                             of the copper"
                        ));
                    }
                    None => return Err(format!("Layer {uuid} needs a side")),
                },
            };
            if let Some(side) = expected
                && layer.side != side
            {
                let want = match side {
                    Some(Side::Top) => "the top copper must have side top",
                    Some(Side::Bottom) => "the bottom copper must have side bottom",
                    None if layer.kind == LayerKind::Copper => "inner copper has no side",
                    None => "this kind of Layer has no side",
                };
                return Err(format!("Layer {uuid}: {want}"));
            }
        }
        Ok(())
    }

    /// Exactly one board-wide record per kind, at most one record per class and kind,
    /// scopes naming classes that exist, and values that fit their kind.
    fn check_rules(&self) -> Result<(), String> {
        let mut scopes = std::collections::BTreeSet::new();
        for rule in self.rules.values() {
            if let Some(class) = rule.scope.net_class
                && !self.net_classes.contains_key(&class)
            {
                return Err(format!(
                    "rule {} is scoped to Net Class {class}, which does not exist",
                    rule.uuid
                ));
            }
            if !scopes.insert((rule.kind, rule.scope)) {
                return Err(match rule.scope.net_class {
                    None => format!("two board-wide `{}` rules", rule.kind.id()),
                    Some(class) => {
                        format!("two `{}` rules for Net Class {class}", rule.kind.id())
                    }
                });
            }
            if let Some(problem) = rule.problem() {
                return Err(format!("rule {}: {problem}", rule.uuid));
            }
        }
        if let Some(kind) = RuleKind::ALL
            .iter()
            .find(|k| !scopes.contains(&(**k, RuleScope::BOARD)))
        {
            return Err(format!("no board-wide `{}` rule", kind.id()));
        }
        Ok(())
    }

    fn check_copper(&self) -> Result<(), String> {
        let is_copper = |layer: Uuid| {
            self.layers()
                .layer(layer)
                .is_some_and(|l| l.kind == LayerKind::Copper)
        };
        let net_exists = |net: Option<Uuid>| net.is_none_or(|n| self.nets.contains_key(&n));
        for track in self.tracks.values() {
            let uuid = track.uuid;
            if !is_copper(track.layer) {
                return Err(format!("Track {uuid} is not on a copper Layer"));
            }
            if !net_exists(track.net) {
                return Err(format!("Track {uuid} is on a Net that does not exist"));
            }
            if track.width <= Nm::ZERO {
                return Err(format!("Track {uuid} has no width"));
            }
            let (from, to) = track.geometry.ends();
            if from == to {
                return Err(format!("Track {uuid} has no length"));
            }
        }
        for via in self.vias.values() {
            let uuid = via.uuid;
            if !is_copper(via.span.from) || !is_copper(via.span.to) {
                return Err(format!("Via {uuid} spans a Layer that is not copper"));
            }
            let number = |l| self.copper_number(l).unwrap_or(0);
            if number(via.span.from) >= number(via.span.to) {
                return Err(format!(
                    "Via {uuid} must span from an upper copper Layer to a lower one"
                ));
            }
            if !net_exists(via.net) {
                return Err(format!("Via {uuid} is on a Net that does not exist"));
            }
            if via.hole <= Nm::ZERO || via.diameter <= via.hole {
                return Err(format!(
                    "Via {uuid} needs a hole and a diameter larger than it"
                ));
            }
        }
        for zone in self.zones.values() {
            let uuid = zone.uuid;
            if !is_copper(zone.layer) {
                return Err(format!("Zone {uuid} is not on a copper Layer"));
            }
            if !net_exists(zone.net) {
                return Err(format!("Zone {uuid} is on a Net that does not exist"));
            }
            reda_geom::check_contour(&zone.vertices).map_err(|e| format!("Zone {uuid}: {e}"))?;
            if zone.min_width <= Nm::ZERO || zone.thermal_spoke_width <= Nm::ZERO {
                return Err(format!(
                    "Zone {uuid} needs a positive minimum width and spoke width"
                ));
            }
            if zone.clearance < Nm::ZERO || zone.thermal_gap < Nm::ZERO {
                return Err(format!("Zone {uuid} has a negative clearance or gap"));
            }
        }
        for keepout in self.keepouts.values() {
            let uuid = keepout.uuid;
            if keepout.layers.is_empty() || !keepout.layers.iter().all(|l| is_copper(*l)) {
                return Err(format!(
                    "Keepout {uuid} must apply to one or more copper Layers"
                ));
            }
            if !keepout.copper && !keepout.vias {
                return Err(format!("Keepout {uuid} forbids nothing"));
            }
            reda_geom::check_contour(&keepout.vertices)
                .map_err(|e| format!("Keepout {uuid}: {e}"))?;
        }
        Ok(())
    }

    fn check_free_pads(&self) -> Result<(), String> {
        for pad in self.free_pads.values() {
            if let Some((_, problem)) = pad.problem() {
                return Err(format!("Free Pad {}: {problem}", pad.uuid));
            }
            if let Some(net) = pad.net
                && !self.nets.contains_key(&net)
            {
                return Err(format!(
                    "Free Pad {} is on Net {net}, which does not exist",
                    pad.uuid
                ));
            }
        }
        Ok(())
    }

    fn check_footprints(&self) -> Result<(), String> {
        let mut keys = std::collections::BTreeSet::new();
        for copy in self.footprint_copies.values() {
            for pad in &copy.footprint.pads {
                if let Some((field, problem)) = crate::opening_problem(
                    pad.kind,
                    pad.side,
                    pad.mask.as_ref(),
                    pad.paste.as_ref(),
                ) {
                    return Err(format!("Pad {}{field}: {problem}", pad.uuid));
                }
            }
            if !keys.insert((copy.footprint.uuid, copy.hash.as_str())) {
                return Err(format!(
                    "the PCB holds Footprint {} with hash {} twice",
                    copy.footprint.uuid, copy.hash
                ));
            }
        }
        let mut components = std::collections::BTreeSet::new();
        for instance in self.footprints.values() {
            let uuid = instance.uuid;
            if let Some(component) = &instance.component
                && !components.insert(component)
            {
                return Err(format!(
                    "two Footprint instances show Component {}",
                    component.uuid
                ));
            }
            let Some(copy) = self.footprint_copies.get(&instance.footprint) else {
                return Err(format!(
                    "Footprint instance {uuid} names Footprint copy {}, which the PCB does \
                     not hold",
                    instance.footprint
                ));
            };
            let mut pads: Vec<Uuid> = copy.footprint.pads.iter().map(|p| p.uuid).collect();
            pads.sort();
            if !instance.pads.iter().map(|p| p.pad).eq(pads) {
                return Err(format!(
                    "Footprint instance {uuid} must list each Pad of its Footprint once, \
                     sorted by UUID"
                ));
            }
            if let Some(net) = instance
                .pads
                .iter()
                .filter_map(|p| p.net)
                .find(|n| !self.nets.contains_key(n))
            {
                return Err(format!(
                    "Footprint instance {uuid} has a Pad on Net {net}, which does not exist"
                ));
            }
        }
        Ok(())
    }
}

/// Serialised as its objects in UUID order, for fingerprinting what a check reads.
impl serde::Serialize for PcbDocument {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(self.objects())
    }
}
