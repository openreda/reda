//! The placement Tool's view Commands: `tool.place` picks a Part up and `tool.place.set`
//! changes it in flight (`tool.rotate`, `tool.mirror_x`, `tool.mirror_y` and
//! `tool.flip_side`, in [`crate::tools`], turn it too).

use std::collections::BTreeMap;

use reda_engine::command::{
    AvailabilityContext, Command, CommandError, Context, FootprintCopyData, Kind, Meta, PartCopy,
    Registry, Unavailable,
};
use reda_engine::model::geom::Millideg;
use reda_engine::model::{ComponentType, Side, Uuid};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{Placement, Placing, Target};
use crate::tools::{ActiveTool, Tool};

/// Adds `tool.place` and `tool.place.set` to `registry`. The Shell does
/// this for its engine.
pub fn register_all(registry: &mut Registry) {
    registry.add(ToolPlace);
    registry.add(ToolPlaceSet);
    registry.add(ToolPlaceUnit);
}

/// Changes the Part in flight with `edit`; nothing when none is.
pub(crate) fn edit_placing(
    cx: &mut Context<'_>,
    edit: impl FnOnce(&mut Placing),
) -> Result<(), CommandError> {
    if let Some(placing) = &mut cx.view::<Placement>()?.placing {
        edit(placing);
    }
    Ok(())
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ToolPlaceParams {
    /// The Document to place on: a Schematic Sheet, or the PCB for a Mechanical Part.
    document: Uuid,
    /// The Library Part, by UUID.
    part: Uuid,
    /// The Unit to place, by name; the first by default (sheets only).
    #[serde(default)]
    unit: Option<String>,
    /// One of the Part's alternate Footprints; the first by default.
    #[serde(default)]
    footprint: Option<Uuid>,
}

/// The Tool now active and the Part it carries.
#[derive(Serialize, JsonSchema)]
pub struct ToolPlaceResult {
    tool: Tool,
    part: Uuid,
}

struct ToolPlace;

impl Command for ToolPlace {
    const META: Meta = Meta {
        id: "tool.place",
        title: "Place Part",
        description: "Picks a Library Part up to place: it follows the pointer over the \
                      Document, turned with Space, X, Y and L, until a click places it. A \
                      sheet takes Parts with a Symbol, the PCB only Mechanical Parts. \
                      Placement repeats per Preference; Escape ends it.",
        category: "tool",
        icon: "component_place",
        kind: Kind::View,
    };
    type Params = ToolPlaceParams;
    type Output = ToolPlaceResult;

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: ToolPlaceParams,
    ) -> Result<ToolPlaceResult, CommandError> {
        let project = cx.project()?;
        let target = if project.schematic_sheet(p.document).is_some() {
            Target::Sheet
        } else if project.pcb_document(p.document).is_some() {
            Target::Board
        } else {
            return Err(CommandError::invalid_params(
                "/document",
                format!("{} is not a Schematic Sheet or the PCB", p.document),
            ));
        };
        let copy = cx.library_part(p.part)?;
        match target {
            Target::Sheet => {
                let Some(symbol) = &copy.symbol else {
                    return Err(CommandError::rejected(format!(
                        "{} has no Symbol, so it cannot go on a sheet",
                        copy.part.name
                    )));
                };
                if let Some(unit) = &p.unit
                    && !symbol.units.iter().any(|u| &u.name == unit)
                {
                    return Err(CommandError::invalid_params(
                        "/unit",
                        format!("{} has no Unit {unit:?}", symbol.name),
                    ));
                }
            }
            Target::Board => {
                if copy.part.component_type != ComponentType::Mechanical {
                    return Err(CommandError::rejected(format!(
                        "{} is a {} Part: it reaches the board through Sync",
                        copy.part.name,
                        copy.part.component_type.name()
                    )));
                }
            }
        }
        if let Some(f) = p.footprint
            && !copy.part.footprints.contains(&f)
        {
            return Err(CommandError::invalid_params(
                "/footprint",
                format!("{f} is not one of {}'s Footprints", copy.part.name),
            ));
        }
        let part = copy.part.uuid;
        let placing = Placing {
            document: p.document,
            target,
            copy,
            unit: p.unit,
            footprint: p.footprint,
            rotation: Millideg::ZERO,
            mirror: false,
            side: Side::Top,
            designator: None,
            fields: BTreeMap::new(),
            component: None,
        };
        cx.view::<Placement>()?.placing = Some(placing);
        cx.view::<ActiveTool>()?.0 = Tool::Place;
        Ok(ToolPlaceResult {
            tool: Tool::Place,
            part,
        })
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ToolPlaceSetParams {
    /// The Designator the next placement takes, counting up after each; empty for the
    /// next free one.
    #[serde(default)]
    designator: Option<String>,
    /// Field values over the Part's defaults, by name (`value`, `mpn`… or a free-form
    /// name); null drops an edit.
    #[serde(default)]
    fields: BTreeMap<String, Option<String>>,
    /// One of the Part's alternate Footprints.
    #[serde(default)]
    footprint: Option<Uuid>,
}

/// The Part in flight's edits now.
#[derive(Serialize, JsonSchema)]
pub struct PlacingEdits {
    designator: Option<String>,
    fields: BTreeMap<String, String>,
    footprint: Option<Uuid>,
}

struct ToolPlaceSet;

impl Command for ToolPlaceSet {
    const META: Meta = Meta {
        id: "tool.place.set",
        title: "Edit Part Being Placed",
        description: "Edits the Part being placed, as the Properties panel does while \
                      placing: its Designator, field values and Footprint. Later \
                      placements in the same run keep the edits, with the Designator \
                      counting up.",
        category: "tool",
        icon: "properties",
        kind: Kind::View,
    };
    type Params = ToolPlaceSetParams;
    type Output = PlacingEdits;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        match cx.project {
            Some(_) => Ok(()),
            None => Err(Unavailable {
                reason: "no Project is open".into(),
            }),
        }
    }

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: ToolPlaceSetParams,
    ) -> Result<PlacingEdits, CommandError> {
        let Some(placing) = &mut cx.view::<Placement>()?.placing else {
            return Err(CommandError::rejected("no Part is being placed"));
        };
        if let Some(f) = p.footprint {
            if !placing.copy.part.footprints.contains(&f) {
                return Err(CommandError::invalid_params(
                    "/footprint",
                    format!("{f} is not one of {}'s Footprints", placing.copy.part.name),
                ));
            }
            placing.footprint = Some(f);
        }
        if let Some(d) = p.designator {
            let d = d.trim();
            placing.designator = (!d.is_empty()).then(|| d.to_owned());
        }
        for (name, value) in p.fields {
            match value {
                Some(v) => placing.fields.insert(name, v),
                None => placing.fields.remove(&name),
            };
        }
        Ok(PlacingEdits {
            designator: placing.designator.clone(),
            fields: placing.fields.clone(),
            footprint: placing.footprint,
        })
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ToolPlaceUnitParams {
    /// The placed Component, by UUID.
    component: Uuid,
    /// The unplaced Unit to place, by name.
    unit: String,
    /// The sheet to place it on.
    document: Uuid,
}

struct ToolPlaceUnit;

impl Command for ToolPlaceUnit {
    const META: Meta = Meta {
        id: "tool.place_unit",
        title: "Place Unit",
        description: "Picks up an unplaced Unit of a placed Component (Properties' Place \
                      button): it follows the pointer over the sheet, turned with Space, X \
                      and Y, until a click places it with schematic.unit.place.",
        category: "tool",
        icon: "component_place",
        kind: Kind::View,
    };
    type Params = ToolPlaceUnitParams;
    type Output = ToolPlaceResult;

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: ToolPlaceUnitParams,
    ) -> Result<ToolPlaceResult, CommandError> {
        let project = cx.project()?;
        if project.schematic_sheet(p.document).is_none() {
            return Err(CommandError::invalid_params(
                "/document",
                format!("{} is not a Schematic Sheet", p.document),
            ));
        }
        let sheets = || {
            project
                .documents()
                .filter_map(|d| project.schematic_sheet(d.uuid))
        };
        let Some((sheet, unit)) = sheets().find_map(|s| {
            s.units()
                .find(|u| u.component.uuid == p.component)
                .map(|u| (s, u))
        }) else {
            return Err(CommandError::invalid_params(
                "/component",
                format!("no placed Component {}", p.component),
            ));
        };
        let entry = sheet.part(unit.part).expect("a Unit's part entry exists");
        let Some(wanted) = entry.symbol.units.iter().find(|u| u.name == p.unit) else {
            return Err(CommandError::invalid_params(
                "/unit",
                format!("{} has no Unit {:?}", entry.symbol.name, p.unit),
            ));
        };
        let placed = sheets()
            .flat_map(|s| s.units())
            .any(|u| u.component.uuid == p.component && u.unit == wanted.uuid);
        if placed {
            return Err(CommandError::rejected(format!(
                "Unit {} of {} is placed already",
                p.unit, unit.designator
            )));
        }
        let copy = PartCopy {
            hash: entry.hash.clone(),
            part: entry.part.clone(),
            symbol: Some(entry.symbol.clone()),
            footprints: entry
                .footprints
                .iter()
                .filter_map(|f| sheet.footprint(*f))
                .map(|f| FootprintCopyData {
                    hash: f.hash.clone(),
                    footprint: f.footprint.clone(),
                })
                .collect(),
        };
        let part = copy.part.uuid;
        let placing = Placing {
            document: p.document,
            target: Target::Sheet,
            copy,
            unit: Some(p.unit),
            footprint: unit.footprint,
            rotation: Millideg::ZERO,
            mirror: false,
            side: Side::Top,
            designator: None,
            fields: BTreeMap::new(),
            component: Some(p.component),
        };
        cx.view::<Placement>()?.placing = Some(placing);
        cx.view::<ActiveTool>()?.0 = Tool::Place;
        Ok(ToolPlaceResult {
            tool: Tool::Place,
            part,
        })
    }
}
