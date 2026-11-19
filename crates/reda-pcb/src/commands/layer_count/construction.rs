//! Apply one physical build to the existing board identities.
use super::*;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Params {
    document: Uuid,
    preset: Uuid,
    construction: String,
    /// Inner copper Layers to delete. Required when shrinking, one per removed Layer.
    #[serde(default)]
    remove: Vec<crate::commands::layer::LayerRef>,
    /// Replace the board's Via types when this Construction supplies a list.
    #[serde(default)]
    via_types: bool,
}

/// Board identities, in table order, for subsequent edits and Journal replay.
#[derive(Serialize, JsonSchema)]
pub struct Applied {
    layers: Vec<Uuid>,
    dielectrics: Vec<Uuid>,
    via_types: Vec<Uuid>,
}

pub(in crate::commands) struct Apply;
impl Command for Apply {
    const META: Meta = Meta {
        id: "pcb.stackup.apply_construction",
        title: "Apply Construction",
        description: "Applies a named Construction to an existing board, retaining surviving copper Layers.",
        category: "pcb",
        icon: "layers",
        kind: Kind::Edit,
    };
    type Params = Params;
    type Output = Applied;
    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }
    fn run(&self, cx: &mut Context<'_>, p: Params) -> Result<Applied, CommandError> {
        let seed = cx.new_id();
        let plan = plan(cx.project()?, p, seed)?;
        let out = Applied {
            layers: plan.stack.copper().map(|c| c.layer).collect(),
            dielectrics: plan.stack.dielectrics().map(|d| d.uuid).collect(),
            via_types: plan.stack.via_types.iter().map(|t| t.uuid).collect(),
        };
        plan.apply(cx.tx()?)?;
        Ok(out)
    }
}

pub(in crate::commands) fn plan(
    project: &Project,
    p: Params,
    seed: Uuid,
) -> Result<Plan, CommandError> {
    let pcb = project
        .pcb_document(p.document)
        .ok_or_else(|| CommandError::not_found("no such PCB Document"))?;
    let preset = project
        .fab_preset(p.preset)
        .ok_or_else(|| CommandError::not_found("no such Project Fab Preset"))?;
    let build = preset
        .constructions
        .iter()
        .find(|b| b.name.eq_ignore_ascii_case(&p.construction))
        .ok_or_else(|| {
            CommandError::invalid_params("/construction", "no such Construction in this preset")
        })?;
    let mut ids = (0..).map(|n| seed.child(&n.to_string())).filter(|id| {
        project.object(*id).is_none()
            && pcb.layers().layer(*id).is_none()
            && !pcb.stackup().dielectrics().any(|d| d.uuid == *id)
            && !pcb.stackup().via_types.iter().any(|v| v.uuid == *id)
    });
    let mut plan = Plan::new(pcb);
    let count = pcb.copper_layers().count();
    let target = build.layer_count();
    if p.remove.len() != count.saturating_sub(target) {
        return Err(CommandError::invalid_params(
            "/remove",
            format!(
                "choose exactly {} inner Layers to remove",
                count.saturating_sub(target)
            ),
        ));
    }
    let mut removed = std::collections::BTreeSet::new();
    for choice in p.remove {
        let id = choice.resolve(pcb, "/remove")?.uuid;
        inner(pcb, id)?;
        if !removed.insert(id) {
            return Err(CommandError::invalid_params(
                "/remove",
                "a Layer is listed twice",
            ));
        }
    }
    if !removed.is_empty() {
        removal::remove_copper(&mut plan, pcb, &removed);
    }
    if target > count {
        let middle = count.div_ceil(2);
        insert_copper(&mut plan, pcb, middle, ids.by_ref().take(target - count));
    }
    let layers: Vec<_> = plan
        .layers
        .layers
        .iter()
        .filter(|l| l.kind == LayerKind::Copper)
        .map(|l| l.uuid)
        .collect();
    plan.stack.entries = build
        .board_entries(&layers, || ids.next().unwrap())
        .map_err(|s| CommandError::invalid_params("/construction", s))?;
    let replace_types = p.via_types && !build.via_types.is_empty();
    if replace_types {
        let mut types = build
            .board_via_types(&layers, pcb.stackup().through_type().uuid, || {
                ids.next().unwrap()
            })
            .map_err(|s| CommandError::invalid_params("/construction", s))?;
        // A type is identified on copper by its span, in either drill direction.
        for kind in &mut types {
            if let Some(old) = plan.stack.type_for_span(kind.span()) {
                kind.uuid = old.uuid;
            }
        }
        plan.stack.via_types = types;
    }
    if replace_types || !removed.is_empty() {
        removal::reconcile(&mut plan, pcb, &removed);
    }
    plan.stack.thickness = build.thickness;
    plan.stack.construction = Some(build.name.clone());
    Ok(plan)
}
