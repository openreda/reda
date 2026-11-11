//! The HUD (docs/spec/v0.1/editing.md#hud): a small readout by the cursor on every canvas,
//! drawn over the canvas like the Active Bar, on by default. Its items each have a
//! checkbox in Preferences (`editing.hud.*`): X/Y, ΔX/ΔY from the last click, the Layer
//! and the grid. Shift+H toggles it (`view.hud.toggle`), Shift+D the delta line
//! (`view.hud.delta`), and Insert resets the delta as a click does
//! (`view.hud.reset_delta`); in the Wire Tool, Insert stays add-vertex. Following the
//! cursor or parked at the canvas's bottom left is a Preference only.

use std::collections::BTreeMap;

use egui::{Align2, Id, Order, Pos2, Rect, Role, Ui, WidgetInfo, vec2};
use reda_engine::command::{Command, CommandError, Context, Kind, Meta, NoParams, Registry};
use reda_engine::model::Uuid;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::shell::Shell;
use crate::status_bar::{cursor, length, units};
use crate::tr;

/// The HUD's session state, in the engine's view state.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HudState {
    /// Shift+H flipped the `editing.hud.show` Preference for the session.
    pub toggled: bool,
    /// Shift+D flipped the `editing.hud.delta` Preference for the session.
    pub delta_toggled: bool,
    /// Where ΔX/ΔY count from, per Document: the last click, in the readout's coordinates
    /// (a board's from its Output Origin).
    pub anchors: BTreeMap<Uuid, (f64, f64)>,
    /// Documents whose delta `view.hud.reset_delta` asked to reset; the Shell resets them
    /// at the cursor, which only it knows.
    pub resets: Vec<Uuid>,
}

/// Adds the HUD's Commands to `registry`. The Shell does this.
pub fn register_all(registry: &mut Registry) {
    registry.add(HudToggle);
    registry.add(HudDelta);
    registry.add(HudResetDelta);
}

/// Whether the HUD shows now.
pub fn shown(shell: &Shell) -> bool {
    let prefs = &shell.preferences().editing.hud;
    prefs.show != state(shell).toggled
}

fn state(shell: &Shell) -> HudState {
    shell
        .engine()
        .view()
        .get::<HudState>()
        .cloned()
        .unwrap_or_default()
}

/// The HUD's lines on `document`, in item order; empty when it is off or the cursor has
/// not been over the canvas.
pub fn lines(shell: &Shell, document: Uuid) -> Vec<String> {
    if !shown(shell) {
        return Vec::new();
    }
    let Some((x, y)) = cursor(shell, document) else {
        return Vec::new();
    };
    let prefs = &shell.preferences().editing.hud;
    let hud = state(shell);
    let units = units(shell, document);
    let mut out = Vec::new();
    if prefs.xy {
        out.push(tr!(
            "status-cursor",
            x = length(shell, x, units),
            y = length(shell, y, units)
        ));
    }
    if prefs.delta != hud.delta_toggled {
        let (ax, ay) = hud.anchors.get(&document).copied().unwrap_or((0.0, 0.0));
        out.push(tr!(
            "hud-delta",
            x = length(shell, x - ax, units),
            y = length(shell, y - ay, units)
        ));
    }
    if prefs.layer
        && shell.is_pcb(document)
        && let Some(layer) = crate::pcb::layer_control::active_layer_name(shell)
    {
        out.push(tr!("hud-layer", layer = layer));
    }
    if prefs.grid {
        out.push(tr!(
            "status-grid",
            grid = crate::grid::label(shell.grid(document))
        ));
    }
    out
}

/// Puts `document`'s delta origin at the cursor, as a click does.
pub fn reset(shell: &mut Shell, document: Uuid) {
    let Some(at) = cursor(shell, document) else {
        return;
    };
    if let Some(hud) = shell.engine_mut().view_mut().get_mut::<HudState>() {
        hud.anchors.insert(document, at);
    }
}

/// Resets the deltas `view.hud.reset_delta` asked for. The Shell calls this after it.
pub fn apply_resets(shell: &mut Shell) {
    let resets = match shell.engine_mut().view_mut().get_mut::<HudState>() {
        Some(hud) => std::mem::take(&mut hud.resets),
        None => return,
    };
    for document in resets {
        reset(shell, document);
    }
}

/// How far the HUD sits from the pointer, and from the canvas corner when parked.
const OFFSET: f32 = 18.0;

