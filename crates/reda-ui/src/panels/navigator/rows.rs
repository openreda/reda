//! What the Board navigator lists in each mode, and what clicking a row does: pure
//! functions of the board, so the drawing in [`super`] only lays them out.

use reda_engine::model::geom::Nm;
use reda_engine::model::{PcbDocument, Side, Uuid, ZoneFills};
use reda_engine::pcb::{RatsnestVisibility, ZoneListing, natural_cmp, select, zone_listing};
use serde_json::json;

use super::Invocation;

/// What clicking a row does; any mix of the three.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OnClick {
    /// Highlight what the row stands for, dimming the rest; off, the highlight clears.
    pub dim: bool,
    /// Select it.
    pub select: bool,
    /// Zoom the canvas to it.
    pub zoom: bool,
}

impl Default for OnClick {
    fn default() -> OnClick {
        OnClick {
            dim: true,
            select: true,
            zoom: true,
        }
    }
}

/// Whether `text` matches the search box: every word of `search` appears in it, ignoring
/// case. An empty search matches everything.
pub fn matches(text: &str, search: &str) -> bool {
    let text = text.to_lowercase();
    search
        .split_whitespace()
        .all(|word| text.contains(&word.to_lowercase()))
}

/// One Net in Nets mode.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NetRow {
    pub uuid: Uuid,
    pub name: String,
    /// Its colour as `#rrggbb`, if set.
    pub color: Option<String>,
    /// Whether its Ratsnest lines show.
    pub ratsnest: bool,
    pub tracks: usize,
    /// The length of all its Tracks.
    pub length: Nm,
}

/// The Nets matching `search`, by name in natural order.
pub fn nets(pcb: &PcbDocument, ratsnest: &RatsnestVisibility, search: &str) -> Vec<NetRow> {
    let mut rows: Vec<NetRow> = pcb
        .nets()
        .filter(|n| matches(&n.name, search))
        .map(|n| {
            let tracks: Vec<_> = pcb.tracks().filter(|t| t.net == Some(n.uuid)).collect();
            let length: f64 = tracks.iter().map(|t| t.geometry.length()).sum();
            NetRow {
                uuid: n.uuid,
                name: n.name.clone(),
                color: n.color.clone(),
                ratsnest: !ratsnest.hidden_nets.contains(&n.uuid),
                tracks: tracks.len(),
                length: Nm(length.round() as i64),
            }
        })
        .collect();
    rows.sort_by(|a, b| natural_cmp(&a.name, &b.name).then(a.uuid.cmp(&b.uuid)));
    rows
}

/// One Footprint in Footprints mode.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FootprintRow {
    pub uuid: Uuid,
    pub designator: String,
    pub side: Side,
    pub locked: bool,
    pub not_fitted: bool,
}

/// The Footprints whose Designator matches `search`, in natural Designator order.
pub fn footprints(pcb: &PcbDocument, search: &str) -> Vec<FootprintRow> {
    let mut rows: Vec<FootprintRow> = pcb
        .footprints()
        .filter(|f| matches(&f.designator, search))
        .map(|f| FootprintRow {
            uuid: f.uuid,
            designator: f.designator.clone(),
            side: f.side,
            locked: f.locked,
            not_fitted: f.not_fitted,
        })
        .collect();
    rows.sort_by(|a, b| natural_cmp(&a.designator, &b.designator).then(a.uuid.cmp(&b.uuid)));
    rows
}

/// The Zones per copper Layer in pour order, as `pcb.zone.list` gives them, keeping the
/// ones whose name or Net name matches `search`. Layers with no Zones are left out.
pub fn zones(pcb: &PcbDocument, fills: Option<&ZoneFills>, search: &str) -> ZoneListing {
    let mut listing = zone_listing(pcb, fills, None);
    for layer in &mut listing.layers {
        layer.zones.retain(|z| {
            let net = z.net.and_then(|n| pcb.net(n)).map(|n| n.name.as_str());
            matches(&z.name, search) || net.is_some_and(|n| matches(n, search))
        });
    }
    listing.layers.retain(|l| !l.zones.is_empty());
    listing
}

/// `order` with `moved` taken out and put back just before `target` (or just after it,
/// with `after`): the pour order a drop asks for. Unchanged when either is missing.
pub fn reordered(order: &[Uuid], moved: Uuid, target: Uuid, after: bool) -> Vec<Uuid> {
    if moved == target || !order.contains(&moved) || !order.contains(&target) {
        return order.to_vec();
    }
    let mut out: Vec<Uuid> = order.iter().copied().filter(|z| *z != moved).collect();
    let at = out.iter().position(|z| *z == target).unwrap_or(out.len());
    out.insert(if after { at + 1 } else { at }, moved);
    out
}

/// What a row stands for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Net(Uuid),
    /// A Footprint or a Zone: one object.
    Object(Uuid),
}

/// The Commands a click on `target`'s row runs on board `document`.
pub fn click(pcb: &PcbDocument, document: Uuid, target: Target, on: OnClick) -> Vec<Invocation> {
    // What draws the target: a Net's copper and the Footprints with a Pad on it.
    let drawn: Vec<Uuid> = match target {
        Target::Net(net) => crate::cross_probe::pcb_objects_of(pcb, net),
        Target::Object(object) => vec![object],
    };
    let mut out = Vec::new();
    if on.dim {
        out.push(("view.highlight".to_owned(), json!({ "objects": drawn })));
    } else {
        out.push(("view.highlight.clear".to_owned(), json!({})));
    }
    if on.select {
        let params = match target {
            Target::Net(net) => {
                let picked = select::net(pcb, net);
                json!({"objects": picked.objects, "pads": picked.pads, "nets": [net]})
            }
            Target::Object(object) => json!({ "objects": [object] }),
        };
        out.push(("selection.set".to_owned(), params));
    }
    if on.zoom && !drawn.is_empty() {
        out.push((
            "view.zoom_to".to_owned(),
            json!({"document": document, "objects": drawn}),
        ));
    }
    out
}
