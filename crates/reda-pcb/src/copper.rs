//! Copper connectivity: which pieces of copper on the board touch, kept in an `rstar`
//! index per copper Layer and updated from each transaction's change set.
//!
//! The copper is every Pad of every placed Footprint and every Free Pad (non-plated Pads
//! have none), every Track, every Via, free text and graphics on copper Layers, and,
//! once handed over with [`Copper::set_zone_fills`], every island of the Zone fills.
//! Copper on one Layer
//! connects where it touches or overlaps, except free text and graphics, which cannot
//! form connections. A Via and a plated through-hole Pad connect their copper on every
//! Layer they reach.
//!
//! [`Copper`] answers:
//! - what copper touches what ([`Copper::contacts`]) and which copper is physically
//!   connected to an object ([`Copper::connected`], "select connected"),
//! - the copper and Net under a point ([`Copper::items_at`], [`Copper::net_at`]),
//! - the islands each Net's copper forms and the Ratsnest that joins them
//!   ([`Copper::ratsnest`]),
//! - whether proposed copper keeps its clearance ([`Copper::clearance`]).

use shape::BoardShape as _;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

use crate::placed::{PlacedPad, placed_free_pad, placed_pads};
use reda_command::Fonts;
use reda_model::geom::Point;
use reda_model::{ChangeSet, LayerKind, Object, PadKind, PcbDocument, Uuid, ZoneFills};
use rstar::{AABB, RTree, RTreeObject};
use schemars::JsonSchema;
use serde::Serialize;

mod clearance;
mod lands;
pub use lands::UnusedLands;
mod ratsnest;
pub(crate) mod shape;
pub(crate) mod ties;

pub use clearance::{ClearanceHit, Halo, Proposal};
pub(crate) use clearance::{breaches_clearance, exempt_pair};
pub use ratsnest::{Connections, RatsnestEnds, RatsnestLine, RatsnestVisibility};

use shape::{P, Shape};

/// One piece of copper on the board.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Item {
    /// A Pad of a placed Footprint.
    Pad {
        /// The Footprint instance.
        footprint: Uuid,
        /// The Pad of its Footprint copy.
        pad: Uuid,
    },
    /// A Pad placed on the board on its own.
    FreePad {
        uuid: Uuid,
    },
    Via {
        uuid: Uuid,
    },
    Track {
        uuid: Uuid,
    },
    /// Free text etched on a copper Layer.
    Text {
        uuid: Uuid,
    },
    /// A free line, arc or rectangle etched on a copper Layer.
    Graphic {
        uuid: Uuid,
    },
    /// One island (connected piece) of a Zone's fill, numbered in fill order.
    Zone {
        zone: Uuid,
        island: u32,
    },
}

impl Item {
    /// Free text and graphics have physical copper but can never carry a connection.
    pub fn is_artwork(self) -> bool {
        matches!(self, Self::Text { .. } | Self::Graphic { .. })
    }

    /// The stored object the copper belongs to: the Footprint instance for a Pad.
    pub fn object(&self) -> Uuid {
        match *self {
            Item::Pad { footprint, .. } => footprint,
            Item::FreePad { uuid }
            | Item::Track { uuid }
            | Item::Via { uuid }
            | Item::Text { uuid }
            | Item::Graphic { uuid } => uuid,
            Item::Zone { zone, .. } => zone,
        }
    }
}

/// An item's copper on one Layer.
#[derive(Clone, Debug)]
struct Piece {
    item: Item,
    layer: Uuid,
    shape: Shape,
}

/// An entry of a Layer's index: the slot of a [`Piece`] and its bounding box.
#[derive(Clone, Debug, PartialEq)]
struct Entry {
    slot: usize,
    envelope: AABB<P>,
}

impl RTreeObject for Entry {
    type Envelope = AABB<P>;
    fn envelope(&self) -> AABB<P> {
        self.envelope
    }
}

/// What an item is: its Net and where its centre or ends are, for the Ratsnest.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Info {
    net: Option<Uuid>,
    anchors: Vec<Point>,
}

/// The board's copper and how it connects. Build it once with [`Copper::build`], then
/// keep it current with [`Copper::apply`] after each committed change set.
#[derive(Clone, Debug)]
pub struct Copper {
    unused: UnusedLands,
    fonts: Fonts,
    pieces: Vec<Option<Piece>>,
    free: Vec<usize>,
    index: BTreeMap<Uuid, RTree<Entry>>,
    slots: BTreeMap<Item, Vec<usize>>,
    info: BTreeMap<Item, Info>,
    contacts: BTreeMap<Item, BTreeSet<Item>>,
    pads: BTreeMap<Uuid, Vec<Item>>,
    /// The Zone fills the index holds, kept to re-index a Zone after a change.
    fills: Option<ZoneFills>,
}

