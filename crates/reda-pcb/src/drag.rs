//! Dragging routed copper (docs/spec/v0.1/pcb.md#dragging-tracks): the pure planners
//! behind `pcb.track.drag`, `pcb.track.drag_end`, `pcb.track.break` and
//! `pcb.copper.drag`. The canvas draws its preview from the same plans, so what it shows
//! in flight is what the Command does.
//!
//! - **Slide** ([`segment`]): a straight segment moves along its normal. With
//!   [`Options::preserve_angle`] a lone neighbour keeps its line and angle and shortens or
//!   lengthens; a neighbour squeezed to nothing goes and the drag stops there. Where a
//!   neighbour can't follow (a Pad, Free Pad or Via, a branch, a parallel, arc or locked
//!   neighbour) a connector is added: 45° with [`Options::miter`], else square. Without
//!   angle keeping the segment moves by the whole offset and its neighbours' ends follow.
//! - **Track end** ([`end`]): a free end, or one on a Footprint Pad, moves alone; a vertex
//!   shared by two segments moves both ([`Corner::Deform`]) or keeps the corner's shape
//!   ([`Corner::Scale`]); an end on a Free Pad or Via drags that object.
//! - **Via and Free Pad** ([`object`]): every Track ending on it stays attached. With
//!   angle keeping the attached segments move with it and their far ends slide along
//!   their neighbours, as a slide's do; without it their ends follow.
//! - **Break** ([`split`]): adds a vertex inside a segment, and can drag it at once.
//! - **Merge Collinear** ([`Options::merge_collinear`]): after any of these, a changed
//!   segment that lines up with its lone neighbour (same width, Net and Layer, nothing
//!   else at the joint) merges into one segment.

use reda_model::geom::Point;
use reda_model::{FreePad, PcbDocument, Track, TrackGeometry, Uuid, Via};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::copper::{Copper, Item};

mod merge;
mod translate;

/// How a drag treats what it leaves and meets. Commands take these as explicit values;
/// the Tool fills them from `pcb.drag.preserve_angle` and its in-flight keys.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Options {
    /// Moves the grabbed copper alone, leaving everything else where it is (`Mod`+drag).
    pub detach: bool,
    /// Neighbours keep their lines and angles (`pcb.drag.preserve_angle`, on).
    pub preserve_angle: bool,
    /// Connectors from fixed ends run at 45° (Include Miters, `C`, on); off, square.
    pub miter: bool,
    /// A changed segment that lines up with its neighbour merges into it (Merge
    /// Collinear, `R`, on).
    pub merge_collinear: bool,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            detach: false,
            preserve_angle: true,
            miter: true,
            merge_collinear: true,
        }
    }
}

/// How dragging a vertex shared by two segments reshapes them (`Space`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Corner {
    /// The vertex follows the cursor and both segments turn to meet it.
    #[default]
    Deform,
    /// The corner keeps its shape: both segments move with the vertex, and their far
    /// ends slide along their neighbours as a slide's do.
    Scale,
}

/// One end of a Track segment.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TrackEnd {
    From,
    To,
}

/// What a drag does to the board.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Plan {
    /// Tracks that change: the dragged ones and the neighbours whose ends follow them.
    pub modified: Vec<Track>,
    /// New segments: connectors back to fixed ends, and the second half of a break.
    pub created: Vec<Track>,
    /// Neighbours squeezed to nothing, and segments merged into a collinear neighbour.
    pub deleted: Vec<Uuid>,
    /// Vias moved, at their new place.
    pub vias: Vec<Via>,
    /// Free Pads moved, at their new place.
    pub free_pads: Vec<FreePad>,
    /// How far the grabbed copper really moved: less than asked where the drag stopped
    /// at a neighbour's end, and only across a slid segment.
    pub by: Point,
}

impl Plan {
    /// The existing objects the plan changes or deletes, for lock checks.
    pub fn touched(&self) -> Vec<Uuid> {
        let mut out: Vec<Uuid> = self
            .modified
            .iter()
            .map(|t| t.uuid)
            .chain(self.deleted.iter().copied())
            .chain(self.vias.iter().map(|v| v.uuid))
            .chain(self.free_pads.iter().map(|p| p.uuid))
            .collect();
        out.sort();
        out.dedup();
        out
    }

    fn modify(&mut self, track: Track) {
        match self.modified.iter_mut().find(|t| t.uuid == track.uuid) {
            Some(t) => *t = track,
            None => self.modified.push(track),
        }
    }
}

