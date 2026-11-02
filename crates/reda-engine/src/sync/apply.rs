//! Carrying out a planned Change List in the open edit.

use std::collections::{BTreeMap, BTreeSet};

use reda_command::{CommandError, Context};
use reda_model::geom::{Millideg, Point};
use reda_model::{FootprintCopy, FootprintInstance, InDocument, Net, Object, PadNet, Side, Uuid};

use super::components::SchComponent;
use super::nets::Target;
use super::{ChangeKind, ChangeList, NO_PCB, Plan, placement};

/// What [`apply`] did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Applied {
    /// The ids of the changes carried out, in list order.
    pub applied: Vec<String>,
    /// The ids left out: those unticked and those depending on them, in list order.
    pub left_out: Vec<String>,
}

/// Applies the pending Change List, leaving out `unticked` and every change that depends
/// on them. Fails with `conflict` when the list no longer hashes to `hash` (the schematic
/// or the PCB changed since the preview), and with `invalid_params` at `/exclude/i` for an
/// id the list does not have.
pub fn apply(
    cx: &mut Context<'_>,
    hash: &str,
    unticked: &[String],
) -> Result<Applied, CommandError> {
    let plan = Plan::new(cx.project()?).ok_or_else(|| CommandError::unavailable(NO_PCB))?;
    if plan.list.hash != hash {
        return Err(CommandError::conflict(
            "the Change List no longer matches the preview; run sync.preview again",
        ));
    }
    let known: BTreeSet<&str> = plan.list.changes().map(|c| c.id.as_str()).collect();
    for (i, id) in unticked.iter().enumerate() {
        if !known.contains(id.as_str()) {
            return Err(CommandError::invalid_params(
                &format!("/exclude/{i}"),
                format!("the Change List has no change {id:?}"),
            ));
        }
    }
    let left_out = plan.list.cascade(&unticked.iter().cloned().collect());
    carry_out(cx, &plan, &left_out)?;
    let (left, applied): (Vec<_>, Vec<_>) = plan
        .list
        .changes()
        .map(|c| c.id.clone())
        .partition(|id| left_out.contains(id));
    Ok(Applied {
        applied,
        left_out: left,
    })
}

/// Carries out every change of `plan` but those in `left_out`, which must already hold
/// every change depending on one left out.
pub(super) fn carry_out(
    cx: &mut Context<'_>,
    plan: &Plan,
    left_out: &BTreeSet<String>,
) -> Result<(), CommandError> {
    Applier::new(cx)?.run(plan, left_out)
}

/// The PCB's Footprint instances as the apply leaves them, written out at the end.
struct Applier<'c, 'e> {
    cx: &'c mut Context<'e>,
    pcb: Uuid,
    instances: BTreeMap<Uuid, FootprintInstance>,
    removed: BTreeSet<Uuid>,
    /// Instances by Component UUID path key, for Pad changes.
    by_component: BTreeMap<String, Uuid>,
}

impl<'c, 'e> Applier<'c, 'e> {
    fn new(cx: &'c mut Context<'e>) -> Result<Applier<'c, 'e>, CommandError> {
        let pcb = cx
            .project()?
            .pcb()
            .ok_or_else(|| CommandError::unavailable(NO_PCB))?;
        let instances: BTreeMap<Uuid, FootprintInstance> =
            pcb.footprints().map(|i| (i.uuid, i.clone())).collect();
        let by_component = instances
            .values()
            .filter_map(|i| Some((super::component_key(i.component.as_ref()?), i.uuid)))
            .collect();
        Ok(Applier {
            pcb: pcb.uuid,
            cx,
            instances,
            removed: BTreeSet::new(),
            by_component,
        })
    }

