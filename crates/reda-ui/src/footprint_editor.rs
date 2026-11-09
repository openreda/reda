//! The Footprint editor: an open Footprint's tab (see [`element_editor`]) with the shared
//! canvas Tools. Its keys are Keymap bindings in the `footprint` context
//! (`keymap/reda.toml`).
//!
//! - Select picks Pads, graphics and the Designator and value text (click, Shift+click,
//!   box), and dragging moves them on the grid: one `library.footprint.move` on release.
//! - Pad places a Pad per click from the Pad Tool's settings ([`PadTemplate`]: SMD,
//!   plated or non-plated; circle, rectangle, rounded rectangle or oval; a round hole or a
//!   slot; mask and paste margins), numbered one past the highest number.
//! - Line, Rect and Circle draw on the drawing Layer ([`ToolSettings::layer`]: Silkscreen,
//!   Courtyard or Assembly, top or bottom) by dragging.
//! - Origin moves the origin to a click, then hands back to Select.
//!
//! The settings and what is in flight are view state; the Footprint changes only through
//! `library.footprint.*` Commands, which the engine refuses for a read-only (starter
//! Library) Footprint, so the editor does not offer them there.
//!
//! [`element_editor`]: crate::element_editor

pub mod inspector;

use std::collections::BTreeMap;

use reda_engine::command::{Command, CommandError, Context, Kind, Meta, Registry};
use reda_engine::model::geom::{Millideg, Nm, Point};
use reda_engine::model::{
    Footprint, FootprintLayer, LibraryElement, PadKind, PadShape, Ratio, Uuid, auto_increment,
};
use serde_json::{Value, json};

use crate::canvas::interact::{BoxMode, Pointer};
use crate::canvas::{LiveHandle, Overlay, RubberBand};
use crate::element_editor::{self, Invocation};
use crate::shell::Shell;
use crate::tools::Tool;

/// A Document point in nm, y up.
type P = (f64, f64);

/// How close, in points on screen, the pointer must be to pick something.
const PICK_POINTS: f64 = 5.0;
/// Segments a circle in flight is drawn with.
const CIRCLE_SEGMENTS: usize = 48;

/// What the Pad Tool places.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PadTemplate {
    pub kind: PadKind,
    pub shape: PadShape,
    pub width: Nm,
    pub height: Nm,
    /// For a rounded rectangle and the uncut corners of a chamfered one; `None` takes the
    /// Command's default.
    pub corner_ratio: Option<Ratio>,
    /// For a chamfered rectangle; `None` is [`DEFAULT_CHAMFER_RATIO`].
    #[serde(default)]
    pub chamfer_ratio: Option<Ratio>,
    pub rotation: Millideg,
    /// The hole of a through-hole Pad, as (width, height): equal is round, else a slot.
    /// SMD Pads ignore it.
    pub hole: Option<(Nm, Nm)>,
    /// Per-Pad overrides of the board's margins; `None` keeps the board's.
    pub mask_margin: Option<Nm>,
    pub paste_margin: Option<Nm>,
}

impl Default for PadTemplate {
    /// A 1 × 1.5 mm rectangular SMD Pad.
    fn default() -> PadTemplate {
        PadTemplate {
            kind: PadKind::Smd,
            shape: PadShape::Rect,
            width: Nm(1_000_000),
            height: Nm(1_500_000),
            corner_ratio: None,
            chamfer_ratio: None,
            rotation: Millideg(0),
            hole: None,
            mask_margin: None,
            paste_margin: None,
        }
    }
}

/// The chamfer a chamfered Pad gets when its settings name none.
pub const DEFAULT_CHAMFER_RATIO: Ratio = Ratio::percent(50);

/// The hole a through-hole Pad gets when its settings name none: 0.8 mm round.
const DEFAULT_HOLE: Nm = Nm(800_000);

impl PadTemplate {
    /// `library.footprint.pad.add` params for this Pad at `at` with `number`.
    pub fn params(&self, footprint: Uuid, number: &str, at: Point) -> Value {
        let mut p = json!({
            "footprint": footprint,
            "number": number,
            "kind": self.kind,
            "shape": self.shape,
            "at": {"x": at.x.0, "y": at.y.0},
            "rotation": self.rotation.0,
            "width": self.width.0,
            "height": self.height.0,
        });
        let cornered = matches!(self.shape, PadShape::RoundedRect | PadShape::ChamferedRect);
        if cornered && let Some(r) = self.corner_ratio {
            p["corner_ratio"] = json!(r.0);
        }
        if self.shape == PadShape::ChamferedRect {
            p["chamfer_ratio"] = json!(self.chamfer_ratio.unwrap_or(DEFAULT_CHAMFER_RATIO).0);
        }
        if self.kind != PadKind::Smd {
            let (w, h) = self.hole.unwrap_or((DEFAULT_HOLE, DEFAULT_HOLE));
            p["hole"] = json!({"width": w.0, "height": h.0});
        }
        if let Some(m) = self.mask_margin {
            p["mask"] = json!({"top": {"expansion": m.0}});
            if self.kind != PadKind::Smd {
                p["mask"]["bottom"] = json!({"expansion": m.0});
            }
        }
        if self.kind == PadKind::Smd
            && let Some(m) = self.paste_margin
        {
            p["paste"] = json!({"expansion": m.0});
        }
        p
    }
}

