//! The Net block of Track Properties: a Net's routed length and routed share, and a
//! Track width's resistance and estimated current capacity.
//!
//! Resistance is copper at [`COPPER_RESISTIVITY`] with the Layer's Stackup thickness. The
//! current is the IPC-2221 estimate I = k · ΔT^0.44 · A^0.725, with A the cross-section in
//! mil² and k [`K_OUTER`] on outer copper or [`K_INNER`] on inner copper; it is always an
//! estimate.

use reda_model::geom::Nm;
use reda_model::{LayerKind, PcbDocument, Uuid};
use schemars::JsonSchema;
use serde::Serialize;

use crate::copper::{Connections, Copper};

/// Copper's resistivity, in Ω·m.
pub const COPPER_RESISTIVITY: f64 = 1.72e-8;
/// The IPC-2221 constant for outer copper.
pub const K_OUTER: f64 = 0.048;
/// The IPC-2221 constant for inner copper.
pub const K_INNER: f64 = 0.024;
/// The temperature rise the estimate uses unless told otherwise, in °C.
pub const DEFAULT_DELTA_T: f64 = 10.0;

const NM_PER_MIL: f64 = 25_400.0;

/// The estimated maximum current in A through copper `width` × `thickness`, warming by
/// `delta_t` °C, on an outer Layer or an inner one.
pub fn max_current(width: Nm, thickness: Nm, outer: bool, delta_t: f64) -> f64 {
    let area = (width.0 as f64 / NM_PER_MIL) * (thickness.0 as f64 / NM_PER_MIL);
    let k = if outer { K_OUTER } else { K_INNER };
    k * delta_t.powf(0.44) * area.powf(0.725)
}

/// The resistance in Ω of `length` nm of copper `width` × `thickness`.
pub fn resistance(length: f64, width: Nm, thickness: Nm) -> f64 {
    COPPER_RESISTIVITY * (length * 1e-9) / ((width.0 as f64 * 1e-9) * (thickness.0 as f64 * 1e-9))
}

/// What the Net block shows for one Net and one Track width on one Layer.
#[derive(Clone, Debug, PartialEq, Serialize, JsonSchema)]
pub struct NetInfo {
    pub net: Uuid,
    pub name: String,
    /// Its Net Classes besides the Default Net Class, which every Net is in.
    pub classes: Vec<Uuid>,
    /// Its colour; left out when unset.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    pub loop_removal: bool,
    /// The length of all its Tracks.
    pub routed_length: Nm,
    /// Its connections, and how many copper makes.
    pub connections: Connections,
    /// The share of its connections copper makes, in percent.
    pub routed_percent: f64,
    /// The width the figures below are for.
    pub width: Nm,
    /// The copper Layer they are for, and its Stackup copper thickness.
    pub layer: Uuid,
    pub thickness: Nm,
    /// Whether the Layer is outer copper.
    pub outer: bool,
    /// The temperature rise the current estimate allows, in °C.
    pub delta_t: f64,
    /// The resistance of 1 mm of this width, in Ω.
    pub resistance_per_mm: f64,
    /// The Track's resistance in Ω, when a Track was given.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track_resistance: Option<f64>,
    /// The IPC-2221 estimate of the most current this width carries, in A. An estimate.
    pub max_current_estimate: f64,
}

/// The figures for `net`. The width and Layer come from `track` when given, else `width`
/// (or the Net Class's track width) on the top copper.
pub fn net_info(
    pcb: &PcbDocument,
    copper: &Copper,
    net: Uuid,
    track: Option<Uuid>,
    width: Option<Nm>,
    delta_t: f64,
) -> Result<NetInfo, String> {
    let record = pcb
        .net(net)
        .ok_or_else(|| format!("no Net has UUID {net}"))?;
    let track = match track {
        Some(uuid) => Some(
            pcb.track(uuid)
                .ok_or_else(|| format!("no Track has UUID {uuid}"))?,
        ),
        None => None,
    };
    let width = width.or(track.map(|t| t.width)).unwrap_or_else(|| {
        crate::rules::net_rules(pcb, Some(net))
            .track_width
            .default_size()
    });
    let layer = match track {
        Some(t) => t.layer,
        None => pcb
            .copper_layers()
            .next()
            .map(|l| l.uuid)
            .ok_or("the board has no copper Layer")?,
    };
    let outer = pcb
        .layers()
        .layers
        .iter()
        .find(|l| l.uuid == layer)
        .is_some_and(|l| l.kind == LayerKind::Copper && l.side.is_some());
    let thickness = pcb
        .stackup()
        .copper_of(layer)
        .map_or(reda_model::DEFAULT_COPPER, |c| c.thickness);
    let routed_length: f64 = pcb
        .tracks()
        .filter(|t| t.net == Some(net))
        .map(|t| t.geometry.length())
        .sum();
    let connections = copper.connections(net);
    Ok(NetInfo {
        net,
        name: record.name.clone(),
        classes: record.classes.clone(),
        color: record.color.clone(),
        loop_removal: record.loop_removal,
        routed_length: Nm(routed_length.round() as i64),
        connections,
        routed_percent: connections.percent(),
        width,
        layer,
        thickness,
        outer,
        delta_t,
        resistance_per_mm: resistance(1e6, width, thickness),
        track_resistance: track.map(|t| resistance(t.geometry.length(), width, thickness)),
        max_current_estimate: max_current(width, thickness, outer, delta_t),
    })
}
