//! Add or remove a sheet within one gap between two copper Layers.

use reda_command::{AvailabilityContext, Command, CommandError, Context, Kind, Meta, Unavailable};
use reda_model::{DielectricEntry, InDocument, Object, Stackup, StackupEntry, Uuid};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::commands::{Created, needs_pcb, pcb};

fn sheet(stackup: &Stackup, uuid: Uuid) -> Result<usize, CommandError> {
    stackup
        .entries
        .iter()
        .position(|e| matches!(e, StackupEntry::Dielectric(d) if d.uuid == uuid))
        .ok_or_else(|| CommandError::not_found(format!("the Stackup has no dielectric {uuid}")))
}

pub(super) struct SheetAdd;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SheetAddParams {
    /// The dielectric immediately above the new sheet.
    pub beside: Uuid,
    /// The new sheet's UUID. Omit to allocate one.
    pub uuid: Option<Uuid>,
}

impl Command for SheetAdd {
    const META: Meta = Meta {
        id: "pcb.stackup.dielectric.add",
        title: "Add Dielectric Sheet",
        description: "Inserts a blank sheet below the given dielectric, sharing unassigned thickness.",
        category: "pcb",
        icon: "layers",
        kind: Kind::Edit,
    };
    type Params = SheetAddParams;
    type Output = Created;
    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }
    fn run(&self, cx: &mut Context<'_>, p: Self::Params) -> Result<Created, CommandError> {
        let uuid = p.uuid.unwrap_or_else(|| cx.new_id());
        let pcb = pcb(cx)?;
        let document = pcb.uuid;
        let mut stackup = pcb.stackup().clone();
        let index = sheet(&stackup, p.beside)?;
        if cx.project()?.identity_in_use(uuid) {
            return Err(CommandError::invalid_params(
                "/uuid",
                "that UUID is already in use",
            ));
        }
        let StackupEntry::Dielectric(beside) = &stackup.entries[index] else {
            unreachable!()
        };
        let added = DielectricEntry::new(uuid, beside.kind);
        stackup
            .entries
            .insert(index + 1, StackupEntry::Dielectric(added));
        cx.tx()?
            .replace(Object::Stackup(InDocument::new(document, stackup)))?;
        Ok(Created { uuid })
    }
}

pub(super) struct SheetRemove;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SheetRemoveParams {
    pub dielectric: Uuid,
}

impl Command for SheetRemove {
    const META: Meta = Meta {
        id: "pcb.stackup.dielectric.remove",
        title: "Remove Dielectric Sheet",
        description: "Removes a sheet if another sheet separates the same copper Layers.",
        category: "pcb",
        icon: "layers",
        kind: Kind::Edit,
    };
    type Params = SheetRemoveParams;
    type Output = ();
    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }
    fn run(&self, cx: &mut Context<'_>, p: Self::Params) -> Result<(), CommandError> {
        let pcb = pcb(cx)?;
        let document = pcb.uuid;
        let mut stackup = pcb.stackup().clone();
        let index = sheet(&stackup, p.dielectric)?;
        if matches!(stackup.entries[index - 1], StackupEntry::Copper(_))
            && matches!(stackup.entries[index + 1], StackupEntry::Copper(_))
        {
            return Err(CommandError::rejected(
                "the only dielectric sheet between two copper Layers must stay",
            ));
        }
        stackup.entries.remove(index);
        cx.tx()?
            .replace(Object::Stackup(InDocument::new(document, stackup)))?;
        Ok(())
    }
}