/// The Footprint Tools' settings, shared by every Footprint tab.
#[derive(Clone, Debug, PartialEq)]
pub struct ToolSettings {
    pub pad: PadTemplate,
    /// The Layer Line, Rect and Circle draw on.
    pub layer: FootprintLayer,
}

impl Default for ToolSettings {
    fn default() -> ToolSettings {
        ToolSettings {
            pad: PadTemplate::default(),
            layer: FootprintLayer::SilkscreenTop,
        }
    }
}

/// What one Footprint canvas has in flight.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Interaction {
    #[default]
    Idle,
    /// A selection box from `from` to `to`.
    Boxing {
        from: P,
        to: P,
        modifiers: egui::Modifiers,
    },
    /// The selection follows the pointer, `offset` from where it was picked up.
    Moving {
        objects: Vec<Uuid>,
        anchor: Point,
        offset: Point,
    },
    /// A line, rectangle or circle being dragged out, snapped.
    Drawing { tool: Tool, from: Point, to: Point },
}

/// The Footprint editor's view state: the Tools' settings and each canvas's interaction.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FootprintEditorState {
    pub settings: ToolSettings,
    pub canvases: BTreeMap<Uuid, Interaction>,
    /// The number of the last Pad the Pad Tool placed in each Footprint tab, which the
    /// next one auto-increments from.
    pub last_pads: BTreeMap<Uuid, String>,
}

fn state(shell: &Shell) -> FootprintEditorState {
    shell
        .engine()
        .view()
        .get::<FootprintEditorState>()
        .cloned()
        .unwrap_or_default()
}

fn state_mut(shell: &mut Shell) -> &mut FootprintEditorState {
    let view = shell.engine.view_mut();
    if view.get::<FootprintEditorState>().is_none() {
        view.set(FootprintEditorState::default());
    }
    view.get_mut::<FootprintEditorState>().expect("just set")
}

/// The Footprint Tools' settings.
pub fn tool_settings(shell: &Shell) -> ToolSettings {
    state(shell).settings
}

/// Sets what the Pad Tool places (`view.footprint_tool.set`).
pub fn set_pad_template(shell: &mut Shell, pad: PadTemplate) -> Result<Value, CommandError> {
    shell.run("view.footprint_tool.set", json!({ "pad": pad }))
}

/// Sets the Layer Line, Rect and Circle draw on (`view.footprint_tool.set`).
pub fn set_layer(shell: &mut Shell, layer: FootprintLayer) -> Result<Value, CommandError> {
    shell.run("view.footprint_tool.set", json!({ "layer": layer }))
}

/// Adds `view.footprint_tool.set` to `registry`. The Shell does this for its engine.
pub fn register_all(registry: &mut Registry) {
    registry.add(ToolSet);
}

#[derive(serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ToolSetParams {
    /// What the Pad Tool places.
    #[serde(default)]
    pub pad: Option<PadTemplate>,
    /// The Layer Line, Rect and Circle draw on.
    #[serde(default)]
    pub layer: Option<FootprintLayer>,
}

struct ToolSet;

impl Command for ToolSet {
    const META: Meta = Meta {
        id: "view.footprint_tool.set",
        title: "Set Footprint Tool Settings",
        description: "Sets what the Footprint editor's Pad Tool places and the Layer its \
                      Line, Rect and Circle Tools draw on. View state, shared by every \
                      Footprint tab.",
        category: "view",
        icon: "pad",
        kind: Kind::View,
    };
    type Params = ToolSetParams;
    type Output = Value;

    fn run(&self, cx: &mut Context<'_>, p: ToolSetParams) -> Result<Value, CommandError> {
        let settings = &mut cx.view::<FootprintEditorState>()?.settings;
        if let Some(pad) = p.pad {
            settings.pad = pad;
        }
        if let Some(layer) = p.layer {
            settings.layer = layer;
        }
        Ok(json!({ "pad": settings.pad, "layer": settings.layer }))
    }
}