    fn run(mut self, plan: &Plan, left_out: &BTreeSet<String>) -> Result<(), CommandError> {
        let list: &ChangeList = &plan.list;
        let included = || list.changes().filter(|c| !left_out.contains(&c.id));
        let doc = self.pcb;
        let default_class = self
            .cx
            .project()?
            .pcb()
            .expect("a PCB")
            .setup()
            .default_net_class;

        // Nets first, so Pads can point at new ones.
        let mut new_nets: BTreeMap<String, Uuid> = BTreeMap::new();
        for change in included() {
            match &change.change {
                ChangeKind::AddNet { name } => {
                    let net = Net {
                        uuid: self.cx.new_id(),
                        name: name.clone(),
                        class: default_class,
                    };
                    new_nets.insert(change.id.clone(), net.uuid);
                    self.cx
                        .tx()?
                        .insert(Object::Net(InDocument::new(doc, net)))?;
                }
                ChangeKind::RenameNet { net, to, .. } => {
                    let mut renamed = self.net(*net)?;
                    renamed.name = to.clone();
                    self.cx
                        .tx()?
                        .replace(Object::Net(InDocument::new(doc, renamed)))?;
                }
                _ => {}
            }
        }

        let mut adds: Vec<&SchComponent> = Vec::new();
        for change in included() {
            match &change.change {
                ChangeKind::RemoveComponent { instance, .. } => {
                    self.instances.remove(instance);
                    self.removed.insert(*instance);
                }
                ChangeKind::ChangeFootprint {
                    component,
                    instance,
                    ..
                } => {
                    let c = plan.components.get(component).expect("a planned Component");
                    let copy = self.ensure_copy(&c.copy)?;
                    let i = self
                        .instances
                        .get_mut(instance)
                        .expect("a planned instance");
                    i.footprint = copy.uuid;
                    i.pads = blank_pads(&copy);
                }
                ChangeKind::Designator { instance, to, .. } => {
                    self.instance(*instance).designator = to.clone();
                }
                ChangeKind::Fields { instance, to, .. } => {
                    self.instance(*instance).fields = to.clone();
                }
                ChangeKind::NotFitted { instance, to, .. } => {
                    self.instance(*instance).not_fitted = *to;
                }
                ChangeKind::AddComponent { component, .. } => {
                    adds.push(plan.components.get(component).expect("a planned Component"));
                }
                _ => {}
            }
        }

        // Adds come in sheet then Designator order, as the cluster wants them.
        let pcb = self.cx.project()?.pcb().expect("a PCB").clone();
        let positions = placement::place(&pcb, &adds);
        for (c, at) in adds.into_iter().zip(positions) {
            self.add(c, at)?;
        }

        for change in included() {
            if let ChangeKind::PadNet { component, pad, .. } = &change.change {
                let target = plan.nets.pad_targets[&change.id].as_ref();
                let net = match target {
                    None => None,
                    Some(Target::Existing(uuid)) => Some(*uuid),
                    Some(Target::New(add)) => Some(new_nets[add]),
                };
                let uuid = self.by_component[&super::component_key(component)];
                let entry = self
                    .instance(uuid)
                    .pads
                    .iter_mut()
                    .find(|p| p.pad == *pad)
                    .expect("a planned Pad");
                entry.net = net;
            }
        }

        let mut merges = Merges::default();
        for change in included() {
            if let ChangeKind::RemoveNet {
                net, merge_into, ..
            } = &change.change
            {
                merges.0.insert(*net, *merge_into);
            }
        }
        // Pads Sync does not manage (Mechanical Parts placed on the PCB) are copper too:
        // they follow a removed Net into its merge target, or go to no Net.
        for instance in self.instances.values_mut() {
            for pad in &mut instance.pads {
                if let Some(to) = merges.follow(pad.net) {
                    pad.net = to;
                }
            }
        }

        self.write(&pcb)?;
        self.reassign_copper(&merges)?;
        for &net in merges.0.keys() {
            self.cx.tx()?.remove(net)?;
        }
        self.prune_copies()
    }

    /// Sync never deletes copper: the copper on a removed Net that Sync does not manage
    /// through Pad Net changes moves to the Net it merged into, or to no Net. Every kind
    /// of copper that carries a Net belongs here (Zones too, once they exist).
    fn reassign_copper(&mut self, merges: &Merges) -> Result<(), CommandError> {
        if merges.0.is_empty() {
            return Ok(());
        }
        let mut moved = Vec::new();
        if let Some(pcb) = self.cx.project()?.pcb() {
            for track in pcb.tracks() {
                if let Some(to) = merges.follow(track.net) {
                    let mut track = track.clone();
                    track.net = to;
                    moved.push(Object::Track(InDocument::new(pcb.uuid, track)));
                }
            }
            for via in pcb.vias() {
                if let Some(to) = merges.follow(via.net) {
                    let mut via = via.clone();
                    via.net = to;
                    moved.push(Object::Via(InDocument::new(pcb.uuid, via)));
                }
            }
            for pad in pcb.free_pads() {
                if let Some(to) = merges.follow(pad.net) {
                    let mut pad = pad.clone();
                    pad.net = to;
                    moved.push(Object::FreePad(InDocument::new(pcb.uuid, pad)));
                }
            }
        }
        for object in moved {
            self.cx.tx()?.replace(object)?;
        }
        Ok(())
    }

