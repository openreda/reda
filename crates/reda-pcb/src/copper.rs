//! Copper connectivity: which pieces of copper on the board touch, kept in an `rstar`
//! index per copper Layer and updated from each transaction's change set.
//!
//! The copper is every Pad of every placed Footprint (non-plated Pads have none), every
//! Track and every Via. Copper on one Layer connects where it touches or overlaps; a Via
//! and a plated through-hole Pad connect their copper on every Layer they reach.
//!
//! [`Copper`] answers:
//! - what copper touches what ([`Copper::contacts`]) and which copper is physically
//!   connected to an object ([`Copper::connected`], "select connected"),
//! - the copper and Net under a point ([`Copper::items_at`], [`Copper::net_at`]),
//! - the islands each Net's copper forms and the Ratsnest that joins them
//!   ([`Copper::ratsnest`]),
//! - whether proposed copper keeps its clearance ([`Copper::clearance`]).

use std::collections::{BTreeMap, BTreeSet};

use reda_model::geom::Point;
use reda_model::{ChangeSet, Object, PadKind, PcbDocument, Side, Uuid};
use rstar::{AABB, RTree, RTreeObject};
use schemars::JsonSchema;
use serde::Serialize;

mod clearance;
mod ratsnest;
mod shape;

pub use clearance::{ClearanceHit, Proposal};
pub use ratsnest::{RatsnestLine, RatsnestVisibility};

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
    Via {
        uuid: Uuid,
    },
    Track {
        uuid: Uuid,
    },
}

impl Item {
    /// The stored object the copper belongs to: the Footprint instance for a Pad.
    pub fn object(&self) -> Uuid {
        match *self {
            Item::Pad { footprint, .. } => footprint,
            Item::Track { uuid } | Item::Via { uuid } => uuid,
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
    pieces: Vec<Option<Piece>>,
    free: Vec<usize>,
    index: BTreeMap<Uuid, RTree<Entry>>,
    slots: BTreeMap<Item, Vec<usize>>,
    info: BTreeMap<Item, Info>,
    contacts: BTreeMap<Item, BTreeSet<Item>>,
    pads: BTreeMap<Uuid, Vec<Item>>,
}

impl Copper {
    /// The copper of `pcb`, indexed from scratch.
    pub fn build(pcb: &PcbDocument) -> Copper {
        let mut copper = Copper {
            pieces: Vec::new(),
            free: Vec::new(),
            index: BTreeMap::new(),
            slots: BTreeMap::new(),
            info: BTreeMap::new(),
            contacts: BTreeMap::new(),
            pads: BTreeMap::new(),
        };
        for instance in pcb.footprints() {
            copper.add_footprint(pcb, instance.uuid);
        }
        for track in pcb.tracks() {
            copper.add_track(pcb, track.uuid);
        }
        for via in pcb.vias() {
            copper.add_via(pcb, via.uuid);
        }
        copper
    }

    /// Brings the index up to date with a change set committed to the Project that `pcb`
    /// belongs to (`pcb` is the PCB Document after it). Only the copper the change set
    /// touches is re-indexed; a change to the Layers re-indexes everything.
    pub fn apply(&mut self, pcb: &PcbDocument, changes: &ChangeSet) {
        let mut footprints = BTreeSet::new();
        let mut tracks = BTreeSet::new();
        let mut vias = BTreeSet::new();
        for (uuid, change) in changes.iter() {
            let object = change.after.as_ref().or(change.before.as_ref());
            match object {
                Some(Object::LayerSet(_)) => {
                    *self = Copper::build(pcb);
                    return;
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
                Some(Object::Track(_)) => {
                    tracks.insert(*uuid);
                }
                Some(Object::Via(_)) => {
                    vias.insert(*uuid);
                }
                _ => {}
            }
        }
        for footprint in footprints {
            for item in self.pads.remove(&footprint).unwrap_or_default() {
                self.remove(item);
            }
            self.add_footprint(pcb, footprint);
        }
        for uuid in tracks {
            self.remove(Item::Track { uuid });
            self.add_track(pcb, uuid);
        }
        for uuid in vias {
            self.remove(Item::Via { uuid });
            self.add_via(pcb, uuid);
        }
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

    /// The Net of the copper on `layer` under `point`: Pads first, then Vias, then
    /// Tracks, the first on a Net.
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
                    .filter(|i| *i != item)
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
        let Some(copy) = pcb.footprint_copy(instance.footprint) else {
            return;
        };
        let copper: Vec<Uuid> = pcb.copper_layers().map(|l| l.uuid).collect();
        let outer = match instance.side {
            Side::Top => pcb.copper_layers().find(|l| l.side == Some(Side::Top)),
            Side::Bottom => pcb.copper_layers().find(|l| l.side == Some(Side::Bottom)),
        }
        .map(|l| l.uuid);
        let mut items = Vec::new();
        for pad in &copy.footprint.pads {
            let layers = match pad.kind {
                PadKind::NonPlated => continue,
                PadKind::PlatedThroughHole => copper.clone(),
                PadKind::Smd => outer.into_iter().collect(),
            };
            let item = Item::Pad {
                footprint: uuid,
                pad: pad.uuid,
            };
            let shape = Shape::pad(instance, pad);
            let centre = shape::board_point(instance, [pad.at.x.0, pad.at.y.0]);
            let info = Info {
                net: instance.pad_net(pad.uuid),
                anchors: vec![Point::new(
                    reda_model::geom::Nm(centre[0]),
                    reda_model::geom::Nm(centre[1]),
                )],
            };
            self.insert(item, info, layers, shape);
            items.push(item);
        }
        self.pads.insert(uuid, items);
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
        self.insert(Item::Track { uuid }, info, vec![track.layer], shape);
    }

    fn add_via(&mut self, pcb: &PcbDocument, uuid: Uuid) {
        let Some(via) = pcb.via(uuid) else {
            return;
        };
        let info = Info {
            net: via.net,
            anchors: vec![via.at],
        };
        let shape = Shape::disc(via.at, via.diameter);
        self.insert(Item::Via { uuid }, info, pcb.span_layers(via.span), shape);
    }

    fn insert(&mut self, item: Item, info: Info, layers: Vec<Uuid>, shape: Shape) {
        let envelope = shape.envelope();
        let mut touching = BTreeSet::new();
        let mut slots = Vec::new();
        for layer in layers {
            touching.extend(
                self.candidates(layer, envelope)
                    .filter(|other| other.item != item && other.shape.touches(&shape))
                    .map(|other| other.item),
            );
            let piece = Piece {
                item,
                layer,
                shape: shape.clone(),
            };
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
