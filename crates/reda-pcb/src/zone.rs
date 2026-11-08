//! Zone fill: computing the copper of every Zone, telling whether stored fills are stale,
//! and the `.reda/` cache's byte format.
//!
//! Zones fill in priority order (highest first, then by UUID); on each Layer a Zone keeps
//! its clearance from the fills already made there by Zones of other Nets. Fills are
//! derived data: the Project keeps them beside the PCB Document
//! ([`reda_model::Project::zone_fills`]), each Zone's with the hash of the inputs it came
//! from ([`input_hashes`]), so an edit makes stale only the Zones it can change.
//!
//! Arcs in outlines and copper are flattened to chords no further than [`ARC_TOLERANCE`]
//! from the true arc.

use crate::copper::shape::BoardShape as _;
use std::collections::{BTreeMap, BTreeSet};

use reda_model::geom::Nm;
use reda_model::{PcbDocument, Uuid, Zone, ZoneFill, ZoneFills};

mod cache;
pub(crate) mod fill;
mod merge;
mod names;
pub(crate) mod poly;

pub use cache::{decode, encode};
pub(crate) use fill::shape_copper;
pub use merge::{LayerFill, layer_fills};
pub use names::{NO_NET, name, names};

use crate::copper::shape::Shape;
use crate::copper::{Copper, Proposal};

/// Bumped whenever the fill algorithm changes, so fills made by an older REDA are stale.
const FILL_VERSION: u32 = 2;

/// How far a chord of a flattened arc may stray from the arc: a fixed 0.001 mm. It only
/// trades size against fidelity, far below what a fab resolves, so it is no setting.
pub const ARC_TOLERANCE: Nm = reda_model::geom::grown::ARC_TOLERANCE;

/// `pcb`'s unshelved Zones in fill order: highest priority first, then by UUID.
pub fn fill_order(pcb: &PcbDocument) -> Vec<&Zone> {
    let mut zones: Vec<&Zone> = pcb.active_zones().collect();
    zones.sort_by(pour_order);
    zones
}

/// Every Zone of `pcb`, shelved ones too, in fill order.
pub(crate) fn fill_order_all(pcb: &PcbDocument) -> Vec<&Zone> {
    let mut zones: Vec<&Zone> = pcb.zones().collect();
    zones.sort_by(pour_order);
    zones
}

/// The pour order: higher priority first; a tie goes to the lower UUID.
pub fn pour_order(a: &&Zone, b: &&Zone) -> std::cmp::Ordering {
    b.priority.cmp(&a.priority).then(a.uuid.cmp(&b.uuid))
}

/// The Zones on Layer `layer`, shelved ones too, in pour order: the order the Zones
/// panel lists and `pcb.zone.reorder` takes.
pub fn layer_order(pcb: &PcbDocument, layer: Uuid) -> Vec<&Zone> {
    let mut zones: Vec<&Zone> = pcb.zones().filter(|z| z.layer == layer).collect();
    zones.sort_by(pour_order);
    zones
}

/// `zones` and every Zone that depends on them: the lower-priority Zones on the same
/// Layer whose outlines overlap one of them, and so on down.
pub fn dependents(pcb: &PcbDocument, zones: &[Uuid]) -> BTreeSet<Uuid> {
    let order = fill_order(pcb);
    let mut out: BTreeSet<Uuid> = zones.iter().copied().collect();
    let outline_box = |z: &Zone| reda_model::geom::contour_bounds(pcb.zone_vertices(z));
    for (i, zone) in order.iter().enumerate() {
        if !out.contains(&zone.uuid) {
            continue;
        }
        let Some(b) = outline_box(zone) else { continue };
        for lower in &order[i + 1..] {
            if lower.layer == zone.layer
                && outline_box(lower).is_some_and(|l| {
                    l.min.x <= b.max.x
                        && b.min.x <= l.max.x
                        && l.min.y <= b.max.y
                        && b.min.y <= l.max.y
                })
            {
                out.insert(lower.uuid);
            }
        }
    }
    out
}

/// Fills every Zone of `pcb`. `step(done, total)` is called before each Zone (and once
/// at the end); returning `false` cancels, and then `None` is returned.
pub fn fill(pcb: &PcbDocument, step: &mut dyn FnMut(usize, usize) -> bool) -> Option<ZoneFills> {
    refill(pcb, None, step)
}

/// Fills the Zones of `pcb` whose stored fill in `stored` is missing or stale, keeping the
/// current ones. `step` reports and cancels as for [`fill`], counting only the Zones that
/// refill.
pub fn refill(
    pcb: &PcbDocument,
    stored: Option<&ZoneFills>,
    step: &mut dyn FnMut(usize, usize) -> bool,
) -> Option<ZoneFills> {
    refill_only(pcb, stored, None, step)
}

