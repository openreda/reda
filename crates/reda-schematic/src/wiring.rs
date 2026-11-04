//! Drawing connectivity: `schematic.wire.add`, `schematic.net_label.add` and
//! `schematic.no_connect.add`, the Wire splitting placement and moves share, and the
//! `schematic.nets` query.

use reda_command::{Angle, Command, CommandError, Context, Kind, Meta};
use reda_model::geom::{Millideg, Point};
use reda_model::{InDocument, NetLabel, NoConnect, Object, Uuid, Wire};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::component::At;
use crate::connectivity::{self, SheetConnectivity};
use crate::sheet;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WireAddParams {
    /// The Schematic Sheet to draw on.
    pub document: Uuid,
    /// The run's corners in order, at least two. Each segment must be horizontal or
    /// vertical; each becomes one Wire.
    pub points: Vec<At>,
}

/// What `schematic.wire.add` made.
#[derive(Debug, Serialize, JsonSchema)]
pub struct WireAddResult {
    /// The new Wires, one per segment, in run order.
    pub wires: Vec<Uuid>,
}

pub struct WireAdd;

impl Command for WireAdd {
    const META: Meta = Meta {
        id: "schematic.wire.add",
        title: "Add Wire",
        description: "Draws a Wire run through the given points on a Schematic Sheet, one \
                      Wire per horizontal or vertical segment. A Wire connects only where \
                      one of its ends touches a Pin or another Wire exactly; Nets and \
                      Junctions follow from the drawing.",
        category: "schematic",
        icon: "wire",
        kind: Kind::Edit,
    };
    type Params = WireAddParams;
    type Output = WireAddResult;

    fn run(&self, cx: &mut Context<'_>, p: WireAddParams) -> Result<WireAddResult, CommandError> {
        sheet(cx.project()?, p.document)?;
        if p.points.len() < 2 {
            return Err(CommandError::invalid_params(
                "/points",
                "a Wire run needs at least two points",
            ));
        }
        let points: Vec<Point> = p.points.iter().map(|a| a.point()).collect();
        let mut wires = Vec::with_capacity(points.len() - 1);
        for (i, pair) in points.windows(2).enumerate() {
            let wire = Wire {
                uuid: cx.new_id(),
                from: pair[0],
                to: pair[1],
            };
            if !wire.is_orthogonal() {
                return Err(CommandError::invalid_params(
                    &format!("/points/{}", i + 1),
                    "each Wire segment must be horizontal or vertical and not empty",
                ));
            }
            wires.push(wire.uuid);
            cx.tx()?
                .insert(Object::Wire(InDocument::new(p.document, wire)))?;
        }
        Ok(WireAddResult { wires })
    }
}

/// Connects placed Unit `unit` to the Wires its Pins land on partway along: each such Wire
/// is split at those Pins, and the pieces between two of the Unit's Pins are removed, so a
/// Component dropped inline on a Wire is not shorted out. The first piece keeps the Wire's
/// UUID. Call it after placing or moving a Unit.
pub(crate) fn split_wires_at_pins(
    cx: &mut Context<'_>,
    document: Uuid,
    unit: Uuid,
) -> Result<(), CommandError> {
    let body = sheet(cx.project()?, document)?;
    let Some(instance) = body.unit(unit) else {
        return Ok(());
    };
    let entry = body
        .part(instance.part)
        .expect("a Unit's part entry exists");
    let points: Vec<Point> = entry
        .symbol
        .unit_pins(instance.unit)
        .map(|pin| connectivity::pin_position(instance, pin))
        .collect();
    let mut cuts: Vec<(Wire, Vec<Point>)> = vec![];
    for wire in body.wires() {
        let mut inside: Vec<Point> = points
            .iter()
            .copied()
            .filter(|&p| connectivity::in_interior(wire, p))
            .collect();
        if inside.is_empty() {
            continue;
        }
        let from = wire.from;
        inside.sort_by_key(|p| (p.x - from.x).0.abs() + (p.y - from.y).0.abs());
        inside.dedup();
        cuts.push((wire.clone(), inside));
    }
    for (wire, inside) in cuts {
        let first = Wire {
            to: inside[0],
            ..wire.clone()
        };
        cx.tx()?
            .replace(Object::Wire(InDocument::new(document, first)))?;
        let last = Wire {
            uuid: cx.new_id(),
            from: *inside.last().expect("at least one cut"),
            to: wire.to,
        };
        cx.tx()?
            .insert(Object::Wire(InDocument::new(document, last)))?;
    }
    Ok(())
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NetLabelAddParams {
    /// The Schematic Sheet to place on.
    pub document: Uuid,
    /// The Net name; surrounding spaces are trimmed and a blank name is refused.
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
                      with the same name on the same sheet.",
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
        let name = p.name.trim();
        if name.is_empty() {
            return Err(CommandError::invalid_params(
                "/name",
                "a Net Label needs a name",
            ));
        }
        let label = NetLabel {
            uuid: cx.new_id(),
            name: name.to_owned(),
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