impl Copper {
    /// The copper of `project`'s PCB Document with the Zone fills the Project holds for it
    /// (stale ones too: they stay copper until the next fill), or `None` without a PCB
    /// Document. The index is kept in the Project and built again only after the board or
    /// its fills change, so callers can ask for it on every query.
    pub fn of(project: &reda_model::Project) -> Option<std::sync::Arc<Copper>> {
        let pcb = project.pcb()?;
        Some(project.derived(pcb.uuid, || {
            Self::with_fills(project, pcb, &Fonts::default())
        }))
    }

    /// The cached copper of a Project, including text in the supplied installed fonts.
    pub fn of_with_fonts(project: &reda_model::Project, fonts: &Fonts) -> Option<Arc<Copper>> {
        if fonts.families().next().is_none() {
            return Self::of(project);
        }
        let pcb = project.pcb()?;
        let cache = project.derived(pcb.uuid, Mutex::<Option<Arc<Copper>>>::default);
        let mut slot = cache.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(copper) = slot.as_ref().filter(|c| c.fonts.same_faces(fonts)) {
            return Some(copper.clone());
        }
        let copper = Arc::new(Self::with_fills(project, pcb, fonts));
        *slot = Some(copper.clone());
        Some(copper)
    }

    fn with_fills(project: &reda_model::Project, pcb: &PcbDocument, fonts: &Fonts) -> Copper {
        let mut copper = Copper::build_with_fonts(pcb, fonts);
        if let Some(fills) = project.zone_fills(pcb.uuid) {
            copper.set_zone_fills(pcb, Some(fills));
        }
        copper
    }

    /// The copper of `pcb`, indexed from scratch.
    pub fn build(pcb: &PcbDocument) -> Copper {
        Self::build_with_fonts(pcb, &Fonts::default())
    }

    /// The copper of `pcb`, with free text in the fonts used for plotting.
    pub fn build_with_fonts(pcb: &PcbDocument, fonts: &Fonts) -> Copper {
        Self::with_lands(pcb, fonts, UnusedLands::of(pcb))
    }

    pub(crate) fn with_lands(pcb: &PcbDocument, fonts: &Fonts, unused: UnusedLands) -> Copper {
        let mut copper = Copper {
            unused,
            fonts: fonts.clone(),
            pieces: Vec::new(),
            free: Vec::new(),
            index: BTreeMap::new(),
            slots: BTreeMap::new(),
            info: BTreeMap::new(),
            contacts: BTreeMap::new(),
            pads: BTreeMap::new(),
            fills: None,
        };
        for instance in pcb.footprints() {
            copper.add_footprint(pcb, instance.uuid);
        }
        for pad in pcb.free_pads() {
            copper.add_free_pad(pcb, pad.uuid);
        }
        for track in pcb.tracks() {
            copper.add_track(pcb, track.uuid);
        }
        for via in pcb.vias() {
            copper.add_via(pcb, via.uuid);
        }
        for text in pcb.texts() {
            copper.add_artwork(pcb, Item::Text { uuid: text.uuid });
        }
        for graphic in pcb.graphics() {
            copper.add_artwork(pcb, Item::Graphic { uuid: graphic.uuid });
        }
        copper
    }

