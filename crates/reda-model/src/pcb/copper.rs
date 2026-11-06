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
}
