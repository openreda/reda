//! Drawing connectivity: `schematic.wire.add`, `schematic.net_label.add` and
//! `schematic.no_connect.add`, and the `schematic.nets` query.

use reda_command::{Angle, Command, CommandError, Context, Kind, Meta};
use reda_model::geom::{Millideg, Point};
use reda_model::{InDocument, NetLabel, NoConnect, Object, Uuid, Wire};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::component::At;
use crate::connectivity::{self, SheetConnectivity};
use crate::normalise::{Wiring, normalise};
use crate::sheet;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WireAddParams {
    /// The Schematic Sheet to draw on.
    pub document: Uuid,
    /// The run's vertices in order, at least two distinct, at any angle. Repeated points
    /// (zero-length segments) are dropped.
    pub points: Vec<At>,
    /// How the Wires this edit touches are normalised; the fixed defaults when left out.
    pub wiring: Option<Wiring>,
}

/// What `schematic.wire.add` made.
#[derive(Debug, Serialize, JsonSchema)]
pub struct WireAddResult {
    /// The Wires now holding the run: the one Wire it makes, or its pieces once
    /// normalising splits it, or the Wire it merged into.
    pub wires: Vec<Uuid>,
}

pub struct WireAdd;

impl Command for WireAdd {
    const META: Meta = Meta {
        id: "schematic.wire.add",
        title: "Add Wire",
        description: "Draws a Wire run through the given points on a Schematic Sheet as \
                      one Wire, at any angle. A Wire connects where an end touches a Pin or \
                      any point of another Wire exactly, and where it passes over a Pin, \
                      Port, Power Port or Net Label; two Wires crossing mid-span do not. \
                      Nets and Junctions follow from the drawing. The Wires it touches are \
                      normalised as `wiring` says.",
        category: "schematic",
        icon: "wire",
        kind: Kind::Edit,
    };
    type Params = WireAddParams;
    type Output = WireAddResult;

