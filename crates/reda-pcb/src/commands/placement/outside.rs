//! `pcb.arrange_outside_board` and `pcb.swap`.

use std::collections::{BTreeMap, BTreeSet};

use reda_command::{
    AvailabilityContext, Command, CommandError, Context, Kind, Meta, Registry, Unavailable,
};
use reda_model::geom::Millideg;
use reda_model::{
    ComponentRef, DocumentKind, FootprintInstance, InDocument, Object, Project, Side, Uuid,
};
use schemars::JsonSchema;
use serde::Deserialize;

use super::arrange::natural_cmp;
use super::{Item, Op, Placed, place, resolve};
use crate::cluster;
use crate::commands::{needs_pcb, pcb};

pub(super) fn register_all(registry: &mut Registry) {
    registry.add(ArrangeOutsideBoard);
    registry.add(Swap);
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ArrangeOutsideBoardParams {
    /// The Footprints to re-cluster, by UUID.
    pub footprints: Vec<Uuid>,
    /// Moves locked Footprints too; without it a locked one refuses the whole Command.
    #[serde(default)]
    pub include_locked: bool,
}

pub struct ArrangeOutsideBoard;

impl Command for ArrangeOutsideBoard {
    const META: Meta = Meta {
        id: "pcb.arrange_outside_board",
        title: "Arrange Outside Board",
        description: "Re-clusters Footprints outside the board with the Sync cluster \
                      packing, so they look freshly synced: right of the Board Outline and \
                      of every other placed Footprint, one block per Schematic Sheet (then \
                      one for Mechanical Parts), rows in natural Designator order, on the \
                      top side at rotation 0. Refused when any of them is locked, unless \
                      `include_locked`.",
        category: "pcb",
        icon: "arrange",
        kind: Kind::Edit,
    };
    type Params = ArrangeOutsideBoardParams;
    type Output = Placed;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: ArrangeOutsideBoardParams,
    ) -> Result<Placed, CommandError> {
        let project = cx.project()?;
        let pcb = pcb(cx)?;
        let document = pcb.uuid;
        if p.footprints.is_empty() {
            return Err(CommandError::invalid_params(
                "/footprints",
                "nothing is selected",
            ));
        }
        let mut instances: Vec<&FootprintInstance> = Vec::new();
        for (i, uuid) in p.footprints.iter().enumerate() {
            let f = pcb.footprint(*uuid).ok_or_else(|| {
                CommandError::invalid_params(
                    &format!("/footprints/{i}"),
                    format!("{uuid} is not a Footprint on the board"),
                )
            })?;
            if !instances.iter().any(|g| g.uuid == f.uuid) {
                instances.push(f);
            }
        }
        crate::lock::refuse(
            pcb,
            instances.iter().map(|f| f.uuid),
            p.include_locked,
            "move",
        )?;

        // One block per sheet in sheet order, Footprints with no sheet last; natural
        // Designator order inside each.
        let sheets = sheet_ranks(project);
        let mut by_block: BTreeMap<usize, Vec<&FootprintInstance>> = BTreeMap::new();
        for f in &instances {
            let rank = f
                .component
                .as_ref()
                .and_then(|c| sheets.get(c))
                .copied()
                .unwrap_or(usize::MAX);
            by_block.entry(rank).or_default().push(f);
        }
        for block in by_block.values_mut() {
            block.sort_by(|a, b| natural_cmp(&a.designator, &b.designator).then(a.uuid.cmp(&b.uuid)));
        }
        let blocks: Vec<Vec<&FootprintInstance>> = by_block.into_values().collect();
        let copies: Vec<Vec<&reda_model::Footprint>> = blocks
            .iter()
            .map(|block| {
                block
                    .iter()
                    .filter_map(|f| pcb.footprint_copy(f.footprint).map(|c| &c.footprint))
                    .collect()
            })
            .collect();
        if copies.iter().map(Vec::len).sum::<usize>() != instances.len() {
            return Err(CommandError::internal(
                "a Footprint instance has no Footprint copy",
            ));
        }
        let moving: BTreeSet<Uuid> = instances.iter().map(|f| f.uuid).collect();
        let origins = cluster::pack(pcb, &moving, &copies);
        let mut moved = Vec::new();
        for (block, at) in blocks.iter().zip(&origins) {
            for (f, at) in block.iter().zip(at) {
                let mut f = (*f).clone();
                f.at = *at;
                f.rotation = Millideg::ZERO;
                f.side = Side::Top;
                moved.push(f);
            }
        }
        let objects = moved.iter().map(|f| f.uuid).collect();
        let tx = cx.tx()?;
        for f in moved {
            tx.replace(Object::FootprintInstance(InDocument::new(document, f)))?;
        }
        Ok(Placed { objects })
    }
}

/// Each Component's sheet, as Sync takes it: the sheet of its first Unit (the lowest Symbol
/// Unit, on the earliest sheet), by index in Document order.
fn sheet_ranks(project: &Project) -> BTreeMap<ComponentRef, usize> {
    let mut best: BTreeMap<ComponentRef, (usize, usize)> = BTreeMap::new();
    let sheets = project
        .documents()
        .filter(|d| d.kind == DocumentKind::Schematic)
        .filter_map(|d| project.schematic_sheet(d.uuid));
    for (sheet_index, sheet) in sheets.enumerate() {
        for unit in sheet.units() {
            let unit_index = sheet
                .part(unit.part)
                .and_then(|e| e.symbol.units.iter().position(|u| u.uuid == unit.unit))
                .unwrap_or(usize::MAX);
            let rank = (unit_index, sheet_index);
            let entry = best.entry(unit.component.clone()).or_insert(rank);
            if rank < *entry {
                *entry = rank;
            }
        }
    }
    best.into_iter().map(|(c, (_, sheet))| (c, sheet)).collect()
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SwapParams {
    /// One Footprint or Free Pad, by UUID.
    pub a: Uuid,
    /// The other.
    pub b: Uuid,
    /// Swaps locked objects too; without it a locked one is refused.
    #[serde(default)]
    pub include_locked: bool,
}

pub struct Swap;

impl Command for Swap {
    const META: Meta = Meta {
        id: "pcb.swap",
        title: "Swap",
        description: "Exchanges the positions of two Footprints (or Free Pads): each origin \
                      goes where the other's was. Each keeps its own rotation and side. \
                      Refused when either is locked, unless `include_locked`.",
        category: "pcb",
        icon: "move",
        kind: Kind::Edit,
    };
    type Params = SwapParams;
    type Output = Placed;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: SwapParams) -> Result<Placed, CommandError> {
        if p.a == p.b {
            return Err(CommandError::invalid_params(
                "/b",
                "pick two different objects to swap",
            ));
        }
        let pcb = pcb(cx)?;
        let items = resolve(pcb, &[p.a, p.b])?;
        for (item, at) in items.iter().zip(["/a", "/b"]) {
            if !matches!(item, Item::Footprint(_) | Item::FreePad(_)) {
                return Err(CommandError::invalid_params(
                    at,
                    "only Footprints and Free Pads swap",
                ));
            }
        }
        let (a, b) = (items[0].at(), items[1].at());
        place(
            cx,
            items,
            [Op::Translate(b - a), Op::Translate(a - b)],
            p.include_locked,
        )
    }
}