/// Slides the Track segment `track` by `by` (see the module docs). `new_id` names new
/// segments.
pub fn segment(
    pcb: &PcbDocument,
    copper: &Copper,
    track: Uuid,
    by: Point,
    options: Options,
    new_id: impl FnMut() -> Uuid,
) -> Result<Plan, String> {
    let dragged = find(pcb, track)?;
    if options.detach {
        let mut plan = Plan {
            by,
            ..Plan::default()
        };
        plan.modify(shifted(dragged, by));
        return Ok(plan);
    }
    let line = matches!(dragged.geometry, TrackGeometry::Line { .. });
    let preserve = options.preserve_angle && line;
    let m = if preserve {
        let (a, b) = dragged.geometry.ends();
        let n = translate::normal(a, b);
        let d = translate::dot(translate::vec(by), n);
        translate::point((n.0 * d, n.1 * d))
    } else {
        by
    };
    let mut plan = translate::run(
        pcb,
        copper,
        &[track],
        None,
        m,
        Options {
            preserve_angle: preserve,
            ..options
        },
        new_id,
    );
    merge::run(pcb, copper, &mut plan, options, None);
    Ok(plan)
}

/// Drags one end of `track` by `by` (see the module docs).
#[allow(clippy::too_many_arguments)]
pub fn end(
    pcb: &PcbDocument,
    copper: &Copper,
    track: Uuid,
    which: TrackEnd,
    by: Point,
    corner: Corner,
    options: Options,
    mut new_id: impl FnMut() -> Uuid,
) -> Result<Plan, String> {
    let dragged = find(pcb, track)?;
    let (from, to) = dragged.geometry.ends();
    let at = match which {
        TrackEnd::From => from,
        TrackEnd::To => to,
    };
    let items = copper.items_at(dragged.layer, at);
    let alone = |plan: &mut Plan| -> Result<(), String> {
        let moved = with_end(dragged, at, at + by);
        if zero(&moved) {
            return Err("the end would land on the Track's other end".into());
        }
        plan.modify(moved);
        Ok(())
    };
    let mut plan = Plan {
        by,
        ..Plan::default()
    };
    if options.detach {
        alone(&mut plan)?;
        return Ok(plan);
    }
    if let Some(object) = items
        .iter()
        .find(|i| matches!(i, Item::Via { .. } | Item::FreePad { .. }))
    {
        return self::object(pcb, copper, object.object(), by, options, new_id);
    }
    let others: Vec<&Track> = joined(pcb, dragged.layer, at, &[track]);
    let on_pad = items.iter().any(|i| matches!(i, Item::Pad { .. }));
    match (&others[..], on_pad) {
        (_, true) | ([], false) => alone(&mut plan)?,
        ([other], false) => match corner {
            Corner::Deform => {
                for t in [dragged, *other] {
                    let moved = with_end(t, at, at + by);
                    if zero(&moved) {
                        plan.deleted.push(t.uuid);
                    } else {
                        plan.modify(moved);
                    }
                }
                if plan.modified.is_empty() {
                    return Err("the corner would fold both segments away".into());
                }
            }
            Corner::Scale => {
                plan = translate::run(
                    pcb,
                    copper,
                    &[track, other.uuid],
                    None,
                    by,
                    options,
                    &mut new_id,
                );
            }
        },
        _ => {
            return Err("the end is a T-junction; dragging one comes in a later version".into());
        }
    }
    merge::run(pcb, copper, &mut plan, options, None);
    Ok(plan)
}

