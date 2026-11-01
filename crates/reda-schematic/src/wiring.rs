//! Drawing connectivity: `schematic.wire.add`, `schematic.net_label.add` and
//! `schematic.no_connect.add`.

use reda_command::{Angle, Command, CommandError, Context, Kind, Meta};
use reda_model::geom::{Millideg, Point};
use reda_model::{InDocument, NetLabel, Object, Uuid, Wire};
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
                      name, Net Label names, Pins, Wires and Net Labels, plus the sheet's \
                      Junctions. Every Pin is in exactly one Net. Nothing is stored.",
        category: "schematic",
        icon: "net",
        kind: Kind::Query,
    };
    type Params = NetsParams;
    type Output = SheetConnectivity;

    fn run(&self, cx: &mut Context<'_>, p: NetsParams) -> Result<SheetConnectivity, CommandError> {
        Ok(connectivity::derive(sheet(cx.project()?, p.document)?))
    }
}
