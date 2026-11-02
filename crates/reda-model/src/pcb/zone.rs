//! Zones (copper fills on a Net) and Keepouts (areas that forbid copper or Vias), plus the
//! fill data computed for Zones, which is never stored in the Document.

use std::collections::BTreeMap;

use reda_geom::{Nm, Point, Vertex};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::Uuid;

/// How a Zone's fill joins the Pads of its own Net.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PadConnection {
    /// A gap around the Pad crossed by four spokes, so the Pad solders easily.
    Thermal,
    /// The fill runs straight into the Pad.
    Solid,
}

/// A solid copper fill on one copper Layer, inside its outline, keeping clear of other
/// Nets' copper. Its fill is computed on demand and cached outside the Document.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Zone {
    pub uuid: Uuid,
    /// The copper Layer it fills.
    pub layer: Uuid,
    /// Its PCB Net; left out for a fill on no Net.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub net: Option<Uuid>,
    /// The outline the fill stays inside; the last vertex joins the first.
    pub vertices: Vec<Vertex>,
    /// The gap kept to other Nets' copper; the Net Classes' clearance wins when larger.
    pub clearance: Nm,
    /// The narrowest copper the fill may leave: thinner necks are removed.
    pub min_width: Nm,
    pub pad_connection: PadConnection,
    /// With thermal relief: the gap around each Pad of the Net.
    pub thermal_gap: Nm,
    /// With thermal relief: the width of the spokes.
    pub thermal_spoke_width: Nm,
    /// Whether pieces of fill that touch no copper of the Net are dropped.
    pub remove_islands: bool,
    /// Where Zones overlap, the higher priority fills first and the others keep clear
    /// of it.
    pub priority: u32,
}

/// An area on some copper Layers where copper or Vias may not go.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Keepout {
    pub uuid: Uuid,
    /// The copper Layers it applies to, top to bottom.
    pub layers: Vec<Uuid>,
    /// The outline; the last vertex joins the first.
    pub vertices: Vec<Vertex>,
    /// Whether copper (Tracks, Pads, Zone fills) is forbidden inside.
    pub copper: bool,
    /// Whether Vias are forbidden inside.
    pub vias: bool,
}

/// One connected piece of a Zone's fill: an outer contour (counter-clockwise) and its
/// holes (clockwise).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FillShape {
    pub outer: Vec<Point>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub holes: Vec<Vec<Point>>,
}

/// The fills of every Zone of one PCB Document, with the hash of the inputs they were
/// computed from: when the board's hash differs, the fills are stale.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ZoneFills {
    /// The hash of everything the fill depends on.
    pub inputs: String,
    /// Each Zone's fill, by Zone UUID; an empty list for a Zone with nothing left to fill.
    pub zones: BTreeMap<Uuid, Vec<FillShape>>,
}
