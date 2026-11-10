//! The PCB selection Commands: everything on a Layer, a Net's copper, the Tracks and Vias
//! up to the next Pad, a Footprint's connections or Nets, the copper connected to the
//! selection (`U`) and the copper selection cycle (`Tab`). Each sets the selection from
//! what [`reda_engine::pcb::select`] picks. Like `selection.set` from scripts, they
//! are explicit picks, so the Selection Filter doesn't apply.

use reda_engine::command::{
    AvailabilityContext, Command, CommandError, Context, Kind, Meta, NoParams, Registry,
    Unavailable,
};
use reda_engine::model::{PcbDocument, Uuid};
use reda_engine::pcb::select::{self, Picked, Scope};
use reda_engine::pcb::{Copper, Item, LayerRef};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::selection::{PadRef, Selection, SelectionSetResult};
use crate::tools::{ActiveTool, Tool};

/// Adds the PCB selection Commands to `registry`.
pub fn register_all(registry: &mut Registry) {
    registry.add(OnLayer);
    registry.add(OfNet);
    registry.add(PhysicalConnection);
    registry.add(FootprintConnections);
    registry.add(FootprintNets);
    registry.add(Connected);
    registry.add(Widen);
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

/// The copper among `objects` and `pads`, as items to start from.
fn seed_items(pcb: &PcbDocument, objects: &[Uuid], pads: &[PadRef]) -> Vec<Item> {
    let mut seeds: Vec<Item> = Vec::new();
    for o in objects {
        if pcb.track(*o).is_some() {
            seeds.push(Item::Track { uuid: *o });
        } else if pcb.via(*o).is_some() {
            seeds.push(Item::Via { uuid: *o });
        } else if pcb.free_pad(*o).is_some() {
            seeds.push(Item::FreePad { uuid: *o });
        } else if let Some(f) = pcb.footprint(*o) {
            seeds.extend(f.pads.iter().map(|p| Item::Pad {
                footprint: *o,
                pad: p.pad,
            }));
        }
    }
    seeds.extend(pads.iter().map(|p| Item::Pad {
        footprint: p.footprint,
        pad: p.pad,
    }));
    seeds
}

/// Adds what `seeds` reach in `scope` to `selection`. A Pad of a selected Footprint is
/// selected through its Footprint already.
fn add_reached(
    pcb: &PcbDocument,
    copper: &Copper,
    seeds: &[Item],
    scope: Scope,
    selection: &mut Selection,
) {
    let picked = select::connected(pcb, copper, seeds, scope);
    for o in picked.objects {
        if !selection.objects.contains(&o) && pcb.footprint(o).is_none() {
            selection.objects.push(o);
        }
    }
    for pad in picked.pads {
        if !selection.pads.contains(&pad) && !selection.objects.contains(&pad.footprint) {
            selection.pads.push(pad);
        }
    }
}

struct Connected;

#[derive(Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ConnectedParams {
    /// Tracks, Vias, Free Pads and Footprints to start from; left out, the selection.
    #[serde(default)]
    pub seeds: Option<Vec<Uuid>>,
    /// How far to reach: `layer` (the contiguous Track on its Layer), `copper` (all the
    /// copper physically connected, the default) or `net` (all the copper of their Nets).
    #[serde(default)]
    pub scope: Scope,
}

