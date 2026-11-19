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
    /// Locked against geometry changes; left out when false.
    #[serde(default, skip_serializing_if = "is_false")]
    pub locked: bool,
    /// False opts both ends out of derived Teardrops.
    #[serde(default = "super::yes", skip_serializing_if = "super::is_true")]
    pub teardrop: bool,
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
/// `from` is the side the drill or laser enters.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LayerSpan {
    pub from: Uuid,
    pub to: Uuid,
}

impl LayerSpan {
    pub fn reaches(self, pcb: &super::PcbDocument, side: crate::Side) -> bool {
        pcb.layers()
            .find(crate::LayerKind::Copper, Some(side))
            .is_some_and(|layer| layer.uuid == self.from || layer.uuid == self.to)
    }

    pub fn joins(self, other: Self) -> bool {
        self == other || (self.from == other.to && self.to == other.from)
    }
}

/// A plated hole joining copper on the Layers of its span.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Via {
    pub uuid: Uuid,
    /// Locked against geometry changes; left out when false.
    #[serde(default, skip_serializing_if = "is_false")]
    pub locked: bool,
    /// The centre.
    pub at: Point,
    /// Its PCB Net; left out for a Via on no Net.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub net: Option<Uuid>,
    /// The copper pad diameter.
    pub diameter: super::ViaDiameter,
    /// The drilled hole diameter.
    pub hole: Nm,
    /// The copper Layers it joins.
    pub span: LayerSpan,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mask: Option<crate::Mask>,
    #[serde(default, skip_serializing_if = "super::UnusedLayers::is_keep")]
    pub unused_layers: super::UnusedLayers,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub zone_connection: Option<crate::ZoneConnection>,
    #[serde(default = "super::yes", skip_serializing_if = "super::is_true")]
    pub teardrop: bool,
}

impl Via {
    /// Copper diameter on a Layer of this Via's span, in drilling order.
    pub fn diameter_on(&self, layer: Uuid) -> Nm {
        if layer == self.span.from {
            self.diameter.start
        } else if layer == self.span.to {
            self.diameter.end.unwrap_or(self.diameter.start)
        } else {
            self.diameter.middle.unwrap_or(self.diameter.start)
        }
    }
}

impl Via {
    /// Change drilling direction on the same span without moving copper between Layers.
    /// A different span carries its start, middle and end sizes to the new Layers.
    pub fn set_span(&mut self, span: LayerSpan) {
        if self.span != span && self.span.joins(span) {
            let old = self.diameter;
            let start = old.end.unwrap_or(old.start);
            let middle = old.middle.unwrap_or(old.start);
            self.diameter = super::ViaDiameter {
                start,
                middle: (middle != start).then_some(middle),
                end: (old.start != start).then_some(old.start),
            };
        }
        self.span = span;
    }
}

fn is_false(b: &bool) -> bool {
    !*b
}
