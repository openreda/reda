//! The routing Tool's keys, as view Commands. What a route has in
//! flight lives in the window, so the Commands that act on it (`tool.cycle_corner`,
//! `pcb.route.add_via`, `pcb.route.finish`, `pcb.route.width`, `pcb.route.mode`,
//! `pcb.route.end`, `pcb.route.chooser`, `pcb.route.via_size`, `pcb.route.via_end`) only
//! check that the routing Tool is active and say what was asked; the window's Shell
//! applies it to the [`Router`](super::route::Router) after they run, as it does for
//! `tool.rotate`. A mode kept in a Preference (`pcb.route.obstacle_mode` and the like)
//! is then set with `app.preferences.set`, so it is journalled and remembered.
//! Backspace's `tool.undo_segment` is shared with the drawing Tools (`crate::tools`).

use reda_engine::command::{
    AvailabilityContext, Command, CommandError, Context, Kind, Length, Meta, NoParams, Preset,
    Registry, Unavailable,
};
use reda_engine::model::geom::Nm;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::tools::{ActiveTool, Tool};

/// Adds the routing Commands to `registry`. The Shell does this for its engine.
pub fn register_all(registry: &mut Registry) {
    registry.add(AddVia);
    registry.add(Finish);
    registry.add(Width);
    registry.add(Mode);
    registry.add(End);
    registry.add(ViaEnd);
    registry.add(ChooserOpen);
    registry.add(ViaSizeSet);
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

route_key!(
    End,
    "pcb.route.end",
    "End Route",
    "Esc or right-click: ends the route and keeps what is placed. With only its start \
     point, drops it; with no route in flight, returns to Select.",
    "track"
);
route_key!(
    ViaEnd,
    "pcb.route.via_end",
    "Via and End Route",
    "Places the route to the cursor, drops a Via there and ends the route, for dropping \
     into a pour.",
    "via"
);

/// A mode of the Track Tool that a key or a Routing Bar button changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RouteMode {
    /// `1`: a click places only the first of the two segments.
    Lookahead,
    /// Space: which leg comes first.
    Posture,
    /// `3`: the next width mode, `pcb.route.width_mode`.
    WidthMode,
    /// Shift+R: the next obstacle mode, `pcb.route.obstacle_mode`.
    ObstacleMode,
    /// Shift+A: `pcb.route.auto_terminate`.
    AutoTerminate,
    /// Shift+L: `pcb.route.loop_removal`.
    LoopRemoval,
    /// C: the next clearance display, `pcb.route.clearance_display`.
    ClearanceDisplay,
}

impl RouteMode {
    pub const ALL: [RouteMode; 7] = [
        RouteMode::Lookahead,
        RouteMode::Posture,
        RouteMode::WidthMode,
        RouteMode::ObstacleMode,
        RouteMode::AutoTerminate,
        RouteMode::LoopRemoval,
        RouteMode::ClearanceDisplay,
    ];

    /// Its name in params and Presets.
    pub fn name(self) -> &'static str {
        match self {
            RouteMode::Lookahead => "lookahead",
            RouteMode::Posture => "posture",
            RouteMode::WidthMode => "width_mode",
            RouteMode::ObstacleMode => "obstacle_mode",
            RouteMode::AutoTerminate => "auto_terminate",
            RouteMode::LoopRemoval => "loop_removal",
            RouteMode::ClearanceDisplay => "clearance_display",
        }
    }

    fn preset(self) -> (&'static str, &'static str) {
        match self {
            RouteMode::Lookahead => ("Toggle Look-Ahead", "route_lookahead"),
            RouteMode::Posture => ("Flip Posture", "route_corner_45"),
            RouteMode::WidthMode => ("Next Width Mode", "rule"),
            RouteMode::ObstacleMode => ("Next Obstacle Mode", "route_obstacle_stop"),
            RouteMode::AutoTerminate => ("Toggle Auto-Terminate", "route_auto_terminate"),
            RouteMode::LoopRemoval => ("Toggle Loop Removal", "route_loop_removal"),
            RouteMode::ClearanceDisplay => ("Next Clearance Display", "route_clearance_near"),
        }
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ModeParams {
    pub mode: RouteMode,
}

/// What was asked for.
#[derive(Serialize, JsonSchema)]
pub struct ModeAsked {
    pub mode: RouteMode,
}

struct Mode;

impl Command for Mode {
    const META: Meta = Meta {
        id: "pcb.route.mode",
        title: "Route Mode",
        description: "Toggles or steps one of the Track Tool's modes: look-ahead, posture, \
                      width mode, obstacle mode, auto-terminate, loop removal or the \
                      clearance display. Modes kept in Preferences are set there.",
        category: "tool",
        icon: "track",
        kind: Kind::View,
    };
    type Params = ModeParams;
    type Output = ModeAsked;

    fn presets(&self) -> Vec<Preset> {
        RouteMode::ALL
            .iter()
            .map(|m| {
                let (title, icon) = m.preset();
                Preset::new(m.name(), title, icon, json!({ "mode": m.name() }))
            })
            .collect()
    }

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: ModeParams) -> Result<ModeAsked, CommandError> {
        routing(cx)?;
        Ok(ModeAsked { mode: p.mode })
    }
}

/// Which chooser to open.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ChooserKind {
    /// Shift+W: the Track widths.
    Width,
    /// Shift+V: the Via sizes.
    Via,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ChooserParams {
    pub kind: ChooserKind,
}

#[derive(Serialize, JsonSchema)]
pub struct ChooserAsked {
    pub kind: ChooserKind,
}

struct ChooserOpen;

impl Command for ChooserOpen {
    const META: Meta = Meta {
        id: "pcb.route.chooser",
        title: "Route Chooser",
        description: "Opens the width chooser (the Net's widths, the favourites and the \
                      widths on the board) or the Via size chooser (the Net's size and the \
                      sizes on the board) for the route in flight.",
        category: "tool",
        icon: "rule",
        kind: Kind::View,
    };
    type Params = ChooserParams;
    type Output = ChooserAsked;

    fn presets(&self) -> Vec<Preset> {
        vec![
            Preset::new("width", "Choose Width", "rule", json!({"kind": "width"})),
            Preset::new("via", "Choose Via Size", "via", json!({"kind": "via"})),
        ]
    }

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: ChooserParams) -> Result<ChooserAsked, CommandError> {
        routing(cx)?;
        Ok(ChooserAsked { kind: p.kind })
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ViaSizeParams {
    /// The copper diameter of the route's next Vias.
    pub diameter: Length,
    /// Their hole.
    pub hole: Length,
}

#[derive(Serialize, JsonSchema)]
pub struct ViaSizeAsked {
    pub diameter: Nm,
    pub hole: Nm,
}

struct ViaSizeSet;

impl Command for ViaSizeSet {
    const META: Meta = Meta {
        id: "pcb.route.via_size",
        title: "Route Via Size",
        description: "Sets the size of the Vias the route in flight places from here on, \
                      clipped to the Net's Via size range.",
        category: "tool",
        icon: "via",
        kind: Kind::View,
    };
    type Params = ViaSizeParams;
    type Output = ViaSizeAsked;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: ViaSizeParams) -> Result<ViaSizeAsked, CommandError> {
        routing(cx)?;
        if p.hole.nm() <= Nm(0) || p.diameter.nm() <= p.hole.nm() {
            return Err(CommandError::invalid_params(
                "/diameter",
                "the diameter must be larger than a positive hole",
            ));
        }
        Ok(ViaSizeAsked {
            diameter: p.diameter.nm(),
            hole: p.hole.nm(),
        })
    }
}

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
