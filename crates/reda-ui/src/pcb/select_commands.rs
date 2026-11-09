//! The PCB selection Commands: everything on a Layer, a Net's copper, the Tracks and Vias
//! up to the next Pad, and a Footprint's connections or Nets. Each replaces the selection
//! with what [`reda_engine::pcb::select`] picks. Like `selection.set` from scripts, they
//! are explicit picks, so the Selection Filter doesn't apply.

use reda_engine::command::{
    AvailabilityContext, Command, CommandError, Context, Kind, Meta, Registry, Unavailable,
};
use reda_engine::model::{PcbDocument, Uuid};
use reda_engine::pcb::select::{self, Picked};
use reda_engine::pcb::{Copper, LayerRef};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::selection::{Selection, SelectionSetResult};

/// Adds the PCB selection Commands to `registry`.
pub fn register_all(registry: &mut Registry) {
    registry.add(OnLayer);
    registry.add(OfNet);
    registry.add(PhysicalConnection);
    registry.add(FootprintConnections);
    registry.add(FootprintNets);
}

fn needs_pcb(cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
    match cx.project {
        Some(p) if p.pcb().is_some() => Ok(()),
        Some(_) => Err(Unavailable {
            reason: "the Project has no PCB Document".into(),
        }),
        None => Err(Unavailable {
            reason: "no Project is open".into(),
        }),
    }
}

/// Picks with `pick` from the board and its copper, then makes that the selection.
fn select_with(
    cx: &mut Context<'_>,
    pick: impl FnOnce(&PcbDocument, &Copper) -> Result<Picked, CommandError>,
) -> Result<SelectionSetResult, CommandError> {
    let project = cx.project()?;
    let pcb = project
        .pcb()
        .ok_or_else(|| CommandError::unavailable("the Project has no PCB Document"))?;
    let copper = Copper::of(project).expect("the Project has a PCB Document");
    let picked = pick(pcb, &copper)?;
    let selection = Selection {
        objects: picked.objects,
        pads: picked.pads,
        ..Selection::default()
    };
    let count = selection.len();
    *cx.view::<Selection>()? = selection;
    Ok(SelectionSetResult { count })
}

fn invalid(pointer: &'static str) -> impl Fn(String) -> CommandError {
    move |message| CommandError::invalid_params(pointer, message)
}

struct OnLayer;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OnLayerParams {
    /// The Layer, such as `copper_top` or `silkscreen_bottom`, or its UUID.
    pub layer: LayerRef,
}

impl Command for OnLayer {
    const META: Meta = Meta {
        id: "pcb.selection.on_layer",
        title: "Select All on Layer",
        description: "Selects everything on a Layer: its copper (Pads as Footprint Pads), \
                      Zones, Zone Cutouts, Keepouts, graphics, text and Dimensions, and on \
                      the Board Outline Layer the outline and Board Cutouts.",
        category: "selection",
        icon: "layers",
        kind: Kind::View,
    };
    type Params = OnLayerParams;
    type Output = SelectionSetResult;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: OnLayerParams,
    ) -> Result<SelectionSetResult, CommandError> {
        select_with(cx, |pcb, copper| {
            let layer = p.layer.resolve(pcb, "/layer")?.uuid;
            Ok(select::on_layer(pcb, copper, layer))
        })
    }
}

struct OfNet;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OfNetParams {
    pub net: Uuid,
}

impl Command for OfNet {
    const META: Meta = Meta {
        id: "pcb.selection.net",
        title: "Select Net",
        description: "Selects all the copper of a Net: its Tracks, Vias, Free Pads, Zones \
                      and Footprint Pads.",
        category: "selection",
        icon: "net",
        kind: Kind::View,
    };
    type Params = OfNetParams;
    type Output = SelectionSetResult;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: OfNetParams,
    ) -> Result<SelectionSetResult, CommandError> {
        select_with(cx, |pcb, _| {
            if pcb.net(p.net).is_none() {
                return Err(CommandError::invalid_params(
                    "/net",
                    format!("no Net has UUID {}", p.net),
                ));
            }
            Ok(select::net(pcb, p.net))
        })
    }
}

struct PhysicalConnection;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PhysicalConnectionParams {
    /// A Track or Via to start from.
    pub seed: Uuid,
    /// Only Tracks on the seed Track's Layer, without passing through Vias.
    #[serde(default)]
    pub single_layer: bool,
}

impl Command for PhysicalConnection {
    const META: Meta = Meta {
        id: "pcb.selection.physical_connection",
        title: "Select Physical Connection",
        description: "Selects the Tracks and Vias joined to a Track or Via, up to the next \
                      Pad; with `single_layer`, only Tracks on the seed's Layer.",
        category: "selection",
        icon: "select",
        kind: Kind::View,
    };
    type Params = PhysicalConnectionParams;
    type Output = SelectionSetResult;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: PhysicalConnectionParams,
    ) -> Result<SelectionSetResult, CommandError> {
        select_with(cx, |pcb, copper| {
            select::physical_connection(pcb, copper, p.seed, p.single_layer)
                .map_err(invalid("/seed"))
        })
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FootprintsParams {
    /// Footprint instances, by UUID.
    pub footprints: Vec<Uuid>,
}

struct FootprintConnections;

impl Command for FootprintConnections {
    const META: Meta = Meta {
        id: "pcb.selection.footprint_connections",
        title: "Select Footprint Connections",
        description: "Selects the Tracks and Vias joined to the Footprints' Pads, each up to \
                      the next Pad.",
        category: "selection",
        icon: "select",
        kind: Kind::View,
    };
    type Params = FootprintsParams;
    type Output = SelectionSetResult;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: FootprintsParams,
    ) -> Result<SelectionSetResult, CommandError> {
        select_with(cx, |pcb, copper| {
            select::footprint_connections(pcb, copper, &p.footprints)
                .map_err(invalid("/footprints"))
        })
    }
}

struct FootprintNets;

impl Command for FootprintNets {
    const META: Meta = Meta {
        id: "pcb.selection.footprint_nets",
        title: "Select Footprint Nets",
        description: "Selects all the copper of every Net the Footprints' Pads are on.",
        category: "selection",
        icon: "net",
        kind: Kind::View,
    };
    type Params = FootprintsParams;
    type Output = SelectionSetResult;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: FootprintsParams,
    ) -> Result<SelectionSetResult, CommandError> {
        select_with(cx, |pcb, _| {
            select::footprint_nets(pcb, &p.footprints).map_err(invalid("/footprints"))
        })
    }
}