/// Everything selectable on `footprint`: Pads, graphics, then the visible texts.
pub fn selectable(footprint: &Footprint) -> Vec<Uuid> {
    let mut out: Vec<Uuid> = footprint.pads.iter().map(|p| p.uuid).collect();
    out.extend(footprint.graphics.iter().map(|g| g.uuid));
    out.extend(footprint.texts.iter().map(|t| t.uuid));
    out
}

/// The number for the next Pad the Pad Tool places in `footprint`: `last` (the one it
/// placed before, if any) stepped by `primary` (letters too, A → B), skipping numbers
/// taken; with nothing to step from, one past the highest numeric number, 1 on an empty
/// Footprint.
pub fn next_number(footprint: &Footprint, last: Option<&str>, primary: i64) -> String {
    let taken = |n: &str| footprint.pads.iter().any(|p| p.number == n);
    last.and_then(|last| auto_increment::next_number(last, primary, |n| !taken(n)))
        .unwrap_or_else(|| {
            let highest = footprint
                .pads
                .iter()
                .filter_map(|p| p.number.parse::<u64>().ok())
                .max()
                .unwrap_or(0);
            let mut n = highest + 1;
            while taken(&n.to_string()) {
                n += 1;
            }
            n.to_string()
        })
}

/// Runs the active Tool on a frame's pointer `events` over the Footprint `element`'s
/// canvas, at `nm_per_point` nm per screen point. Returns the Commands to run; updates the
/// tab's marks and overlay.
pub fn pointer(
    shell: &mut Shell,
    element: Uuid,
    events: &[Pointer],
    nm_per_point: f64,
) -> Vec<Invocation> {
    let Some(LibraryElement::Footprint(footprint)) = shell.engine().element(element).cloned()
    else {
        return Vec::new();
    };
    let Some(tab) = shell.document_tab(element) else {
        return Vec::new();
    };
    let mut hovered = tab.live.hovered;
    let mut cursor = tab.cursor;
    let state = state(shell)
        .canvases
        .get(&element)
        .cloned()
        .unwrap_or_default();
    let read_only = element_editor::read_only(shell.engine(), element);
    let mut cx = Cx {
        shell,
        element,
        footprint: &footprint,
        pick: PICK_POINTS * nm_per_point,
        read_only,
        commands: Vec::new(),
    };
    let mut state = cx.sync_tool(state);
    for &event in events {
        if let Some(at) = event_at(event) {
            cursor = Some(at);
        }
        state = cx.step(state, event, &mut hovered);
    }
    let commands = cx.commands;
    if !matches!(state, Interaction::Idle) {
        hovered = None;
    }
    show(shell, element, state, hovered, cursor);
    commands
}

/// Drops whatever is in flight on the Footprint canvas `element` (Escape).
pub fn cancel(shell: &mut Shell, element: Uuid) {
    let cursor = shell.document_tab(element).and_then(|t| t.cursor);
    show(shell, element, Interaction::Idle, None, cursor);
}

fn event_at(event: Pointer) -> Option<P> {
    match event {
        Pointer::Hover(at) | Pointer::Drag(at) | Pointer::DragEnd(at) => Some(at),
        Pointer::Click { at, .. } | Pointer::DragStart { at, .. } => Some(at),
    }
}

fn point(p: Point) -> P {
    (p.x.0 as f64, p.y.0 as f64)
}

/// The outline of what `tool` draws from `from` to `to`, as segments.
fn outline(tool: Tool, from: Point, to: Point) -> Vec<(Point, Point)> {
    match tool {
        Tool::Rect => {
            let (a, c) = (from, to);
            let (b, d) = (Point::new(c.x, a.y), Point::new(a.x, c.y));
            vec![(a, b), (b, c), (c, d), (d, a)]
        }
        Tool::Circle => {
            let r = radius(from, to) as f64;
            let at = |i: usize| {
                let t = i as f64 / CIRCLE_SEGMENTS as f64 * std::f64::consts::TAU;
                Point::new(
                    Nm(from.x.0 + (r * t.cos()).round() as i64),
                    Nm(from.y.0 + (r * t.sin()).round() as i64),
                )
            };
            (0..CIRCLE_SEGMENTS).map(|i| (at(i), at(i + 1))).collect()
        }
        _ => vec![(from, to)],
    }
}

fn radius(from: Point, to: Point) -> i64 {
    let (dx, dy) = ((to.x.0 - from.x.0) as f64, (to.y.0 - from.y.0) as f64);
    dx.hypot(dy).round() as i64
}

