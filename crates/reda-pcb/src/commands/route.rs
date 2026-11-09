//! Clearing and tidying routed copper: `pcb.route.unroute` and `pcb.track.cleanup`.

use std::collections::{BTreeMap, BTreeSet};

use reda_command::{
    AvailabilityContext, Command, CommandError, Context, Kind, Meta, Registry, Unavailable,
};
use reda_model::geom::Point;
use reda_model::{InDocument, Object, PcbDocument, Track, TrackGeometry, Uuid};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{board_copper, needs_pcb, pcb};
use crate::copper::{Copper, Item};
use crate::select;

pub(super) fn register_all(registry: &mut Registry) {
    registry.add(Unroute);
    registry.add(TrackCleanup);
}

pub struct Unroute;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UnrouteParams {
    /// Every Track and Via on the board.
    #[serde(default)]
    pub all: bool,
    /// Every Track and Via of these Nets.
    #[serde(default)]
    pub nets: Vec<Uuid>,
    /// A Track or Via: the Tracks and Vias joined to it up to the next Pad, Free Pad or
    /// Zone.
    #[serde(default)]
    pub connection: Option<Uuid>,
    /// The Tracks and Vias joined to these Footprints' Pads, each up to the next Pad.
    #[serde(default)]
    pub footprints: Vec<Uuid>,
    /// Takes locked copper too; without it locked copper stays.
    #[serde(default)]
    pub include_locked: bool,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct UnrouteResult {
    /// Every deleted Track and Via, sorted.
    pub deleted: Vec<Uuid>,
    /// Locked copper left in place, sorted; left out when none.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub kept_locked: Vec<Uuid>,
}

impl Command for Unroute {
    const META: Meta = Meta {
        id: "pcb.route.unroute",
        title: "Unroute",
        description: "Deletes routed copper (Tracks and Vias): `all` of it, that of some \
                      `nets`, one `connection` (from a Track or Via up to the next Pad), or \
                      what leaves some `footprints`' Pads. Locked copper stays unless \
                      `include_locked`.",
        category: "pcb",
        icon: "delete",
        kind: Kind::Edit,
    };
    type Params = UnrouteParams;
    type Output = UnrouteResult;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: UnrouteParams) -> Result<UnrouteResult, CommandError> {
        let pcb = pcb(cx)?;
        if !p.all && p.nets.is_empty() && p.connection.is_none() && p.footprints.is_empty() {
            return Err(CommandError::invalid_params(
                "",
                "say what to unroute: `all`, `nets`, `connection` or `footprints`",
            ));
        }
        for (i, net) in p.nets.iter().enumerate() {
            if pcb.net(*net).is_none() {
                return Err(CommandError::invalid_params(
                    &format!("/nets/{i}"),
                    format!("no Net has UUID {net}"),
                ));
            }
        }
        let copper = board_copper(cx)?;
        let mut found = BTreeSet::new();
        if p.all {
            found.extend(pcb.tracks().map(|t| t.uuid));
            found.extend(pcb.vias().map(|v| v.uuid));
        }
        for net in &p.nets {
            found.extend(
                select::net(pcb, *net)
                    .objects
                    .into_iter()
                    .filter(|o| pcb.track(*o).is_some() || pcb.via(*o).is_some()),
            );
        }
        if let Some(seed) = p.connection {
            let picked = select::physical_connection(pcb, &copper, seed, false)
                .map_err(|e| CommandError::invalid_params("/connection", e))?;
            found.extend(picked.objects);
        }
        if !p.footprints.is_empty() {
            let picked = select::footprint_connections(pcb, &copper, &p.footprints)
                .map_err(|e| CommandError::invalid_params("/footprints", e))?;
            found.extend(picked.objects);
        }
        let (locked, deleted): (Vec<Uuid>, Vec<Uuid>) = found
            .into_iter()
            .partition(|o| !p.include_locked && is_locked(pcb, *o));
        let tx = cx.tx()?;
        for uuid in &deleted {
            tx.remove(*uuid)?;
        }
        Ok(UnrouteResult {
            deleted,
            kept_locked: locked,
        })
    }
}

/// Whether routed copper is locked. Tracks and Vias carry no lock yet (Locks on every
/// placed kind add it), so none is.
fn is_locked(_pcb: &PcbDocument, _object: Uuid) -> bool {
    false
}

pub struct TrackCleanup;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TrackCleanupParams {
    /// Only these Nets' Tracks; left out, every Track.
    #[serde(default)]
    pub nets: Option<Vec<Uuid>>,
}

#[derive(Debug, Default, Serialize, JsonSchema)]
pub struct TrackCleanupResult {
    /// Stacked duplicates deleted, sorted.
    pub duplicates: Vec<Uuid>,
    /// Tracks merged into a collinear neighbour and deleted, sorted.
    pub merged: Vec<Uuid>,
    /// Tracks lengthened to take in a merged neighbour, sorted.
    pub lengthened: Vec<Uuid>,
}