    fn run(&self, cx: &mut Context<'_>, p: WireAddParams) -> Result<WireAddResult, CommandError> {
        let before = sheet(cx.project()?, p.document)?.clone();
        if p.points.len() < 2 {
            return Err(CommandError::invalid_params(
                "/points",
                "a Wire run needs at least two points",
            ));
        }
        let points: Vec<Point> = p.points.iter().map(|a| a.point()).collect();
        let Some(wire) = Wire::through(cx.new_id(), points) else {
            return Err(CommandError::invalid_params(
                "/points",
                "a Wire run needs at least two different points",
            ));
        };
        let uuid = wire.uuid;
        cx.tx()?
            .insert(Object::Wire(InDocument::new(p.document, wire)))?;
        let lineage = normalise(cx, p.document, &before, &p.wiring.unwrap_or_default())?;
        Ok(WireAddResult {
            wires: lineage.trace(uuid),
        })
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NetLabelAddParams {
    /// The Schematic Sheet to place on.
    pub document: Uuid,
    /// The Net name; surrounding spaces are trimmed, and a blank name or one holding
    /// `[ ] { } , ..` or a control character is refused.
    pub name: String,
    /// The connection point: it joins the Wires through it and any Pin there.
    pub at: At,
    /// Counter-clockwise; 0 by default. Only changes how the label is drawn.
    pub rotation: Option<Angle>,
}

/// What `schematic.net_label.add` made.
#[derive(Debug, Serialize, JsonSchema)]
pub struct NetLabelAddResult {
    pub net_label: Uuid,
}

pub struct NetLabelAdd;

impl Command for NetLabelAdd {
    const META: Meta = Meta {
        id: "schematic.net_label.add",
        title: "Add Net Label",
        description: "Places a Net Label on a Schematic Sheet. It names the Net of the Wires \
                      through its point (and any Pin there) and joins it to every Net Label \
                      with the same name, ignoring case, on the same sheet.",
        category: "schematic",
        icon: "net_label",
        kind: Kind::Edit,
    };
    type Params = NetLabelAddParams;
    type Output = NetLabelAddResult;

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: NetLabelAddParams,
    ) -> Result<NetLabelAddResult, CommandError> {
        sheet(cx.project()?, p.document)?;
        let name = crate::ports::net_name(&p.name, "Net Label")?;
        let label = NetLabel {
            uuid: cx.new_id(),
            name,
            at: p.at.point(),
            rotation: p.rotation.map_or(Millideg::ZERO, Angle::millideg),
        };
        let uuid = label.uuid;
        cx.tx()?
            .insert(Object::NetLabel(InDocument::new(p.document, label)))?;
        Ok(NetLabelAddResult { net_label: uuid })
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NetLabelSetParams {
    /// The Net Label's UUID.
    pub uuid: Uuid,
    /// The new Net name, checked as `schematic.net_label.add` checks it.
    pub name: String,
}

pub struct NetLabelSet;

impl Command for NetLabelSet {
    const META: Meta = Meta {
        id: "schematic.net_label.set",
        title: "Rename Net Label",
        description: "Renames a Net Label, with the name checks of schematic.net_label.add \
                      (in-place edit uses it).",
        category: "schematic",
        icon: "net_label",
        kind: Kind::Edit,
    };
    type Params = NetLabelSetParams;
    type Output = crate::ports::SetResult;

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: NetLabelSetParams,
    ) -> Result<crate::ports::SetResult, CommandError> {
        let Some(Object::NetLabel(mut label)) = cx.project()?.object(p.uuid) else {
            return Err(CommandError::not_found(format!(
                "no Net Label has UUID {}",
                p.uuid
            )));
        };
        label.object.name = crate::ports::net_name(&p.name, "Net Label")?;
        cx.tx()?.replace(Object::NetLabel(label))?;
        Ok(crate::ports::SetResult { uuid: p.uuid })
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NoConnectAddParams {
    /// The Schematic Sheet.
    pub document: Uuid,
    /// A Pin's connection point.
    pub at: At,
}

/// What `schematic.no_connect.add` made.
#[derive(Debug, Serialize, JsonSchema)]
pub struct NoConnectAddResult {
    pub no_connect: Uuid,
}

pub struct NoConnectAdd;

impl Command for NoConnectAdd {
    const META: Meta = Meta {
        id: "schematic.no_connect.add",
        title: "Add No-Connect Marker",
        description: "Marks the Pin at a point as deliberately left unconnected. Refused \
                      where no Pin connects or the Pin already has a marker.",
        category: "schematic",
        icon: "no_connect",
        kind: Kind::Edit,
    };
    type Params = NoConnectAddParams;
    type Output = NoConnectAddResult;

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: NoConnectAddParams,
    ) -> Result<NoConnectAddResult, CommandError> {
        let body = sheet(cx.project()?, p.document)?;
        let at = p.at.point();
        if !connectivity::placed_pins(body)
            .iter()
            .any(|(_, _, pin)| *pin == at)
        {
            return Err(CommandError::rejected(
                "No-Connect Markers go on Pins, and no Pin connects at that point",
            ));
        }
        if body.no_connects().any(|m| m.at == at) {
            return Err(CommandError::rejected(
                "the Pin at that point already has a No-Connect Marker",
            ));
        }
        let marker = NoConnect {
            uuid: cx.new_id(),
            at,
        };
        let uuid = marker.uuid;
        cx.tx()?
            .insert(Object::NoConnect(InDocument::new(p.document, marker)))?;
        Ok(NoConnectAddResult { no_connect: uuid })
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NetsParams {
    /// The Schematic Sheet.
    pub document: Uuid,
}

pub struct SchematicNets;

impl Command for SchematicNets {
    const META: Meta = Meta {
        id: "schematic.nets",
        title: "Schematic Nets",
        description: "Derives the Nets of a Schematic Sheet from its drawing: each Net's \
                      name (chosen over the Project-wide Net it is part of), the Net Label, \
                      Port and Power Port names on it, its Pins, Wires, Net Labels, Ports \
                      and Power Ports, plus the sheet's Junctions. Every Pin is in exactly \
                      one Net. Nothing is stored.",
        category: "schematic",
        icon: "net",
        kind: Kind::Query,
    };
    type Params = NetsParams;
    type Output = SheetConnectivity;

    fn run(&self, cx: &mut Context<'_>, p: NetsParams) -> Result<SheetConnectivity, CommandError> {
        let project = cx.project()?;
        sheet(project, p.document)?;
        Ok(connectivity::sheet_connectivity(project, p.document).expect("the sheet exists"))
    }
}
