//! Snapping on the board: to electrical points (Pad centres, Track ends, Via centres)
//! near the cursor, else to the snap grid (the shared
//! [`snap_to_grid`](crate::canvas::interact::snap_to_grid)). Unlike a sheet's
//! [`SnapTargets`](crate::canvas::interact::SnapTargets), each point says what it is, for
//! the board's snap marker.

use reda_engine::model::geom::{Nm, Point};
use reda_engine::model::{PcbDocument, Uuid};
use reda_engine::pcb::placed::{placed_free_pad, placed_pads};

use crate::canvas::interact::snap_to_grid;

/// What a point snapped to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SnapKind {
    Grid,
    Pad,
    TrackEnd,
    Via,
}

/// An electrical point to snap to, and the object it belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Target {
    pub at: Point,
    pub kind: SnapKind,
    /// The Footprint instance, Free Pad, Track or Via.
    pub object: Uuid,
}

/// Every electrical point of `pcb`: Pad centres, Track ends and Via centres.
pub fn targets(pcb: &PcbDocument) -> Vec<Target> {
    let mut out = Vec::new();
    for instance in pcb.footprints() {
        for pad in placed_pads(pcb, instance) {
            out.push(Target {
                at: pad.at,
                kind: SnapKind::Pad,
                object: instance.uuid,
            });
        }
    }
    for pad in pcb.free_pads() {
        out.push(Target {
            at: placed_free_pad(pad).at,
            kind: SnapKind::Pad,
            object: pad.uuid,
        });
    }
    for track in pcb.tracks() {
        let (from, to) = track.geometry.ends();
        for at in [from, to] {
            out.push(Target {
                at,
                kind: SnapKind::TrackEnd,
                object: track.uuid,
            });
        }
    }
    for via in pcb.vias() {
        out.push(Target {
            at: via.at,
            kind: SnapKind::Via,
            object: via.uuid,
        });
    }
    out
}

/// The nearest of `targets` within `radius` of `p`.
pub fn nearest(targets: &[Target], p: Point, radius: Nm) -> Option<Target> {
    let r = radius.0 as f64;
    targets
        .iter()
        .map(|t| (distance(t.at, p), t))
        .filter(|(d, _)| *d <= r)
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, t)| *t)
}

/// `p` on the nearest grid point of pitch `grid`.
pub fn to_grid(p: Point, grid: Nm) -> Point {
    snap_to_grid((p.x.0 as f64, p.y.0 as f64), grid)
}

/// `p` snapped: to the nearest electrical target within `radius` when there is one,
/// else to the grid.
pub fn snap(p: Point, grid: Nm, targets: &[Target], radius: Nm) -> (Point, SnapKind) {
    match nearest(targets, p, radius) {
        Some(t) => (t.at, t.kind),
        None => (to_grid(p, grid), SnapKind::Grid),
    }
}

fn distance(a: Point, b: Point) -> f64 {
    let (dx, dy) = ((a.x.0 - b.x.0) as f64, (a.y.0 - b.y.0) as f64);
    (dx * dx + dy * dy).sqrt()
}
