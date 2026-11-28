//! Guides: placed lines and points the cursor snaps to (docs/spec/v0.3/grids-guides.md#guides).
//!
//! A Guide is an editor aid. It sits on no Layer, shows on both sides of the board, and
//! no output, export, 3D view or check reads it. A PCB Document stores them in `guides`;
//! a Footprint will store the same rows.

use reda_geom::{Millideg, Point};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::Uuid;

/// Whether a Guide is an infinite line or a single point.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GuideKind {
    /// An infinite line through `at` at `angle`.
    Line,
    /// The point `at`.
    Point,
}

/// A placed Guide. Field order is the on-disk key order.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Guide {
    pub uuid: Uuid,
    pub kind: GuideKind,
    /// The point a line runs through, or the point itself.
    pub at: Point,
    /// A line's direction, counter-clockwise from +x, in `[0°, 180°)`; a point has none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub angle: Option<Millideg>,
    /// An optional name, shown in the Grids & Guides list.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Drawn on the canvas. A hidden Guide still snaps while it is enabled.
    pub shown: bool,
    /// The cursor snaps to it.
    pub enabled: bool,
    /// Locked against moves and deletes, as any placed object.
    pub locked: bool,
}

impl Guide {
    /// A shown, enabled, unlocked line Guide through `at` at `angle`.
    pub fn line(uuid: Uuid, at: Point, angle: Millideg) -> Guide {
        Guide {
            uuid,
            kind: GuideKind::Line,
            at,
            angle: Some(line_angle(angle)),
            name: None,
            shown: true,
            enabled: true,
            locked: false,
        }
    }

    /// A shown, enabled, unlocked point Guide at `at`.
    pub fn point(uuid: Uuid, at: Point) -> Guide {
        Guide {
            uuid,
            kind: GuideKind::Point,
            at,
            angle: None,
            name: None,
            shown: true,
            enabled: true,
            locked: false,
        }
    }

    /// Why the row is not a valid Guide, if it is not: a line needs an angle in
    /// `[0°, 180°)`, a point has none, and a name is not blank.
    pub fn problem(&self) -> Option<String> {
        match (self.kind, self.angle) {
            (GuideKind::Line, None) => {
                return Some(format!("line Guide {} has no angle", self.uuid));
            }
            (GuideKind::Line, Some(a)) if !(0..180_000).contains(&a.0) => {
                return Some(format!(
                    "line Guide {} has angle {} millidegrees; it must be in [0, 180000)",
                    self.uuid, a.0
                ));
            }
            (GuideKind::Point, Some(_)) => {
                return Some(format!("point Guide {} has an angle", self.uuid));
            }
            _ => {}
        }
        if self.name.as_deref().is_some_and(|n| n.trim().is_empty()) {
            return Some(format!("Guide {} has a blank name", self.uuid));
        }
        None
    }
}

/// `angle` as a line's direction: a line at θ and at θ + 180° is the same line, so the
/// angle is kept in `[0°, 180°)`.
pub fn line_angle(angle: Millideg) -> Millideg {
    Millideg(angle.0.rem_euclid(180_000))
}
