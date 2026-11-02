//! Editing on a schematic canvas: the Select, Move and Drag Tools, and the keys.
//!
//! The editor turns a frame's [`Pointer`] events into Commands the window runs after the
//! frame (`selection.set`, `schematic.drag`, `schematic.move`, `tool.activate`), and keeps
//! what is in flight on the Document's tab ([`Interaction`]). A move in flight touches
//! neither the Project nor the Journal: the canvas draws it from the tab's marks and
//! overlay, from [`shift_preview`], until release commits exactly one Command.
//!
//! Picking, box selection and snapping are the shared [`interact`](crate::canvas::interact)
//! functions; this module only says what on a sheet can be picked and snapped to.

pub mod keys;

use std::collections::BTreeSet;

use reda_engine::model::geom::{Nm, Point};
use reda_engine::model::{SchematicSheet, Uuid};
use reda_engine::schematic::connectivity::placed_pins;
use reda_engine::schematic::{ShiftPreview, shift_preview};
use serde_json::{Value, json};

use crate::canvas::interact::{
    BoxMode, Combine, Pointer, SnapTargets, box_select, combine, hit_test,
};
use crate::canvas::{LiveHandle, Overlay};
use crate::shell::Shell;
use crate::tools::Tool;

/// A Command for the window to run after the frame.
pub type Invocation = (String, Value);

/// A Document point in nm, y up.
type P = (f64, f64);

/// How close, in points on screen, the pointer must be to pick an object.
const PICK_POINTS: f64 = 5.0;
/// How close, in points on screen, the pointer must be to snap onto a Pin or Wire.
const SNAP_POINTS: f64 = 10.0;

/// What a schematic canvas has in flight.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Interaction {
    #[default]
    Idle,
    /// A selection box is being dragged out from `from` to `to`.
    Boxing {
        from: P,
        to: P,
        modifiers: egui::Modifiers,
    },
    /// The selection is moving with the pointer.
    Flight(Flight),
}

/// A move or drag in flight.
#[derive(Clone, Debug, PartialEq)]
pub struct Flight {
    pub objects: Vec<Uuid>,
    /// A drag (Wires rubber-band) rather than a detached move.
    pub keep_connections: bool,
    /// Started by the Move or Drag Tool (a click puts it down), not by dragging.
    pub by_tool: bool,
    /// The snapped point picked up; the selection moves by the snapped pointer minus it.
    pub anchor: Point,
    /// What the pointer snaps to while moving: everything on the sheet not moving.
    pub targets: SnapTargets,
    pub offset: Point,
    pub preview: ShiftPreview,
    /// Released: its Command is queued, and it keeps showing until the Shell runs it.
    pub committing: bool,
}

/// How far on the Document the pick and snap distances reach at the canvas's zoom.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Scale {
    pub pick: f64,
    pub snap: f64,
}

impl Scale {
    /// The distances for a canvas showing `nm_per_point` nm per screen point.
    pub fn at(nm_per_point: f64) -> Scale {
        Scale {
            pick: PICK_POINTS * nm_per_point,
            snap: SNAP_POINTS * nm_per_point,
        }
    }
}

/// Runs the active Tool on a frame's pointer `events` over `document`'s canvas. Returns
/// the Commands to run; updates the tab's marks and overlay.
pub fn pointer(
    shell: &mut Shell,
    document: Uuid,
    events: &[Pointer],
    scale: Scale,
) -> Vec<Invocation> {
    let Some(tab) = shell.document_tab_mut(document) else {
        return Vec::new();
    };
    let state = std::mem::take(&mut tab.interaction);
    let mut hovered = tab.live.hovered;
    let mut cursor = tab.cursor;
    let mut cx = Cx {
        shell,
        document,
        scale,
        commands: Vec::new(),
    };
    let mut state = cx.sync_tool(state);
    // The Move and Drag Tools pick the selection up where the pointer was when they were
    // activated, so its first movement already moves the selection.
    if let Some(at) = cursor {
        state = cx.start_tool_flight(state, at);
    }
    for &event in events {
        if let Some(at) = event_at(event) {
            cursor = Some(at);
        }
        state = cx.step(state, event, &mut hovered);
    }
    if let Some(at) = cursor {
        state = cx.start_tool_flight(state, at);
    }
    let commands = cx.commands;
    if !matches!(state, Interaction::Idle) {
        hovered = None;
    }
    show(shell, document, state, hovered, cursor);
    commands
}

/// Drops whatever is in flight on `document`'s canvas (Escape).
pub fn cancel(shell: &mut Shell, document: Uuid) {
    if let Some(tab) = shell.document_tab_mut(document)
        && !matches!(&tab.interaction, Interaction::Flight(f) if f.committing)
    {
        let cursor = tab.cursor;
        show(shell, document, Interaction::Idle, None, cursor);
    }
}

fn event_at(event: Pointer) -> Option<P> {
    match event {
        Pointer::Hover(at) | Pointer::Drag(at) | Pointer::DragEnd(at) => Some(at),
        Pointer::Click { at, .. } | Pointer::DragStart { at, .. } => Some(at),
    }
}

