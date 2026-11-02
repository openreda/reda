//! `sync.preview` and `sync.apply`: Update PCB from Schematic.

use reda_command::{
    AvailabilityContext, Command, CommandError, Context, Kind, Meta, NO_PROJECT, NoParams,
    Registry, Unavailable,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::sync::{self, ChangeList, NO_PCB};

pub(crate) fn register_all(registry: &mut Registry) {
    registry.add(SyncPreview);
    registry.add(SyncApply);
}

/// Both Commands need an open Project with a PCB Document.
fn needs_pcb(cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
    let reason = match cx.project {
        None => NO_PROJECT,
        Some(project) if project.pcb().is_none() => NO_PCB,
        Some(_) => return Ok(()),
    };
    Err(Unavailable {
        reason: reason.into(),
    })
}

pub struct SyncPreview;

impl Command for SyncPreview {
    const META: Meta = Meta {
        id: "sync.preview",
        title: "Preview Update PCB from Schematic",
        description: "Compares the Schematic Sheets with the PCB Document and returns the \
                      pending Change List, grouped by kind (components, footprints, \
                      component data, Nets, Pad Nets). Each change has a stable id and the \
                      ids it requires; unticking a change also unticks every change that \
                      requires it. `count` is the out-of-sync count, `hash` goes to \
                      `sync.apply`. Nothing is stored.",
        category: "sync",
        icon: "sync",
        kind: Kind::Query,
    };
    type Params = NoParams;
    type Output = ChangeList;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, _: NoParams) -> Result<ChangeList, CommandError> {
        sync::plan(cx.project()?).ok_or_else(|| CommandError::unavailable(NO_PCB))
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SyncApplyParams {
    /// The `hash` of the previewed Change List.
    pub hash: String,
    /// Ids of changes to leave out; the changes that require them are left out too.
    #[serde(default)]
    pub exclude: Vec<String>,
}

/// What `sync.apply` did.
#[derive(Debug, Serialize, JsonSchema)]
pub struct SyncApplyResult {
    /// The ids of the changes carried out, in list order.
    pub applied: Vec<String>,
    /// The ids left out (excluded or depending on an excluded one), in list order. They
    /// show up again in the next preview.
    pub left_out: Vec<String>,
}

pub struct SyncApply;

impl Command for SyncApply {
    const META: Meta = Meta {
        id: "sync.apply",
        title: "Update PCB from Schematic",
        description: "Applies the Change List of `sync.preview` to the PCB Document in one \
                      transaction, leaving out the `exclude`d changes and every change that \
                      requires one. New Footprints go into a fresh cluster outside the Board \
                      Outline (at the origin with no outline), grouped by sheet and sorted \
                      by Designator. Fails with `conflict` when the Change List no longer \
                      matches `hash`.",
        category: "sync",
        icon: "sync",
        kind: Kind::Edit,
    };
    type Params = SyncApplyParams;
    type Output = SyncApplyResult;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: SyncApplyParams,
    ) -> Result<SyncApplyResult, CommandError> {
        let done = sync::apply(cx, &p.hash, &p.exclude)?;
        Ok(SyncApplyResult {
            applied: done.applied,
            left_out: done.left_out,
        })
    }
}
