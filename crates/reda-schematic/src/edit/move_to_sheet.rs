//! `schematic.selection.move_to_sheet`: moving a selection to another Schematic Sheet
//! with its identity kept, so the PCB stays linked.
//!
//! Unlike cut and paste, every UUID and Designator survives the move. The part-table entries
//! and Footprint copies the moved Units use move along: one the target sheet already has is
//! reused, one the source sheet still needs is copied, and the rest move with their UUIDs.
//! The Command is one transaction, but the change set splits into an independent entry on
//! each sheet's undo stack (see `ChangeSet::split_by_stack`).

use std::collections::{BTreeMap, BTreeSet};

use reda_command::{Command, CommandError, Context, Kind, Meta};
use reda_model::geom::Point;
use reda_model::{InDocument, Object, Uuid};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::bounds::union;
use super::selection::{one_sheet, prune_part_table, resolve};
use super::transform::{Placement, plan};
use crate::component::At;
use crate::connectivity::{ProjectNet, project_nets};
use crate::normalise::{Wiring, normalise};

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MoveToSheetParams {
    /// The objects to move, by UUID, all on one sheet: down to a single Unit.
    pub objects: Vec<Uuid>,
    /// The Schematic Sheet to move them to.
    pub document: Uuid,
    /// Where the lower-left corner of the selection's extent goes; the objects keep their
    /// coordinates when left out.
    pub at: Option<At>,
    /// How the Wires this edit touches are normalised, on both sheets; the fixed defaults
    /// when left out.
    pub wiring: Option<Wiring>,
}

/// What `schematic.selection.move_to_sheet` did.
#[derive(Debug, Serialize, JsonSchema)]
pub struct MoveToSheetResult {
    /// The moved objects on the target sheet, in selection order, ready to select. They
    /// keep their UUIDs, unless normalising merged a Wire into another.
    pub objects: Vec<Uuid>,
    /// The Nets the move changed, as pairs of the name a Net had and the name its Pins
    /// have now (the name the board gets). A Net Label is sheet-local, so moving one can
    /// split a Net: that gives one pair per piece. Sorted; empty when no Net changed.
    pub nets: Vec<NetChange>,
}

/// One Net the move renamed, split or joined.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, JsonSchema)]
pub struct NetChange {
    /// The Net's name before; `null` for Pins that were on no named Net.
    pub from: Option<String>,
    /// The name of the Net (some of) its Pins are on now.
    pub to: Option<String>,
}

pub struct MoveToSheet;

impl Command for MoveToSheet {
    const META: Meta = Meta {
        id: "schematic.selection.move_to_sheet",
        title: "Move to Sheet",
        description: "Moves the selected objects to another Schematic Sheet in one Command, \
                      keeping every UUID and Designator, so the Footprints, their placement \
                      and routing stay linked and Sync shows no Component changes. \
                      Part-table entries go with their Units. Objects keep their \
                      coordinates unless `at` is given. Returns the Nets that changed. Each \
                      sheet gets its own undo entry: undo on the target sheet first.",
        category: "schematic",
        icon: "move",
        kind: Kind::Edit,
    };
    type Params = MoveToSheetParams;
    type Output = MoveToSheetResult;

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: MoveToSheetParams,
    ) -> Result<MoveToSheetResult, CommandError> {
        let project = cx.project()?;
        let objects = resolve(project, &p.objects, "/objects")?;
        let source = one_sheet(&objects, "/objects")?;
        let target = p.document;
        let target_body = crate::sheet(project, target)?.clone();
        if source == target {
            return Err(CommandError::invalid_params(
                "/document",
                "the objects are on this sheet already; use schematic.move",
            ));
        }
        let source_body = crate::sheet(project, source)?.clone();
        let nets_before = project_nets(project);
        let offset = p.at.map_or(Point::default(), |at| {
            at.point() - union(&source_body, &objects).min
        });
        let moves: Vec<(Object, Placement)> = objects
            .iter()
            .map(|o| (o.clone(), Placement::Translate(offset)))
            .collect();
        let placed = plan(&source_body, &moves, false)?;

        let entries = move_part_table(cx, &source_body, &target_body, &objects)?;
        let tx = cx.tx()?;
        for uuid in placed.removed {
            tx.remove(uuid)?;
        }
        let mut moved = Vec::with_capacity(placed.replaced.len());
        for object in placed.replaced {
            tx.remove(object.uuid())?;
            moved.push(object.uuid());
            let object = match object {
                Object::Unit(mut u) => {
                    u.object.part = entries[&u.part];
                    Object::Unit(u)
                }
                other => other,
            };
            tx.insert(rehome(object, target))?;
        }
        let wiring = p.wiring.unwrap_or_default();
        prune_part_table(cx, source)?;
        normalise(cx, source, &source_body, &wiring)?;
        let lineage = normalise(cx, target, &target_body, &wiring)?;
        let nets = changed_nets(&nets_before, &project_nets(cx.project()?));
        Ok(MoveToSheetResult {
            objects: lineage.trace_all(&moved),
            nets,
        })
    }
}