/// Writes `state` and what it shows to the tab and the view state.
fn show(
    shell: &mut Shell,
    element: Uuid,
    state: Interaction,
    hovered: Option<Uuid>,
    cursor: Option<P>,
) {
    let rules = crate::snap::rules(shell, element);
    let mode = match &state {
        Interaction::Boxing { from, to, .. } => {
            crate::selection::rules::box_mode(shell, element, *from, *to)
        }
        _ => BoxMode::Enclose,
    };
    let Some(tab) = shell.document_tab_mut(element) else {
        return;
    };
    let snapped = cursor.map(|c| rules.to_grid(c));
    let (moving, offset, overlay) = match &state {
        Interaction::Idle => (
            Vec::new(),
            (0.0, 0.0),
            Overlay {
                cursor: snapped,
                ..Overlay::default()
            },
        ),
        Interaction::Boxing { from, to, .. } => (
            Vec::new(),
            (0.0, 0.0),
            Overlay {
                rubber_band: Some(RubberBand {
                    from: *from,
                    to: *to,
                    mode,
                }),
                ..Overlay::default()
            },
        ),
        Interaction::Moving {
            objects,
            anchor,
            offset,
        } => (
            objects.clone(),
            point(*offset),
            Overlay {
                cursor: Some(*anchor + *offset),
                ..Overlay::default()
            },
        ),
        Interaction::Drawing { tool, from, to } => (
            Vec::new(),
            (0.0, 0.0),
            Overlay {
                wires: outline(*tool, *from, *to).into(),
                cursor: Some(*to),
                ..Overlay::default()
            },
        ),
    };
    if !tab.live.shows(hovered, &moving, &[]) {
        tab.live = LiveHandle::new(hovered, &moving, &[]);
    }
    tab.moving_offset = offset;
    tab.overlay = overlay;
    tab.cursor = cursor;
    state_mut(shell).canvases.insert(element, state);
}

struct Cx<'a> {
    shell: &'a mut Shell,
    element: Uuid,
    footprint: &'a Footprint,
    pick: f64,
    read_only: bool,
    commands: Vec<Invocation>,
}

