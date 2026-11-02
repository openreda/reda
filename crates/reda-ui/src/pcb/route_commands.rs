//! The routing Tool's keys and select connected, as view Commands. What a route has in
//! flight lives in the window, so the Commands that act on it (`tool.cycle_corner`,
//! `pcb.route.add_via`, `pcb.route.finish`, `pcb.route.width`) only check that the
//! routing Tool is active and say what was asked; the window's Shell applies it to the
//! [`Router`](super::route::Router) after they run, as it does for `tool.rotate`.
//! Backspace's `tool.undo_segment` is shared with the drawing Tools (`crate::tools`).

use reda_engine::command::{
    AvailabilityContext, Command, CommandError, Context, Kind, Length, Meta, NoParams, Preset,
    Registry, Unavailable,
};
use reda_engine::model::Uuid;
use reda_engine::model::geom::Nm;
use reda_engine::pcb::{Copper, Item};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::selection::{PadRef, Selection};
use crate::tools::{ActiveTool, Tool};

/// Adds the routing Commands to `registry`. The Shell does this for its engine.
pub fn register_all(registry: &mut Registry) {
    registry.add(CycleCorner);
    registry.add(AddVia);
    registry.add(Finish);
    registry.add(Width);
    registry.add(SelectConnected);
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

/// Refuses unless the routing Tool is active.
fn routing(cx: &mut Context<'_>) -> Result<(), CommandError> {
    if cx.view::<ActiveTool>()?.0 != Tool::Route {
        return Err(CommandError::rejected(
            "only while routing: activate the routing Tool (T) first",
        ));
    }
    Ok(())
}

/// What the routing keys return.
#[derive(Serialize, JsonSchema)]
pub struct Done {}

macro_rules! route_key {
    ($name:ident, $id:literal, $title:literal, $description:literal, $icon:literal) => {
        struct $name;

        impl Command for $name {
            const META: Meta = Meta {
                id: $id,
                title: $title,
                description: $description,
                category: "tool",
                icon: $icon,
                kind: Kind::View,
            };
            type Params = NoParams;
            type Output = Done;

            fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
                needs_pcb(cx)
            }

            fn run(&self, cx: &mut Context<'_>, _: NoParams) -> Result<Done, CommandError> {
                routing(cx)?;
                Ok(Done {})
            }
        }
    };
}

route_key!(
    CycleCorner,
    "tool.cycle_corner",
    "Cycle Corner Mode",
    "Switches the route's corners between straight-then-45°, 45°-then-straight, \
     horizontal-then-vertical and vertical-then-horizontal.",
    "line"
);
route_key!(
    AddVia,
    "pcb.route.add_via",
    "Add Via and Switch Layer",
    "Places the route to the cursor, drops a Via there and carries on on the other side \
     of the board.",
    "via"
);
route_key!(
    Finish,
    "pcb.route.finish",
    "Finish Route",
    "Commits the route placed so far as Tracks (and its Vias), one undo step.",
    "track"
);

/// A step through the board's Track widths.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WidthStep {
    Next,
    Previous,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WidthParams {
    /// The width to route at from here on.
    #[serde(default)]
    width: Option<Length>,
    /// Or a step to the board's next or previous Track width (its rule and Net Classes').
    #[serde(default)]
    step: Option<WidthStep>,
}

/// What was asked for.
#[derive(Serialize, JsonSchema)]
pub struct WidthAsked {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<Nm>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step: Option<WidthStep>,
}

struct Width;

impl Command for Width {
    const META: Meta = Meta {
        id: "pcb.route.width",
        title: "Route Width",
        description: "Changes the width the route in flight continues at: a given width, or \
                      the board's next or previous Track width.",
        category: "tool",
        icon: "rule",
        kind: Kind::View,
    };
    type Params = WidthParams;
    type Output = WidthAsked;

    fn presets(&self) -> Vec<Preset> {
        vec![
            Preset::new("next", "Next Track Width", "rule", json!({"step": "next"})),
            Preset::new(
                "previous",
                "Previous Track Width",
                "rule",
                json!({"step": "previous"}),
            ),
        ]
    }

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: WidthParams) -> Result<WidthAsked, CommandError> {
        routing(cx)?;
        let width = match p.width {
            Some(w) if w.nm() <= Nm(0) => {
                return Err(CommandError::invalid_params(
                    "/width",
                    "the width must be positive",
                ));
            }
            w => w.map(|w| w.nm()),
        };
        if width.is_some() == p.step.is_some() {
            return Err(CommandError::invalid_params(
                "",
                "give either a width or a step",
            ));
        }
        Ok(WidthAsked {
            width,
            step: p.step,
        })
    }
}

/// How many things are now selected.
#[derive(Serialize, JsonSchema)]
pub struct Selected {
    pub count: usize,
}

/// `pcb.selection.connected`: adds to the selection all copper physically connected to
/// the selected copper.
struct SelectConnected;

impl Command for SelectConnected {
    const META: Meta = Meta {
        id: "pcb.selection.connected",
        title: "Select Connected",
        description: "Adds to the selection every piece of copper physically connected to the \
                      selected Tracks, Vias, Pads and Footprints, whatever its Net.",
        category: "selection",
        icon: "select_connected",
        kind: Kind::View,
    };
    type Params = NoParams;
    type Output = Selected;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, _: NoParams) -> Result<Selected, CommandError> {
        let mut selection = cx.view::<Selection>()?.clone();
        let project = cx.project()?;
        let pcb = project
            .pcb()
            .ok_or_else(|| CommandError::unavailable("the Project has no PCB Document"))?;
        let copper = Copper::of(project).expect("the Project has a PCB Document");
        let mut starts: Vec<Item> = Vec::new();
        for o in &selection.objects {
            if pcb.track(*o).is_some() {
                starts.push(Item::Track { uuid: *o });
            } else if pcb.via(*o).is_some() {
                starts.push(Item::Via { uuid: *o });
            } else if pcb.free_pad(*o).is_some() {
                starts.push(Item::FreePad { uuid: *o });
            } else if let Some(f) = pcb.footprint(*o) {
                starts.extend(f.pads.iter().map(|p| Item::Pad {
                    footprint: *o,
                    pad: p.pad,
                }));
            }
        }
        starts.extend(selection.pads.iter().map(|p| Item::Pad {
            footprint: p.footprint,
            pad: p.pad,
        }));
        for item in starts.iter().flat_map(|s| copper.connected(*s)) {
            match item {
                Item::Pad { footprint, pad } => {
                    let pad = PadRef { footprint, pad };
                    if !selection.pads.contains(&pad) && !selection.objects.contains(&footprint) {
                        selection.pads.push(pad);
                    }
                }
                other => {
                    let o: Uuid = other.object();
                    if !selection.objects.contains(&o) {
                        selection.objects.push(o);
                    }
                }
            }
        }
        let count = selection.objects.len() + selection.pads.len() + selection.nets.len();
        *cx.view::<Selection>()? = selection;
        Ok(Selected { count })
    }
}
