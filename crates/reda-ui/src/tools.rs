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
        }
    }
}

/// The active Tool, in the engine's view state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ActiveTool(pub Tool);

/// Adds `tool.activate`, `tool.rotate` and `tool.flip` to `registry`. The Shell does this
/// for its engine.
pub fn register_all(registry: &mut Registry) {
    registry.add(ToolActivate);
    registry.add(ToolRotate);
    registry.add(ToolFlip);
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
                      draw copper on the board; Select (also what Escape returns to) \
                      cancels a Tool in flight.",
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
        ]
    }

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: ToolActivateParams,
    ) -> Result<ToolActivateResult, CommandError> {
        let carries = matches!(p.tool, Tool::Move | Tool::Drag);
        if carries && cx.view::<Selection>()?.objects.is_empty() {
            return Err(CommandError::rejected(
                "nothing is selected: select what to move first",
            ));
        }
        cx.view::<ActiveTool>()?.0 = p.tool;
        Ok(ToolActivateResult { tool: p.tool })
    }
}

/// What `tool.rotate` and `tool.flip` return.
#[derive(Serialize, JsonSchema)]
pub struct Turned {}

/// `tool.rotate` (Space): turns what the active Tool carries by 90°, or else the
/// selection. What a Tool carries lives in the window, so the window's Shell applies it
/// after the Command runs (the PCB Tool: `Shell::pcb_rotate`); the Command records the
/// request, so keys, menus and the Palette call it like any other.
struct ToolRotate;

impl Command for ToolRotate {
    const META: Meta = Meta {
        id: "tool.rotate",
        title: "Rotate",
        description: "Turns what the active Tool carries by 90°, or else the selection, on \
                      the active board.",
        category: "tool",
        icon: "rotate",
        kind: Kind::View,
    };
    type Params = NoParams;
    type Output = Turned;

    fn run(&self, _: &mut Context<'_>, _: NoParams) -> Result<Turned, CommandError> {
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

    fn run(&self, _: &mut Context<'_>, _: NoParams) -> Result<Turned, CommandError> {
        Ok(Turned {})
    }
}
