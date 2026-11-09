//! Moving Footprints (docs/spec/v0.1/pcb.md#moving-footprints): the placement modes the
//! `pcb.place.*` Preferences keep, and the pure checks behind them.
//!
//! - [`overlaps`]: the live overlap, Footprints whose Courtyards (or fallback boxes) come
//!   closer than `courtyard_clearance` once moved, exactly as DRC finds them.
//! - [`avoid`]: in [`ObstacleMode::Avoid`], how far a move gets before it would break
//!   `courtyard_clearance`: the last clear point on the straight path.
//! - [`alignment_guides`]: the lines a moved Footprint's held point snaps across to, so its
//!   Pad centres and Courtyard edges line up with its neighbours'.
//! - [`pad_to_pad`]: the Ratsnest's Pad To Pad mode during a move.

use std::collections::{BTreeMap, BTreeSet};

use reda_model::geom::{Nm, Point};
use reda_model::{FootprintInstance, PcbDocument, Side, Uuid};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::drc::placement::{courtyard_clearance, courtyards_or_box, too_close};
use crate::placed::placed_pads;
use crate::zone::poly::{self, Area};

/// Why X and Y (same-side mirroring) are refused on a Footprint: the model holds only a
/// side, and a mirrored part can't be built.
pub const MIRROR_REFUSED: &str = "Mirroring makes an unbuildable part: L flips to the other side";

/// What a Footprint move does at other Footprints (`R`): kept in `pcb.place.obstacle_mode`.
/// `push` is reserved for v0.4.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ObstacleMode {
    /// Goes anywhere; overlaps show in the violation colour.
    #[default]
    Ignore,
    /// Stops at the last position that keeps `courtyard_clearance`.
    Avoid,
}

/// The Ratsnest during a Footprint move (`N`): kept in `pcb.place.ratsnest_mode`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RatsnestMode {
    /// Only the lines the move changes.
    #[default]
    Breaks,
    /// No lines.
    Hidden,
    /// Every line from the moved Pads, Pad to Pad.
    PadToPad,
}

/// Two Footprints closer than `courtyard_clearance`: a moved one and one standing still.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, JsonSchema)]
pub struct Overlap {
    pub moved: Uuid,
    pub other: Uuid,
}

/// The Courtyards of every Footprint not in `moving`, as they stand.
fn standing(pcb: &PcbDocument, moving: &[Uuid]) -> Vec<(Uuid, BTreeMap<Side, Area>)> {
    pcb.footprints()
        .filter(|f| !moving.contains(&f.uuid))
        .map(|f| (f.uuid, courtyards_or_box(pcb, f)))
        .filter(|(_, c)| !c.is_empty())
        .collect()
}

/// `moving` (Footprint UUIDs) as they would be moved by `by`.
fn moved(pcb: &PcbDocument, moving: &[Uuid], by: Point) -> Vec<FootprintInstance> {
    moving
        .iter()
        .filter_map(|u| pcb.footprint(*u))
        .map(|f| {
            let mut f = f.clone();
            f.at = f.at + by;
            f
        })
        .collect()
}

fn overlaps_among(
    pcb: &PcbDocument,
    moved: &[FootprintInstance],
    standing: &[(Uuid, BTreeMap<Side, Area>)],
    required: Nm,
) -> Vec<Overlap> {
    let mut out = Vec::new();
    for f in moved {
        let yards = courtyards_or_box(pcb, f);
        for (other, other_yards) in standing {
            if too_close(&yards, other_yards, required) {
                out.push(Overlap {
                    moved: f.uuid,
                    other: *other,
                });
            }
        }
    }
    out.sort();
    out
}