/// Writes `state` and what it shows to the tab.
fn show(
    shell: &mut Shell,
    document: Uuid,
    state: Interaction,
    hovered: Option<Uuid>,
    cursor: Option<P>,
) {
    let Some(tab) = shell.document_tab_mut(document) else {
        return;
    };
    let (moving, hidden, offset, overlay) = match &state {
        Interaction::Idle => (Vec::new(), Vec::new(), (0.0, 0.0), Overlay::default()),
        Interaction::Boxing { from, to, .. } => (
            Vec::new(),
            Vec::new(),
            (0.0, 0.0),
            Overlay {
                rubber_band: Some((*from, *to, BoxMode::of_drag(*from, *to))),
                ..Overlay::default()
            },
        ),
        Interaction::Flight(f) => (
            f.preview.moved.clone(),
            f.preview.hidden.clone(),
            (f.offset.x.0 as f64, f.offset.y.0 as f64),
            Overlay {
                wires: f.preview.wires.clone().into(),
                cursor: Some(f.anchor + f.offset),
                ..Overlay::default()
            },
        ),
    };
    if !tab.live.shows(hovered, &moving, &hidden) {
        tab.live = LiveHandle::new(hovered, &moving, &hidden);
    }
    tab.moving_offset = offset;
    tab.overlay = overlay;
    tab.interaction = state;
    tab.cursor = cursor;
}

struct Cx<'a> {
    shell: &'a Shell,
    document: Uuid,
    scale: Scale,
    commands: Vec<Invocation>,
}

