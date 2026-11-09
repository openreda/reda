//! Editing routed copper: dragging a Track segment with its neighbours attached (or
//! detached), and deleting Tracks and Vias together.
//!
//! [`drag_plan`] is the pure planner behind `pcb.track.drag`; the routing Tool draws its
//! preview from the same plan, so what it shows in flight is what the Command does.

use reda_command::{
    AvailabilityContext, Command, CommandError, Context, Kind, Meta, Registry, Unavailable,
};
use reda_model::geom::Point;
use reda_model::{InDocument, Object, PcbDocument, Track, TrackGeometry, Uuid};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::outline::PointParam;
use super::{needs_pcb, pcb};
use crate::copper::{Copper, Item};

pub(super) fn register_all(registry: &mut Registry) {
    registry.add(TrackDrag);
    registry.add(CopperDelete);
}

/// What dragging a segment does to the board.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DragPlan {
    /// Tracks that change: the dragged one and the neighbours whose ends follow it.
    pub modified: Vec<Track>,
    /// New segments joining a dragged end to the Pad or Via it left.
    pub created: Vec<Track>,
    /// Neighbours squeezed to nothing.
    pub deleted: Vec<Uuid>,
}

/// Plans dragging the Track segment `track` by `by`. Unless `detach`, each end keeps what
/// it was joined to: Tracks ending there (on its Layer) stretch to follow it, and where it
/// left a Pad or Via a new segment joins the two. `new_id` names the new segments.
pub fn drag_plan(
    pcb: &PcbDocument,
    copper: &Copper,
    track: Uuid,
    by: Point,
    detach: bool,
    mut new_id: impl FnMut() -> Uuid,
) -> Result<DragPlan, String> {
    let dragged = pcb
        .track(track)
        .ok_or_else(|| format!("no Track has UUID {track}"))?;
    let shift = |p: Point| p + by;
    let mut moved = dragged.clone();
    moved.geometry = match dragged.geometry {
        TrackGeometry::Line { from, to } => TrackGeometry::Line {
            from: shift(from),
            to: shift(to),
        },
        TrackGeometry::Arc { from, to, sweep } => TrackGeometry::Arc {
            from: shift(from),
            to: shift(to),
            sweep,
        },
    };
    let mut plan = DragPlan {
        modified: vec![moved],
        ..DragPlan::default()
    };
    if detach {
        return Ok(plan);
    }
    let (a, b) = dragged.geometry.ends();
    for end in [a, b] {
        let fixed = copper.items_at(dragged.layer, end).into_iter().any(|item| {
            matches!(
                item,
                Item::Pad { .. } | Item::FreePad { .. } | Item::Via { .. }
            )
        });
        let neighbours: Vec<&Track> = pcb
            .tracks()
            .filter(|t| t.uuid != track && t.layer == dragged.layer)
            .filter(|t| {
                let (p, q) = t.geometry.ends();
                p == end || q == end
            })
            .collect();
        // A locked neighbour stays put: the drag stops at it, as at a Pad.
        let stretchy = !fixed
            && neighbours
                .iter()
                .all(|t| matches!(t.geometry, TrackGeometry::Line { .. }) && !t.locked);
        if !stretchy {
            if fixed || !neighbours.is_empty() {
                plan.created.push(Track {
                    locked: false,
                    uuid: new_id(),
                    layer: dragged.layer,
                    net: dragged.net,
                    width: dragged.width,
                    geometry: TrackGeometry::Line {
                        from: end,
                        to: shift(end),
                    },
                });
            }
            continue;
        }
        for neighbour in neighbours {
            let TrackGeometry::Line { from, to } = plan
                .modified
                .iter()
                .find(|t| t.uuid == neighbour.uuid)
                .map_or(neighbour.geometry, |t| t.geometry)
            else {
                continue;
            };
            let (from, to) = if from == end {
                (shift(from), to)
            } else {
                (from, shift(to))
            };
            if from == to {
                plan.modified.retain(|t| t.uuid != neighbour.uuid);
                plan.deleted.push(neighbour.uuid);
                continue;
            }
            let geometry = TrackGeometry::Line { from, to };
            match plan.modified.iter_mut().find(|t| t.uuid == neighbour.uuid) {
                Some(t) => t.geometry = geometry,
                None => {
                    let mut stretched = neighbour.clone();
                    stretched.geometry = geometry;
                    plan.modified.push(stretched);
                }
            }
        }
    }
    Ok(plan)
}

