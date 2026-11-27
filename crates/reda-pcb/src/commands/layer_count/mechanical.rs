//! The `pcb.layer.*` plans for Mechanical Layers: user-owned, named Layers after the Board
//! Outline with a Purpose and an optional side. None of them touches the Stackup.
use super::super::layer::LayerRef;
use super::*;
use reda_model::Purpose;

/// The Mechanical Layers of `layers`, in Layer order.
fn mechanical(layers: &LayerSet) -> impl Iterator<Item = &Layer> {
    layers
        .layers
        .iter()
        .filter(|l| l.kind == LayerKind::Mechanical)
}

/// `name` trimmed, checked to be a name no other Mechanical Layer has (`except` is the
/// Layer being renamed).
pub(in crate::commands) fn checked_name(
    layers: &LayerSet,
    name: &str,
    except: Option<Uuid>,
    pointer: &str,
) -> Result<String, CommandError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(CommandError::invalid_params(
            pointer,
            "a Mechanical Layer needs a name",
        ));
    }
    if mechanical(layers).any(|l| Some(l.uuid) != except && l.name.as_deref() == Some(name)) {
        return Err(CommandError::invalid_params(
            pointer,
            format!("the board already has a Mechanical Layer named {name:?}"),
        ));
    }
    Ok(name.to_owned())
}

/// The name a new Layer with `purpose` takes: the Purpose's own, numbered on a clash
/// ("Dimensions 2").
fn default_name(layers: &LayerSet, purpose: Purpose) -> String {
    let taken = |name: &str| mechanical(layers).any(|l| l.name.as_deref() == Some(name));
    let title = purpose.title();
    if !taken(title) {
        return title.to_owned();
    }
    (2..)
        .map(|n| format!("{title} {n}"))
        .find(|name| !taken(name))
        .expect("an unbounded count of names")
}

/// The Mechanical Layer `layer` names, or an error at `pointer`.
fn mechanical_layer(
    pcb: &PcbDocument,
    layer: LayerRef,
    pointer: &str,
) -> Result<Uuid, CommandError> {
    let layer = layer.resolve(pcb, pointer)?;
    if layer.kind != LayerKind::Mechanical {
        return Err(CommandError::invalid_params(
            pointer,
            "choose a Mechanical Layer",
        ));
    }
    Ok(layer.uuid)
}

pub(super) fn add_plan(
    pcb: &PcbDocument,
    p: AddParams,
    uuid: Uuid,
) -> Result<(Plan, AddedLayer), CommandError> {
    if p.split.is_some() {
        return Err(CommandError::invalid_params(
            "/split",
            "a Mechanical Layer splits no dielectric; split is for copper Layers",
        ));
    }
    let mut plan = Plan::new(pcb);
    plan.stack_changed = false;
    let layers = &plan.layers;
    let purpose = p.purpose.unwrap_or_default();
    let name = match &p.name {
        Some(name) => checked_name(layers, name, None, "/name")?,
        None => default_name(layers, purpose),
    };
    let at = match p.before {
        Some(before) => {
            let before = mechanical_layer(pcb, before, "/before")?;
            layers.layers.iter().position(|l| l.uuid == before).unwrap()
        }
        None => layers.layers.len(),
    };
    plan.layers.layers.insert(
        at,
        Layer {
            uuid,
            kind: LayerKind::Mechanical,
            side: p.side,
            name: Some(name),
            purpose,
            role: CopperRole::Signal,
        },
    );
    Ok((
        plan,
        AddedLayer {
            uuid,
            dielectric: None,
        },
    ))
}

/// Removes a Mechanical Layer with every object on it.
pub(super) fn remove_plan(pcb: &PcbDocument, layer: Uuid) -> Plan {
    let mut plan = Plan::new(pcb);
    plan.stack_changed = false;
    plan.layers.layers.retain(|l| l.uuid != layer);
    for (on, kind, uuid) in pcb.layer_objects() {
        if on == layer {
            *plan.counts.objects_deleted.entry(kind.into()).or_default() += 1;
            plan.deleted.push(uuid);
        }
    }
    plan
}

/// Swaps a Mechanical Layer with the Mechanical Layer above or below it.
pub(super) fn move_plan(
    pcb: &PcbDocument,
    layer: Uuid,
    direction: Direction,
) -> Result<Plan, CommandError> {
    let mut plan = Plan::new(pcb);
    plan.stack_changed = false;
    let order: Vec<Uuid> = mechanical(&plan.layers).map(|l| l.uuid).collect();
    let at = order.iter().position(|l| *l == layer).unwrap();
    let other = match direction {
        Direction::Up => at.checked_sub(1),
        Direction::Down => Some(at + 1).filter(|n| *n < order.len()),
    }
    .ok_or_else(|| {
        CommandError::rejected(match direction {
            Direction::Up => "the Layer is already the first Mechanical Layer",
            Direction::Down => "the Layer is already the last Mechanical Layer",
        })
    })?;
    let layers = &mut plan.layers.layers;
    let a = layers.iter().position(|l| l.uuid == layer).unwrap();
    let b = layers.iter().position(|l| l.uuid == order[other]).unwrap();
    layers.swap(a, b);
    Ok(plan)
}
