//! ΔX/ΔY (docs/spec/v0.1/editing.md#delta): how far the cursor has moved since the last
//! click on a canvas, read in the status bar after X/Y. A click on the canvas or Insert
//! (`view.delta.reset`) puts its origin at the cursor; in the Wire Tool, Insert stays
//! add-vertex. Shift+D (`view.delta.toggle`) hides or shows it for the session, and the
//! `editing.delta` Preference sets the default.

use std::collections::BTreeMap;

use egui::{Rect, Ui};
use reda_engine::command::{Command, CommandError, Context, Kind, Meta, NoParams, Registry};
use reda_engine::model::Uuid;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::shell::Shell;
use crate::status_bar::{cursor, length, units};
use crate::tr;

/// The delta's session state, in the engine's view state.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DeltaState {
    /// Shift+D flipped the `editing.delta` Preference for the session.
    pub toggled: bool,
    /// Where ΔX/ΔY count from, per Document: the last click, in the readout's coordinates
    /// (a board's from its Output Origin).
    pub anchors: BTreeMap<Uuid, (f64, f64)>,
    /// Documents whose delta `view.delta.reset` asked to reset; the Shell resets them at
    /// the cursor, which only it knows.
    pub resets: Vec<Uuid>,
}

/// Adds the delta's Commands to `registry`. The Shell does this.
pub fn register_all(registry: &mut Registry) {
    registry.add(DeltaToggle);
    registry.add(DeltaReset);
}

fn state(shell: &Shell) -> DeltaState {
    shell
        .engine()
        .view()
        .get::<DeltaState>()
        .cloned()
        .unwrap_or_default()
}

/// "ΔX 1.000 mm  ΔY -1.000 mm" on `document`; `None` when it is hidden or the cursor has
/// not been over the canvas.
pub fn text(shell: &Shell, document: Uuid) -> Option<String> {
    let delta = state(shell);
    if shell.preferences().editing.delta == delta.toggled {
        return None;
    }
    let (x, y) = cursor(shell, document)?;
    let (ax, ay) = delta.anchors.get(&document).copied().unwrap_or((0.0, 0.0));
    let units = units(shell, document);
    Some(tr!(
        "status-delta",
        x = length(shell, x - ax, units),
        y = length(shell, y - ay, units)
    ))
}

/// Puts `document`'s delta origin at the cursor, as a click does.
pub fn reset(shell: &mut Shell, document: Uuid) {
    let Some(at) = cursor(shell, document) else {
        return;
    };
    if let Some(delta) = shell.engine_mut().view_mut().get_mut::<DeltaState>() {
        delta.anchors.insert(document, at);
    }
}

/// Resets the deltas `view.delta.reset` asked for. The Shell calls this after it.
pub fn apply_resets(shell: &mut Shell) {
    let resets = match shell.engine_mut().view_mut().get_mut::<DeltaState>() {
        Some(delta) => std::mem::take(&mut delta.resets),
        None => return,
    };
    for document in resets {
        reset(shell, document);
    }
}

/// Resets `document`'s delta on a click on its canvas `rect`, but not on a bar or
/// anything else floating over it.
pub fn track_clicks(ui: &Ui, shell: &mut Shell, document: Uuid, rect: Rect) {
    let clicked = ui.input(|i| {
        i.pointer
            .primary_clicked()
            .then(|| i.pointer.interact_pos())
            .flatten()
    });
    if clicked.is_some_and(|p| rect.contains(p) && ui.ctx().layer_id_at(p) == Some(ui.layer_id())) {
        reset(shell, document);
    }
}

/// The Document a delta Command acts on.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DocumentParams {
    /// The Document whose delta to reset.
    document: Uuid,
}

/// Whether the session now flips the Preference.
#[derive(Serialize, JsonSchema)]
pub struct Toggled {
    toggled: bool,
}

/// Nothing to report.
#[derive(Serialize, JsonSchema)]
pub struct Done {}

struct DeltaToggle;

impl Command for DeltaToggle {
    const META: Meta = Meta {
        id: "view.delta.toggle",
        title: "Toggle Delta",
        description: "Shows or hides the status bar's ΔX/ΔY from the last click for the \
                      session. The `editing.delta` Preference sets the default.",
        category: "view",
        icon: "measure",
        kind: Kind::View,
    };
    type Params = NoParams;
    type Output = Toggled;

    fn run(&self, cx: &mut Context<'_>, _: NoParams) -> Result<Toggled, CommandError> {
        let delta = cx.view::<DeltaState>()?;
        delta.toggled = !delta.toggled;
        Ok(Toggled {
            toggled: delta.toggled,
        })
    }
}

struct DeltaReset;

impl Command for DeltaReset {
    const META: Meta = Meta {
        id: "view.delta.reset",
        title: "Reset Delta",
        description: "Makes the cursor's position the origin the status bar's ΔX/ΔY count \
                      from, as a click on the canvas does.",
        category: "view",
        icon: "measure",
        kind: Kind::View,
    };
    type Params = DocumentParams;
    type Output = Done;

    fn run(&self, cx: &mut Context<'_>, p: DocumentParams) -> Result<Done, CommandError> {
        cx.view::<DeltaState>()?.resets.push(p.document);
        Ok(Done {})
    }
}
