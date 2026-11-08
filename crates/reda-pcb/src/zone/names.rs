//! Zone names: a Zone's typed name, or one derived from the board's
//! [`ZoneNameScheme`]. Derived names are never stored, so they follow the Net, the Layer
//! order and the pour order as those change.

use std::collections::{BTreeMap, BTreeSet};

use reda_model::{LayerKind, PcbDocument, Side, Uuid, Zone, ZoneNameScheme};

/// What stands in for the Net of a Zone with none.
pub const NO_NET: &str = "NONET";

/// The name every Zone of `pcb` shows, by UUID: its typed name, or the derived one. Names
/// are unique: a derived name that a typed name already takes gets a `_2`, `_3`… suffix.
pub fn names(pcb: &PcbDocument) -> BTreeMap<Uuid, String> {
    let scheme = pcb.setup().zone_name_scheme;
    let typed: BTreeSet<&str> = pcb.zones().filter_map(|z| z.name.as_deref()).collect();
    let mut counts: BTreeMap<(Uuid, Option<Uuid>), usize> = BTreeMap::new();
    let mut taken: BTreeSet<String> = typed.iter().map(|s| (*s).to_owned()).collect();
    let mut out = BTreeMap::new();
    for zone in super::fill_order_all(pcb) {
        let count = counts.entry((zone.layer, zone.net)).or_default();
        let number = *count;
        *count += 1;
        if let Some(name) = &zone.name {
            out.insert(zone.uuid, name.clone());
            continue;
        }
        let base = derived(pcb, scheme, zone, number);
        let mut name = base.clone();
        let mut k = 2;
        while taken.contains(&name) {
            name = format!("{base}_{k}");
            k += 1;
        }
        taken.insert(name.clone());
        out.insert(zone.uuid, name);
    }
    out
}

/// The name Zone `zone` shows.
pub fn name(pcb: &PcbDocument, zone: Uuid) -> Option<String> {
    names(pcb).remove(&zone)
}

/// The name `scheme` gives `zone`, the `number`th Zone (from 0) of its Net and Layer in
/// priority order.
fn derived(pcb: &PcbDocument, scheme: ZoneNameScheme, zone: &Zone, number: usize) -> String {
    let net = zone
        .net
        .and_then(|n| pcb.net(n))
        .map_or(NO_NET, |n| n.name.as_str());
    let copper = pcb.copper_number(zone.layer).unwrap_or(0);
    let lxx = format!("L{copper:02}");
    let layer_name = layer_name(pcb, zone.layer);
    let p = format!("P{number:03}");
    match scheme {
        ZoneNameScheme::NetLxxPxxx => format!("{net}_{lxx}_{p}"),
        ZoneNameScheme::LxxNetPxxx => format!("{lxx}_{net}_{p}"),
        ZoneNameScheme::NetLayernamePxxx => format!("{net}_{layer_name}_{p}"),
        ZoneNameScheme::LayernameNetPxxx => format!("{layer_name}_{net}_{p}"),
    }
}

/// A copper Layer's name as Commands take it: `copper_top`, `copper_bottom`, or
/// `copper_l2` for inner copper.
fn layer_name(pcb: &PcbDocument, layer: Uuid) -> String {
    let found = pcb.layers().layer(layer);
    match found.map(|l| (l.kind, l.side)) {
        Some((LayerKind::Copper, Some(Side::Top))) => "copper_top".into(),
        Some((LayerKind::Copper, Some(Side::Bottom))) => "copper_bottom".into(),
        _ => format!("copper_l{}", pcb.copper_number(layer).unwrap_or(0)),
    }
}