pub struct TrackDrag;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TrackDragParams {
    /// The Track segment.
    pub track: Uuid,
    /// How far to move it.
    pub by: PointParam,
    /// Moves the segment alone, leaving its neighbours where they are.
    #[serde(default)]
    pub detach: bool,
    /// Drags a locked Track too; without it a locked one is refused.
    #[serde(default)]
    pub include_locked: bool,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct TrackDragResult {
    /// The Tracks moved, stretched or added, sorted by UUID.
    pub tracks: Vec<Uuid>,
    /// Neighbours squeezed to nothing, sorted by UUID.
    pub deleted: Vec<Uuid>,
}

impl Command for TrackDrag {
    const META: Meta = Meta {
        id: "pcb.track.drag",
        title: "Drag Track",
        description: "Moves a Track segment. Its neighbours stay attached: Tracks ending at \
                      its ends stretch to follow, and an end that leaves a Pad or Via gets a \
                      new segment back to it. With `detach` only the segment moves.",
        category: "pcb",
        icon: "drag",
        kind: Kind::Edit,
    };
    type Params = TrackDragParams;
    type Output = TrackDragResult;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: TrackDragParams,
    ) -> Result<TrackDragResult, CommandError> {
        let pcb = pcb(cx)?;
        let document = pcb.uuid;
        if pcb.track(p.track).is_none() {
            return Err(CommandError::invalid_params(
                "/track",
                format!("no Track has UUID {}", p.track),
            ));
        }
        crate::lock::refuse(pcb, [p.track], p.include_locked, "drag")?;
        let by = p.by.point();
        if by == Point::default() {
            return Err(CommandError::invalid_params(
                "/by",
                "the drag must move the segment",
            ));
        }
        let copper = super::board_copper(cx)?;
        let mut plan = drag_plan(pcb, &copper, p.track, by, p.detach, Uuid::default)
            .map_err(|e| CommandError::invalid_params("/track", e))?;
        for track in &mut plan.created {
            track.uuid = cx.new_id();
        }
        let tx = cx.tx()?;
        let mut tracks = Vec::new();
        for track in plan.modified {
            tracks.push(track.uuid);
            tx.replace(Object::Track(InDocument::new(document, track)))?;
        }
        for track in plan.created {
            tracks.push(track.uuid);
            tx.insert(Object::Track(InDocument::new(document, track)))?;
        }
        for uuid in &plan.deleted {
            tx.remove(*uuid)?;
        }
        tracks.sort();
        let mut deleted = plan.deleted;
        deleted.sort();
        Ok(TrackDragResult { tracks, deleted })
    }
}

pub struct CopperDelete;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CopperDeleteParams {
    /// Track segments and Vias, by UUID.
    pub objects: Vec<Uuid>,
    /// Deletes each Track's whole Track instead of the segment: every segment joined end
    /// to end with it on the same Layer, up to a Pad, a Via or a branch.
    #[serde(default)]
    pub whole: bool,
    /// Deletes locked objects too; without it a locked one refuses the whole Command.
    #[serde(default)]
    pub include_locked: bool,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct CopperDeleteResult {
    /// Every deleted Track and Via, sorted by UUID.
    pub deleted: Vec<Uuid>,
}

impl Command for CopperDelete {
    const META: Meta = Meta {
        id: "pcb.copper.delete",
        title: "Delete Copper",
        description: "Deletes Track segments and Vias together, or with `whole` every \
                      segment of the Tracks up to a Pad, Via or branch.",
        category: "pcb",
        icon: "delete",
        kind: Kind::Edit,
    };
    type Params = CopperDeleteParams;
    type Output = CopperDeleteResult;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: CopperDeleteParams,
    ) -> Result<CopperDeleteResult, CommandError> {
        let pcb = pcb(cx)?;
        for (i, uuid) in p.objects.iter().enumerate() {
            if pcb.track(*uuid).is_none() && pcb.via(*uuid).is_none() {
                return Err(CommandError::invalid_params(
                    &format!("/objects/{i}"),
                    format!("{uuid} is not a Track or Via"),
                ));
            }
        }
        let mut deleted: Vec<Uuid> = if p.whole {
            let copper = super::board_copper(cx)?;
            p.objects
                .iter()
                .flat_map(|o| {
                    let whole = copper.whole_track(*o);
                    if whole.is_empty() { vec![*o] } else { whole }
                })
                .collect()
        } else {
            p.objects.clone()
        };
        deleted.sort();
        deleted.dedup();
        crate::lock::refuse(pcb, deleted.iter().copied(), p.include_locked, "delete")?;
        let tx = cx.tx()?;
        for uuid in &deleted {
            tx.remove(*uuid)?;
        }
        Ok(CopperDeleteResult { deleted })
    }
}
