//! Editing routed copper: dragging Track segments, Track ends, Vias and Free Pads with
//! their Tracks attached (or detached), breaking a segment, and deleting Tracks and Vias
//! together.
//!
//! The planners are in [`crate::drag`]; the routing Tool draws its preview from the same
//! plans, so what it shows in flight is what the Command does. Every drag Command takes
//! its options as explicit values: the Tool reads `pcb.drag.preserve_angle` and passes it.

use reda_command::{
    AvailabilityContext, Command, CommandError, Context, Kind, Meta, Registry, Unavailable,
};
use reda_model::geom::Point;
use reda_model::{InDocument, Object, Uuid};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::outline::PointParam;
use super::{needs_pcb, pcb};
use crate::drag::{self, Corner, Options, Plan, TrackEnd};

pub(super) fn register_all(registry: &mut Registry) {
    registry.add(TrackDrag);
    registry.add(TrackDragEnd);
    registry.add(TrackBreak);
    registry.add(CopperDrag);
    registry.add(CopperDelete);
}

fn yes() -> bool {
    true
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct TrackDragResult {
    /// The Tracks moved, stretched, merged or added, sorted by UUID.
    pub tracks: Vec<Uuid>,
    /// Tracks squeezed to nothing or merged into a neighbour, sorted by UUID.
    pub deleted: Vec<Uuid>,
    /// The Vias and Free Pads moved, sorted by UUID.
    pub moved: Vec<Uuid>,
    /// How far the grabbed copper really moved: less than `by` where the drag stopped at
    /// a neighbour's end, and only across a slid segment.
    pub by: Point,
}

/// Refuses a drag that gets nowhere, or locked copper the plan would change, then
/// applies the plan as one transaction.
fn apply(
    cx: &mut Context<'_>,
    mut plan: Plan,
    include_locked: bool,
    must_move: bool,
) -> Result<TrackDragResult, CommandError> {
    let pcb = pcb(cx)?;
    let document = pcb.uuid;
    crate::lock::refuse(pcb, plan.touched(), include_locked, "drag")?;
    if must_move && plan.by == Point::default() {
        return Err(CommandError::rejected(
            "the drag goes nowhere: a straight segment slides only across itself, and \
             stops where a neighbour has no length left",
        ));
    }
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
    let mut moved = Vec::new();
    for via in plan.vias {
        moved.push(via.uuid);
        tx.replace(Object::Via(InDocument::new(document, via)))?;
    }
    for pad in plan.free_pads {
        moved.push(pad.uuid);
        tx.replace(Object::FreePad(InDocument::new(document, pad)))?;
    }
    tracks.sort();
    moved.sort();
    let mut deleted = plan.deleted;
    deleted.sort();
    Ok(TrackDragResult {
        tracks,
        deleted,
        moved,
        by: plan.by,
    })
}

fn track_param(cx: &Context<'_>, track: Uuid, include_locked: bool) -> Result<(), CommandError> {
    let pcb = pcb(cx)?;
    if pcb.track(track).is_none() {
        return Err(CommandError::invalid_params(
            "/track",
            format!("no Track has UUID {track}"),
        ));
    }
    crate::lock::refuse(pcb, [track], include_locked, "drag")
}

fn moves(by: PointParam) -> Result<Point, CommandError> {
    let by = by.point();
    if by == Point::default() {
        return Err(CommandError::invalid_params("/by", "the drag must move"));
    }
    Ok(by)
}

pub struct TrackDrag;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TrackDragParams {
    /// The Track segment.
    pub track: Uuid,
    /// How far to drag it. With `preserve_angle` a straight segment moves only across
    /// itself: the part of `by` along it is dropped.
    pub by: PointParam,
    /// Moves the segment alone, leaving its neighbours where they are (`Mod`+drag).
    #[serde(default)]
    pub detach: bool,
    /// Angle keeping (on; the Tool passes `pcb.drag.preserve_angle`): a lone neighbour
    /// keeps its line and shortens or lengthens, and the drag stops where one runs out.
    /// Off, the segment moves by the whole of `by` and its neighbours' ends follow.
    #[serde(default = "yes")]
    pub preserve_angle: bool,
    /// Include Miters (on): a connector from an end that can't follow runs at 45°; off,
    /// square.
    #[serde(default = "yes")]
    pub miter: bool,
    /// Merge Collinear (on): a segment left in line with its lone neighbour merges into
    /// it.
    #[serde(default = "yes")]
    pub merge_collinear: bool,
    /// Drags a locked Track too; without it a locked one is refused.
    #[serde(default)]
    pub include_locked: bool,
}

impl Command for TrackDrag {
    const META: Meta = Meta {
        id: "pcb.track.drag",
        title: "Drag Track",
        description: "Slides a Track segment across itself. A lone neighbour keeps its \
                      angle and shortens or lengthens; one squeezed to nothing goes, a \
                      collinear segment beyond it merges, and the drag stops there. Where a \
                      neighbour can't follow (a Pad, Via, branch or parallel neighbour) a \
                      connector is added, 45° with `miter`, else square. With `detach` \
                      only the segment moves; without `preserve_angle` it moves freely and \
                      its neighbours' ends follow.",
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
        track_param(cx, p.track, p.include_locked)?;
        let by = moves(p.by)?;
        let options = Options {
            detach: p.detach,
            preserve_angle: p.preserve_angle,
            miter: p.miter,
            merge_collinear: p.merge_collinear,
        };
        let copper = super::board_copper(cx)?;
        let plan = drag::segment(pcb(cx)?, &copper, p.track, by, options, Uuid::default)
            .map_err(|e| CommandError::invalid_params("/track", e))?;
        apply(cx, plan, p.include_locked, true)
    }
}

pub struct TrackDragEnd;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TrackDragEndParams {
    /// The Track segment.
    pub track: Uuid,
    /// Which end.
    pub end: TrackEnd,
    /// How far to drag it.
    pub by: PointParam,
    /// At a vertex shared by two segments: `deform` (the vertex follows and both
    /// segments turn) or `scale` (the corner keeps its shape).
    #[serde(default)]
    pub corner: Corner,
    /// Moves the end alone, leaving whatever it is on (`Mod`+drag).
    #[serde(default)]
    pub detach: bool,
    /// Angle keeping for `scale` corners and Via or Free Pad ends, as in
    /// `pcb.track.drag` (on).
    #[serde(default = "yes")]
    pub preserve_angle: bool,
    /// Include Miters (on), as in `pcb.track.drag`.
    #[serde(default = "yes")]
    pub miter: bool,
    /// Merge Collinear (on): a segment left in line with its lone neighbour merges into
    /// it.
    #[serde(default = "yes")]
    pub merge_collinear: bool,
    /// Drags locked copper too; without it a drag that would change any is refused.
    #[serde(default)]
    pub include_locked: bool,
}

impl Command for TrackDragEnd {
    const META: Meta = Meta {
        id: "pcb.track.drag_end",
        title: "Drag Track End",
        description: "Drags one end of a Track segment. A free end, or one on a Footprint \
                      Pad, moves alone (the Ratsnest shows the gap). A vertex shared by two \
                      segments moves both: `deform` turns them to meet it, `scale` keeps \
                      the corner's shape. An end on a Free Pad or Via drags that object \
                      with all its Tracks, as `pcb.copper.drag` does. T-junctions are \
                      refused.",
        category: "pcb",
        icon: "drag",
        kind: Kind::Edit,
    };
    type Params = TrackDragEndParams;
    type Output = TrackDragResult;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: TrackDragEndParams,
    ) -> Result<TrackDragResult, CommandError> {
        track_param(cx, p.track, p.include_locked)?;
        let by = moves(p.by)?;
        let options = Options {
            detach: p.detach,
            preserve_angle: p.preserve_angle,
            miter: p.miter,
            merge_collinear: p.merge_collinear,
        };
        let copper = super::board_copper(cx)?;
        let plan = drag::end(
            pcb(cx)?,
            &copper,
            p.track,
            p.end,
            by,
            p.corner,
            options,
            Uuid::default,
        )
        .map_err(CommandError::rejected)?;
        apply(cx, plan, p.include_locked, true)
    }
}

