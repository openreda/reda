//! The Footprint editor: an open Footprint's tab (see [`element_editor`]) with the shared
//! canvas Tools. Its keys are Keymap bindings in the `footprint` context
//! (`keymap/reda.toml`).
//!
//! - Select picks Pads, graphics and the Designator and value text (click, Shift+click,
//!   box), and dragging moves them on the grid: one `library.footprint.move` on release.
//! - Pad places a Pad per click from the `defaults.footprint.pad` Object Default (SMD,
//!   plated or non-plated; its shape and size; a round hole or a slot; mask and paste
//!   margins), numbered one past the highest number.
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
mod preview;
pub(crate) use preview::{CopperLayer, preview_layer};

use std::collections::BTreeMap;

use reda_engine::command::{Command, CommandError, Context, Kind, Meta, Registry};
use reda_engine::model::geom::{Nm, Point};
use reda_engine::model::{
    Footprint, FootprintLayer, LibraryElement, PadKind, PadShape, Uuid, auto_increment,
};
use reda_engine::preferences::{DrawingKind, PadDefaults};
use serde_json::{Value, json};

use crate::canvas::interact::Pointer;
use crate::canvas::{LiveHandle, Overlay};
use crate::element_editor::select::{self, Drag, Editor};
use crate::element_editor::{self, Invocation};
use crate::shell::Shell;
use crate::tools::Tool;

/// A Document point in nm, y up.
type P = (f64, f64);

/// Segments a circle in flight is drawn with.
const CIRCLE_SEGMENTS: usize = 48;

/// `library.footprint.pad.add` params for the Pad record `pad` at `at` with `number`.
pub fn pad_params(pad: &PadDefaults, footprint: Uuid, number: &str, at: Point) -> Value {
    let mut p = json!({
        "footprint": footprint,
        "number": number,
        "kind": pad.kind,
        "shape": pad.shape,
        "at": {"x": at.x.0, "y": at.y.0},
        "width": pad.width.nm().0,
        "height": pad.height.nm().0,
    });
    if pad.shape == PadShape::ChamferedRect {
        p["chamfer_ratio"] = json!(pad.chamfer_ratio.0);
    }
    if pad.kind != PadKind::Smd {
        p["hole"] = json!({"width": pad.hole_width.nm().0, "height": pad.hole_height.nm().0});
    }
    if let Some(m) = pad.mask_margin {
        p["mask"] = json!({"top": {"expansion": m.nm().0}});
        if pad.kind != PadKind::Smd {
            p["mask"]["bottom"] = json!({"expansion": m.nm().0});
        }
    }
    if pad.kind == PadKind::Smd
        && let Some(m) = pad.paste_margin
    {
        p["paste"] = json!({"expansion": m.nm().0});
    }
    p
}

/// The Footprint Tools' settings, shared by every Footprint tab. What the Pad Tool places
/// and how graphics and text look are Object Defaults ([`crate::object_defaults`]).
#[derive(Clone, Debug, PartialEq)]
pub struct ToolSettings {
    /// The Layer Line, Rect, Circle, Arc and Polygon draw on, and Text places on (on the
    /// Silkscreen of the same side for a Courtyard Layer).
    pub layer: FootprintLayer,
    /// The field the Text Tool's text shows: `Designator`, `Value` or any field name.
    pub text_field: String,
}

impl Default for ToolSettings {
    fn default() -> ToolSettings {
        ToolSettings {
            layer: FootprintLayer::SilkscreenTop,
            text_field: reda_engine::model::DESIGNATOR_FIELD.to_owned(),
        }
    }
}

/// What one Footprint canvas has in flight.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Interaction {
    #[default]
    Idle,
    Selecting(Drag),
    /// A line, rectangle or circle being dragged out, snapped.
    Drawing {
        tool: Tool,
        from: Point,
        to: Point,
    },
    /// An arc (centre, start, end) or polygon being clicked out, corner by corner; `at`
    /// is the pointer.
    Drafting {
        tool: Tool,
        points: Vec<Point>,
        at: Point,
    },
    /// Measure: from the first click to the pointer, or to the second click once
    /// `done`.
    Measuring {
        from: Point,
        to: Point,
        done: bool,
    },
}

/// The Footprint editor's view state: the Tools' settings and each canvas's interaction.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FootprintEditorState {
    pub settings: ToolSettings,
    preview_layers: BTreeMap<Uuid, CopperLayer>,
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
    /// The copper group to preview in one open Footprint.
    #[serde(default)]
    preview: Option<preview::Preview>,
    /// The Layer Line, Rect, Circle, Arc and Polygon draw on.
    #[serde(default)]
    pub layer: Option<FootprintLayer>,
    /// The field the Text Tool places.
    #[serde(default)]
    pub text_field: Option<String>,
}

struct ToolSet;