impl Cx<'_> {
    /// `at` snapped onto the Footprint's Hotspots, else its grid.
    fn snap(&self, at: P) -> Point {
        self.snap_moving(at, &[])
    }

    /// `at` snapped as a move of `objects` lands: onto the other objects' Hotspots.
    fn snap_moving(&self, at: P, objects: &[Uuid]) -> Point {
        let targets = crate::snap::hotspots::without(
            crate::snap::hotspots::footprint(self.footprint),
            objects,
        );
        crate::snap::snap_at(self.shell, self.element, at, &targets).at
    }

    fn run(&mut self, id: &str, params: Value) {
        self.commands.push((id.to_owned(), params));
    }

    fn hit(&self, at: P) -> Option<Uuid> {
        crate::selection::picking::hit(self.shell, self.element, at, self.pick)
    }

    /// Drops a drawing whose Tool is no longer active.
    fn sync_tool(&self, state: Interaction) -> Interaction {
        match state {
            Interaction::Drawing { tool, .. } if tool != self.shell.tool() => Interaction::Idle,
            Interaction::Moving { .. } | Interaction::Boxing { .. }
                if self.shell.tool() != Tool::Select =>
            {
                Interaction::Idle
            }
            other => other,
        }
    }

    fn step(
        &mut self,
        state: Interaction,
        event: Pointer,
        hovered: &mut Option<Uuid>,
    ) -> Interaction {
        match self.shell.tool() {
            Tool::Pad | Tool::Origin => {
                if let Pointer::Click { at, .. } = event {
                    self.click_tool(at);
                }
                Interaction::Idle
            }
            tool @ (Tool::Line | Tool::Rect | Tool::Circle) => self.draw(tool, state, event),
            _ => self.select_step(state, event, hovered),
        }
    }

    fn click_tool(&mut self, at: P) {
        if self.read_only {
            return;
        }
        let at = self.snap(at);
        match self.shell.tool() {
            Tool::Pad => {
                let primary = self.shell.preferences().schematic.auto_increment.primary;
                let last = state(self.shell).last_pads.get(&self.element).cloned();
                let number = next_number(self.footprint, last.as_deref(), primary);
                state_mut(self.shell)
                    .last_pads
                    .insert(self.element, number.clone());
                let params = tool_settings(self.shell)
                    .pad
                    .params(self.element, &number, at);
                self.run("library.footprint.pad.add", params);
            }
            Tool::Origin => {
                self.run(
                    "library.footprint.origin.set",
                    json!({"footprint": self.element, "at": {"x": at.x.0, "y": at.y.0}}),
                );
                self.run("tool.activate:select", json!({}));
            }
            _ => {}
        }
    }

    fn draw(&mut self, tool: Tool, state: Interaction, event: Pointer) -> Interaction {
        if self.read_only {
            return Interaction::Idle;
        }
        match (state, event) {
            (_, Pointer::DragStart { from, at, .. }) => Interaction::Drawing {
                tool,
                from: self.snap(from),
                to: self.snap(at),
            },
            (Interaction::Drawing { tool, from, .. }, Pointer::Drag(at)) => Interaction::Drawing {
                tool,
                from,
                to: self.snap(at),
            },
            (Interaction::Drawing { tool, from, .. }, Pointer::DragEnd(at)) => {
                let to = self.snap(at);
                self.add_graphic(tool, from, to);
                Interaction::Idle
            }
            (state, _) => state,
        }
    }

    fn add_graphic(&mut self, tool: Tool, from: Point, to: Point) {
        let p = |p: Point| json!({"x": p.x.0, "y": p.y.0});
        let shape = match tool {
            Tool::Rect if from.x != to.x && from.y != to.y => {
                json!({"kind": "rect", "start": p(from), "end": p(to)})
            }
            Tool::Circle if from != to => {
                json!({"kind": "circle", "center": p(from), "radius": radius(from, to)})
            }
            Tool::Line if from != to => json!({"kind": "line", "start": p(from), "end": p(to)}),
            _ => return,
        };
        let layer = tool_settings(self.shell).layer;
        self.run(
            "library.footprint.graphic.add",
            json!({"footprint": self.element, "layer": layer, "shape": shape}),
        );
    }

    fn select_step(
        &mut self,
        state: Interaction,
        event: Pointer,
        hovered: &mut Option<Uuid>,
    ) -> Interaction {
        match (state, event) {
            (
                Interaction::Moving {
                    objects, anchor, ..
                },
                Pointer::Drag(at),
            ) => Interaction::Moving {
                offset: self.snap_moving(at, &objects) - anchor,
                objects,
                anchor,
            },
            (
                Interaction::Moving {
                    objects, anchor, ..
                },
                Pointer::DragEnd(at),
            ) => {
                let offset = self.snap_moving(at, &objects) - anchor;
                if offset != Point::default() && !self.read_only {
                    self.run(
                        "library.footprint.move",
                        json!({"footprint": self.element, "objects": objects,
                               "by": {"x": offset.x.0, "y": offset.y.0}}),
                    );
                }
                Interaction::Idle
            }
            (
                Interaction::Boxing {
                    from, modifiers, ..
                },
                Pointer::Drag(to),
            ) => Interaction::Boxing {
                from,
                to,
                modifiers,
            },
            (
                Interaction::Boxing {
                    from, modifiers, ..
                },
                Pointer::DragEnd(to),
            ) => {
                self.box_select(from, to, modifiers);
                Interaction::Idle
            }
            (Interaction::Idle, Pointer::Hover(at)) => {
                crate::selection::picking::hover(self.shell, self.element, at, self.pick);
                *hovered = self.hit(at);
                Interaction::Idle
            }
            (Interaction::Idle, Pointer::Click { at, modifiers }) => {
                if let Some(command) = crate::selection::picking::click(
                    self.shell,
                    self.element,
                    at,
                    self.pick,
                    modifiers,
                ) {
                    self.commands.push(command);
                }
                Interaction::Idle
            }
            (
                Interaction::Idle,
                Pointer::DragStart {
                    from,
                    at,
                    modifiers,
                },
            ) => {
                *hovered = None;
                let Some(hit) = crate::selection::picking::movement_hit(
                    self.shell,
                    self.element,
                    from,
                    self.pick,
                ) else {
                    return Interaction::Boxing {
                        from,
                        to: at,
                        modifiers,
                    };
                };
                let parts = element_editor::parts(self.shell.engine(), self.element);
                let drag = crate::selection::picking::drag_start(
                    self.shell,
                    self.element,
                    hit,
                    modifiers,
                    |id| parts.contains(&id),
                );
                self.commands.extend(drag.select);
                let objects = drag.objects;
                let anchor = self.snap(from);
                Interaction::Moving {
                    offset: self.snap_moving(at, &objects) - anchor,
                    objects,
                    anchor,
                }
            }
            (state, _) => state,
        }
    }

    fn box_select(&mut self, from: P, to: P, modifiers: egui::Modifiers) {
        if let Some(command) =
            crate::selection::picking::box_selection(self.shell, self.element, from, to, modifiers)
        {
            self.commands.push(command);
        }
    }
}
