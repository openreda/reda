//! `pcb.footprint.set`: Component data edited from the PCB Properties panel.
//!
//! A Footprint synced from the schematic writes through: every Unit of its Component and
//! the Footprint instance change together, one undo entry in each Document. A Mechanical
//! Part placed only on the PCB is edited there, as `pcb.mechanical.set` does.

use std::collections::BTreeMap;

use reda_command::{
    AvailabilityContext, Command, CommandError, Context, Kind, Meta, NO_PROJECT, Registry,
    Unavailable,
};
use reda_model::{FootprintInstance, Uuid};
use reda_pcb::{MechanicalSet, MechanicalSetParams};
use reda_schematic::{ComponentSet, ComponentSetParams};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::sync::{self, NO_PCB};

pub(crate) fn register_all(registry: &mut Registry) {
    registry.add(FootprintSet);
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FootprintSetParams {
    /// The Footprint instances to edit, by UUID: synced Footprints, Mechanical Parts or
    /// both.
    pub footprints: Vec<Uuid>,
    /// The new Designator. Only with a single Footprint, and not one another Component
    /// or Footprint uses.
    #[serde(default)]
    pub designator: Option<String>,
    /// The new value.
    #[serde(default)]
    pub value: Option<String>,
    /// Fields to set, by name: the built-in `description`, `datasheet`, `manufacturer`,
    /// `mpn` and `keywords`, or any free-form name such as `LCSC`. An empty value or
    /// `null` clears a built-in field and removes a free-form one.
    #[serde(default)]
    pub fields: BTreeMap<String, Option<String>>,
    /// Whether the Footprints are left off the board and out of the BOM.
    #[serde(default)]
    pub not_fitted: Option<bool>,
    /// The Footprint, by Library Footprint UUID: one of each Component's Part's
    /// alternates. Only for synced Footprints; it is swapped in place as Sync would.
    #[serde(default)]
    pub footprint: Option<Uuid>,
}

/// What an edit changed.
#[derive(Debug, Serialize, JsonSchema)]
pub struct FootprintSetResult {
    /// The Footprint instances edited, in the order given.
    pub footprints: Vec<Uuid>,
    /// Every Unit rewritten on the schematic, Component by Component in the order given.
    pub units: Vec<Uuid>,
}

pub struct FootprintSet;

impl Command for FootprintSet {
    const META: Meta = Meta {
        id: "pcb.footprint.set",
        title: "Edit Footprint Properties",
        description: "Sets the Designator, value, fields, Not Fitted or Footprint of \
                      Footprints from the PCB side. A Footprint synced from the schematic \
                      writes through: every Unit of its Component changes too, and the \
                      board takes the change as Sync would (a new Footprint is swapped in \
                      place and its Pads rejoin their Nets), with one undo entry in each \
                      Document and nothing left pending. Mechanical Parts placed only on \
                      the PCB are edited there.",
        category: "pcb",
        icon: "properties",
        kind: Kind::Edit,
    };
    type Params = FootprintSetParams;
    type Output = FootprintSetResult;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        let reason = match cx.project {
            None => NO_PROJECT,
            Some(project) if project.pcb().is_none() => NO_PCB,
            Some(_) => return Ok(()),
        };
        Err(Unavailable {
            reason: reason.into(),
        })
    }

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: FootprintSetParams,
    ) -> Result<FootprintSetResult, CommandError> {
        let chosen = footprints(cx, &p.footprints)?;
        if p.designator.is_some() && chosen.len() != 1 {
            return Err(CommandError::invalid_params(
                "/designator",
                "a Designator can be set on one Footprint at a time",
            ));
        }
        let (synced, mechanical): (Vec<_>, Vec<_>) =
            chosen.iter().partition(|f| f.component.is_some());
        if !mechanical.is_empty() {
            if p.footprint.is_some() {
                return Err(CommandError::invalid_params(
                    "/footprint",
                    format!(
                        "{} is a Mechanical Part; its Footprint is not swapped here",
                        mechanical[0].designator
                    ),
                ));
            }
            MechanicalSet.run(
                cx,
                MechanicalSetParams {
                    footprints: mechanical.iter().map(|f| f.uuid).collect(),
                    designator: p.designator.clone(),
                    value: p.value.clone(),
                    fields: p.fields.clone(),
                    not_fitted: p.not_fitted,
                },
            )?;
        }
        let mut units = Vec::new();
        if !synced.is_empty() {
            if let Some(designator) = &p.designator {
                check_designator_free(cx, synced[0], designator)?;
            }
            let params = ComponentSetParams {
                components: synced
                    .iter()
                    .filter_map(|f| f.component.as_ref())
                    .map(|c| c.uuid)
                    .collect(),
                designator: p.designator,
                designator_locked: None,
                value: p.value,
                fields: p.fields,
                footprint: p.footprint,
                not_fitted: p.not_fitted,
                visible: BTreeMap::new(),
            };
            units = sync::write_through(cx, |cx| ComponentSet.run(cx, params))?.units;
        }
        Ok(FootprintSetResult {
            footprints: chosen.iter().map(|f| f.uuid).collect(),
            units,
        })
    }
}

/// The Footprint instances named by `uuids` (duplicates dropped), or `invalid_params` at
/// `/footprints/i`.
fn footprints(cx: &Context<'_>, uuids: &[Uuid]) -> Result<Vec<FootprintInstance>, CommandError> {
    let pcb = cx
        .project()?
        .pcb()
        .ok_or_else(|| CommandError::unavailable(NO_PCB))?;
    if uuids.is_empty() {
        return Err(CommandError::invalid_params(
            "/footprints",
            "nothing is selected",
        ));
    }
    let mut out: Vec<FootprintInstance> = Vec::new();
    for (i, uuid) in uuids.iter().enumerate() {
        let f = pcb.footprint(*uuid).ok_or_else(|| {
            CommandError::invalid_params(
                &format!("/footprints/{i}"),
                format!("{uuid} is not a Footprint on the board"),
            )
        })?;
        if !out.iter().any(|o| o.uuid == *uuid) {
            out.push(f.clone());
        }
    }
    Ok(out)
}

/// Refuses a Designator a Mechanical Part on the board already carries. The schematic's
/// Units are checked by the schematic edit itself.
fn check_designator_free(
    cx: &Context<'_>,
    footprint: &FootprintInstance,
    designator: &str,
) -> Result<(), CommandError> {
    let pcb = cx.project()?.pcb().expect("a PCB");
    let holder = pcb
        .footprints()
        .filter(|f| f.uuid != footprint.uuid && f.component.is_none())
        .find(|f| f.designator == designator);
    match holder {
        Some(f) => Err(CommandError::conflict(format!(
            "Designator {designator} is already used by Mechanical Part {}",
            f.uuid
        ))),
        None => Ok(()),
    }
}
