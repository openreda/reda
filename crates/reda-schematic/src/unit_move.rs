//! `schematic.unit.move`: moving a placed Unit on its sheet, detached from its Wires.

use reda_command::{Angle, Command, CommandError, Context, Kind, Meta};
use reda_model::{InDocument, Object, Uuid};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::component::At;
use crate::normalise::{Wiring, normalise};

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UnitMoveParams {
    /// The placed Unit, by UUID.
    pub unit: Uuid,
    /// Where its Symbol origin goes.
    pub at: At,
    /// Counter-clockwise; unchanged by default.
    pub rotation: Option<Angle>,
    /// Mirror left to right before rotating; unchanged by default.
    pub mirror: Option<bool>,
    /// How the Wires this edit touches are normalised; the fixed defaults when left out.
    pub wiring: Option<Wiring>,
}

/// What `schematic.unit.move` moved.
#[derive(Debug, Serialize, JsonSchema)]
pub struct UnitMoveResult {
    pub unit: Uuid,
    /// The sheet it is on.
    pub document: Uuid,
}

pub struct UnitMove;

impl Command for UnitMove {
    const META: Meta = Meta {
        id: "schematic.unit.move",
        title: "Move Unit",
        description: "Moves a placed Unit to a new origin, optionally changing its rotation \
                      and mirroring. Its Wires stay where they are (a detached move); Pins \
                      that land partway along a Wire split it and connect, as when placing.",
        category: "schematic",
        icon: "move",
        kind: Kind::Edit,
    };
    type Params = UnitMoveParams;
    type Output = UnitMoveResult;

    fn run(&self, cx: &mut Context<'_>, p: UnitMoveParams) -> Result<UnitMoveResult, CommandError> {
        let project = cx.project()?;
        let found = project
            .documents()
            .filter_map(|d| project.schematic_sheet(d.uuid))
            .find_map(|s| s.unit(p.unit).map(|u| (s.uuid, u.clone())));
        let Some((document, mut unit)) = found else {
            return Err(CommandError::not_found(format!(
                "no placed Unit has UUID {}",
                p.unit
            )));
        };
        let before = crate::sheet(project, document)?.clone();
        unit.at = p.at.point();
        if let Some(rotation) = p.rotation {
            unit.rotation = rotation.millideg();
        }
        if let Some(mirror) = p.mirror {
            unit.mirror = mirror;
        }
        cx.tx()?
            .replace(Object::Unit(InDocument::new(document, unit)))?;
        normalise(cx, document, &before, &p.wiring.unwrap_or_default())?;
        Ok(UnitMoveResult {
            unit: p.unit,
            document,
        })
    }
}
