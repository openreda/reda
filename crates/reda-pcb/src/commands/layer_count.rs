//! Structural copper edits. The preview and edit construct the same transaction plan.
use reda_command::{AvailabilityContext, Command, CommandError, Context, Kind, Meta, Unavailable};
use reda_model::geom::Nm;
use reda_model::{
    CopperEntry, CopperRole, InDocument, Layer, LayerKind, LayerSet, Object, PcbDocument, Project,
    Stackup, StackupEntry, Transaction, Uuid,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{needs_pcb, stackup::preview::Counts};

pub(crate) struct Plan {
    document: Uuid,
    layers: LayerSet,
    stack: Stackup,
    pub counts: Counts,
    deleted: Vec<Uuid>,
    changed: Vec<Object>,
}

impl Plan {
    fn new(pcb: &PcbDocument) -> Self {
        Self {
            document: pcb.uuid,
            layers: pcb.layers().clone(),
            stack: pcb.stackup().clone(),
            deleted: Vec::new(),
            changed: Vec::new(),
            counts: Counts::default(),
        }
    }

    pub fn apply(self, tx: &mut Transaction<'_>) -> Result<(), CommandError> {
        tx.replace(Object::LayerSet(InDocument::new(
            self.document,
            self.layers,
        )))?;
        tx.replace(Object::Stackup(InDocument::new(self.document, self.stack)))?;
        for uuid in self.deleted {
            tx.remove(uuid)?;
        }
        for obj in self.changed {
            tx.replace(obj)?;
        }
        Ok(())
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AddParams {
    pub document: Uuid,
    /// Split this dielectric, or the middle dielectric sheet when omitted.
    pub split: Option<Uuid>,
    /// The new copper Layer's UUID.
    pub uuid: Option<Uuid>,
}

#[derive(Serialize, JsonSchema)]
pub struct AddedLayer {
    pub uuid: Uuid,
    /// New sheet below the inserted Layer. The upper sheet keeps its UUID.
    pub dielectric: Uuid,
}

pub(super) struct Add;
impl Command for Add {
    const META: Meta = Meta {
        id: "pcb.layer.add",
        title: "Add Copper Layer",
        description: "Adds an inner copper Layer by splitting a dielectric sheet.",
        category: "pcb",
        icon: "layers",
        kind: Kind::Edit,
    };
    type Params = AddParams;
    type Output = AddedLayer;
    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }
    fn run(&self, cx: &mut Context<'_>, p: AddParams) -> Result<AddedLayer, CommandError> {
        let (project, ids) = cx.project_and_ids()?;
        let (plan, added) = add_plan(project, p, std::iter::from_fn(|| Some(ids.next_id())))?;
        plan.apply(cx.tx()?)?;
        Ok(added)
    }
}

pub(crate) fn add_plan(
    project: &Project,
    p: AddParams,
    ids: impl Iterator<Item = Uuid>,
) -> Result<(Plan, AddedLayer), CommandError> {
    let pcb = project
        .pcb_document(p.document)
        .ok_or_else(|| CommandError::not_found("no such PCB Document"))?;
    let mut ids = ids.filter(|id| !project.identity_in_use(*id));
    let uuid = match p.uuid {
        Some(id) if project.identity_in_use(id) => {
            return Err(CommandError::invalid_params(
                "/uuid",
                "that UUID is already in use",
            ));
        }
        Some(id) => id,
        None => ids.next().expect("unbounded ID source"),
    };
    let lower = ids.find(|id| *id != uuid).expect("unbounded ID source");
    let mut plan = Plan::new(pcb);
    let stack = &mut plan.stack;
    let copper: Vec<_> = stack.copper().copied().collect();
    if copper.len() >= 32 {
        return Err(CommandError::rejected(
            "the board already has 32 copper Layers",
        ));
    }
    let split = p.split.unwrap_or_else(|| {
        let middle = stack.dielectrics().count() / 2;
        stack
            .dielectrics()
            .nth(middle)
            .expect("a Stackup has dielectric")
            .uuid
    });
    let index = stack
        .entries
        .iter()
        .position(|e| matches!(e, StackupEntry::Dielectric(d) if d.uuid==split))
        .ok_or_else(|| {
            CommandError::invalid_params("/split", "choose a dielectric in this Stackup")
        })?;
    let before = stack.entries[..index]
        .iter()
        .filter(|e| matches!(e, StackupEntry::Copper(_)))
        .count();
    // Capture the physical sizes before adding copper. Otherwise the target thickness
    // would silently absorb the new copper by shrinking the remaining blank sheets.
    resolve_blanks(stack);
    if stack.dielectrics().any(|d| d.thickness.is_none()) {
        return Err(CommandError::rejected(
            "give the blank dielectric sheets a positive physical thickness first",
        ));
    }
    // The two halves of the split sheet are equally close. Compare the rest
    // of its gap, preferring the upper foil on a tie.
    let above = distance_to_copper(stack.entries[..index].iter().rev());
    let below = distance_to_copper(stack.entries[index + 1..].iter());
    let mut nearest = if above <= below { before - 1 } else { before };
    if copper.len() > 2 {
        nearest = nearest.clamp(1, copper.len() - 2);
    }
    let thickness = copper[nearest].thickness;
    let StackupEntry::Dielectric(upper) = &mut stack.entries[index] else {
        unreachable!()
    };
    let total = upper.thickness.unwrap().0;
    if total < 2 {
        return Err(CommandError::rejected(
            "the dielectric must be at least two nanometres thick to split",
        ));
    }
    let mut bottom = upper.clone();
    upper.thickness = Some(Nm(total / 2));
    bottom.uuid = lower;
    bottom.thickness = Some(Nm(total - total / 2));
    stack.entries.splice(
        index + 1..index + 1,
        [
            StackupEntry::Copper(CopperEntry {
                layer: uuid,
                thickness,
            }),
            StackupEntry::Dielectric(bottom),
        ],
    );
    if stack.checked_totals().is_none() {
        return Err(CommandError::rejected(
            "the Stackup would exceed the supported thickness",
        ));
    }
    insert_copper(&mut plan, pcb, before, [uuid]);
    Ok((
        plan,
        AddedLayer {
            uuid,
            dielectric: lower,
        },
    ))
}

/// Insert new signal Layers in one gap. Every span crossing it stretches once,
/// regardless of the number of new Layers.
fn insert_copper(
    plan: &mut Plan,
    pcb: &PcbDocument,
    before: usize,
    ids: impl IntoIterator<Item = Uuid>,
) {
    let lower = pcb.copper_layers().nth(before).unwrap().uuid;
    let at = plan
        .layers
        .layers
        .iter()
        .position(|l| l.uuid == lower)
        .unwrap();
    plan.layers.layers.splice(
        at..at,
        ids.into_iter().map(|uuid| Layer {
            uuid,
            kind: LayerKind::Copper,
            side: None,
            name: None,
            purpose: Default::default(),
            role: CopperRole::Signal,
        }),
    );
    let stretches = |a, b| {
        let a = pcb.copper_number(a).unwrap();
        let b = pcb.copper_number(b).unwrap();
        a.min(b) <= before && before < a.max(b)
    };
    plan.counts.vias_stretched = pcb
        .vias()
        .filter(|v| stretches(v.span.from, v.span.to))
        .count();
    plan.counts.via_types_changed = pcb
        .stackup()
        .via_types
        .iter()
        .filter(|t| stretches(t.from, t.to))
        .count();
}

fn distance_to_copper<'a>(entries: impl Iterator<Item = &'a StackupEntry>) -> i64 {
    entries
        .map_while(|entry| match entry {
            StackupEntry::Copper(_) => None,
            StackupEntry::Dielectric(sheet) => Some(sheet.thickness.expect("resolved sheet").0),
        })
        .sum()
}

/// Store equal shares to whole nanometres, distributing the remainder from the top.
fn resolve_blanks(stack: &mut Stackup) {
    let Some(total) = stack.totals().remainder else {
        return;
    };
    let blanks = stack
        .dielectrics()
        .filter(|d| d.thickness.is_none())
        .count() as i64;
    let share = total.0 / blanks;
    let mut extra = total.0 % blanks;
    for e in &mut stack.entries {
        if let StackupEntry::Dielectric(d) = e
            && d.thickness.is_none()
        {
            let size = share + i64::from(extra > 0);
            d.thickness = (size > 0).then_some(Nm(size));
            extra -= 1;
        }
    }
}

mod removal;
pub(crate) use removal::remove_plan;

fn inner(pcb: &PcbDocument, layer: Uuid) -> Result<usize, CommandError> {
    let row = pcb
        .layers()
        .layer(layer)
        .ok_or_else(|| CommandError::not_found("no such Layer"))?;
    if row.kind != LayerKind::Copper || row.side.is_some() {
        return Err(CommandError::rejected(
            "only inner copper Layers can be removed or moved",
        ));
    }
    Ok(pcb.copper_number(layer).unwrap() - 1)
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RemoveParams {
    pub layer: super::layer::LayerRef,
}

pub(super) struct Remove;
impl Command for Remove {
    const META: Meta = Meta {
        id: "pcb.layer.remove",
        title: "Remove Copper Layer",
        description: "Removes an inner copper Layer, its objects and empty spans, shortening surviving spans and updating rules.",
        category: "pcb",
        icon: "layers",
        kind: Kind::Edit,
    };
    type Params = RemoveParams;
    type Output = ();
    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }
    fn run(&self, cx: &mut Context<'_>, p: RemoveParams) -> Result<(), CommandError> {
        let pcb = super::pcb(cx)?;
        remove_plan(pcb, p.layer.resolve(pcb, "/layer")?.uuid)?.apply(cx.tx()?)
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Up,
    Down,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MoveParams {
    pub layer: super::layer::LayerRef,
    pub direction: Direction,
}

pub(super) fn move_plan(pcb: &PcbDocument, p: MoveParams) -> Result<Plan, CommandError> {
    let layer = p.layer.resolve(pcb, "/layer")?.uuid;
    let at = inner(pcb, layer)?;
    let next = match p.direction {
        Direction::Up => at - 1,
        Direction::Down => at + 1,
    };
    let other = pcb.copper_layers().nth(next).unwrap();
    if other.side.is_some() {
        return Err(CommandError::rejected(
            "the outer copper Layers must stay in place",
        ));
    }
    let other = other.uuid;
    let mut plan = Plan::new(pcb);
    let crosses = |span: reda_model::LayerSpan| {
        let moves = |id| id == layer || id == other;
        moves(span.from) != moves(span.to)
    };
    plan.counts.vias_using_span = pcb.vias().filter(|v| crosses(v.span)).count();
    plan.counts.via_types_changed = plan
        .stack
        .via_types
        .iter()
        .filter(|k| crosses(k.span()))
        .count();
    let layers = &mut plan.layers.layers;
    let a = layers.iter().position(|l| l.uuid == layer).unwrap();
    let b = layers.iter().position(|l| l.uuid == other).unwrap();
    layers.swap(a, b);
    let entries = &mut plan.stack.entries;
    let a = entries.iter().position(|e| e.uuid() == layer).unwrap();
    let b = entries.iter().position(|e| e.uuid() == other).unwrap();
    entries.swap(a, b);
    Ok(plan)
}

pub(super) struct Move;
impl Command for Move {
    const META: Meta = Meta {
        id: "pcb.layer.move",
        title: "Move Copper Layer",
        description: "Swaps an inner copper Layer with its inner neighbour. Objects and span ends keep their Layer identities.",
        category: "pcb",
        icon: "layers",
        kind: Kind::Edit,
    };
    type Params = MoveParams;
    type Output = ();
    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }
    fn run(&self, cx: &mut Context<'_>, p: MoveParams) -> Result<(), CommandError> {
        move_plan(super::pcb(cx)?, p)?.apply(cx.tx()?)
    }
}

pub(super) mod construction;
