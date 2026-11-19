//! Count the effects of the same plan that the edit applies.
use super::super::{needs_pcb, pcb, via_type};
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
    pub vias_without_type: usize,
}

#[derive(Deserialize, JsonSchema)]
#[serde(tag = "id", content = "params", deny_unknown_fields)]
pub enum Change {
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
            Change::ViaTypeRemove(p) => Ok(via_type::remove_plan(pcb, p)?.counts),
            Change::ViaTypeSet(p) => Ok(via_type::set_plan(pcb, p)?.counts),
        }
    }
}
