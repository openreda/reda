//! Connecting across sheets: `schematic.port.add`, `schematic.power_port.add` and the
//! `schematic.project_nets` query.

use reda_command::{Angle, Command, CommandError, Context, Kind, Meta, NoParams};
use reda_model::geom::Millideg;
use reda_model::{InDocument, Object, Port, PortDirection, PowerPort, PowerPortStyle, Uuid};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::component::At;
use crate::connectivity::{self, ProjectNet};
use crate::sheet;

/// A trimmed, non-blank name, or `invalid_params` at `/name`.
fn name(name: &str, what: &str) -> Result<String, CommandError> {
    match name.trim() {
        "" => Err(CommandError::invalid_params(
            "/name",
            format!("a {what} needs a name"),
        )),
        name => Ok(name.to_owned()),
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PortAddParams {
    /// The Schematic Sheet to place on.
    pub document: Uuid,
    /// The Net name; surrounding spaces are trimmed and a blank name is refused.
    pub name: String,
    /// The connection point: it joins the Wires through it and any Pin there.
    pub at: At,
    /// `in`, `out` or `bidirectional` (the default). Only changes how the Port is drawn
    /// and checked.
    #[serde(default)]
    pub direction: PortDirection,
    /// Counter-clockwise; 0 by default. Only changes how the Port is drawn.
    pub rotation: Option<Angle>,
}

/// What `schematic.port.add` made.
#[derive(Debug, Serialize, JsonSchema)]
pub struct PortAddResult {
    pub port: Uuid,
}

pub struct PortAdd;

impl Command for PortAdd {
    const META: Meta = Meta {
        id: "schematic.port.add",
        title: "Add Port",
        description: "Places a Port on a Schematic Sheet. It joins the Wires through its \
                      point (and any Pin there) to every Port with the same name on any \
                      sheet of the Project, and names that Net unless a Power Port does.",
        category: "schematic",
        icon: "port",
        kind: Kind::Edit,
    };
    type Params = PortAddParams;
    type Output = PortAddResult;

    fn run(&self, cx: &mut Context<'_>, p: PortAddParams) -> Result<PortAddResult, CommandError> {
        sheet(cx.project()?, p.document)?;
        let port = Port {
            uuid: cx.new_id(),
            name: name(&p.name, "Port")?,
            direction: p.direction,
            at: p.at.point(),
            rotation: p.rotation.map_or(Millideg::ZERO, Angle::millideg),
        };
        let uuid = port.uuid;
        cx.tx()?
            .insert(Object::Port(InDocument::new(p.document, port)))?;
        Ok(PortAddResult { port: uuid })
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PowerPortAddParams {
    /// The Schematic Sheet to place on.
    pub document: Uuid,
    /// The Net name, such as GND or 3V3; surrounding spaces are trimmed and a blank name
    /// is refused.
    pub name: String,
    /// The connection point: it joins the Wires through it and any Pin there.
    pub at: At,
    /// The built-in look. Defaults to `ground` for names with GND or VSS in them and to
    /// `bar` for everything else.
    pub style: Option<PowerPortStyle>,
    /// Counter-clockwise; 0 by default. Only changes how the symbol is drawn.
    pub rotation: Option<Angle>,
}

/// What `schematic.power_port.add` made.
#[derive(Debug, Serialize, JsonSchema)]
pub struct PowerPortAddResult {
    pub power_port: Uuid,
}

pub struct PowerPortAdd;

impl Command for PowerPortAdd {
    const META: Meta = Meta {
        id: "schematic.power_port.add",
        title: "Add Power Port",
        description: "Places a Power Port (a built-in symbol in a fixed style, not a Library \
                      element) on a Schematic Sheet. It joins the Wires through its point \
                      (and any Pin there) to the Project-wide Net of its name, which it \
                      names ahead of any Port or Net Label.",
        category: "schematic",
        icon: "power_port",
        kind: Kind::Edit,
    };
    type Params = PowerPortAddParams;
    type Output = PowerPortAddResult;

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: PowerPortAddParams,
    ) -> Result<PowerPortAddResult, CommandError> {
        sheet(cx.project()?, p.document)?;
        let name = name(&p.name, "Power Port")?;
        let style = p.style.unwrap_or_else(|| default_style(&name));
        let port = PowerPort {
            uuid: cx.new_id(),
            name,
            style,
            at: p.at.point(),
            rotation: p.rotation.map_or(Millideg::ZERO, Angle::millideg),
        };
        let uuid = port.uuid;
        cx.tx()?
            .insert(Object::PowerPort(InDocument::new(p.document, port)))?;
        Ok(PowerPortAddResult { power_port: uuid })
    }
}

/// `ground` for ground-like names, else `bar`.
fn default_style(name: &str) -> PowerPortStyle {
    let upper = name.to_uppercase();
    if upper.contains("GND") || upper.contains("VSS") {
        PowerPortStyle::Ground
    } else {
        PowerPortStyle::Bar
    }
}

/// What `schematic.project_nets` returns.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ProjectNetsResult {
    /// Named Nets first, in natural order of name, then unnamed ones.
    pub nets: Vec<ProjectNet>,
}

pub struct ProjectNets;

impl Command for ProjectNets {
    const META: Meta = Meta {
        id: "schematic.project_nets",
        title: "Project Nets",
        description: "Derives the Nets of the whole Project: each sheet's Nets joined \
                      across sheets by Ports and Power Ports, with the winning name \
                      (Power Port > Port > Net Label > auto-name), every name on the Net, \
                      the sheets it is on and its Pins. Nothing is stored.",
        category: "schematic",
        icon: "net",
        kind: Kind::Query,
    };
    type Params = NoParams;
    type Output = ProjectNetsResult;

    fn run(&self, cx: &mut Context<'_>, _: NoParams) -> Result<ProjectNetsResult, CommandError> {
        Ok(ProjectNetsResult {
            nets: connectivity::project_nets(cx.project()?),
        })
    }
}
