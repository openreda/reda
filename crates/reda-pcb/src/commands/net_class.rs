//! Net Classes and assigning Nets to them.

use reda_command::{
    AvailabilityContext, Command, CommandError, Context, Kind, Length, Meta, Registry, Unavailable,
};
use reda_model::{InDocument, Net, NetClass, Object, PcbDocument, Uuid};
use schemars::JsonSchema;
use serde::Deserialize;

use super::{Created, needs_pcb, optional_non_negative, pcb};

pub(super) fn register_all(registry: &mut Registry) {
    registry.add(NetClassAdd);
    registry.add(NetClassRename);
    registry.add(NetClassSetOverrides);
    registry.add(NetClassDelete);
    registry.add(NetSetClass);
}

/// The rules a Net Class overrides; each one left out uses the board-wide rule.
struct Overrides {
    clearance: Option<Length>,
    track_width: Option<Length>,
    via_diameter: Option<Length>,
    via_hole: Option<Length>,
}

/// The four override params of a Command's params struct.
macro_rules! overrides {
    ($p:expr) => {
        Overrides {
            clearance: $p.clearance,
            track_width: $p.track_width,
            via_diameter: $p.via_diameter,
            via_hole: $p.via_hole,
        }
    };
}

impl Overrides {
    /// Writes these overrides into `class`, replacing all four.
    fn apply(&self, class: &mut NetClass) -> Result<(), CommandError> {
        class.clearance = optional_non_negative("/clearance", self.clearance)?;
        class.track_width = optional_non_negative("/track_width", self.track_width)?;
        class.via_diameter = optional_non_negative("/via_diameter", self.via_diameter)?;
        class.via_hole = optional_non_negative("/via_hole", self.via_hole)?;
        Ok(())
    }
}

/// A trimmed name no other Net Class (but `except`) has, compared case-insensitively.
fn free_name(pcb: &PcbDocument, name: &str, except: Option<Uuid>) -> Result<String, CommandError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(CommandError::invalid_params("/name", "the name is empty"));
    }
    if let Some(other) = pcb.net_class_named(name)
        && Some(other.uuid) != except
    {
        return Err(CommandError::invalid_params(
            "/name",
            format!("a Net Class named {:?} already exists", other.name),
        ));
    }
    Ok(name.to_owned())
}

fn class(pcb: &PcbDocument, uuid: Uuid) -> Result<NetClass, CommandError> {
    pcb.net_class(uuid)
        .cloned()
        .ok_or_else(|| CommandError::not_found(format!("no Net Class has UUID {uuid}")))
}

fn put_class(cx: &mut Context<'_>, document: Uuid, class: NetClass) -> Result<(), CommandError> {
    cx.tx()?
        .replace(Object::NetClass(InDocument::new(document, class)))?;
    Ok(())
}

pub struct NetClassAdd;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NetClassAddParams {
    /// Unique among the board's Net Classes, ignoring case.
    pub name: String,
    /// Clearance to copper of other Nets.
    #[serde(default)]
    pub clearance: Option<Length>,
    /// Track width.
    #[serde(default)]
    pub track_width: Option<Length>,
    /// Via diameter.
    #[serde(default)]
    pub via_diameter: Option<Length>,
    /// Via hole.
    #[serde(default)]
    pub via_hole: Option<Length>,
    /// Nets to move into the new class.
    #[serde(default)]
    pub nets: Vec<Uuid>,
}

impl Command for NetClassAdd {
    const META: Meta = Meta {
        id: "pcb.net_class.add",
        title: "Add Net Class",
        description: "Adds a Net Class called `name`, optionally overriding the board-wide \
                      `clearance`, `track_width`, `via_diameter` and `via_hole` for its Nets, \
                      and moves `nets` into it.",
        category: "pcb",
        icon: "net_class_add",
        kind: Kind::Edit,
    };
    type Params = NetClassAddParams;
    type Output = Created;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: NetClassAddParams) -> Result<Created, CommandError> {
        let pcb = pcb(cx)?;
        let document = pcb.uuid;
        let name = free_name(pcb, &p.name, None)?;
        let mut nets = moved_nets(pcb, &p.nets, Uuid::default())?;
        let mut class = NetClass {
            uuid: cx.new_id(),
            name,
            clearance: None,
            track_width: None,
            via_diameter: None,
            via_hole: None,
        };
        overrides!(p).apply(&mut class)?;
        let uuid = class.uuid;
        for net in &mut nets {
            net.class = uuid;
        }
        let tx = cx.tx()?;
        tx.insert(Object::NetClass(InDocument::new(document, class)))?;
        for net in nets {
            tx.replace(Object::Net(InDocument::new(document, net)))?;
        }
        Ok(Created { uuid })
    }
}

