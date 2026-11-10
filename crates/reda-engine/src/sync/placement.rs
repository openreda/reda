//! Where new Footprints go: each Sync places them in a fresh cluster, one block per
//! Schematic Sheet, packed by [`reda_pcb::cluster`].

use std::collections::BTreeSet;

use reda_model::PcbDocument;
use reda_model::geom::Point;

use super::components::SchComponent;

/// The origin of each new Footprint, in the order given, which must be sheet order, then
/// natural Designator order. Each run of one sheet is a block.
pub(crate) fn place(pcb: &PcbDocument, new: &[&SchComponent]) -> Vec<Point> {
    let blocks: Vec<Vec<_>> = new
        .chunk_by(|a, b| a.sheet_index == b.sheet_index)
        .map(|block| block.iter().map(|c| &c.copy.footprint).collect())
        .collect();
    reda_pcb::cluster::pack(pcb, &BTreeSet::new(), &blocks)
        .into_iter()
        .flatten()
        .collect()
}