/// Draws the HUD over the canvas `rect` of `document` (following the pointer while it is
/// over the canvas, or parked at the bottom left), and resets the delta on a click there.
pub fn show(ui: &Ui, shell: &mut Shell, document: Uuid, rect: Rect, active: bool) {
    let clicked = ui.input(|i| {
        i.pointer
            .primary_clicked()
            .then(|| i.pointer.interact_pos())
            .flatten()
    });
    if clicked.is_some_and(|p| rect.contains(p) && ui.ctx().layer_id_at(p) == Some(ui.layer_id())) {
        reset(shell, document);
    }
    let lines = lines(shell, document);
    if lines.is_empty() {
        return;
    }
    // Over a bar floating on the canvas the HUD would sit where that bar's tooltips open.
    let hover = ui
        .input(|i| i.pointer.hover_pos())
        .filter(|p| rect.contains(*p) && ui.ctx().layer_id_at(*p) == Some(ui.layer_id()));
    let follow = shell.preferences().editing.hud.follow_cursor;
    let (at, pivot): (Pos2, Align2) = match (follow, hover) {
        (true, Some(p)) => (p + vec2(OFFSET, OFFSET), Align2::LEFT_TOP),
        (true, None) => return,
        (false, _) if !active => return,
        _ => (
            rect.left_bottom()
                + vec2(
                    8.0,
                    -crate::bars::draw::bottom_inset(crate::pcb::layer_control::strip_shown(shell))
                        - 2.0 * OFFSET,
                ),
            Align2::LEFT_BOTTOM,
        ),
    };
    let id = Id::NULL.with(("hud", document));
    egui::Area::new(id)
        .order(Order::Middle)
        .fixed_pos(at)
        .pivot(pivot)
        .interactable(false)
        // A fade never ends under a test window's fixed input time.
        .fade_in(false)
        .constrain_to(rect)
        .show(ui.ctx(), |ui| {
            let frame = egui::Frame::window(ui.style())
                .inner_margin(egui::Margin::symmetric(6, 3))
                .corner_radius(egui::CornerRadius::same(4))
                .multiply_with_opacity(0.85);
            let response = frame
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    for line in &lines {
                        ui.label(egui::RichText::new(line).monospace().small());
                    }
                })
                .response;
            let name = tr!("hud-name");
            response.widget_info(|| WidgetInfo::labeled(Role::Group, true, name.clone()));
        });
}

/// The Document a HUD Command acts on.
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

struct HudToggle;

impl Command for HudToggle {
    const META: Meta = Meta {
        id: "view.hud.toggle",
        title: "Toggle HUD",
        description: "Shows or hides the HUD, the readout by the cursor on every canvas, \
                      for the session. The `editing.hud.show` Preference sets the default.",
        category: "view",
        icon: "grid",
        kind: Kind::View,
    };
    type Params = NoParams;
    type Output = Toggled;

    fn run(&self, cx: &mut Context<'_>, _: NoParams) -> Result<Toggled, CommandError> {
        let hud = cx.view::<HudState>()?;
        hud.toggled = !hud.toggled;
        Ok(Toggled {
            toggled: hud.toggled,
        })
    }
}

struct HudDelta;

impl Command for HudDelta {
    const META: Meta = Meta {
        id: "view.hud.delta",
        title: "Toggle HUD Delta",
        description: "Shows or hides the HUD's ΔX/ΔY line for the session. The \
                      `editing.hud.delta` Preference sets the default.",
        category: "view",
        icon: "grid",
        kind: Kind::View,
    };
    type Params = NoParams;
    type Output = Toggled;

    fn run(&self, cx: &mut Context<'_>, _: NoParams) -> Result<Toggled, CommandError> {
        let hud = cx.view::<HudState>()?;
        hud.delta_toggled = !hud.delta_toggled;
        Ok(Toggled {
            toggled: hud.delta_toggled,
        })
    }
}

struct HudResetDelta;

impl Command for HudResetDelta {
    const META: Meta = Meta {
        id: "view.hud.reset_delta",
        title: "Reset HUD Delta",
        description: "Makes the cursor's position the origin the HUD's ΔX/ΔY count from, \
                      as a click on the canvas does.",
        category: "view",
        icon: "grid",
        kind: Kind::View,
    };
    type Params = DocumentParams;
    type Output = Done;

    fn run(&self, cx: &mut Context<'_>, p: DocumentParams) -> Result<Done, CommandError> {
        cx.view::<HudState>()?.resets.push(p.document);
        Ok(Done {})
    }
}