impl Command for TrackCleanup {
    const META: Meta = Meta {
        id: "pcb.track.cleanup",
        title: "Clean Up Tracks",
        description: "Deletes stacked duplicate Track segments (the same ends on the same \
                      Layer and Net, keeping the widest) and merges collinear segments of \
                      one width that meet end to end with nothing else at the joint.",
        category: "pcb",
        icon: "track",
        kind: Kind::Edit,
    };
    type Params = TrackCleanupParams;
    type Output = TrackCleanupResult;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: TrackCleanupParams,
    ) -> Result<TrackCleanupResult, CommandError> {
        let pcb = pcb(cx)?;
        if let Some(nets) = &p.nets {
            for (i, net) in nets.iter().enumerate() {
                if pcb.net(*net).is_none() {
                    return Err(CommandError::invalid_params(
                        &format!("/nets/{i}"),
                        format!("no Net has UUID {net}"),
                    ));
                }
            }
        }
        let copper = board_copper(cx)?;
        let document = pcb.uuid;
        let plan = cleanup(pcb, &copper, p.nets.as_deref());
        let tx = cx.tx()?;
        for uuid in plan.duplicates.iter().chain(&plan.merged) {
            tx.remove(*uuid)?;
        }
        let mut lengthened: Vec<Uuid> = Vec::new();
        for track in plan.changed {
            lengthened.push(track.uuid);
            tx.replace(Object::Track(InDocument::new(document, track)))?;
        }
        lengthened.sort();
        Ok(TrackCleanupResult {
            duplicates: plan.duplicates,
            merged: plan.merged,
            lengthened,
        })
    }
}

/// What makes two Tracks a stack: Layer, Net and both ends, in order.
type StackKey = (Uuid, Option<Uuid>, [(i64, i64); 2]);

#[derive(Default)]
struct Cleanup {
    duplicates: Vec<Uuid>,
    merged: Vec<Uuid>,
    changed: Vec<Track>,
}

fn cleanup(pcb: &PcbDocument, copper: &Copper, nets: Option<&[Uuid]>) -> Cleanup {
    let mut out = Cleanup::default();
    let mut tracks: BTreeMap<Uuid, Track> = pcb
        .tracks()
        .filter(|t| matches!(t.geometry, TrackGeometry::Line { .. }))
        .filter(|t| nets.is_none_or(|n| t.net.is_some_and(|net| n.contains(&net))))
        .map(|t| (t.uuid, t.clone()))
        .collect();

    // Stacked duplicates: the same two ends on one Layer and Net; the widest stays.
    let mut stacks: BTreeMap<StackKey, Vec<Uuid>> = BTreeMap::new();
    for t in tracks.values() {
        let (a, b) = t.geometry.ends();
        let mut ends = [(a.x.0, a.y.0), (b.x.0, b.y.0)];
        ends.sort();
        stacks
            .entry((t.layer, t.net, ends))
            .or_default()
            .push(t.uuid);
    }
    for group in stacks.into_values().filter(|g| g.len() > 1) {
        let keep = *group
            .iter()
            .max_by_key(|u| (tracks[*u].width, std::cmp::Reverse(**u)))
            .expect("a group");
        out.duplicates
            .extend(group.into_iter().filter(|u| *u != keep));
    }
    for uuid in &out.duplicates {
        tracks.remove(uuid);
    }
    out.duplicates.sort();

    // Collinear merges, until none is left.
    // Tracks meeting `p` on `layer`: those being cleaned up as they now stand, and the
    // board's others (Nets left out of the clean-up).
    let all_ends = |tracks: &BTreeMap<Uuid, Track>, gone: &[Uuid], layer: Uuid, p: Point| {
        pcb.tracks()
            .filter(|t| !tracks.contains_key(&t.uuid) && !gone.contains(&t.uuid))
            .chain(tracks.values())
            .filter(|t| t.layer == layer)
            .filter(|t| {
                let (a, b) = t.geometry.ends();
                a == p || b == p || p.on_segment(a, b)
            })
            .count()
    };
    let other_copper = |layer: Uuid, p: Point| {
        copper
            .items_at(layer, p)
            .into_iter()
            .any(|i| !matches!(i, Item::Track { .. } | Item::Zone { .. }))
    };
    let mut changed: BTreeSet<Uuid> = BTreeSet::new();
    'merge: loop {
        let uuids: Vec<Uuid> = tracks.keys().copied().collect();
        for (i, ua) in uuids.iter().enumerate() {
            for ub in &uuids[i + 1..] {
                let (ta, tb) = (&tracks[ua], &tracks[ub]);
                if ta.layer != tb.layer || ta.net != tb.net || ta.width != tb.width {
                    continue;
                }
                let Some((from, joint, to)) = joined(ta, tb) else {
                    continue;
                };
                let gone: Vec<Uuid> = out.duplicates.iter().chain(&out.merged).copied().collect();
                if all_ends(&tracks, &gone, ta.layer, joint) != 2 || other_copper(ta.layer, joint) {
                    continue;
                }
                let mut grown = ta.clone();
                grown.geometry = TrackGeometry::Line { from, to };
                let gone = *ub;
                tracks.insert(*ua, grown);
                tracks.remove(&gone);
                changed.insert(*ua);
                changed.remove(&gone);
                out.merged.push(gone);
                continue 'merge;
            }
        }
        break;
    }
    out.merged.sort();
    out.changed = changed.into_iter().map(|u| tracks[&u].clone()).collect();
    out
}

/// Where two segments meet end to end, running on in one straight line in opposite
/// directions from the joint: `(far end of a, joint, far end of b)`.
fn joined(a: &Track, b: &Track) -> Option<(Point, Point, Point)> {
    let (a0, a1) = a.geometry.ends();
    let (b0, b1) = b.geometry.ends();
    let (from, joint, to) = if a1 == b0 {
        (a0, a1, b1)
    } else if a1 == b1 {
        (a0, a1, b0)
    } else if a0 == b0 {
        (a1, a0, b1)
    } else if a0 == b1 {
        (a1, a0, b0)
    } else {
        return None;
    };
    let (ux, uy) = (
        (joint.x.0 - from.x.0) as i128,
        (joint.y.0 - from.y.0) as i128,
    );
    let (vx, vy) = ((to.x.0 - joint.x.0) as i128, (to.y.0 - joint.y.0) as i128);
    let collinear = ux * vy - uy * vx == 0 && ux * vx + uy * vy > 0;
    collinear.then_some((from, joint, to))
}
