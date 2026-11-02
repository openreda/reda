//! Zone fill: computing the copper of every Zone, telling whether stored fills are stale,
//! and the `.reda/` cache's byte format.
//!
//! Zones fill in priority order (highest first, then by UUID); on each Layer a Zone keeps
//! its clearance from the fills already made there by Zones of other Nets. Fills are
//! derived data: the Project keeps them beside the PCB Document
//! ([`reda_model::Project::zone_fills`]) together with the hash of the inputs they came
//! from ([`inputs_hash`]), so an edit that changes any input makes them stale.

use std::collections::BTreeMap;

use reda_model::{PcbDocument, Uuid, Zone, ZoneFills};

mod cache;
mod fill;
mod poly;

pub use cache::{decode, encode};
pub(crate) use fill::shape_copper;

use crate::copper::Copper;

/// Bumped whenever the fill algorithm changes, so fills made by an older REDA are stale.
const FILL_VERSION: u32 = 1;

/// Fills every Zone of `pcb`. `step(done, total)` is called before each Zone (and once
/// at the end); returning `false` cancels, and then `None` is returned.
pub fn fill(pcb: &PcbDocument, step: &mut dyn FnMut(usize, usize) -> bool) -> Option<ZoneFills> {
    let copper = Copper::build(pcb);
    let mut zones: Vec<&Zone> = pcb.zones().collect();
    zones.sort_by(|a, b| b.priority.cmp(&a.priority).then(a.uuid.cmp(&b.uuid)));
    let total = zones.len();
    let mut done: Vec<fill::Filled<'_>> = Vec::new();
    for (i, zone) in zones.iter().enumerate() {
        if !step(i, total) {
            return None;
        }
        let before: Vec<fill::Filled<'_>> = done
            .iter()
            .filter(|f| f.zone.layer == zone.layer)
            .map(|f| fill::Filled {
                zone: f.zone,
                area: f.area.clone(),
            })
            .collect();
        let area = fill::fill_zone(pcb, &copper, zone, &before);
        done.push(fill::Filled { zone, area });
    }
    if !step(total, total) {
        return None;
    }
    let zones: BTreeMap<Uuid, _> = done
        .iter()
        .map(|f| (f.zone.uuid, fill::to_shapes(&f.area)))
        .collect();
    Some(ZoneFills {
        inputs: inputs_hash(pcb),
        zones,
    })
}

/// The hash of everything on `pcb` that Zone fills depend on: Layers, rules, the board
/// edge, Net Classes and Nets, Footprints, Tracks, Vias, Zones and Keepouts. Text and
/// graphics are left out.
pub fn inputs_hash(pcb: &PcbDocument) -> String {
    let nets: Vec<_> = pcb.nets().map(|n| (n.uuid, n.class)).collect();
    let classes: Vec<_> = pcb.net_classes().map(|c| (c.uuid, c.clearance)).collect();
    let copies: Vec<_> = pcb
        .footprint_copies()
        .map(|c| (c.uuid, &c.footprint.pads))
        .collect();
    let footprints: Vec<_> = pcb
        .footprints()
        .map(|f| (f.uuid, f.footprint, f.at, f.rotation, f.side, &f.pads))
        .collect();
    let tracks: Vec<_> = pcb.tracks().collect();
    let vias: Vec<_> = pcb.vias().collect();
    let zones: Vec<_> = pcb.zones().collect();
    let keepouts: Vec<_> = pcb.keepouts().collect();
    let cutouts: Vec<_> = pcb.cutouts().collect();
    let setup = pcb.setup();
    reda_model::fingerprint(&[
        &FILL_VERSION,
        pcb.layers(),
        &setup.rules,
        &setup.default_net_class,
        &pcb.outline(),
        &cutouts,
        &classes,
        &nets,
        &copies,
        &footprints,
        &tracks,
        &vias,
        &zones,
        &keepouts,
    ])
}

/// Where a PCB Document's stored fills stand.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, schemars::JsonSchema)]
pub struct FillStatus {
    /// Whether the status bar should say "Zones need refilling": some Zone has no fill
    /// yet or the board changed since the fills were made.
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
    /// Filled, but the board has changed since: the fill is drawn dimmed until refilled.
    Stale,
    /// Never filled: only the outline is drawn.
    Unfilled,
}

/// The state of every Zone of `pcb` given the stored `fills`.
pub fn status(pcb: &PcbDocument, fills: Option<&ZoneFills>) -> FillStatus {
    let fresh = fills.is_some_and(|f| f.inputs == inputs_hash(pcb));
    let zones: Vec<ZoneStatus> = pcb
        .zones()
        .map(|z| {
            let state = match fills.map(|f| f.zones.contains_key(&z.uuid)) {
                Some(true) if fresh => FillState::Filled,
                Some(true) => FillState::Stale,
                _ => FillState::Unfilled,
            };
            ZoneStatus {
                zone: z.uuid,
                state,
            }
        })
        .collect();
    FillStatus {
        needs_refill: zones.iter().any(|z| z.state != FillState::Filled),
        zones,
    }
}

/// The fills to use for `pcb` now: the stored ones when they are current, otherwise a
/// fresh fill (never cancelled). DRC and outputs call this so they never see an old fill.
pub fn current(pcb: &PcbDocument, stored: Option<&ZoneFills>) -> ZoneFills {
    match stored {
        Some(f) if f.inputs == inputs_hash(pcb) => f.clone(),
        _ => fill(pcb, &mut |_, _| true).expect("an uncancelled fill finishes"),
    }
}
