//! The `stop` obstacle mode for drags: how far a drag can go before its copper breaks a
//! clearance rule it did not already break.

use std::collections::BTreeSet;

use reda_model::PcbDocument;
use reda_model::geom::{Nm, Point};

use super::Plan;
use crate::copper::{Copper, Item, Proposal};

/// The steps a drag is walked in before the first blocked one is narrowed down.
const MAX_STEPS: i64 = 256;
/// The finest walking step, in nm.
const MIN_STEP: f64 = 20_000.0;

/// The copper of other Nets the plan's Tracks and Vias come too close to.
pub fn hits(pcb: &PcbDocument, copper: &Copper, plan: &Plan) -> BTreeSet<Item> {
    let ignore = plan.touched();
    let mut out = BTreeSet::new();
    for track in plan.modified.iter().chain(&plan.created) {
        let proposal = Proposal::Track {
            layer: track.layer,
            geometry: track.geometry,
            width: track.width,
            net: track.net,
        };
        out.extend(
            copper
                .clearance(pcb, &proposal, &ignore)
                .into_iter()
                .map(|h| h.item),
        );
    }
    for via in &plan.vias {
        let proposal = Proposal::Via {
            at: via.at,
            diameter: via.diameter,
            net: via.net,
        };
        out.extend(
            copper
                .clearance(pcb, &proposal, &ignore)
                .into_iter()
                .map(|h| h.item),
        );
    }
    out
}

/// The furthest point along the straight drag from nothing to `want` that `plan_for`
/// keeps clear: its copper breaks no clearance rule it did not already break before the
/// drag (at `plan_for(0)`). The result may be off grid, as a route's `stop` end is. A
/// drag `plan_for` refuses counts as blocked.
pub fn last_clear(
    pcb: &PcbDocument,
    copper: &Copper,
    want: Point,
    mut plan_for: impl FnMut(Point) -> Option<Plan>,
) -> Point {
    if want == Point::default() {
        return want;
    }
    let before = plan_for(Point::default())
        .map(|p| hits(pcb, copper, &p))
        .unwrap_or_default();
    let mut clear = |t: f64| -> bool {
        let by = along(want, t);
        by == Point::default()
            || plan_for(by).is_some_and(|p| hits(pcb, copper, &p).is_subset(&before))
    };
    let length = (want.x.0 as f64).hypot(want.y.0 as f64);
    let steps = ((length / MIN_STEP).ceil() as i64).clamp(1, MAX_STEPS);
    let mut lo = 0.0;
    let mut hi = None;
    for i in 1..=steps {
        let t = i as f64 / steps as f64;
        if clear(t) {
            lo = t;
        } else {
            hi = Some(t);
            break;
        }
    }
    let Some(mut hi) = hi else {
        return want;
    };
    while (hi - lo) * length > 1.0 {
        let mid = (lo + hi) / 2.0;
        if clear(mid) {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    along(want, lo)
}

fn along(want: Point, t: f64) -> Point {
    Point::new(
        Nm((want.x.0 as f64 * t).trunc() as i64),
        Nm((want.y.0 as f64 * t).trunc() as i64),
    )
}
