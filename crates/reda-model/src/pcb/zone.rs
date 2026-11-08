//! Zones (copper fills on a Net) and Keepouts (areas that forbid copper or Vias), plus the
//! fill data computed for Zones, which is never stored in the Document.

use std::collections::BTreeMap;

use reda_geom::{Nm, Point, Vertex};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::Uuid;

/// How a Zone's fill treats copper of its own Net.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PourOver {
    /// Same-Net Zones merge; same-Net Tracks keep clear by the clearance.
    #[default]
    ZonesOnly,
    /// Same-Net Zones and Tracks merge.
    All,
    /// Same-Net Zones and Tracks keep clear by the clearance.
    None,
}

/// A solid copper fill on one copper Layer, inside its outline, keeping clear of other
/// Nets' copper. Its fill is computed on demand and cached outside the Document. How it
/// joins Pads and Vias of its Net comes from the `zone_connection` rule.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Zone {
    pub uuid: Uuid,
    /// Its typed name, unique on the board. Left out, it shows a name derived from the
    /// board's [`ZoneNameScheme`], which is never stored.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The copper Layer it fills.
    pub layer: Uuid,
    /// Its PCB Net; left out for a fill on no Net.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub net: Option<Uuid>,
    /// The outline the fill stays inside; the last vertex joins the first. Empty while
    /// the Zone [follows the Board Outline](Self::follows_outline).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub vertices: Vec<Vertex>,
    /// Whether the outline is the Board Outline, following its later edits; the fill
    /// keeps the edge clearance from it. Left out when false.
    #[serde(default, skip_serializing_if = "is_false")]
    pub follows_outline: bool,
    /// The gap kept to other Nets' copper. Left out, the clearance rule applies; set, the
    /// larger of it and the rule applies.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clearance: Option<Nm>,
    /// The narrowest copper the fill may leave: thinner necks are removed. Left out, the
    /// Net's `track_width` minimum.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_width: Option<Nm>,
    /// How the fill treats same-Net Zones and Tracks.
    pub pour_over: PourOver,
    /// Whether pieces of fill that touch no copper of the Net are dropped. No effect on a
    /// Zone with no Net.
    pub remove_unconnected: bool,
    /// Pieces of fill smaller than this, in square nanometres, are dropped, connected or
    /// not.
    pub min_island_area: i64,
    /// Where Zones overlap, the higher priority fills first and the others keep clear
    /// of it.
    pub priority: u32,
    /// Whether it is shelved: kept in the file but hidden from the canvas and skipped by
    /// fill, DRC, connectivity and outputs. Left out when false.
    #[serde(default, skip_serializing_if = "is_false")]
    pub shelved: bool,
}

fn is_false(b: &bool) -> bool {
    !*b
}

/// How a Zone with no typed name is named: its Net (`NONET` for none), its copper Layer
/// as `Lxx` (copper number) or by name, and `Pxxx`, counting the Zones of that Net and
/// Layer in priority order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub enum ZoneNameScheme {
    /// Such as `GND_L01_P000`.
    #[default]
    #[serde(rename = "NET_LXX_PXXX")]
    NetLxxPxxx,
    /// Such as `L01_GND_P000`.
    #[serde(rename = "LXX_NET_PXXX")]
    LxxNetPxxx,
    /// Such as `GND_Top_P000`.
    #[serde(rename = "NET_LAYERNAME_PXXX")]
    NetLayernamePxxx,
    /// Such as `Top_GND_P000`.
    #[serde(rename = "LAYERNAME_NET_PXXX")]
    LayernameNetPxxx,
}

impl ZoneNameScheme {
    pub fn is_default(&self) -> bool {
        *self == ZoneNameScheme::default()
    }
}

/// A Zone Cutout: removes the fill of every Zone on its copper Layer inside its outline,
/// exactly to its edge. It belongs to no one Zone and is never plotted.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ZoneCutout {
    pub uuid: Uuid,
    /// The copper Layer whose Zones it cuts.
    pub layer: Uuid,
    /// The outline; the last vertex joins the first.
    pub vertices: Vec<Vertex>,
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

/// The fills of the Zones of one PCB Document. Each Zone's fill carries the hash of the
/// inputs it was computed from: when the Zone's hash differs, its fill is stale.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ZoneFills {
    /// Each Zone's fill, by Zone UUID.
    pub zones: BTreeMap<Uuid, ZoneFill>,
}

/// One Zone's fill and the hash of what it depends on.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ZoneFill {
    /// The hash of everything this Zone's fill depends on.
    pub inputs: String,
    /// The pieces of fill; empty for a Zone with nothing left to fill.
    pub shapes: Vec<FillShape>,
}
