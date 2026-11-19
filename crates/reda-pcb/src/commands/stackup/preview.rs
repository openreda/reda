//! Count the effects of the same plan that the edit applies.
use super::super::{layer_count, needs_pcb, pcb, via_type};
use reda_command::{AvailabilityContext, Command, CommandError, Context, Kind, Meta, Unavailable};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Default, Serialize, JsonSchema)]
pub struct Counts {
    pub objects_deleted: BTreeMap<String, usize>,
    pub vias_using_span: usize,
    pub vias_stretched: usize,
    pub vias_shortened: usize,
    pub vias_deleted: usize,
    pub via_types_changed: usize,
    pub via_types_removed: usize,
    pub rules_changed: usize,
    pub rules_removed: usize,
    pub allowed_lists_emptied: usize,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub classes_allowing_all: Vec<String>,
    pub vias_without_type: usize,
}

#[derive(Deserialize, JsonSchema)]
#[serde(tag = "id", content = "params", deny_unknown_fields)]
pub enum Change {
    #[serde(rename = "pcb.stackup.apply_construction")]
    ApplyConstruction(layer_count::construction::Params),
    #[serde(rename = "pcb.layer.add")]
    LayerAdd(layer_count::AddParams),
    #[serde(rename = "pcb.layer.remove")]
    LayerRemove(layer_count::RemoveParams),
    #[serde(rename = "pcb.layer.move")]
    LayerMove(layer_count::MoveParams),
    #[serde(rename = "pcb.via_type.set")]
    ViaTypeSet(via_type::SetParams),
    #[serde(rename = "pcb.via_type.remove")]
    ViaTypeRemove(via_type::RemoveParams),
}
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Params {
    change: Change,
}

pub(super) struct Preview;
impl Command for Preview {
    const META: Meta = Meta {
        id: "pcb.stackup.change_preview",
        title: "Preview Stackup Change",
        description: "Counts the objects and rules affected by a proposed Stackup edit.",
        category: "pcb",
        icon: "layers",
        kind: Kind::Query,
    };
    type Params = Params;
    type Output = Counts;
    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }
    fn run(&self, cx: &mut Context<'_>, p: Params) -> Result<Counts, CommandError> {
        let pcb = pcb(cx)?;
        match p.change {
            Change::ApplyConstruction(p) => Ok(layer_count::construction::plan(
                cx.project()?,
                p,
                reda_model::Uuid::from_u128(0),
            )?
            .counts),
            Change::LayerAdd(p) => {
                Ok(layer_count::add_plan(cx.project()?, p.document, p.split, p.uuid)?.counts)
            }
            Change::LayerMove(p) => Ok(layer_count::move_plan(pcb, p)?.counts),
            Change::LayerRemove(p) => {
                Ok(layer_count::remove_plan(pcb, p.layer.resolve(pcb, "/layer")?.uuid)?.counts)
            }
            Change::ViaTypeRemove(p) => Ok(via_type::remove_plan(pcb, p)?.counts),
            Change::ViaTypeSet(p) => Ok(via_type::set_plan(pcb, p)?.counts),
        }
    }
}
