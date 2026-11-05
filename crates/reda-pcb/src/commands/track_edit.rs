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
        let stretchy = !fixed
            && neighbours
                .iter()
                .all(|t| matches!(t.geometry, TrackGeometry::Line { .. }));
        if !stretchy {
            if fixed || !neighbours.is_empty() {
                plan.created.push(Track {
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

/// PROTOTYPE (issue 136): an angle-keeping slide of a straight segment. Returns the plan
/// and how far it really moved, which is less than `by` where a neighbour ran out.
///
/// - A lone neighbour keeps its line: the segment's end slides along it.
/// - A neighbour squeezed to nothing goes, and a collinear segment beyond it merges in.
/// - An end on a Pad or Via, at a branch, or beside a parallel neighbour stays put and
///   gets a connector, at 45\u{b0} with `miter`, else square.
pub fn slide_plan(
    pcb: &PcbDocument,
    copper: &Copper,
    track: Uuid,
    by: Point,
    miter: bool,
    mut new_id: impl FnMut() -> Uuid,
) -> Result<(DragPlan, Point), String> {
    let dragged = pcb
        .track(track)
        .ok_or_else(|| format!("no Track has UUID {track}"))?;
    let TrackGeometry::Line { from: a, to: b } = dragged.geometry else {
        let plan = drag_plan(pcb, copper, track, by, false, new_id)?;
        return Ok((plan, by));
    };
    let v = |p: Point, q: Point| ((q.x.0 - p.x.0) as f64, (q.y.0 - p.y.0) as f64);
    let (ux, uy) = {
        let (x, y) = v(a, b);
        let l = x.hypot(y);
        (x / l, y / l)
    };
    let (nx, ny) = (-uy, ux);
    let mut d = by.x.0 as f64 * nx + by.y.0 as f64 * ny;

    enum End {
        Free,
        Fixed,
        Stretch(Track, Point),
    }
    let classify = |end: Point| -> End {
        let fixed = copper.items_at(dragged.layer, end).into_iter().any(|i| {
            matches!(i, Item::Pad { .. } | Item::FreePad { .. } | Item::Via { .. })
        });
        let neighbours: Vec<&Track> = pcb
            .tracks()
            .filter(|t| t.uuid != track && t.layer == dragged.layer)
            .filter(|t| {
                let (p, q) = t.geometry.ends();
                p == end || q == end
            })
            .collect();
        if fixed {
            return End::Fixed;
        }
        match neighbours[..] {
            [] => End::Free,
            [t] => match t.geometry {
                TrackGeometry::Line { from, to } => {
                    let far = if from == end { to } else { from };
                    let (x, y) = v(end, far);
                    if (x * nx + y * ny).abs() < 1.0 {
                        End::Fixed
                    } else {
                        End::Stretch(t.clone(), far)
                    }
                }
                TrackGeometry::Arc { .. } => End::Fixed,
            },
            _ => End::Fixed,
        }
    };
    let ends = [classify(a), classify(b)];
    let origin = [a, b];
    // How far along the segment each end travels per unit of `d`.
    let slope = |i: usize, d: f64| -> f64 {
        match &ends[i] {
            End::Free => 0.0,
            End::Fixed if miter => (if i == 0 { 1.0 } else { -1.0 }) * d.signum(),
            End::Fixed => 0.0,
            End::Stretch(_, far) => {
                let (x, y) = v(origin[i], *far);
                (x * ux + y * uy) / (x * nx + y * ny)
            }
        }
    };
    // A neighbour may shrink to nothing, not past it.
    for (i, end) in ends.iter().enumerate() {
        if let End::Stretch(_, far) = end {
            let (x, y) = v(origin[i], *far);
            let reach = x * nx + y * ny;
            if d / reach > 1.0 {
                d = reach;
            }
        }
    }
    // The segment itself keeps at least a micrometre.
    let len0 = {
        let (x, y) = v(a, b);
        x.hypot(y)
    };
    let shrink = (slope(1, d) - slope(0, d)) * d;
    if len0 + shrink < 1_000.0 {
        d *= (len0 - 1_000.0).max(0.0) / -shrink;
    }
    let place = |i: usize| -> Point {
        let t = slope(i, d) * d;
        Point::new(
            reda_model::geom::Nm((origin[i].x.0 as f64 + d * nx + t * ux).round() as i64),
            reda_model::geom::Nm((origin[i].y.0 as f64 + d * ny + t * uy).round() as i64),
        )
    };
    let mut new_ends = [place(0), place(1)];
    let mut plan = DragPlan::default();
    for i in 0..2 {
        match &ends[i] {
            End::Free => {}
            End::Fixed => {
                if new_ends[i] != origin[i] {
                    plan.created.push(Track {
                        uuid: new_id(),
                        layer: dragged.layer,
                        net: dragged.net,
                        width: dragged.width,
                        geometry: TrackGeometry::Line {
                            from: origin[i],
                            to: new_ends[i],
                        },
                    });
                }
            }
            End::Stretch(n, far) => {
                let (x, y) = v(new_ends[i], *far);
                if x.hypot(y) <= 4.0 {
                    plan.deleted.push(n.uuid);
                    new_ends[i] = *far;
                    // Merge a collinear segment beyond it.
                    let beyond: Vec<&Track> = pcb
                        .tracks()
                        .filter(|t| t.uuid != n.uuid && t.uuid != track)
                        .filter(|t| t.layer == dragged.layer)
                        .filter(|t| {
                            let (p, q) = t.geometry.ends();
                            p == *far || q == *far
                        })
                        .collect();
                    let solid = copper.items_at(dragged.layer, *far).into_iter().any(|i| {
                        matches!(i, Item::Pad { .. } | Item::FreePad { .. } | Item::Via { .. })
                    });
                    if let ([m], false) = (&beyond[..], solid)
                        && m.width == dragged.width
                        && let TrackGeometry::Line { from, to } = m.geometry
                    {
                        let other = if from == *far { to } else { from };
                        let (x, y) = v(*far, other);
                        if (x * nx + y * ny).abs() < 2.0 {
                            plan.deleted.push(m.uuid);
                            new_ends[i] = other;
                        }
                    }
                } else {
                    let mut stretched = n.clone();
                    stretched.geometry = TrackGeometry::Line {
                        from: *far,
                        to: new_ends[i],
                    };
                    plan.modified.push(stretched);
                }
            }
        }
    }
    let mut moved = dragged.clone();
    moved.geometry = TrackGeometry::Line {
        from: new_ends[0],
        to: new_ends[1],
    };
    plan.modified.insert(0, moved);
    let applied = Point::new(
        reda_model::geom::Nm((d * nx).round() as i64),
        reda_model::geom::Nm((d * ny).round() as i64),
    );
    Ok((plan, applied))
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
    /// PROTOTYPE (issue 136): a connector from a fixed end runs at 45 degrees.
    #[serde(default = "yes")]
    pub miter: bool,
}

fn yes() -> bool {
    true
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
        let by = p.by.point();
        if by == Point::default() {
            return Err(CommandError::invalid_params(
                "/by",
                "the drag must move the segment",
            ));
        }
        let copper = super::board_copper(cx)?;
        let mut plan = if p.detach {
            drag_plan(pcb, &copper, p.track, by, true, Uuid::default)
        } else {
            slide_plan(pcb, &copper, p.track, by, p.miter, Uuid::default).map(|(plan, _)| plan)
        }
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
        let tx = cx.tx()?;
        for uuid in &deleted {
            tx.remove(*uuid)?;
        }
        Ok(CopperDeleteResult { deleted })
    }
}
