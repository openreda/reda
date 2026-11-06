//! Connecting across sheets: `schematic.port.add` / `set`, `schematic.power_port.add` /
//! `set`, the Power Port Presets and the `schematic.project_nets` query.

use reda_command::{Angle, Command, CommandError, Context, Kind, Meta, NoParams};
use reda_model::geom::Millideg;
use reda_model::{InDocument, Object, Port, PortDirection, PowerPort, PowerPortStyle, Uuid};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::component::At;
use crate::connectivity::{self, ProjectNet};
use crate::sheet;

/// A trimmed, non-blank Net name without reserved characters (`[ ] { } , ..` and
/// control characters), or `invalid_params` at `/name`.
pub(crate) fn net_name(name: &str, what: &str) -> Result<String, CommandError> {
    match name.trim() {
        "" => Err(CommandError::invalid_params(
            "/name",
            format!("a {what} needs a name"),
        )),
        name => match reda_model::net_name::name_problem(name) {
            Some(problem) => Err(CommandError::invalid_params("/name", problem)),
            None => Ok(name.to_owned()),
        },
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PortAddParams {
    /// The Schematic Sheet to place on.
    pub document: Uuid,
    /// The Net name; surrounding spaces are trimmed, and a blank name or one holding
    /// `[ ] { } , ..` or a control character is refused.
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
                      sheet of the Project, and names that Net unless a Net Label or Power \
                      Port does (or the Project stops Ports naming Nets).",
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
            name: net_name(&p.name, "Port")?,
            direction: p.direction,
            at: p.at.point(),
            rotation: p.rotation.map_or(Millideg::ZERO, Angle::millideg),
            hide_cross_reference: false,
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
    /// The Net name, such as GND or 3V3; surrounding spaces are trimmed, and a blank name
    /// or one holding `[ ] { } , ..` or a control character is refused.
    pub name: String,
    /// The connection point: it joins the Wires through it and any Pin there.
    pub at: At,
    /// The built-in look. Defaults to `ground` for names with GND or VSS in them and to
    /// `bar` for everything else.
    pub style: Option<PowerPortStyle>,
    /// Counter-clockwise; 0 by default. Only changes how the symbol is drawn.
    pub rotation: Option<Angle>,
    /// Draws the symbol without its name; off by default. The name still joins the Net.
    #[serde(default)]
    pub hide_name: bool,
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
                      names ahead of any Port, and ahead of Net Labels when the Project's \
                      Power Port Names Take Priority option is on.",
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
        let name = net_name(&p.name, "Power Port")?;
        let style = p.style.unwrap_or_else(|| default_power_port_style(&name));
        let port = PowerPort {
            uuid: cx.new_id(),
            name,
            style,
            at: p.at.point(),
            rotation: p.rotation.map_or(Millideg::ZERO, Angle::millideg),
            hide_name: p.hide_name,
        };
        let uuid = port.uuid;
        cx.tx()?
            .insert(Object::PowerPort(InDocument::new(p.document, port)))?;
        Ok(PowerPortAddResult { power_port: uuid })
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PortSetParams {
    /// The Port's UUID.
    pub uuid: Uuid,
    /// A new Net name, checked as `schematic.port.add` checks it.
    pub name: Option<String>,
    /// `in`, `out` or `bidirectional`.
    pub direction: Option<PortDirection>,
    /// Hides the Port's cross-reference note.
    pub hide_cross_reference: Option<bool>,
}

/// The object a `set` Command changed.
#[derive(Debug, Serialize, JsonSchema)]
pub struct SetResult {
    pub uuid: Uuid,
}

pub struct PortSet;

impl Command for PortSet {
    const META: Meta = Meta {
        id: "schematic.port.set",
        title: "Edit Port",
        description: "Changes a Port's name, direction or whether it hides its \
                      cross-reference note; what is left out stays.",
        category: "schematic",
        icon: "port",
        kind: Kind::Edit,
    };
    type Params = PortSetParams;
    type Output = SetResult;

    fn run(&self, cx: &mut Context<'_>, p: PortSetParams) -> Result<SetResult, CommandError> {
        let Some(Object::Port(mut port)) = cx.project()?.object(p.uuid) else {
            return Err(CommandError::not_found(format!(
                "no Port has UUID {}",
                p.uuid
            )));
        };
        if let Some(name) = p.name {
            port.object.name = net_name(&name, "Port")?;
        }
        if let Some(direction) = p.direction {
            port.object.direction = direction;
        }
        if let Some(hide) = p.hide_cross_reference {
            port.object.hide_cross_reference = hide;
        }
        cx.tx()?.replace(Object::Port(port))?;
        Ok(SetResult { uuid: p.uuid })
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PowerPortSetParams {
    /// The Power Port's UUID.
    pub uuid: Uuid,
    /// A new Net name, checked as `schematic.power_port.add` checks it.
    pub name: Option<String>,
    /// A new built-in look.
    pub style: Option<PowerPortStyle>,
    /// Draws the symbol without its name; the name still joins the Net.
    pub hide_name: Option<bool>,
}

pub struct PowerPortSet;

impl Command for PowerPortSet {
    const META: Meta = Meta {
        id: "schematic.power_port.set",
        title: "Edit Power Port",
        description: "Changes a Power Port's name, style or whether its name is hidden; \
                      what is left out stays. A hidden name still joins the Net.",
        category: "schematic",
        icon: "power_port",
        kind: Kind::Edit,
    };
    type Params = PowerPortSetParams;
    type Output = SetResult;

    fn run(&self, cx: &mut Context<'_>, p: PowerPortSetParams) -> Result<SetResult, CommandError> {
        let Some(Object::PowerPort(mut port)) = cx.project()?.object(p.uuid) else {
            return Err(CommandError::not_found(format!(
                "no Power Port has UUID {}",
                p.uuid
            )));
        };
        if let Some(name) = p.name {
            port.object.name = net_name(&name, "Power Port")?;
        }
        if let Some(style) = p.style {
            port.object.style = style;
        }
        if let Some(hide) = p.hide_name {
            port.object.hide_name = hide;
        }
        cx.tx()?.replace(Object::PowerPort(port))?;
        Ok(SetResult { uuid: p.uuid })
    }
}

/// The Power Port Presets the Active Bar offers: a name and the look it gets.
pub const POWER_PORT_PRESETS: [(&str, PowerPortStyle); 4] = [
    ("GND", PowerPortStyle::Ground),
    ("3V3", PowerPortStyle::Bar),
    ("5V", PowerPortStyle::Bar),
    ("VCC", PowerPortStyle::Bar),
];

/// The look a Power Port named `name` gets when none is given: `ground` for ground-like
/// names (GND, VSS), else `bar`.
pub fn default_power_port_style(name: &str) -> PowerPortStyle {
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
                      across sheets by Ports and Power Ports (names compare ignoring \
                      case), with the winning name (Net Label > Power Port > Port > \
                      auto-name, as the Project's options change it, then the lowest by \
                      code point), the board name, every name on the Net, the sheets it \
                      is on and its Pins. Nothing is stored.",
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