pub struct TrackBreak;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TrackBreakParams {
    /// The straight Track segment.
    pub track: Uuid,
    /// Where to add the vertex: the nearest point on the segment, strictly inside it.
    pub at: PointParam,
    /// Drags the new vertex this far at once, as dragging the midpoint handle does.
    #[serde(default)]
    pub by: Option<PointParam>,
    /// Merge Collinear (on): after a drag, a segment left in line with its lone neighbour
    /// merges into it.
    #[serde(default = "yes")]
    pub merge_collinear: bool,
    /// Breaks a locked Track too; without it a locked one is refused.
    #[serde(default)]
    pub include_locked: bool,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct TrackBreakResult {
    /// The changed and new Tracks, sorted by UUID: the first half keeps the segment's
    /// UUID and the second is new.
    pub tracks: Vec<Uuid>,
    /// Tracks merged into a neighbour by the drag, sorted by UUID.
    pub deleted: Vec<Uuid>,
}

impl Command for TrackBreak {
    const META: Meta = Meta {
        id: "pcb.track.break",
        title: "Break Track",
        description: "Adds a vertex inside a straight Track segment, splitting it in two, \
                      and with `by` drags the new vertex at once (one undo step).",
        category: "pcb",
        icon: "drag",
        kind: Kind::Edit,
    };
    type Params = TrackBreakParams;
    type Output = TrackBreakResult;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: TrackBreakParams,
    ) -> Result<TrackBreakResult, CommandError> {
        track_param(cx, p.track, p.include_locked)?;
        let by = p.by.map_or(Point::default(), PointParam::point);
        let options = Options {
            merge_collinear: p.merge_collinear,
            ..Options::default()
        };
        let copper = super::board_copper(cx)?;
        let plan = drag::split(
            pcb(cx)?,
            &copper,
            p.track,
            p.at.point(),
            by,
            options,
            Uuid::default,
        )
        .map_err(|e| CommandError::invalid_params("/at", e))?;
        let out = apply(cx, plan, p.include_locked, false)?;
        Ok(TrackBreakResult {
            tracks: out.tracks,
            deleted: out.deleted,
        })
    }
}