/// Gives the target sheet the part-table entries (and their Footprint copies) the moved
/// Units use, and returns the entry each source entry maps to. A matching entry the target
/// already has is reused; an entry no Unit left behind uses moves with its UUID; one still
/// in use on the source sheet is copied under a fresh UUID. Footprint copies follow the same
/// rule. What the source no longer needs is pruned after the move.
fn move_part_table(
    cx: &mut Context<'_>,
    source: &reda_model::SchematicSheet,
    target: &reda_model::SchematicSheet,
    objects: &[Object],
) -> Result<BTreeMap<Uuid, Uuid>, CommandError> {
    let moving: BTreeSet<Uuid> = objects
        .iter()
        .filter_map(|o| match o {
            Object::Unit(u) => Some(u.uuid),
            _ => None,
        })
        .collect();
    let used: BTreeSet<Uuid> = (source.units())
        .filter(|u| moving.contains(&u.uuid))
        .map(|u| u.part)
        .collect();
    let staying: BTreeSet<Uuid> = (source.units())
        .filter(|u| !moving.contains(&u.uuid))
        .map(|u| u.part)
        .collect();

    let mut map = BTreeMap::new();
    let mut to_add = vec![];
    for &uuid in &used {
        let entry = source.part(uuid).expect("a Unit's part entry exists");
        if let Some(existing) = target.part_entry_for(entry.part.uuid, &entry.hash) {
            map.insert(uuid, existing.uuid);
            continue;
        }
        let mut entry = entry.clone();
        if staying.contains(&uuid) {
            entry.uuid = cx.new_id();
        } else {
            cx.tx()?.remove(uuid)?;
        }
        map.insert(uuid, entry.uuid);
        to_add.push(entry);
    }

    // The copies the source still lists once the moving entries are gone.
    let kept: BTreeSet<Uuid> = (source.parts())
        .filter(|e| !used.contains(&e.uuid) || staying.contains(&e.uuid))
        .flat_map(|e| e.footprints.iter().copied())
        .collect();
    let mut copies: BTreeMap<Uuid, Uuid> = BTreeMap::new();
    for entry in &mut to_add {
        for listed in &mut entry.footprints {
            if let Some(&new) = copies.get(listed) {
                *listed = new;
                continue;
            }
            let copy = source.footprint(*listed).expect("listed copies exist");
            let new = match target.footprint_copy_for(copy.footprint.uuid, &copy.hash) {
                Some(existing) => existing.uuid,
                None => {
                    let mut copy = copy.clone();
                    if kept.contains(listed) {
                        copy.uuid = cx.new_id();
                    } else {
                        cx.tx()?.remove(copy.uuid)?;
                    }
                    let uuid = copy.uuid;
                    cx.tx()?
                        .insert(Object::FootprintCopy(InDocument::new(target.uuid, copy)))?;
                    uuid
                }
            };
            copies.insert(*listed, new);
            *listed = new;
        }
    }
    for entry in to_add {
        cx.tx()?
            .insert(Object::PartEntry(InDocument::new(target.uuid, entry)))?;
    }
    Ok(map)
}

/// `object`, a sheet object, as it is on `document`.
fn rehome(object: Object, document: Uuid) -> Object {
    fn to<T>(o: InDocument<T>, document: Uuid) -> InDocument<T> {
        InDocument::new(document, o.object)
    }
    match object {
        Object::Unit(o) => Object::Unit(to(o, document)),
        Object::Wire(o) => Object::Wire(to(o, document)),
        Object::NetLabel(o) => Object::NetLabel(to(o, document)),
        Object::NoConnect(o) => Object::NoConnect(to(o, document)),
        Object::Port(o) => Object::Port(to(o, document)),
        Object::PowerPort(o) => Object::PowerPort(to(o, document)),
        Object::SheetText(o) => Object::SheetText(to(o, document)),
        Object::SheetGraphic(o) => Object::SheetGraphic(to(o, document)),
        _ => unreachable!("selections hold only sheet objects"),
    }
}

/// The Nets whose Pins are not grouped and named as they were: for every Net before and
/// every Net after that share a Pin, unless the Net is unchanged.
fn changed_nets(before: &[ProjectNet], after: &[ProjectNet]) -> Vec<NetChange> {
    type Pins = BTreeSet<(Uuid, Uuid)>;
    let groups = |nets: &[ProjectNet]| -> Vec<(Option<String>, Pins)> {
        nets.iter()
            .filter(|n| !n.pins.is_empty())
            .map(|n| {
                let pins = n.pins.iter().map(|p| (p.pin.unit, p.pin.pin)).collect();
                (n.board_name.clone(), pins)
            })
            .collect()
    };
    let (before, after) = (groups(before), groups(after));
    let mut out = BTreeSet::new();
    for was in before.iter().filter(|b| !after.contains(b)) {
        for now in after.iter().filter(|a| !a.1.is_disjoint(&was.1)) {
            out.insert(NetChange {
                from: was.0.clone(),
                to: now.0.clone(),
            });
        }
    }
    for now in after.iter().filter(|a| !before.contains(a)) {
        if before.iter().all(|b| b.1.is_disjoint(&now.1)) {
            out.insert(NetChange {
                from: None,
                to: now.0.clone(),
            });
        }
    }
    out.into_iter().collect()
}