    fn net(&self, uuid: Uuid) -> Result<Net, CommandError> {
        self.cx
            .project()?
            .pcb()
            .and_then(|p| p.net(uuid))
            .cloned()
            .ok_or_else(|| CommandError::internal(format!("planned Net {uuid} is gone")))
    }

    fn instance(&mut self, uuid: Uuid) -> &mut FootprintInstance {
        self.instances.get_mut(&uuid).expect("a planned instance")
    }

    /// The PCB's copy of this Footprint, inserted when the PCB has none.
    fn ensure_copy(&mut self, from: &FootprintCopy) -> Result<FootprintCopy, CommandError> {
        let existing = self
            .cx
            .project()?
            .pcb()
            .and_then(|p| p.footprint_copy_for(from.footprint.uuid, &from.hash))
            .cloned();
        if let Some(copy) = existing {
            return Ok(copy);
        }
        let copy = FootprintCopy {
            uuid: self.cx.new_id(),
            hash: from.hash.clone(),
            footprint: from.footprint.clone(),
        };
        self.cx.tx()?.insert(Object::FootprintCopy(InDocument::new(
            self.pcb,
            copy.clone(),
        )))?;
        Ok(copy)
    }

    fn add(&mut self, c: &SchComponent, at: Point) -> Result<(), CommandError> {
        let copy = self.ensure_copy(&c.copy)?;
        let instance = FootprintInstance {
            uuid: self.cx.new_id(),
            component: Some(c.key.clone()),
            footprint: copy.uuid,
            at,
            rotation: Millideg::ZERO,
            side: Side::Top,
            locked: false,
            designator: c.designator.clone(),
            fields: c.fields.clone(),
            not_fitted: c.not_fitted,
            designator_text: None,
            pads: blank_pads(&copy),
        };
        self.by_component
            .insert(super::component_key(&c.key), instance.uuid);
        self.instances.insert(instance.uuid, instance);
        Ok(())
    }

    /// Writes every instance that changed, and removes the removed ones.
    fn write(&mut self, before: &reda_model::PcbDocument) -> Result<(), CommandError> {
        let doc = self.pcb;
        for uuid in std::mem::take(&mut self.removed) {
            self.cx.tx()?.remove(uuid)?;
        }
        for instance in self.instances.values() {
            let object = Object::FootprintInstance(InDocument::new(doc, instance.clone()));
            match before.footprint(instance.uuid) {
                Some(old) if old == instance => {}
                Some(_) => self.cx.tx()?.replace(object)?,
                None => self.cx.tx()?.insert(object)?,
            }
        }
        Ok(())
    }

    /// Removes the PCB's Footprint copies no instance uses any more.
    fn prune_copies(&mut self) -> Result<(), CommandError> {
        let pcb = self.cx.project()?.pcb().expect("a PCB");
        let used: BTreeSet<Uuid> = pcb.footprints().map(|i| i.footprint).collect();
        let unused: Vec<Uuid> = pcb
            .footprint_copies()
            .map(|c| c.uuid)
            .filter(|u| !used.contains(u))
            .collect();
        for uuid in unused {
            self.cx.tx()?.remove(uuid)?;
        }
        Ok(())
    }
}

/// The Nets an apply removes, each with the Net it merged into (`None` = none).
#[derive(Default)]
struct Merges(BTreeMap<Uuid, Option<Uuid>>);

impl Merges {
    /// Where copper on `net` goes: `Some(target)` when `net` is removed (`target` = its
    /// merge target or no Net), `None` when it stays put.
    fn follow(&self, net: Option<Uuid>) -> Option<Option<Uuid>> {
        net.and_then(|n| self.0.get(&n).copied())
    }
}

/// One entry per Pad of the copy, sorted by Pad UUID, all on no Net.
fn blank_pads(copy: &FootprintCopy) -> Vec<PadNet> {
    let mut pads: Vec<PadNet> = copy
        .footprint
        .pads
        .iter()
        .map(|p| PadNet {
            pad: p.uuid,
            net: None,
        })
        .collect();
    pads.sort_by_key(|p| p.pad);
    pads
}
