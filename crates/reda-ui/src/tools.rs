//! The active Tool: what the left mouse button does on a canvas. It is view state, changed
//! only by the view Command `tool.activate` (with a Preset per Tool, which the Active Bars
//! and keys bind to), so scripts and MCP switch Tools the way the window does.
//!
//! Editors read [`Shell::tool`](crate::shell::Shell::tool) and keep what a Tool has in
//! flight themselves; a Tool that is no longer active drops it.

use reda_engine::command::{
    Command, CommandError, Context, Kind, Meta, NoParams, Preset, Registry,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::placement::commands::edit_placing;
use crate::placement::{Placement, Placing};
use crate::selection::Selection;

/// A canvas Tool.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Tool {
    /// Click and box selection; dragging the selection moves it (keeping connections, or
    /// detached with Mod).
    #[default]
    Select,
    /// The selection follows the pointer, detached from its Wires, until a click puts it
    /// down (M).
    Move,
    /// The selection follows the pointer with its Wires rubber-banding, until a click puts
    /// it down (D).
    Drag,
    /// Routes Tracks on the board: clicks place corners, same-Net copper ends the route (T).
    Route,
    /// Places stitching Vias on the board, one per click (V).
    Via,
    /// A Library Part follows the pointer until a click places it (`tool.place`).
    Place,
    /// Draws orthogonal Wire runs: each click fixes a corner, and the run ends on a Pin or
    /// Wire (W).
    Wire,
    /// Places Net Labels, one per click (N).
    NetLabel,
    /// Places Ports, one per click (P).
    Port,
    /// Places Power Ports, one per click (B).
    PowerPort,
    /// Marks Pins as deliberately left open, one per click (Q).
    NoConnect,
}

impl Tool {
    /// The Tool's id, as `tool.activate` takes it.
    pub fn id(self) -> &'static str {
        match self {
            Tool::Select => "select",
            Tool::Move => "move",
            Tool::Drag => "drag",
            Tool::Route => "route",
            Tool::Via => "via",
            Tool::Place => "place",
            Tool::Wire => "wire",
            Tool::NetLabel => "net_label",
            Tool::Port => "port",
            Tool::PowerPort => "power_port",
            Tool::NoConnect => "no_connect",
        }
    }

    /// Whether the Tool carries the selection, so needs one to start.
    pub fn moves_selection(self) -> bool {
        matches!(self, Tool::Move | Tool::Drag)
    }
}

/// The active Tool, in the engine's view state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ActiveTool(pub Tool);

/// Adds `tool.activate` and the in-flight Commands (`tool.rotate`, `tool.mirror`,
/// `tool.flip`, `tool.corner`, `tool.remove_last`) to `registry`. The Shell does this for
/// its engine.
pub fn register_all(registry: &mut Registry) {
    registry.add(ToolActivate);
    registry.add(ToolRotate);
    registry.add(ToolMirror);
    registry.add(ToolFlip);
    registry.add(ToolCorner);
    registry.add(ToolRemoveLast);
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ToolActivateParams {
    /// The Tool to make active.
    tool: Tool,
}

/// The Tool now active.
#[derive(Serialize, JsonSchema)]
pub struct ToolActivateResult {
    tool: Tool,
}

struct ToolActivate;

impl Command for ToolActivate {
    const META: Meta = Meta {
        id: "tool.activate",
        title: "Activate Tool",
        description: "Makes a canvas Tool active. Move and Drag pick up the selection, which \
                      then follows the pointer until a click puts it down; Route and Via \
                      draw copper on the board; Wire draws Wire runs, and Net Label, Port, \
                      Power Port and No-Connect place one object per click; Select (also \
                      what Escape returns to) cancels a Tool in flight.",
        category: "tool",
        icon: "select",
        kind: Kind::View,
    };
    type Params = ToolActivateParams;
    type Output = ToolActivateResult;

    fn presets(&self) -> Vec<Preset> {
        vec![
            Preset::new("select", "Select", "select", json!({"tool": "select"})),
            Preset::new("move", "Move", "move", json!({"tool": "move"})),
            Preset::new("drag", "Drag", "drag", json!({"tool": "drag"})),
            Preset::new("route", "Route Tracks", "track", json!({"tool": "route"})),
            Preset::new("via", "Place Vias", "via", json!({"tool": "via"})),
            Preset::new("wire", "Wire", "wire", json!({"tool": "wire"})),
            Preset::new(
                "net_label",
                "Net Label",
                "net_label",
                json!({"tool": "net_label"}),
            ),
            Preset::new("port", "Port", "port", json!({"tool": "port"})),
            Preset::new(
                "power_port",
                "Power Port",
                "power_port",
                json!({"tool": "power_port"}),
            ),
            Preset::new(
                "no_connect",
                "No-Connect Marker",
                "no_connect",
                json!({"tool": "no_connect"}),
            ),
        ]
    }

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: ToolActivateParams,
    ) -> Result<ToolActivateResult, CommandError> {
        if p.tool == Tool::Place {
            return Err(CommandError::rejected(
                "pick the Part to place in the Library panel (tool.place)",
            ));
        }
        if p.tool.moves_selection() && cx.view::<Selection>()?.objects.is_empty() {
            return Err(CommandError::rejected(
                "nothing is selected: select what to move first",
            ));
        }
        cx.view::<ActiveTool>()?.0 = p.tool;
        // Another Tool ends a placement.
        cx.view::<Placement>()?.placing = None;
        Ok(ToolActivateResult { tool: p.tool })
    }
}

