//! What each Pin of a Component reaches on its current Footprint:
//! `schematic.component.pins`, which the sheet draws as the Pin's Pad number(s).

use reda_command::{Command, CommandError, Context, Kind, Meta};
use reda_model::{Project, Uuid};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::properties::units_of;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ComponentPinsParams {
    /// The Component, by UUID.
    pub component: Uuid,
}

/// A Component's Pins with the Pads they reach.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ComponentPins {
    /// The chosen Footprint, by Library UUID; none when the Part has no Footprint.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub footprint: Option<Uuid>,
    /// Every Pin of the Symbol, in its order.
    pub pins: Vec<PinPads>,
}

/// One Pin and the Pad numbers it reaches on the current Footprint.
#[derive(Debug, Serialize, JsonSchema)]
pub struct PinPads {
    pub pin: Uuid,
    /// The name of the Unit the Pin belongs to.
    pub unit: String,
    pub number: String,
    pub name: String,
    /// The Pad numbers the Part's pin map sends this Pin to that the Footprint has, in
    /// the pin map's order. Empty when the Footprint has none of them.
    pub pads: Vec<String>,
}

pub struct ComponentPinsQuery;

impl Command for ComponentPinsQuery {
    const META: Meta = Meta {
        id: "schematic.component.pins",
        title: "Component Pins",
        description: "Lists a Component's Pins with the Pad number(s) each reaches on the \
                      Component's current Footprint, through the Part's pin map.",
        category: "schematic",
        icon: "pin",
        kind: Kind::Query,
    };
    type Params = ComponentPinsParams;
    type Output = ComponentPins;

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: ComponentPinsParams,
    ) -> Result<ComponentPins, CommandError> {
        component_pins(cx.project()?, p.component)
    }
}

/// The Pins of `component` and the Pads each reaches on its current Footprint.
pub fn component_pins(project: &Project, component: Uuid) -> Result<ComponentPins, CommandError> {
    let Some(&(sheet, unit)) = units_of(project, component).first() else {
        return Err(CommandError::not_found(format!(
            "no Component has UUID {component}"
        )));
    };
    let sheet = crate::sheet(project, sheet)?;
    let entry = sheet
        .part(unit.part)
        .expect("a Unit's part entry is on its sheet");
    let footprint = unit.footprint.and_then(|chosen| {
        let i = entry.part.footprints.iter().position(|f| *f == chosen)?;
        sheet.footprint(entry.footprints[i])
    });
    let pins = entry
        .symbol
        .pins
        .iter()
        .map(|pin| {
            let pads = match footprint {
                Some(copy) => entry
                    .part
                    .pin_map
                    .pads_of(&pin.number)
                    .into_iter()
                    .filter(|n| copy.footprint.pads.iter().any(|pad| pad.number == *n))
                    .map(str::to_owned)
                    .collect(),
                None => vec![],
            };
            let unit = entry.symbol.units.iter().find(|u| u.uuid == pin.unit);
            PinPads {
                pin: pin.uuid,
                unit: unit.map(|u| u.name.clone()).unwrap_or_default(),
                number: pin.number.clone(),
                name: pin.name.clone(),
                pads,
            }
        })
        .collect();
    Ok(ComponentPins {
        footprint: unit.footprint,
        pins,
    })
}