impl Command for Connected {
    const META: Meta = Meta {
        id: "pcb.selection.connected",
        title: "Select Connected",
        description: "Adds to the selection the copper the seeds reach (the selected \
                      Tracks, Vias, Pads and Footprints unless given): the contiguous Track \
                      on its Layer, all the copper physically connected whatever its Net \
                      (the default), or all the copper of their Nets.",
        category: "selection",
        icon: "select_connected",
        kind: Kind::View,
    };
    type Params = ConnectedParams;
    type Output = SelectionSetResult;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: ConnectedParams,
    ) -> Result<SelectionSetResult, CommandError> {
        let mut selection = cx.view::<Selection>()?.clone();
        let project = cx.project()?;
        let pcb = project
            .pcb()
            .ok_or_else(|| CommandError::unavailable("the Project has no PCB Document"))?;
        let copper = Copper::of(project).expect("the Project has a PCB Document");
        let seeds = match &p.seeds {
            Some(seeds) => {
                if let Some(bad) = seeds.iter().find(|s| {
                    pcb.track(**s).is_none()
                        && pcb.via(**s).is_none()
                        && pcb.free_pad(**s).is_none()
                        && pcb.footprint(**s).is_none()
                }) {
                    return Err(CommandError::invalid_params(
                        "/seeds",
                        format!("{bad} is not a Track, Via, Free Pad or Footprint"),
                    ));
                }
                seed_items(pcb, seeds, &[])
            }
            None => seed_items(pcb, &selection.objects, &selection.pads),
        };
        add_reached(pcb, &copper, &seeds, p.scope, &mut selection);
        let count = selection.len();
        *cx.view::<Selection>()? = selection;
        Ok(SelectionSetResult { count })
    }
}

/// Where the `Tab` cycle is: the selection it started from, the step it is on, and the
/// selection that step made (another selection starts a new cycle).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CopperCycle {
    start: Selection,
    step: usize,
    made: Selection,
}

/// The steps of the cycle after the starting selection.
const CYCLE: [Scope; 3] = [Scope::Layer, Scope::Copper, Scope::Net];

struct Widen;

/// What `Tab` did.
#[derive(Debug, Serialize, JsonSchema)]
pub struct WidenResult {
    /// The step now shown: `start`, `layer`, `copper` or `net`.
    pub step: String,
    /// How many objects are selected.
    pub count: usize,
}

impl Command for Widen {
    const META: Meta = Meta {
        id: "pcb.selection.widen",
        title: "Widen Copper Selection",
        description: "Widens a copper selection a step at a time (Tab): the contiguous \
                      Track on its Layer, the connected copper, the whole Net, then back \
                      to the selection it started from. Only in Select with nothing in \
                      flight.",
        category: "selection",
        icon: "select_connected",
        kind: Kind::View,
    };
    type Params = NoParams;
    type Output = WidenResult;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, _: NoParams) -> Result<WidenResult, CommandError> {
        if cx.view::<ActiveTool>()?.0 != Tool::Select {
            return Err(CommandError::rejected(
                "only in Select, with no Tool running",
            ));
        }
        let selection = cx.view::<Selection>()?.clone();
        let mut cycle = cx.view::<CopperCycle>()?.clone();
        if cycle.made != selection || cycle.made == Selection::default() {
            cycle = CopperCycle {
                start: selection.clone(),
                step: 0,
                made: selection,
            };
        }
        let project = cx.project()?;
        let pcb = project
            .pcb()
            .ok_or_else(|| CommandError::unavailable("the Project has no PCB Document"))?;
        let copper = Copper::of(project).expect("the Project has a PCB Document");
        let seeds = seed_items(pcb, &cycle.start.objects, &cycle.start.pads);
        if seeds.is_empty() {
            return Err(CommandError::rejected("select copper first"));
        }
        cycle.step = (cycle.step + 1) % (CYCLE.len() + 1);
        let mut made = cycle.start.clone();
        let step = match cycle.step {
            0 => "start".to_owned(),
            n => {
                let scope = CYCLE[n - 1];
                add_reached(pcb, &copper, &seeds, scope, &mut made);
                serde_json::to_value(scope)
                    .ok()
                    .and_then(|v| v.as_str().map(str::to_owned))
                    .unwrap_or_default()
            }
        };
        cycle.made = made.clone();
        let count = made.len();
        *cx.view::<Selection>()? = made;
        *cx.view::<CopperCycle>()? = cycle;
        Ok(WidenResult { step, count })
    }
}