/// What `tool.rotate` and `tool.flip` return.
#[derive(Serialize, JsonSchema)]
pub struct Turned {}

/// `tool.rotate` (Space): turns what the active Tool carries by 90°, or else the
/// selection. What a Tool carries lives in the window, so the window's Shell applies it
/// after the Command runs (the PCB Tool: `Shell::pcb_rotate`; a sheet's:
/// `schematic_editor::turn`); the Command records the request, so keys, menus and the
/// Palette call it like any other.
struct ToolRotate;

impl Command for ToolRotate {
    const META: Meta = Meta {
        id: "tool.rotate",
        title: "Rotate",
        description: "Turns what the active Tool carries by 90°, or else the selection, on \
                      the active board or sheet.",
        category: "tool",
        icon: "rotate",
        kind: Kind::View,
    };
    type Params = NoParams;
    type Output = Turned;

    fn run(&self, cx: &mut Context<'_>, _: NoParams) -> Result<Turned, CommandError> {
        edit_placing(cx, Placing::rotate)?;
        Ok(Turned {})
    }
}

/// `tool.flip` (L): flips what the active Tool carries, or else the selection, to the
/// other side of the board; applied by the window's Shell as [`ToolRotate`] is.
struct ToolFlip;

impl Command for ToolFlip {
    const META: Meta = Meta {
        id: "tool.flip",
        title: "Flip Side",
        description: "Flips what the active Tool carries, or else the selection, to the \
                      other side of the active board.",
        category: "tool",
        icon: "flip",
        kind: Kind::View,
    };
    type Params = NoParams;
    type Output = Turned;

    fn run(&self, cx: &mut Context<'_>, _: NoParams) -> Result<Turned, CommandError> {
        edit_placing(cx, Placing::flip)?;
        Ok(Turned {})
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ToolMirrorParams {
    /// `x` flips left to right, `y` top to bottom.
    pub axis: MirrorAxis,
}

/// What `tool.mirror` returns: the axis, for the Shell to apply.
#[derive(Serialize, JsonSchema)]
pub struct Mirrored {
    pub axis: MirrorAxis,
}

/// Which way `tool.mirror` flips.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MirrorAxis {
    X,
    Y,
}

/// `tool.mirror` (X / Y): flips what the active Tool carries (a Part being placed, a move
/// or drag, a label or Port being placed), or else the selection, on the active sheet. A
/// Part being placed turns here; the rest is applied by the window's Shell as
/// [`ToolRotate`] is.
struct ToolMirror;

impl Command for ToolMirror {
    const META: Meta = Meta {
        id: "tool.mirror",
        title: "Mirror",
        description: "Flips what the active Tool carries (a Part being placed, a move or \
                      drag, a label or Port being placed), or else the selection, left to \
                      right (axis x) or top to bottom (axis y) on the active sheet. A board \
                      placement turns over with Flip Side instead.",
        category: "tool",
        icon: "mirror",
        kind: Kind::View,
    };
    type Params = ToolMirrorParams;
    type Output = Mirrored;

    fn presets(&self) -> Vec<Preset> {
        vec![
            Preset::new("x", "Mirror X", "mirror", json!({"axis": "x"})),
            Preset::new("y", "Mirror Y", "mirror", json!({"axis": "y"})),
        ]
    }

    fn run(&self, cx: &mut Context<'_>, p: ToolMirrorParams) -> Result<Mirrored, CommandError> {
        edit_placing(cx, |placing| placing.mirror(p.axis == MirrorAxis::X))?;
        Ok(Mirrored { axis: p.axis })
    }
}

/// `tool.corner` (/): turns the corner of the Wire run in flight the other way
/// (horizontal first or vertical first); applied by the window's Shell.
struct ToolCorner;

impl Command for ToolCorner {
    const META: Meta = Meta {
        id: "tool.corner",
        title: "Toggle Corner",
        description: "Turns the corner of the Wire run in flight the other way: horizontal \
                      then vertical, or vertical then horizontal.",
        category: "tool",
        icon: "wire",
        kind: Kind::View,
    };
    type Params = NoParams;
    type Output = Turned;

    fn run(&self, _: &mut Context<'_>, _: NoParams) -> Result<Turned, CommandError> {
        Ok(Turned {})
    }
}

/// `tool.remove_last` (Backspace): takes the last segment off the Wire run in flight;
/// applied by the window's Shell.
struct ToolRemoveLast;

impl Command for ToolRemoveLast {
    const META: Meta = Meta {
        id: "tool.remove_last",
        title: "Remove Last Segment",
        description: "Takes the last fixed segment off the Wire run in flight; with none \
                      left, the run ends without drawing anything.",
        category: "tool",
        icon: "wire",
        kind: Kind::View,
    };
    type Params = NoParams;
    type Output = Turned;

    fn run(&self, _: &mut Context<'_>, _: NoParams) -> Result<Turned, CommandError> {
        Ok(Turned {})
    }
}