pub struct NetClassRename;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NetClassRenameParams {
    /// The Net Class's UUID.
    pub class: Uuid,
    /// The new name, unique among the board's Net Classes, ignoring case.
    pub name: String,
}

impl Command for NetClassRename {
    const META: Meta = Meta {
        id: "pcb.net_class.rename",
        title: "Rename Net Class",
        description: "Renames a Net Class. Its Nets stay in it.",
        category: "pcb",
        icon: "rename",
        kind: Kind::Edit,
    };
    type Params = NetClassRenameParams;
    type Output = ();

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: NetClassRenameParams) -> Result<(), CommandError> {
        let pcb = pcb(cx)?;
        let document = pcb.uuid;
        let mut class = class(pcb, p.class)?;
        class.name = free_name(pcb, &p.name, Some(p.class))?;
        put_class(cx, document, class)
    }
}

pub struct NetClassSetOverrides;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NetClassSetOverridesParams {
    /// The Net Class's UUID.
    pub class: Uuid,
    /// Clearance to copper of other Nets.
    #[serde(default)]
    pub clearance: Option<Length>,
    /// Track width.
    #[serde(default)]
    pub track_width: Option<Length>,
    /// Via diameter.
    #[serde(default)]
    pub via_diameter: Option<Length>,
    /// Via hole.
    #[serde(default)]
    pub via_hole: Option<Length>,
}

impl Command for NetClassSetOverrides {
    const META: Meta = Meta {
        id: "pcb.net_class.set_overrides",
        title: "Set Net Class Rules",
        description: "Replaces a Net Class's overrides of `clearance`, `track_width`, \
                      `via_diameter` and `via_hole`. Any left out use the board-wide rule.",
        category: "pcb",
        icon: "net_class",
        kind: Kind::Edit,
    };
    type Params = NetClassSetOverridesParams;
    type Output = ();

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: NetClassSetOverridesParams) -> Result<(), CommandError> {
        let pcb = pcb(cx)?;
        let document = pcb.uuid;
        let mut class = class(pcb, p.class)?;
        overrides!(p).apply(&mut class)?;
        put_class(cx, document, class)
    }
}

pub struct NetClassDelete;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NetClassDeleteParams {
    /// The Net Class's UUID.
    pub class: Uuid,
}

impl Command for NetClassDelete {
    const META: Meta = Meta {
        id: "pcb.net_class.delete",
        title: "Delete Net Class",
        description: "Deletes a Net Class; its Nets move to the Default Net Class, which \
                      cannot be deleted.",
        category: "pcb",
        icon: "delete",
        kind: Kind::Edit,
    };
    type Params = NetClassDeleteParams;
    type Output = ();

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: NetClassDeleteParams) -> Result<(), CommandError> {
        let pcb = pcb(cx)?;
        let document = pcb.uuid;
        let class = class(pcb, p.class)?;
        let default = pcb.setup().default_net_class;
        if class.uuid == default {
            return Err(CommandError::rejected(
                "the Default Net Class cannot be deleted",
            ));
        }
        let moved: Vec<Net> = pcb
            .nets()
            .filter(|n| n.class == class.uuid)
            .map(|n| Net {
                class: default,
                ..n.clone()
            })
            .collect();
        let tx = cx.tx()?;
        for net in moved {
            tx.replace(Object::Net(InDocument::new(document, net)))?;
        }
        tx.remove(class.uuid)?;
        Ok(())
    }
}

pub struct NetSetClass;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NetSetClassParams {
    /// The Nets' UUIDs.
    pub nets: Vec<Uuid>,
    /// The Net Class's UUID.
    pub class: Uuid,
}

impl Command for NetSetClass {
    const META: Meta = Meta {
        id: "pcb.net.set_class",
        title: "Set Net Class",
        description: "Assigns each of `nets` to the Net Class `class`.",
        category: "pcb",
        icon: "net_class",
        kind: Kind::Edit,
    };
    type Params = NetSetClassParams;
    type Output = ();

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: NetSetClassParams) -> Result<(), CommandError> {
        let pcb = pcb(cx)?;
        let document = pcb.uuid;
        class(pcb, p.class)?;
        let nets = moved_nets(pcb, &p.nets, p.class)?;
        let tx = cx.tx()?;
        for net in nets {
            tx.replace(Object::Net(InDocument::new(document, net)))?;
        }
        Ok(())
    }
}

/// `nets` as they are once moved into `class`; an unknown Net is `not_found`.
fn moved_nets(pcb: &PcbDocument, nets: &[Uuid], class: Uuid) -> Result<Vec<Net>, CommandError> {
    nets.iter()
        .enumerate()
        .map(|(i, uuid)| {
            let net = pcb.net(*uuid).ok_or_else(|| {
                CommandError::not_found(format!("no Net has UUID {uuid} (`nets[{i}]`)"))
            })?;
            Ok(Net {
                class,
                ..net.clone()
            })
        })
        .collect()
}
