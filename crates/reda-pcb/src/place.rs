//! Moving Footprints (docs/spec/v0.1/pcb.md#moving-footprints): the placement modes the
//! `pcb.place.*` Preferences keep, and the pure checks behind them.
//!
//! - [`overlaps`]: the live overlap, Footprints whose Courtyards (or fallback boxes) come
//!   closer than `courtyard_clearance` once moved, exactly as DRC finds them.
//! - [`avoid`]: in [`ObstacleMode::Avoid`], how far a move gets before it would break
//!   `courtyard_clearance`: the last clear point on the straight path.

use std::collections::BTreeMap;

use reda_model::geom::{Nm, Point};
use reda_model::{FootprintInstance, PcbDocument, Side, Uuid};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::drc::placement::{courtyard_clearance, courtyards_or_box, too_close};
use crate::zone::poly::Area;

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