/// Fills the Zones `only` names and those that [depend](dependents) on them, keeping
/// every other Zone's stored fill as it is, current or stale; `None` refills every Zone
/// that is missing or stale, as [`refill`] does. Shelved Zones are never filled.
pub fn refill_only(
    pcb: &PcbDocument,
    stored: Option<&ZoneFills>,
    only: Option<&[Uuid]>,
    step: &mut dyn FnMut(usize, usize) -> bool,
) -> Option<ZoneFills> {
    let hashes = input_hashes(pcb);
    let forced = only.map(|only| dependents(pcb, only));
    let current = |zone: &Zone| {
        let kept = stored.and_then(|s| s.zones.get(&zone.uuid));
        match &forced {
            Some(forced) => kept.filter(|_| !forced.contains(&zone.uuid)),
            None => kept.filter(|f| f.inputs == hashes[&zone.uuid]),
        }
    };
    let zones = fill_order(pcb);
    let total = zones.iter().filter(|z| current(z).is_none()).count();
    let copper = (total > 0).then(|| Copper::build(pcb));
    let mut done: Vec<fill::Filled<'_>> = Vec::new();
    let mut out = BTreeMap::new();
    let mut count = 0;
    for zone in zones {
        let area = match current(zone) {
            Some(kept) => {
                out.insert(zone.uuid, kept.clone());
                fill::to_area(&kept.shapes)
            }
            None => {
                if !step(count, total) {
                    return None;
                }
                count += 1;
                let before: Vec<fill::Filled<'_>> = done
                    .iter()
                    .filter(|f| f.zone.layer == zone.layer)
                    .map(|f| fill::Filled {
                        zone: f.zone,
                        area: f.area.clone(),
                    })
                    .collect();
                let copper = copper.as_ref().expect("built when a Zone refills");
                let area = fill::fill_zone(pcb, copper, zone, &before);
                out.insert(
                    zone.uuid,
                    ZoneFill {
                        inputs: hashes[&zone.uuid].clone(),
                        shapes: fill::to_shapes(&area),
                    },
                );
                area
            }
        };
        done.push(fill::Filled { zone, area });
    }
    if !step(total, total) {
        return None;
    }
    Some(ZoneFills { zones: out })
}

/// Each Zone's input hash, by UUID: everything its fill depends on. That is the Zone
/// itself, the Layers, rules, board edge, Net Classes and Nets, Footprints and Free Pads,
/// the Tracks, Vias, Keepouts and Zone Cutouts on its Layer, and the hashes of the
/// higher-priority unshelved Zones on its Layer (whose fills it keeps clear of or merges with). Text, graphics and
/// copper on other Layers are left out, so an edit there leaves the Zone current.
pub fn input_hashes(pcb: &PcbDocument) -> BTreeMap<Uuid, String> {
    let nets: Vec<_> = pcb.nets().map(|n| (n.uuid, &n.classes)).collect();
    let classes: Vec<_> = pcb.net_classes().map(|c| (c.uuid, c.rank)).collect();
    let rules: Vec<_> = pcb.rules().collect();
    let copies: Vec<_> = pcb
        .footprint_copies()
        .map(|c| (c.uuid, &c.footprint.pads))
        .collect();
    let footprints: Vec<_> = pcb
        .footprints()
        .map(|f| (f.uuid, f.footprint, f.at, f.rotation, f.side, &f.pads))
        .collect();
    let free_pads: Vec<_> = pcb.free_pads().collect();
    let cutouts: Vec<_> = pcb.cutouts().collect();
    let zone_cutouts: Vec<_> = pcb.zone_cutouts().collect();
    let mut shared = reda_command::Fingerprint::new();
    shared.add(&FILL_VERSION);
    shared.add(pcb.layers());
    shared.add(&rules);
    shared.add(&pcb.outline());
    shared.add(&cutouts);
    shared.add(&classes);
    shared.add(&nets);
    shared.add(&copies);
    shared.add(&footprints);
    shared.add(&free_pads);
    let shared = shared.finish();

    let mut layers: BTreeMap<Uuid, String> = BTreeMap::new();
    let mut out: BTreeMap<Uuid, String> = BTreeMap::new();
    let mut prior: BTreeMap<Uuid, Vec<String>> = BTreeMap::new();
    for zone in fill_order(pcb) {
        let layer = layers.entry(zone.layer).or_insert_with(|| {
            let tracks: Vec<_> = pcb.tracks().filter(|t| t.layer == zone.layer).collect();
            let vias: Vec<_> = pcb
                .vias()
                .filter(|v| pcb.span_layers(v.span).contains(&zone.layer))
                .collect();
            let keepouts: Vec<_> = pcb
                .keepouts()
                .filter(|k| k.layers.contains(&zone.layer))
                .collect();
            let cutouts: Vec<_> = zone_cutouts
                .iter()
                .filter(|c| c.layer == zone.layer)
                .collect();
            let mut f = reda_command::Fingerprint::new();
            f.add(&shared);
            f.add(&zone.layer);
            f.add(&cutouts);
            f.add(&tracks);
            f.add(&vias);
            f.add(&keepouts);
            f.finish()
        });
        let above = prior.entry(zone.layer).or_default();
        let mut f = reda_command::Fingerprint::new();
        f.add(layer);
        f.add(zone);
        f.add(above);
        let hash = f.finish();
        above.push(hash.clone());
        out.insert(zone.uuid, hash);
    }
    out
}

