//! Routed copper: Tracks and Vias.

use reda_geom::{Millideg, Nm, Point};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::Uuid;

/// One copper segment on a copper Layer. A route is a chain of Tracks that share ends.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Track {
    pub uuid: Uuid,
    /// The copper Layer it is on.
    pub layer: Uuid,
    /// Its PCB Net; left out for copper on no Net.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub net: Option<Uuid>,
    /// The copper width; the ends are round.
    pub width: Nm,
    pub geometry: TrackGeometry,
}

/// The centreline of a [`Track`]. v0.1 creates only lines.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TrackGeometry {
    /// A straight segment.
    Line { from: Point, to: Point },
    /// A circular arc from `from` to `to`, sweeping `sweep` (counter-clockwise when
    /// positive).
    Arc {
        from: Point,
        to: Point,
        sweep: Millideg,
    },
}

impl TrackGeometry {
    /// The two ends.
    pub fn ends(&self) -> (Point, Point) {
        match *self {
            TrackGeometry::Line { from, to } | TrackGeometry::Arc { from, to, .. } => (from, to),
        }
    }

    /// The length of the centreline in nm: an arc's along its curve. 0 when the ends
    /// coincide.
    pub fn length(&self) -> f64 {
        let (from, to) = self.ends();
        let chord = ((to.x.0 - from.x.0) as f64).hypot((to.y.0 - from.y.0) as f64);
        match *self {
            TrackGeometry::Line { .. } => chord,
            TrackGeometry::Arc { sweep, .. } => {
                let angle = (sweep.0 as f64 / 1000.0).to_radians().abs();
                let half = (angle / 2.0).sin();
                if chord == 0.0 || half == 0.0 {
                    chord
                } else {
                    chord / (2.0 * half) * angle
                }
            }
        }
    }
}

/// The copper Layers a Via joins: every copper Layer from `from` to `to` in Layer order.
/// `from` is the upper Layer, so a through Via is stored top to bottom. `from` is also the
/// side it is drilled from, which matters only once blind Vias come.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LayerSpan {
    pub from: Uuid,
    pub to: Uuid,
}

/// A plated hole joining copper on the Layers of its span.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Via {
    pub uuid: Uuid,
    /// The centre.
    pub at: Point,
    /// Its PCB Net; left out for a Via on no Net.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub net: Option<Uuid>,
    /// The copper pad diameter.
    pub diameter: Nm,
    /// The drilled hole diameter.
    pub hole: Nm,
    /// The copper Layers it joins.
    pub span: LayerSpan,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mask: Option<crate::Mask>,
}
