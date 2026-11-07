//! `schematic.unit.cycle`: Next Unit, which turns a placed symbol into the next Unit of
//! its Component that is not placed yet. Every Unit stays unique.

use std::collections::BTreeSet;

use reda_command::{Command, CommandError, Context, Kind, Meta};
use reda_model::{InDocument, Object, Uuid};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::fields::placed_unit;
use crate::normalise::{Wiring, normalise};
use crate::properties::units_of;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UnitCycleParams {
    /// The placed Unit, by UUID.
    pub unit: Uuid,
    /// How the Wires this edit touches are normalised; the fixed defaults when left out.
    pub wiring: Option<Wiring>,
}

/// What `schematic.unit.cycle` changed.
#[derive(Debug, Serialize, JsonSchema)]
pub struct UnitCycleResult {
    pub unit: Uuid,
    /// The sheet it is on.
    pub document: Uuid,
    /// The Symbol Unit it shows now, by UUID.
    pub shows: Uuid,
    /// That Unit's name.
    pub name: String,
}

pub struct UnitCycle;

impl Command for UnitCycle {
    const META: Meta = Meta {
        id: "schematic.unit.cycle",
        title: "Next Unit",
        description: "Turns a placed symbol into the next Unit of its Component, in Symbol \
                      order and wrapping round, skipping every Unit already placed on any \
                      sheet. Refused when every Unit is placed, so no Unit is ever placed \
                      twice. The symbol keeps its position, rotation and fields.",
        category: "schematic",
        icon: "units",
        kind: Kind::Edit,
    };
    type Params = UnitCycleParams;
    type Output = UnitCycleResult;

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: UnitCycleParams,
    ) -> Result<UnitCycleResult, CommandError> {
        let project = cx.project()?;
        let (document, mut unit) = placed_unit(project, p.unit)?;
        let sheet = crate::sheet(project, document)?;
        let symbol = &sheet
            .part(unit.part)
            .expect("a Unit's part entry is on its sheet")
            .symbol;
        let placed: BTreeSet<Uuid> = units_of(project, unit.component.uuid)
            .into_iter()
            .map(|(_, u)| u.unit)
            .collect();
        let at = symbol
            .units
            .iter()
            .position(|u| u.uuid == unit.unit)
            .unwrap_or(0);
        let n = symbol.units.len();
        let next = (1..n)
            .map(|i| &symbol.units[(at + i) % n])
            .find(|u| !placed.contains(&u.uuid))
            .ok_or_else(|| {
                CommandError::rejected(format!(
                    "every Unit of {} is placed, so there is no next Unit to show",
                    unit.designator
                ))
            })?;
        let (shows, name) = (next.uuid, next.name.clone());
        let before = sheet.clone();
        unit.unit = shows;
        cx.tx()?
            .replace(Object::Unit(InDocument::new(document, unit)))?;
        normalise(cx, document, &before, &p.wiring.unwrap_or_default())?;
        Ok(UnitCycleResult {
            unit: p.unit,
            document,
            shows,
            name,
        })
    }
}