impl Command for ToolSet {
    const META: Meta = Meta {
        id: "view.footprint_tool.set",
        title: "Set Footprint Tool Settings",
        description: "Sets the Layer the Footprint editor's Line, Rect and Circle Tools \
                      draw on and the field its Text Tool places. View state, shared by \
                      every Footprint tab. What the Pad Tool places is the \
                      `defaults.footprint.pad` Object Default.",
        category: "view",
        icon: "pad",
        kind: Kind::View,
    };
    type Params = ToolSetParams;
    type Output = Value;

    fn run(&self, cx: &mut Context<'_>, p: ToolSetParams) -> Result<Value, CommandError> {
        if let Some(preview) = &p.preview {
            preview.apply(cx)?;
        }
        let settings = &mut cx.view::<FootprintEditorState>()?.settings;
        if let Some(layer) = p.layer {
            settings.layer = layer;
        }
        if let Some(field) = p.text_field.filter(|f| !f.trim().is_empty()) {
            settings.text_field = field.trim().to_owned();
        }
        let mut out = json!({ "layer": settings.layer, "text_field": settings.text_field });
        if let Some(preview) = p.preview {
            out["preview"] = json!(preview);
        }
        Ok(out)
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
        pick: select::PICK_POINTS * nm_per_point,
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

/// Closes the polygon being clicked out on the Footprint canvas `element` (Enter): the
/// Command that adds it, if it has three corners.
pub fn finish(shell: &mut Shell, element: Uuid) -> Option<Invocation> {
    let Interaction::Drafting {
        tool: Tool::Polygon,
        points,
        ..
    } = state(shell).canvases.get(&element).cloned()?
    else {
        return None;
    };
    let shape =
        (points.len() >= 3).then(|| crate::symbol_editor::shape_of(Tool::Polygon, &points))??;
    let cursor = shell.document_tab(element).and_then(|t| t.cursor);
    show(shell, element, Interaction::Idle, None, cursor);
    Some(graphic_add(shell, element, shape))
}

/// The `library.footprint.graphic.add` of `shape` on the drawing Layer, with the width
/// and fill of its kind's Object Defaults for that Layer kind.
fn graphic_add(shell: &Shell, element: Uuid, shape: Value) -> Invocation {
    let layer = tool_settings(shell).layer;
    let kind = DrawingKind::of_footprint(layer);
    let pcb = crate::object_defaults::current(shell).pcb;
    let outline = |o: &reda_engine::preferences::Outline| (o.width, o.fill);
    let (width, fill) = match shape["kind"].as_str().unwrap_or_default() {
        "rect" => outline(pcb.rectangle.of(kind)),
        "circle" => outline(pcb.circle.of(kind)),
        "polygon" => outline(pcb.polygon.of(kind)),
        "arc" => (pcb.arc.of(kind).width, false),
        _ => (pcb.line.of(kind).width, false),
    };
    (
        "library.footprint.graphic.add".to_owned(),
        json!({"footprint": element, "layer": layer, "shape": shape,
               "width": width.nm().0, "fill": fill}),
    )
}

/// The distance Measure shows from `from` to `to`, for the status bar.
pub fn measure_note(from: Point, to: Point) -> String {
    let mm = |n: i64| crate::pcb::view::format_length(Nm(n), reda_engine::preferences::Units::Mm);
    let (dx, dy) = (to.x.0 - from.x.0, to.y.0 - from.y.0);
    let d = (dx as f64).hypot(dy as f64).round() as i64;
    crate::tr!(
        "footprint-measure-note",
        distance = mm(d),
        dx = mm(dx.abs()),
        dy = mm(dy.abs())
    )
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
        Interaction::Selecting(drag) => drag.preview(shell, element),
        Interaction::Drawing { tool, from, to } => (
            Vec::new(),
            (0.0, 0.0),
            Overlay {
                wires: outline(*tool, *from, *to).into(),
                cursor: Some(*to),
                ..Overlay::default()
            },
        ),
        Interaction::Drafting { points, at, .. } => {
            let mut corners = points.clone();
            corners.push(*at);
            (
                Vec::new(),
                (0.0, 0.0),
                Overlay {
                    wires: corners
                        .windows(2)
                        .map(|w| (w[0], w[1]))
                        .collect::<Vec<_>>()
                        .into(),
                    cursor: Some(*at),
                    ..Overlay::default()
                },
            )
        }
        Interaction::Measuring { from, to, .. } => (
            Vec::new(),
            (0.0, 0.0),
            Overlay {
                wires: vec![(*from, *to)].into(),
                cursor: Some(*to),
                ..Overlay::default()
            },
        ),
    };
    let Some(tab) = shell.document_tab_mut(element) else {
        return;
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
        Editor::Footprint(self.footprint).snap(self.shell, at, &[])
    }

    fn run(&mut self, id: &str, params: Value) {
        self.commands.push((id.to_owned(), params));
    }

    /// Drops a drawing whose Tool is no longer active.
    fn sync_tool(&self, state: Interaction) -> Interaction {
        match state {
            Interaction::Drawing { tool, .. } | Interaction::Drafting { tool, .. }
                if tool != self.shell.tool() =>
            {
                Interaction::Idle
            }
            Interaction::Measuring { .. } if self.shell.tool() != Tool::Measure => {
                Interaction::Idle
            }
            Interaction::Selecting(_) if self.shell.tool() != Tool::Select => Interaction::Idle,
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
            Tool::Pad | Tool::Origin | Tool::Text => {
                if let Pointer::Click { at, .. } = event {
                    self.click_tool(at);
                }
                Interaction::Idle
            }
            tool @ (Tool::Line | Tool::Rect | Tool::Circle) => self.draw(tool, state, event),
            tool @ (Tool::Arc | Tool::Polygon) => self.draft(tool, state, event),
            Tool::Measure => self.measure(state, event),
            _ => {
                let drag = match state {
                    Interaction::Selecting(drag) => Some(drag),
                    _ => None,
                };
                Editor::Footprint(self.footprint)
                    .select(
                        self.shell,
                        drag,
                        event,
                        self.pick,
                        hovered,
                        &mut self.commands,
                    )
                    .map_or(Interaction::Idle, Interaction::Selecting)
            }
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
                let pad = crate::object_defaults::current(self.shell).footprint.pad;
                let params = pad_params(&pad, self.element, &number, at);
                self.run("library.footprint.pad.add", params);
            }
            Tool::Text => {
                let settings = tool_settings(self.shell);
                let layer = inspector::text_layer(settings.layer);
                let defaults = crate::object_defaults::current(self.shell);
                let look = defaults.pcb.text.of(DrawingKind::of_footprint(layer));
                self.run(
                    "library.footprint.text.add",
                    json!({"footprint": self.element, "field": settings.text_field,
                           "at": {"x": at.x.0, "y": at.y.0}, "layer": layer,
                           "height": look.height.nm().0, "thickness": look.thickness.nm().0,
                           "font": look.font, "justify": defaults.footprint.text.justify}),
                );
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

    /// Arc and Polygon: each click adds a corner; an arc is done after three (centre,
    /// start, end), a polygon on a click on its first corner or Enter ([`finish`]).
    fn draft(&mut self, tool: Tool, state: Interaction, event: Pointer) -> Interaction {
        if self.read_only {
            return Interaction::Idle;
        }
        let mut points = match state {
            Interaction::Drafting {
                tool: t, points, ..
            } if t == tool => points,
            _ => Vec::new(),
        };
        let at = match event {
            Pointer::Click { at, .. } => self.snap(at),
            Pointer::Hover(at) | Pointer::Drag(at) => {
                if points.is_empty() {
                    return Interaction::Idle;
                }
                let at = self.snap(at);
                return Interaction::Drafting { tool, points, at };
            }
            _ if points.is_empty() => return Interaction::Idle,
            _ => {
                let at = *points.last().expect("not empty");
                return Interaction::Drafting { tool, points, at };
            }
        };
        if points.last() == Some(&at) {
            return Interaction::Drafting { tool, points, at };
        }
        let closes = tool == Tool::Polygon && points.len() >= 3 && points[0] == at;
        if !closes {
            points.push(at);
        }
        let needed = crate::symbol_editor::corners(tool);
        if !closes && needed.is_none_or(|n| points.len() < n) {
            return Interaction::Drafting { tool, points, at };
        }
        if let Some(shape) = crate::symbol_editor::shape_of(tool, &points) {
            let (id, params) = graphic_add(self.shell, self.element, shape);
            self.run(&id, params);
        }
        Interaction::Idle
    }

    /// Measure: a click starts, the second click ends and shows the distance; a third
    /// click starts again.
    fn measure(&mut self, state: Interaction, event: Pointer) -> Interaction {
        match (state, event) {
            (
                Interaction::Measuring {
                    from, done: false, ..
                },
                Pointer::Hover(at),
            ) => Interaction::Measuring {
                from,
                to: self.snap(at),
                done: false,
            },
            (
                Interaction::Measuring {
                    from, done: false, ..
                },
                Pointer::Click { at, .. },
            ) => {
                let to = self.snap(at);
                self.shell.set_note(measure_note(from, to));
                Interaction::Measuring {
                    from,
                    to,
                    done: true,
                }
            }
            (_, Pointer::Click { at, .. }) => {
                let from = self.snap(at);
                Interaction::Measuring {
                    from,
                    to: from,
                    done: false,
                }
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
        let (id, params) = graphic_add(self.shell, self.element, shape);
        self.run(&id, params);
    }
}
