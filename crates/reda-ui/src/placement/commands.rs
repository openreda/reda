//! The placement Tool's view Commands: `tool.place` picks a Part up, `tool.mirror` and
//! `tool.place.set` change it in flight (`tool.rotate` and `tool.flip`, in
//! [`crate::tools`], turn it too).

use std::collections::BTreeMap;

use reda_engine::command::{
    AvailabilityContext, Command, CommandError, Context, Kind, Meta, Preset, Registry, Unavailable,
};
use reda_engine::model::geom::Millideg;
use reda_engine::model::{ComponentType, Side, Uuid};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::{Placement, Placing, Target};
use crate::tools::{ActiveTool, Tool};

/// Adds `tool.place`, `tool.mirror` and `tool.place.set` to `registry`. The Shell does
/// this for its engine.
pub fn register_all(registry: &mut Registry) {
    registry.add(ToolPlace);
    registry.add(ToolMirror);
    registry.add(ToolPlaceSet);
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
                        "{} is a Standard Part: it reaches the board through Sync",
                        copy.part.name
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
        };
        cx.view::<Placement>()?.placing = Some(placing);
        cx.view::<ActiveTool>()?.0 = Tool::Place;
        Ok(ToolPlaceResult {
            tool: Tool::Place,
            part,
        })
    }
}

#[derive(Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Axis {
    /// Left to right, about the vertical axis.
    X,
    /// Top to bottom, about the horizontal axis.
    Y,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ToolMirrorParams {
    axis: Axis,
}

/// What `tool.mirror` returns.
#[derive(Serialize, JsonSchema)]
pub struct Mirrored {}

struct ToolMirror;

impl Command for ToolMirror {
    const META: Meta = Meta {
        id: "tool.mirror",
        title: "Mirror",
        description: "Mirrors the Part being placed on a sheet, left to right (x) or top to \
                      bottom (y). A board placement turns over with Flip Side instead.",
        category: "tool",
        icon: "mirror",
        kind: Kind::View,
    };
    type Params = ToolMirrorParams;
    type Output = Mirrored;

    fn presets(&self) -> Vec<Preset> {
        vec![
            Preset::new("x", "Mirror X", "mirror", json!({"axis": "x"})),
            Preset::new("y", "Mirror Y", "mirror", json!({"axis": "y"})),
        ]
    }

    fn run(&self, cx: &mut Context<'_>, p: ToolMirrorParams) -> Result<Mirrored, CommandError> {
        edit_placing(cx, |placing| placing.mirror(matches!(p.axis, Axis::X)))?;
        Ok(Mirrored {})
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