/// The pairs that come closer than `courtyard_clearance` when the Footprints `moving` are
/// moved by `by`: each moved Footprint against every Footprint standing still, by the
/// Courtyard, or else the fallback box, on each side. Other kinds in `moving` are ignored.
pub fn overlaps(pcb: &PcbDocument, moving: &[Uuid], by: Point) -> Vec<Overlap> {
    let required = courtyard_clearance(pcb);
    overlaps_among(
        pcb,
        &moved(pcb, moving, by),
        &standing(pcb, moving),
        required,
    )
}

/// How far the Footprints `moving` get along the straight move by `by` in `avoid` mode:
/// `by` itself when the whole path keeps `courtyard_clearance`, else the last clear point
/// before the first that doesn't (to within 1 µm). Pairs already too close where the move
/// starts don't stop it, so a part can be dragged out of an overlap.
pub fn avoid(pcb: &PcbDocument, moving: &[Uuid], by: Point) -> Point {
    let required = courtyard_clearance(pcb);
    let standing = standing(pcb, moving);
    let at_start = overlaps_among(
        pcb,
        &moved(pcb, moving, Point::default()),
        &standing,
        required,
    );
    let blocked = |t: f64| {
        let step = Point::new(
            Nm((by.x.0 as f64 * t).round() as i64),
            Nm((by.y.0 as f64 * t).round() as i64),
        );
        overlaps_among(pcb, &moved(pcb, moving, step), &standing, required)
            .iter()
            .any(|o| !at_start.contains(o))
    };
    let along = |t: f64| {
        Point::new(
            Nm((by.x.0 as f64 * t).round() as i64),
            Nm((by.y.0 as f64 * t).round() as i64),
        )
    };
    let length = (by.x.0 as f64).hypot(by.y.0 as f64);
    if length == 0.0 {
        return by;
    }
    // Walk the path in steps of at most 0.1 mm, then close in on the first blocked step.
    let steps = ((length / 100_000.0).ceil() as usize).clamp(1, 2000);
    let mut clear = 0.0;
    for i in 1..=steps {
        let t = i as f64 / steps as f64;
        if !blocked(t) {
            clear = t;
            continue;
        }
        let mut hit = t;
        while (hit - clear) * length > 1000.0 {
            let mid = (clear + hit) / 2.0;
            if blocked(mid) {
                hit = mid;
            } else {
                clear = mid;
            }
        }
        return along(clear);
    }
    by
}

/// The outlines a Footprint is checked by in `avoid` and the live overlap: its Courtyard
/// contours on each side, or else the fallback box, as closed loops of board points.
pub fn selection_outlines(pcb: &PcbDocument, footprint: Uuid) -> Vec<Vec<Point>> {
    let Some(f) = pcb.footprint(footprint) else {
        return Vec::new();
    };
    courtyards_or_box(pcb, f)
        .values()
        .flatten()
        .flatten()
        .map(|contour| contour.iter().map(|p| poly::point(*p)).collect())
        .collect()
}

/// The box around a Footprint's [`selection_outlines`], as (min, max).
fn selection_box(pcb: &PcbDocument, f: &FootprintInstance) -> Option<(Point, Point)> {
    courtyards_or_box(pcb, f)
        .values()
        .filter_map(poly::bounds)
        .map(|(a, b)| {
            (
                Point::new(Nm(a[0]), Nm(a[1])),
                Point::new(Nm(b[0]), Nm(b[1])),
            )
        })
        .reduce(|(a, b), (c, d)| {
            (
                Point::new(a.x.min(c.x), a.y.min(c.y)),
                Point::new(b.x.max(d.x), b.y.max(d.y)),
            )
        })
}

/// What a Footprint lines up by: its selection box's edges and its Pad centres.
#[derive(Default)]
struct Features {
    edges_x: Vec<Nm>,
    edges_y: Vec<Nm>,
    pads: Vec<Point>,
}

fn features(pcb: &PcbDocument, f: &FootprintInstance) -> Features {
    let mut out = Features::default();
    if let Some((lo, hi)) = selection_box(pcb, f) {
        out.edges_x = vec![lo.x, hi.x];
        out.edges_y = vec![lo.y, hi.y];
    }
    out.pads = placed_pads(pcb, f).iter().map(|p| p.at).collect();
    out
}