impl Cx<'_> {
    fn sheet(&self) -> Option<&SchematicSheet> {
        self.shell
            .engine()
            .project()?
            .schematic_sheet(self.document)
    }

    fn grid(&self) -> Nm {
        self.shell.grid(self.document)
    }

    /// The selected objects on this sheet.
    fn selected(&self) -> Vec<Uuid> {
        let on_sheet = self.sheet().map(selectable).unwrap_or_default();
        self.shell
            .selection()
            .objects
            .iter()
            .copied()
            .filter(|o| on_sheet.contains(o))
            .collect()
    }

    fn select(&mut self, objects: Vec<Uuid>) {
        if objects != self.shell.selection().objects {
            self.commands
                .push(("selection.set".to_owned(), json!({ "objects": objects })));
        }
    }

    fn hit(&self, at: P) -> Option<Uuid> {
        let tab = self.shell.document_tab(self.document)?;
        let pickable = self.sheet().map(selectable).unwrap_or_default();
        hit_test(&tab.scene.scene, at, self.scale.pick, |o| {
            pickable.contains(&o)
        })
    }

    /// Ends a Tool flight once its Tool is no longer active, and a committed flight's
    /// Command has run (the Shell drops it then).
    fn sync_tool(&mut self, state: Interaction) -> Interaction {
        match state {
            Interaction::Flight(f) if f.by_tool && !f.committing => {
                let wanted = if f.keep_connections {
                    Tool::Drag
                } else {
                    Tool::Move
                };
                if self.shell.tool() == wanted {
                    Interaction::Flight(f)
                } else {
                    Interaction::Idle
                }
            }
            other => other,
        }
    }

    /// Picks the selection up at `at` when the Move or Drag Tool is active.
    fn start_tool_flight(&mut self, state: Interaction, at: P) -> Interaction {
        let keep_connections = match (self.shell.tool(), &state) {
            (Tool::Move, Interaction::Idle) => false,
            (Tool::Drag, Interaction::Idle) => true,
            _ => return state,
        };
        let objects = self.selected();
        if objects.is_empty() {
            self.commands
                .push(("tool.activate:select".to_owned(), json!({})));
            return state;
        }
        self.start_flight(objects, keep_connections, true, at, at)
    }

    fn start_flight(
        &mut self,
        objects: Vec<Uuid>,
        keep_connections: bool,
        by_tool: bool,
        from: P,
        at: P,
    ) -> Interaction {
        let Some(sheet) = self.sheet() else {
            return Interaction::Idle;
        };
        let (grid, snap) = (self.grid(), self.scale.snap);
        let anchor = snap_targets(sheet, &BTreeSet::new())
            .snap(from, grid, snap)
            .at;
        let moving: BTreeSet<Uuid> = objects.iter().copied().collect();
        let flight = Flight {
            targets: snap_targets(sheet, &moving),
            objects,
            keep_connections,
            by_tool,
            anchor,
            offset: Point::default(),
            preview: ShiftPreview::default(),
            committing: false,
        };
        let mut flight = self.fly(flight, from, true);
        flight = self.fly(flight, at, false);
        Interaction::Flight(flight)
    }

    /// Moves `flight` to the pointer at `at`.
    fn fly(&self, mut flight: Flight, at: P, force: bool) -> Flight {
        let target = flight.targets.snap(at, self.grid(), self.scale.snap).at;
        let offset = target - flight.anchor;
        if offset != flight.offset || force {
            flight.offset = offset;
            flight.preview = self
                .shell
                .engine()
                .project()
                .and_then(|p| {
                    shift_preview(p, &flight.objects, offset, flight.keep_connections).ok()
                })
                .unwrap_or_else(|| ShiftPreview {
                    moved: flight.objects.clone(),
                    ..ShiftPreview::default()
                });
        }
        flight
    }

    /// Puts `flight` down: queues its one Command, unless it never moved.
    fn commit(&mut self, mut flight: Flight) -> Interaction {
        if flight.by_tool {
            self.commands
                .push(("tool.activate:select".to_owned(), json!({})));
        }
        if flight.offset == Point::default() {
            return Interaction::Idle;
        }
        let id = if flight.keep_connections {
            "schematic.drag"
        } else {
            "schematic.move"
        };
        self.commands.push((
            id.to_owned(),
            json!({
                "objects": flight.objects,
                "by": {"x": flight.offset.x.0, "y": flight.offset.y.0},
            }),
        ));
        flight.committing = true;
        Interaction::Flight(flight)
    }

    fn step(
        &mut self,
        state: Interaction,
        event: Pointer,
        hovered: &mut Option<Uuid>,
    ) -> Interaction {
        match (state, event) {
            // A released move keeps showing until its Command has run.
            (Interaction::Flight(f), _) if f.committing => Interaction::Flight(f),
            (Interaction::Flight(f), Pointer::Hover(at) | Pointer::Drag(at)) => {
                Interaction::Flight(self.fly(f, at, false))
            }
            (Interaction::Flight(f), Pointer::DragEnd(at)) if !f.by_tool => {
                let f = self.fly(f, at, false);
                self.commit(f)
            }
            (Interaction::Flight(f), Pointer::Click { at, .. } | Pointer::DragEnd(at)) => {
                let f = self.fly(f, at, false);
                self.commit(f)
            }
            (Interaction::Flight(f), Pointer::DragStart { at, .. }) => {
                Interaction::Flight(self.fly(f, at, false))
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
            (
                Interaction::Boxing {
                    from,
                    to,
                    modifiers,
                },
                _,
            ) => Interaction::Boxing {
                from,
                to,
                modifiers,
            },
            (Interaction::Idle, Pointer::Hover(at)) => {
                *hovered = self.hit(at);
                Interaction::Idle
            }
            (Interaction::Idle, Pointer::Click { at, modifiers }) => {
                let how = Combine::of(modifiers);
                let current = self.shell.selection().objects.clone();
                let picked: Vec<Uuid> = self.hit(at).into_iter().collect();
                if picked.is_empty() && how != Combine::Replace {
                    return Interaction::Idle;
                }
                self.select(combine(&current, &picked, how));
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
                let Some(hit) = self.hit(from) else {
                    return Interaction::Boxing {
                        from,
                        to: at,
                        modifiers,
                    };
                };
                let selected = self.selected();
                let objects = if selected.contains(&hit) {
                    selected
                } else if modifiers.shift {
                    combine(&selected, &[hit], Combine::Add)
                } else {
                    vec![hit]
                };
                self.select(objects.clone());
                // Mod+drag detaches; a plain drag keeps connections.
                self.start_flight(objects, !modifiers.command, false, from, at)
            }
            (Interaction::Idle, Pointer::Drag(_) | Pointer::DragEnd(_)) => Interaction::Idle,
        }
    }

    fn box_select(&mut self, from: P, to: P, modifiers: egui::Modifiers) {
        let Some(tab) = self.shell.document_tab(self.document) else {
            return;
        };
        let pickable = self.sheet().map(selectable).unwrap_or_default();
        let picked = box_select(
            &tab.scene.scene,
            from,
            to,
            BoxMode::of_drag(from, to),
            |o| pickable.contains(&o),
        );
        let current = self.shell.selection().objects.clone();
        self.select(combine(&current, &picked, Combine::of(modifiers)));
    }
}

/// Everything on `sheet` that can be selected.
pub fn selectable(sheet: &SchematicSheet) -> BTreeSet<Uuid> {
    let mut out = BTreeSet::new();
    out.extend(sheet.units().map(|o| o.uuid));
    out.extend(sheet.wires().map(|o| o.uuid));
    out.extend(sheet.net_labels().map(|o| o.uuid));
    out.extend(sheet.no_connects().map(|o| o.uuid));
    out.extend(sheet.ports().map(|o| o.uuid));
    out.extend(sheet.power_ports().map(|o| o.uuid));
    out.extend(sheet.texts().map(|o| o.uuid));
    out.extend(sheet.graphics().map(|o| o.uuid));
    out
}

/// The Pins, Wire ends and Wires on `sheet` the pointer can snap onto, leaving out those
/// of the `moving` objects.
fn snap_targets(sheet: &SchematicSheet, moving: &BTreeSet<Uuid>) -> SnapTargets {
    let mut targets = SnapTargets::default();
    for (unit, _, at) in placed_pins(sheet) {
        if !moving.contains(&unit.uuid) {
            targets.points.push(at);
        }
    }
    for wire in sheet.wires().filter(|w| !moving.contains(&w.uuid)) {
        targets.points.extend([wire.from, wire.to]);
        targets.segments.push((wire.from, wire.to));
    }
    targets
}