pub struct CopperDrag;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CopperDragParams {
    /// The Via or Free Pad.
    pub object: Uuid,
    /// How far to drag it.
    pub by: PointParam,
    /// Moves it alone, leaving its Tracks where they are (`Mod`+drag).
    #[serde(default)]
    pub detach: bool,
    /// Angle keeping (on): the Tracks ending on it move with it, and their far ends slide
    /// along their neighbours as a slide's do. Off, their ends on it follow and they
    /// turn.
    #[serde(default = "yes")]
    pub preserve_angle: bool,
    /// Include Miters (on), as in `pcb.track.drag`.
    #[serde(default = "yes")]
    pub miter: bool,
    /// Merge Collinear (on): a segment left in line with its lone neighbour merges into
    /// it.
    #[serde(default = "yes")]
    pub merge_collinear: bool,
    /// Drags locked copper too; without it a drag that would change any is refused.
    #[serde(default)]
    pub include_locked: bool,
}

impl Command for CopperDrag {
    const META: Meta = Meta {
        id: "pcb.copper.drag",
        title: "Drag Via or Free Pad",
        description: "Drags a Via or Free Pad with every Track ending on it attached. With \
                      `preserve_angle` the Tracks keep their angles, as a slide's \
                      neighbours do; without it their ends follow. With `detach` it moves \
                      alone.",
        category: "pcb",
        icon: "drag",
        kind: Kind::Edit,
    };
    type Params = CopperDragParams;
    type Output = TrackDragResult;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: CopperDragParams,
    ) -> Result<TrackDragResult, CommandError> {
        let pcb = pcb(cx)?;
        if pcb.via(p.object).is_none() && pcb.free_pad(p.object).is_none() {
            return Err(CommandError::invalid_params(
                "/object",
                format!("{} is not a Via or Free Pad", p.object),
            ));
        }
        crate::lock::refuse(pcb, [p.object], p.include_locked, "drag")?;
        let by = moves(p.by)?;
        let options = Options {
            detach: p.detach,
            preserve_angle: p.preserve_angle,
            miter: p.miter,
            merge_collinear: p.merge_collinear,
        };
        let copper = super::board_copper(cx)?;
        let plan = drag::object(
            super::pcb(cx)?,
            &copper,
            p.object,
            by,
            options,
            Uuid::default,
        )
        .map_err(|e| CommandError::invalid_params("/object", e))?;
        apply(cx, plan, p.include_locked, true)
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