/// Alignment guides for a move: the x and y values the held point may snap to.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Guides {
    pub x: BTreeSet<Nm>,
    pub y: BTreeSet<Nm>,
}

/// The alignment guides of a move of the Footprints `moving` (as they stand in `pcb`)
/// held at `reference`: each value puts one of their Pad centres on a neighbour's Pad
/// centre, or one of their selection-box edges on a neighbour's edge (aligned or
/// abutting), on that axis. Neighbours are the Footprints not moving.
pub fn alignment_guides(pcb: &PcbDocument, moving: &[Uuid], reference: Point) -> Guides {
    let mut own = Features::default();
    for f in pcb.footprints().filter(|f| moving.contains(&f.uuid)) {
        let mut one = features(pcb, f);
        own.edges_x.append(&mut one.edges_x);
        own.edges_y.append(&mut one.edges_y);
        own.pads.append(&mut one.pads);
    }
    let mut guides = Guides::default();
    let lines = |out: &mut BTreeSet<Nm>, own: &[Nm], theirs: &[Nm], at: Nm| {
        for o in own {
            for t in theirs {
                out.insert(*t + (at - *o));
            }
        }
    };
    let own_px: BTreeSet<Nm> = own.pads.iter().map(|p| p.x).collect();
    let own_py: BTreeSet<Nm> = own.pads.iter().map(|p| p.y).collect();
    let (own_px, own_py): (Vec<Nm>, Vec<Nm>) =
        (own_px.into_iter().collect(), own_py.into_iter().collect());
    for f in pcb.footprints().filter(|f| !moving.contains(&f.uuid)) {
        let theirs = features(pcb, f);
        lines(&mut guides.x, &own.edges_x, &theirs.edges_x, reference.x);
        lines(&mut guides.y, &own.edges_y, &theirs.edges_y, reference.y);
        let px: Vec<Nm> = theirs.pads.iter().map(|p| p.x).collect();
        let py: Vec<Nm> = theirs.pads.iter().map(|p| p.y).collect();
        lines(&mut guides.x, &own_px, &px, reference.x);
        lines(&mut guides.y, &own_py, &py, reference.y);
    }
    guides
}

/// One Pad To Pad line during a move.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PadLine {
    pub net: Uuid,
    /// The moved Pad.
    pub from: Point,
    /// The nearest Pad on its Net of a Footprint not moving.
    pub to: Point,
}

/// The Ratsnest's Pad To Pad mode: from each Pad of the Footprints `moving` that is on a
/// Net, a line to the nearest Pad on that Net of a Footprint not moving, whatever copper
/// joins them already.
pub fn pad_to_pad(pcb: &PcbDocument, moving: &[Uuid]) -> Vec<PadLine> {
    let mut standing: BTreeMap<Uuid, Vec<Point>> = BTreeMap::new();
    for f in pcb.footprints().filter(|f| !moving.contains(&f.uuid)) {
        for pad in placed_pads(pcb, f) {
            if let Some(net) = pad.net {
                standing.entry(net).or_default().push(pad.at);
            }
        }
    }
    let distance = |a: Point, b: Point| ((a.x.0 - b.x.0) as f64).hypot((a.y.0 - b.y.0) as f64);
    let mut out = Vec::new();
    for f in pcb.footprints().filter(|f| moving.contains(&f.uuid)) {
        for pad in placed_pads(pcb, f) {
            let Some(net) = pad.net else { continue };
            let nearest = standing.get(&net).and_then(|pads| {
                pads.iter()
                    .copied()
                    .min_by(|a, b| distance(pad.at, *a).total_cmp(&distance(pad.at, *b)))
            });
            if let Some(to) = nearest {
                out.push(PadLine {
                    net,
                    from: pad.at,
                    to,
                });
            }
        }
    }
    out
}