    /// Brings the index up to date with a change set committed to the Project that `pcb`
    /// belongs to (`pcb` is the PCB Document after it). Only the copper the change set
    /// touches is re-indexed: a Footprint instance that moved, turned, flipped or changed
    /// a Pad's Net has all its Pads re-indexed. A change to the Layers re-indexes
    /// everything.
    pub fn apply(&mut self, pcb: &PcbDocument, changes: &ChangeSet) {
        let mut footprints = BTreeSet::new();
        let mut free_pads = BTreeSet::new();
        let mut tracks = BTreeSet::new();
        let mut vias = BTreeSet::new();
        let mut zones = BTreeSet::new();
        let mut artwork = Vec::new();
        for (uuid, change) in changes.iter() {
            let object = change.after.as_ref().or(change.before.as_ref());
            match object {
                Some(Object::LayerSet(_)) => {
                    let fills = self.fills.take();
                    *self = Copper::build_with_fonts(pcb, &self.fonts);
                    self.set_zone_fills(pcb, fills.as_ref());
                    return;
                }
                Some(Object::PcbText(_)) => artwork.push(Item::Text { uuid: *uuid }),
                Some(Object::PcbGraphic(_)) => artwork.push(Item::Graphic { uuid: *uuid }),
                Some(Object::Zone(_)) => {
                    zones.insert(*uuid);
                }
                Some(Object::FootprintInstance(_)) => {
                    footprints.insert(*uuid);
                }
                Some(Object::FootprintCopy(_)) => {
                    footprints.extend(
                        pcb.footprints()
                            .filter(|f| f.footprint == *uuid)
                            .map(|f| f.uuid),
                    );
                }
                Some(Object::FreePad(_)) => {
                    free_pads.insert(*uuid);
                }
                Some(Object::Track(_)) => {
                    tracks.insert(*uuid);
                }
                Some(Object::Via(_)) => {
                    vias.insert(*uuid);
                }
                _ => {}
            }
        }
        // An edit to a Track end, Zone outline or its connection rules can also
        // change a land on a different object. Re-index just the affected owners.
        let unused = UnusedLands::of(pcb);
        for &(item, _) in self.unused.0.symmetric_difference(&unused.0) {
            match item {
                Item::Pad { footprint, .. } => {
                    footprints.insert(footprint);
                }
                Item::FreePad { uuid } => {
                    free_pads.insert(uuid);
                }
                Item::Via { uuid } => {
                    vias.insert(uuid);
                }
                _ => unreachable!("only Pads and Vias have unused lands"),
            }
        }
        self.unused = unused;
        for footprint in footprints {
            for item in self.pads.remove(&footprint).unwrap_or_default() {
                self.remove(item);
            }
            self.add_footprint(pcb, footprint);
        }
        for uuid in free_pads {
            self.remove(Item::FreePad { uuid });
            self.add_free_pad(pcb, uuid);
        }
        for uuid in tracks {
            self.remove(Item::Track { uuid });
            self.add_track(pcb, uuid);
        }
        for uuid in vias {
            self.remove(Item::Via { uuid });
            self.add_via(pcb, uuid);
        }
        for item in artwork {
            self.remove(item);
            self.add_artwork(pcb, item);
        }
        for uuid in zones {
            self.remove_zone(uuid);
            self.add_zone(pcb, uuid);
        }
    }

    /// Hands the index the Zone fills to treat as copper (`None` drops them). A fill of a
    /// Zone that no longer exists, or is shelved, is skipped; each island takes its Zone's current Net and
    /// Layer. [`Copper::apply`] keeps them in step with later changes to the Zones.
    pub fn set_zone_fills(&mut self, pcb: &PcbDocument, fills: Option<&ZoneFills>) {
        let old: Vec<Uuid> = self
            .fills
            .as_ref()
            .map(|f| f.zones.keys().copied().collect())
            .unwrap_or_default();
        for zone in old {
            self.remove_zone(zone);
        }
        self.fills = fills.cloned();
        let zones: Vec<Uuid> = self
            .fills
            .as_ref()
            .map(|f| f.zones.keys().copied().collect())
            .unwrap_or_default();
        for zone in zones {
            self.add_zone(pcb, zone);
        }
    }

    /// The Pad and Via lands omitted from this index.
    pub fn unused_lands(&self) -> &UnusedLands {
        &self.unused
    }