/// Drags the Via or Free Pad `object` by `by` with every Track ending on it (see the
/// module docs).
pub fn object(
    pcb: &PcbDocument,
    copper: &Copper,
    object: Uuid,
    by: Point,
    options: Options,
    new_id: impl FnMut() -> Uuid,
) -> Result<Plan, String> {
    let item = if pcb.via(object).is_some() {
        Item::Via { uuid: object }
    } else if pcb.free_pad(object).is_some() {
        Item::FreePad { uuid: object }
    } else {
        return Err(format!("{object} is not a Via or Free Pad"));
    };
    if options.detach {
        let mut plan = Plan {
            by,
            ..Plan::default()
        };
        move_object(pcb, &mut plan, item, by);
        return Ok(plan);
    }
    let attached: Vec<Uuid> = pcb
        .tracks()
        .filter(|t| {
            let (a, b) = t.geometry.ends();
            [a, b]
                .into_iter()
                .any(|e| copper.items_at(t.layer, e).contains(&item))
        })
        .map(|t| t.uuid)
        .collect();
    let mut plan = if options.preserve_angle {
        translate::run(pcb, copper, &attached, Some(item), by, options, new_id)
    } else {
        let mut plan = Plan {
            by,
            ..Plan::default()
        };
        for uuid in &attached {
            let t = pcb.track(*uuid).expect("an attached Track");
            let mut moved = t.clone();
            moved.geometry = translate::map_ends(t.geometry, |e| {
                if copper.items_at(t.layer, e).contains(&item) {
                    e + by
                } else {
                    e
                }
            });
            if zero(&moved) {
                plan.deleted.push(t.uuid);
            } else {
                plan.modify(moved);
            }
        }
        move_object(pcb, &mut plan, item, by);
        plan
    };
    merge::run(pcb, copper, &mut plan, options, Some(item));
    Ok(plan)
}

/// Breaks `track` at the point on it nearest `at`, adding a vertex there, then drags the
/// new vertex by `by` (which may be zero). The first half keeps the Track's UUID; the
/// second is new.
pub fn split(
    pcb: &PcbDocument,
    copper: &Copper,
    track: Uuid,
    at: Point,
    by: Point,
    options: Options,
    mut new_id: impl FnMut() -> Uuid,
) -> Result<Plan, String> {
    let t = find(pcb, track)?;
    let TrackGeometry::Line { from, to } = t.geometry else {
        return Err("only a straight segment can be broken".into());
    };
    let (a, b) = (translate::vec(from), translate::vec(to));
    let u = translate::sub(b, a);
    let len2 = translate::dot(u, u);
    let s = translate::dot(translate::sub(translate::vec(at), a), u) / len2;
    let vertex = translate::point((a.0 + u.0 * s, a.1 + u.1 * s));
    if vertex == from || vertex == to || !(0.0..=1.0).contains(&s) {
        return Err("the break must be inside the segment".into());
    }
    let moved = vertex + by;
    let mut first = t.clone();
    first.geometry = TrackGeometry::Line { from, to: moved };
    let mut second = t.clone();
    second.uuid = new_id();
    second.locked = false;
    second.geometry = TrackGeometry::Line { from: moved, to };
    if zero(&first) || zero(&second) {
        return Err("the new vertex would land on the segment's end".into());
    }
    let mut plan = Plan {
        modified: vec![first],
        created: vec![second],
        by,
        ..Plan::default()
    };
    if by != Point::default() {
        merge::run(pcb, copper, &mut plan, options, None);
    }
    Ok(plan)
}

fn find(pcb: &PcbDocument, track: Uuid) -> Result<&Track, String> {
    pcb.track(track)
        .ok_or_else(|| format!("no Track has UUID {track}"))
}

/// The Tracks other than `except` on `layer` with an end at `at`.
fn joined<'a>(pcb: &'a PcbDocument, layer: Uuid, at: Point, except: &[Uuid]) -> Vec<&'a Track> {
    pcb.tracks()
        .filter(|t| t.layer == layer && !except.contains(&t.uuid))
        .filter(|t| {
            let (p, q) = t.geometry.ends();
            p == at || q == at
        })
        .collect()
}

fn shifted(t: &Track, by: Point) -> Track {
    let mut moved = t.clone();
    moved.geometry = translate::map_ends(t.geometry, |e| e + by);
    moved
}

fn with_end(t: &Track, at: Point, to: Point) -> Track {
    let mut moved = t.clone();
    moved.geometry = translate::map_ends(t.geometry, |e| if e == at { to } else { e });
    moved
}

fn zero(t: &Track) -> bool {
    let (a, b) = t.geometry.ends();
    a == b
}

fn move_object(pcb: &PcbDocument, plan: &mut Plan, item: Item, by: Point) {
    match item {
        Item::Via { uuid } => {
            let mut via = pcb.via(uuid).expect("a Via").clone();
            via.at = via.at + by;
            plan.vias.push(via);
        }
        Item::FreePad { uuid } => {
            let mut pad = pcb.free_pad(uuid).expect("a Free Pad").clone();
            pad.at = pad.at + by;
            plan.free_pads.push(pad);
        }
        _ => {}
    }
}