/// Where a PCB Document's stored fills stand.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, schemars::JsonSchema)]
pub struct FillStatus {
    /// Whether the status bar should say "Zones need refilling": some Zone has no fill
    /// yet or has changed inputs since it was filled.
    pub needs_refill: bool,
    /// Every Zone, sorted by UUID.
    pub zones: Vec<ZoneStatus>,
}

/// One Zone's fill state.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, schemars::JsonSchema)]
pub struct ZoneStatus {
    pub zone: Uuid,
    pub state: FillState,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FillState {
    /// Filled from the board as it is now.
    Filled,
    /// Filled, but something its fill depends on has changed since: the fill is drawn
    /// dimmed until refilled.
    Stale,
    /// Never filled: only the outline is drawn.
    Unfilled,
    /// Shelved: not filled, and hidden from the canvas, until restored.
    Shelved,
}

/// The state of every Zone of `pcb` given the stored `fills`, each judged by its own
/// input hash.
pub fn status(pcb: &PcbDocument, fills: Option<&ZoneFills>) -> FillStatus {
    let hashes = input_hashes(pcb);
    let zones: Vec<ZoneStatus> = pcb
        .zones()
        .map(|z| {
            let state = match fills.and_then(|f| f.zones.get(&z.uuid)) {
                _ if z.shelved => FillState::Shelved,
                Some(f) if f.inputs == hashes[&z.uuid] => FillState::Filled,
                Some(_) => FillState::Stale,
                None => FillState::Unfilled,
            };
            ZoneStatus {
                zone: z.uuid,
                state,
            }
        })
        .collect();
    FillStatus {
        needs_refill: zones
            .iter()
            .any(|z| matches!(z.state, FillState::Stale | FillState::Unfilled)),
        zones,
    }
}

/// The fills to use for `pcb` now: the stored ones that are current, with the missing and
/// stale Zones refilled (never cancelled). DRC and outputs call this so they never see an
/// old fill.
pub fn current(pcb: &PcbDocument, stored: Option<&ZoneFills>) -> ZoneFills {
    refill(pcb, stored, &mut |_, _| true).expect("an uncancelled fill finishes")
}

/// The Keepouts `proposal` would enter, sorted: a Track on a Layer where a Keepout
/// forbids copper, a Via where one forbids Vias or copper on any Layer it spans. Touching
/// the outline counts as entering.
pub fn keepouts_hit(pcb: &PcbDocument, proposal: &Proposal) -> Vec<Uuid> {
    let (shape, layers, is_via) = match *proposal {
        Proposal::Track {
            layer,
            geometry,
            width,
            ..
        } => (Shape::track(&geometry, width), vec![layer], false),
        Proposal::Via { at, diameter, .. } => (
            Shape::disc(at, diameter),
            pcb.copper_layers().map(|l| l.uuid).collect(),
            true,
        ),
    };
    keepouts_entered(pcb, &shape, &layers, is_via)
}

/// The Keepouts that copper `shape` on `layers` enters, sorted: those forbidding copper on
/// any of the Layers, and for a Via (`is_via`) also those forbidding Vias.
pub(crate) fn keepouts_entered(
    pcb: &PcbDocument,
    shape: &Shape,
    layers: &[Uuid],
    is_via: bool,
) -> Vec<Uuid> {
    let mut copper = None;
    pcb.keepouts()
        .filter(|k| {
            let forbids =
                (k.copper || (is_via && k.vias)) && k.layers.iter().any(|l| layers.contains(l));
            forbids && {
                let copper = copper.get_or_insert_with(|| poly::union(poly::grown(shape, 0)));
                let area = poly::polygon(poly::contour(&k.vertices));
                poly::overlaps(&area, copper)
            }
        })
        .map(|k| k.uuid)
        .collect()
}