    /// Every piece of copper, sorted.
    pub fn items(&self) -> impl Iterator<Item = Item> + '_ {
        self.info.keys().copied()
    }

    /// The Net an item is on, `None` for no Net or an unknown item.
    pub fn net(&self, item: Item) -> Option<Uuid> {
        self.info.get(&item).and_then(|i| i.net)
    }

    /// The copper an item touches, sorted.
    pub fn contacts(&self, item: Item) -> impl Iterator<Item = Item> + '_ {
        self.contacts.get(&item).into_iter().flatten().copied()
    }

    /// Every pair of touching items `(a, b)` with `a < b`, sorted. A pair on different
    /// Nets is a short.
    pub fn touching_pairs(&self) -> Vec<(Item, Item)> {
        self.contacts
            .iter()
            .flat_map(|(a, bs)| bs.iter().filter(move |b| *a < **b).map(move |b| (*a, *b)))
            .collect()
    }

    /// All the copper physically connected to `item` (itself included), whatever its Net,
    /// sorted: what "select connected" selects.
    pub fn connected(&self, item: Item) -> Vec<Item> {
        if !self.info.contains_key(&item) {
            return Vec::new();
        }
        self.flood(item, |_| true).into_iter().collect()
    }

    /// The pieces of copper on `layer` under `point`, sorted.
    pub fn items_at(&self, layer: Uuid, point: Point) -> Vec<Item> {
        let probe = Shape::disc(point, reda_model::geom::Nm::ZERO);
        let mut found: Vec<Item> = self
            .candidates(layer, probe.envelope())
            .filter(|piece| piece.shape.contains(point))
            .map(|piece| piece.item)
            .collect();
        found.sort();
        found.dedup();
        found
    }

    /// The Net of the copper on `layer` under `point`: Pads first, then Free Pads, then
    /// Vias, then Tracks, the first on a Net.
    pub fn net_at(&self, layer: Uuid, point: Point) -> Option<Uuid> {
        self.items_at(layer, point)
            .into_iter()
            .find_map(|item| self.net(item))
    }

    /// The Track segments of the whole Track `track` is part of: neighbours on its Layer
    /// joined end to end, up to a Pad, a Via or a branch. Empty for an unknown Track.
    pub fn whole_track(&self, track: Uuid) -> Vec<Uuid> {
        let start = Item::Track { uuid: track };
        let Some(&first) = self.slots.get(&start).and_then(|s| s.first()) else {
            return Vec::new();
        };
        let layer = self.piece(first).layer;
        let mut seen = BTreeSet::from([start]);
        let mut todo = vec![start];
        while let Some(item) = todo.pop() {
            for end in &self.info[&item].anchors {
                let others: Vec<Item> = self
                    .items_at(layer, *end)
                    .into_iter()
                    .filter(|i| *i != item && !i.is_artwork() && !matches!(i, Item::Zone { .. }))
                    .collect();
                if let [next @ Item::Track { .. }] = others[..]
                    && self.info[&next].anchors.contains(end)
                    && seen.insert(next)
                {
                    todo.push(next);
                }
            }
        }
        seen.into_iter().map(|i| i.object()).collect()
    }

    /// The islands of `net`: groups of its copper that touch each other, through copper
    /// on the same Net only. Each island is sorted; islands are in order of their first
    /// item.
    pub fn islands(&self, net: Uuid) -> Vec<Vec<Item>> {
        let mut left: BTreeSet<Item> = self
            .info
            .iter()
            .filter(|(_, i)| i.net == Some(net))
            .map(|(item, _)| *item)
            .collect();
        let mut islands = Vec::new();
        while let Some(start) = left.pop_first() {
            let island = self.flood(start, |i| self.net(i) == Some(net));
            left.retain(|i| !island.contains(i));
            islands.push(island.into_iter().collect());
        }
        islands
    }

    /// The Nets any copper is on, sorted.
    pub fn nets(&self) -> BTreeSet<Uuid> {
        self.info.values().filter_map(|i| i.net).collect()
    }

    fn flood(&self, start: Item, keep: impl Fn(Item) -> bool) -> BTreeSet<Item> {
        let mut seen = BTreeSet::from([start]);
        let mut todo = vec![start];
        while let Some(item) = todo.pop() {
            for next in self.contacts(item) {
                if keep(next) && seen.insert(next) {
                    todo.push(next);
                }
            }
        }
        seen
    }

    /// Every piece of copper on `layer` with its shape, in no particular order.
    pub(crate) fn pieces_on(&self, layer: Uuid) -> impl Iterator<Item = (Item, &Shape)> + '_ {
        self.index
            .get(&layer)
            .into_iter()
            .flat_map(|tree| tree.iter())
            .map(|entry| {
                let piece = self.piece(entry.slot);
                (piece.item, &piece.shape)
            })
    }

    /// Indexed pieces whose bounding boxes lie within `reach` of a physical shape's box.
    pub(crate) fn pieces_near(
        &self,
        layer: Uuid,
        shape: &Shape,
        reach: reda_model::geom::Nm,
    ) -> impl Iterator<Item = (Item, &Shape)> {
        let (lo, hi) = shape.bounds();
        let search = AABB::from_corners(
            [lo[0] - reach.0, lo[1] - reach.0],
            [hi[0] + reach.0, hi[1] + reach.0],
        );
        self.candidates(layer, search).map(|p| (p.item, &p.shape))
    }

    /// The copper of `item`, one shape per Layer it is on.
    pub(crate) fn item_pieces(&self, item: Item) -> Vec<(Uuid, &Shape)> {
        self.slots
            .get(&item)
            .into_iter()
            .flatten()
            .map(|&slot| {
                let piece = self.piece(slot);
                (piece.layer, &piece.shape)
            })
            .collect()
    }

    /// The items that can form a connection and touch `shape` on `layer`, sorted.
    pub(crate) fn items_touching(&self, layer: Uuid, shape: &Shape) -> Vec<Item> {
        let mut found: Vec<Item> = self
            .candidates(layer, shape.envelope())
            .filter(|piece| !piece.item.is_artwork() && piece.shape.touches(shape))
            .map(|piece| piece.item)
            .collect();
        found.sort();
        found.dedup();
        found
    }

    /// Whether `item`'s copper on `layer` touches any other copper there.
    pub(crate) fn touches_on(&self, item: Item, layer: Uuid) -> bool {
        self.item_pieces(item)
            .into_iter()
            .filter(|(l, _)| *l == layer)
            .any(|(_, shape)| {
                self.candidates(layer, shape.envelope()).any(|other| {
                    !other.item.is_artwork() && other.item != item && other.shape.touches(shape)
                })
            })
    }

    fn piece(&self, slot: usize) -> &Piece {
        self.pieces[slot].as_ref().expect("an indexed slot")
    }

    fn candidates(&self, layer: Uuid, envelope: AABB<P>) -> impl Iterator<Item = &Piece> + '_ {
        self.index
            .get(&layer)
            .into_iter()
            .flat_map(move |tree| tree.locate_in_envelope_intersecting(envelope))
            .map(|entry| self.piece(entry.slot))
    }

    fn add_footprint(&mut self, pcb: &PcbDocument, uuid: Uuid) {
        let Some(instance) = pcb.footprint(uuid) else {
            return;
        };
        let mut items = Vec::new();
        for pad in placed_pads(pcb, instance) {
            let item = Item::Pad {
                footprint: uuid,
                pad: pad.pad,
            };
            if pad.kind != PadKind::NonPlated {
                self.add_pad(pcb, item, &pad);
                items.push(item);
            }
        }
        self.pads.insert(uuid, items);
    }

    fn add_free_pad(&mut self, pcb: &PcbDocument, uuid: Uuid) {
        if let Some(pad) = pcb.free_pad(uuid).filter(|p| p.kind != PadKind::NonPlated) {
            self.add_pad(pcb, Item::FreePad { uuid }, &placed_free_pad(pad));
        }
    }

    fn add_pad(&mut self, pcb: &PcbDocument, item: Item, pad: &PlacedPad) {
        let info = Info {
            net: pad.net,
            anchors: vec![pad.at],
        };
        let pieces = pcb
            .copper_layers()
            .filter(|l| pad.side.is_none() || pad.side == l.side)
            .filter(|l| !self.unused.contains(item, l.uuid))
            .map(|l| (l.uuid, lands::pad_shape(pad, l.side)))
            .collect::<Vec<_>>();
        self.insert(item, info, pieces);
    }

    fn add_artwork(&mut self, pcb: &PcbDocument, item: Item) {
        let on_copper = |layer| {
            pcb.layers()
                .layer(layer)
                .is_some_and(|l| l.kind == LayerKind::Copper)
        };
        let (layer, shape) = match item {
            Item::Text { uuid } => {
                let Some(t) = pcb.text(uuid).filter(|t| on_copper(t.layer)) else {
                    return;
                };
                (t.layer, crate::text_geometry(t, &self.fonts).shape())
            }
            Item::Graphic { uuid } => {
                let Some(g) = pcb.graphic(uuid).filter(|g| on_copper(g.layer)) else {
                    return;
                };
                (g.layer, shape::graphic_shape(&g.shape, g.width))
            }
            _ => unreachable!("expected free text or a graphic"),
        };
        if !shape.prims.is_empty() {
            self.insert(
                item,
                Info {
                    net: None,
                    anchors: Vec::new(),
                },
                [(layer, shape)],
            );
        }
    }

    fn add_track(&mut self, pcb: &PcbDocument, uuid: Uuid) {
        let Some(track) = pcb.track(uuid) else {
            return;
        };
        let (from, to) = track.geometry.ends();
        let info = Info {
            net: track.net,
            anchors: vec![from, to],
        };
        let shape = Shape::track(&track.geometry, track.width);
        self.insert(Item::Track { uuid }, info, [(track.layer, shape)]);
    }

    fn add_via(&mut self, pcb: &PcbDocument, uuid: Uuid) {
        let Some(via) = pcb.via(uuid) else {
            return;
        };
        let info = Info {
            net: via.net,
            anchors: vec![via.at],
        };
        let item = Item::Via { uuid };
        let pieces = pcb
            .span_layers(via.span)
            .into_iter()
            .filter(|l| !self.unused.contains(item, *l))
            .map(|l| (l, Shape::disc(via.at, via.diameter_on(l))))
            .collect::<Vec<_>>();
        if !pieces.is_empty() {
            self.insert(item, info, pieces);
        }
    }

    fn add_zone(&mut self, pcb: &PcbDocument, uuid: Uuid) {
        let (Some(zone), Some(shapes)) = (
            pcb.zone(uuid).filter(|z| !z.shelved),
            self.fills.as_ref().and_then(|f| f.zones.get(&uuid)),
        ) else {
            return;
        };
        let pieces: Vec<(Info, Shape)> = shapes
            .shapes
            .iter()
            .map(|s| {
                // Up to 32 points along the outer edge anchor the Ratsnest.
                let step = s.outer.len().div_ceil(32).max(1);
                let anchors = s.outer.iter().step_by(step).copied().collect();
                (
                    Info {
                        net: zone.net,
                        anchors,
                    },
                    crate::zone::shape_copper(s),
                )
            })
            .collect();
        let layer = zone.layer;
        for (island, (info, shape)) in pieces.into_iter().enumerate() {
            let item = Item::Zone {
                zone: uuid,
                island: island as u32,
            };
            self.insert(item, info, [(layer, shape)]);
        }
    }

    fn remove_zone(&mut self, uuid: Uuid) {
        let islands: Vec<Item> = self
            .slots
            .range(
                Item::Zone {
                    zone: uuid,
                    island: 0,
                }..=Item::Zone {
                    zone: uuid,
                    island: u32::MAX,
                },
            )
            .map(|(item, _)| *item)
            .collect();
        for item in islands {
            self.remove(item);
        }
    }

    fn insert(&mut self, item: Item, info: Info, pieces: impl IntoIterator<Item = (Uuid, Shape)>) {
        let mut touching = BTreeSet::new();
        let mut slots = Vec::new();
        for (layer, shape) in pieces {
            let envelope = shape.envelope();
            if !item.is_artwork() {
                touching.extend(
                    self.candidates(layer, envelope)
                        .filter(|other| {
                            !other.item.is_artwork()
                                && other.item != item
                                && other.shape.touches(&shape)
                        })
                        .map(|other| other.item),
                );
            }
            let piece = Piece { item, layer, shape };
            let slot = match self.free.pop() {
                Some(slot) => {
                    self.pieces[slot] = Some(piece);
                    slot
                }
                None => {
                    self.pieces.push(Some(piece));
                    self.pieces.len() - 1
                }
            };
            self.index
                .entry(layer)
                .or_default()
                .insert(Entry { slot, envelope });
            slots.push(slot);
        }
        for other in &touching {
            self.contacts.entry(*other).or_default().insert(item);
        }
        self.contacts.insert(item, touching);
        self.slots.insert(item, slots);
        self.info.insert(item, info);
    }

    fn remove(&mut self, item: Item) {
        for slot in self.slots.remove(&item).unwrap_or_default() {
            let piece = self.pieces[slot].take().expect("an indexed slot");
            let entry = Entry {
                slot,
                envelope: piece.shape.envelope(),
            };
            if let Some(tree) = self.index.get_mut(&piece.layer) {
                tree.remove(&entry);
            }
            self.free.push(slot);
        }
        for other in self.contacts.remove(&item).unwrap_or_default() {
            if let Some(set) = self.contacts.get_mut(&other) {
                set.remove(&item);
            }
        }
        self.info.remove(&item);
    }
}

impl PartialEq for Copper {
    /// Two indexes are equal when they hold the same copper with the same contacts, however
    /// they were built.
    fn eq(&self, other: &Copper) -> bool {
        self.info == other.info
            && self.contacts == other.contacts
            && self.layers() == other.layers()
    }
}

impl Copper {
    fn layers(&self) -> BTreeMap<Item, Vec<(Uuid, Shape)>> {
        self.slots
            .iter()
            .map(|(item, slots)| {
                let mut pieces: Vec<(Uuid, Shape)> = slots
                    .iter()
                    .map(|s| (self.piece(*s).layer, self.piece(*s).shape.clone()))
                    .collect();
                pieces.sort_by_key(|p| p.0);
                (*item, pieces)
            })
            .collect()
    }
}
