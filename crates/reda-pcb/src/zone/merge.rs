//! A board's fills Layer by Layer and Net by Net, the way outputs plot them: Zones of one
//! Net on one Layer may overlap, so their fills are joined into one copper area.

use std::collections::BTreeMap;

use reda_model::{FillShape, PcbDocument, Uuid, ZoneFills};

use super::{fill, poly};

/// The copper the fills put on one Layer for one Net.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LayerFill {
    pub layer: Uuid,
    pub net: Option<Uuid>,
    /// Outer rings counter-clockwise, holes clockwise; no two overlap.
    pub shapes: Vec<FillShape>,
}

/// The fills of `pcb`'s Zones grouped by Layer (in Layer order) and Net (by UUID, no Net
/// first). A group of one Zone keeps its fill as it is; several are joined.
pub fn layer_fills(pcb: &PcbDocument, fills: &ZoneFills) -> Vec<LayerFill> {
    let order: Vec<Uuid> = pcb.layers().layers.iter().map(|l| l.uuid).collect();
    let mut groups: BTreeMap<(usize, Option<Uuid>), Vec<&Vec<FillShape>>> = BTreeMap::new();
    for zone in pcb.zones() {
        let (Some(fill), Some(layer)) = (
            fills.zones.get(&zone.uuid),
            order.iter().position(|l| *l == zone.layer),
        ) else {
            continue;
        };
        if !fill.shapes.is_empty() {
            groups
                .entry((layer, zone.net))
                .or_default()
                .push(&fill.shapes);
        }
    }
    groups
        .into_iter()
        .map(|((layer, net), zones)| {
            let shapes = match zones.as_slice() {
                [one] => (*one).clone(),
                many => {
                    let rings = many.iter().flat_map(|z| fill::to_area(z)).collect();
                    fill::to_shapes(&poly::union(rings))
                }
            };
            LayerFill {
                layer: order[layer],
                net,
                shapes,
            }
        })
        .collect()
}
