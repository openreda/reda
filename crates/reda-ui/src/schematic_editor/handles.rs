//! Editing a placed Wire with the Select Tool (docs/spec/v0.1/schematic.md#editing-a-placed-wire):
//! segment selection, and the handles of the selected Wires and segments.
//!
//! - A click on a segment of a selected Wire selects only that segment; Shift+click on a
//!   selected Wire turns it into its segment, added to the segments already selected.
//! - A selected Wire has a handle at each vertex and in the middle of each segment; a
//!   selected segment has red ones at its ends and middle. Dragging a vertex handle (an
//!   end or a corner) moves that vertex only; dragging a segment, by its middle handle or
//!   (selected) by its body, moves its two vertices only. A drag commits one
//!   `schematic.wire.set_vertices` on release.
//! - While a vertex is held, Delete removes it ([`delete_held_vertex`]); while a segment
//!   is held, Insert adds a vertex there that the pointer then carries
//!   ([`insert_held_vertex`]).

use reda_engine::model::geom::{Nm, Point};
use reda_engine::model::{SchematicSheet, Uuid, Wire};
use serde_json::json;

use super::{Cx, Interaction, Invocation, P};
use crate::canvas::interact::Pointer;
use crate::selection::{Selection, WireSegment};
use crate::shell::Shell;
use crate::snap::{self, Target};

/// What a handle moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Grip {
    /// The vertex at this index.
    Vertex(usize),
    /// The segment from this vertex to the next.
    Segment(usize),
}

/// A handle being dragged.
#[derive(Clone, Debug, PartialEq)]
pub struct HandleDrag {
    pub wire: Uuid,
    pub grip: Grip,
    /// Drawn red: the handle of a selected segment.
    pub red: bool,
    /// The Wire's vertices when it was picked up.
    pub original: Vec<Point>,
    /// The vertices as the drag has them now.
    pub vertices: Vec<Point>,
    /// The point that follows the snapped pointer: the vertex held, or a segment's first.
    anchor: Point,
    /// From the pointer to the anchor where it was picked up (nm).
    lead: (f64, f64),
    /// Where the pointer is, snapped.
    at: Point,
    targets: Vec<Target>,
}

impl HandleDrag {
    fn moved(&self, offset: Point) -> Vec<Point> {
        let mut v = self.original.clone();
        match self.grip {
            Grip::Vertex(i) => v[i] = v[i] + offset,
            Grip::Segment(i) => {
                v[i] = v[i] + offset;
                v[i + 1] = v[i + 1] + offset;
            }
        }
        v
    }

    /// The Command that puts the Wire down as dragged; `None` when it never moved.
    fn commit(&self) -> Option<Invocation> {
        (self.vertices != self.original).then(|| set_vertices(self.wire, &self.vertices))
    }
}

/// One handle on the canvas.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Handle {
    pub at: Point,
    pub wire: Uuid,
    pub grip: Grip,
    /// A selected segment's handle, drawn red.
    pub red: bool,
}

/// `schematic.wire.set_vertices` for `wire` through `vertices`.
fn set_vertices(wire: Uuid, vertices: &[Point]) -> Invocation {
    let vertices: Vec<_> = (vertices.iter())
        .map(|p| json!({"x": p.x.0, "y": p.y.0}))
        .collect();
    (
        "schematic.wire.set_vertices".to_owned(),
        json!({"wire": wire, "vertices": vertices}),
    )
}

fn middle(a: Point, b: Point) -> Point {
    Point::new(Nm((a.x.0 + b.x.0) / 2), Nm((a.y.0 + b.y.0) / 2))
}

/// The handles of a Wire through `v`: one per vertex and one per segment.
fn wire_handles(wire: Uuid, v: &[Point], red: bool, out: &mut Vec<Handle>) {
    for (i, &at) in v.iter().enumerate() {
        out.push(Handle {
            at,
            wire,
            grip: Grip::Vertex(i),
            red,
        });
    }
    for i in 0..v.len().saturating_sub(1) {
        out.push(Handle {
            at: middle(v[i], v[i + 1]),
            wire,
            grip: Grip::Segment(i),
            red,
        });
    }
}

/// The handles `selection` shows on `sheet`, while the Select Tool is idle.
pub fn handles(sheet: &SchematicSheet, selection: &Selection) -> Vec<Handle> {
    let mut out = Vec::new();
    for wire in selection.objects.iter().filter_map(|o| sheet.wire(*o)) {
        wire_handles(wire.uuid, &wire.vertices, false, &mut out);
    }
    for s in &selection.segments {
        let Some(wire) = sheet.wire(s.wire) else {
            continue;
        };
        let Some(pair) = wire.vertices.get(s.index..s.index + 2) else {
            continue;
        };
        for (k, &at) in pair.iter().enumerate() {
            out.push(Handle {
                at,
                wire: s.wire,
                grip: Grip::Vertex(s.index + k),
                red: true,
            });
        }
        out.push(Handle {
            at: middle(pair[0], pair[1]),
            wire: s.wire,
            grip: Grip::Segment(s.index),
            red: true,
        });
    }
    out
}

/// The selected segments on `sheet`, as (from, to).
pub fn selected_segments(sheet: &SchematicSheet, selection: &Selection) -> Vec<(Point, Point)> {
    (selection.segments.iter())
        .filter_map(|s| {
            let v = &sheet.wire(s.wire)?.vertices;
            Some((*v.get(s.index)?, *v.get(s.index + 1)?))
        })
        .collect()
}

/// How far `p` is from the segment from `a` to `b` (nm).
fn distance_to(a: Point, b: Point, p: P) -> f64 {
    let (ax, ay) = (a.x.0 as f64, a.y.0 as f64);
    let (dx, dy) = (b.x.0 as f64 - ax, b.y.0 as f64 - ay);
    let length = dx * dx + dy * dy;
    let t = if length == 0.0 {
        0.0
    } else {
        (((p.0 - ax) * dx + (p.1 - ay) * dy) / length).clamp(0.0, 1.0)
    };
    (p.0 - ax - t * dx).hypot(p.1 - ay - t * dy)
}

/// The segment of `wire` nearest to `at`.
pub fn nearest_segment(wire: &Wire, at: P) -> usize {
    (0..wire.vertices.len() - 1)
        .min_by(|&i, &j| {
            let d = |k: usize| distance_to(wire.vertices[k], wire.vertices[k + 1], at);
            d(i).total_cmp(&d(j))
        })
        .unwrap_or(0)
}

impl Cx<'_> {
    /// A click with the Select Tool on a Wire that is selected, whole or by a segment:
    /// selects the segment under the pointer (Shift adds it, or drops it when it is
    /// selected already). `false` when the click is not one of these, or lands where
    /// objects overlap (a repeated click cycles them), and picks as usual.
    pub(super) fn segment_click(&mut self, at: P, modifiers: egui::Modifiers) -> bool {
        // Where objects overlap, a repeated click cycles through them instead.
        let stack =
            crate::selection::picking::stack(self.shell, self.document, at, self.scale.pick);
        let [hit] = stack[..] else {
            return false;
        };
        let Some(wire) = self.sheet().and_then(|s| s.wire(hit)).cloned() else {
            return false;
        };
        let mut selection = self.shell.selection().clone();
        let whole = selection.objects.contains(&wire.uuid);
        let by_segment = selection.segments.iter().any(|s| s.wire == wire.uuid);
        if !whole && !by_segment || modifiers.command || modifiers.alt {
            return false;
        }
        let segment = WireSegment {
            wire: wire.uuid,
            index: nearest_segment(&wire, at),
        };
        if !modifiers.shift {
            selection = Selection {
                segments: vec![segment],
                ..Selection::default()
            };
        } else if whole {
            selection.objects.retain(|o| *o != wire.uuid);
            selection.segments.push(segment);
        } else if selection.segments.contains(&segment) {
            selection.segments.retain(|s| *s != segment);
        } else {
            selection.segments.push(segment);
        }
        self.commands
            .push(crate::selection::picking::set_command(&selection));
        true
    }

    /// A drag with the Select Tool that starts on a handle, or on a selected segment:
    /// picks the handle up.
    pub(super) fn grab_handle(&mut self, from: P, at: P) -> Option<Interaction> {
        let sheet = self.sheet()?;
        let selection = self.shell.selection();
        let reach = self.scale.pick * 1.5;
        let near = |p: Point| (p.x.0 as f64 - from.0).hypot(p.y.0 as f64 - from.1);
        let all = handles(sheet, selection);
        let grabbed = (all.iter())
            .filter(|h| near(h.at) <= reach)
            .min_by(|a, b| {
                let vertex_first = |h: &Handle| matches!(h.grip, Grip::Segment(_));
                (vertex_first(a), near(a.at))
                    .partial_cmp(&(vertex_first(b), near(b.at)))
                    .expect("distances are numbers")
            })
            .copied()
            .or_else(|| {
                // A selected segment also drags by its body.
                selection.segments.iter().find_map(|s| {
                    let v = &sheet.wire(s.wire)?.vertices;
                    let (a, b) = (*v.get(s.index)?, *v.get(s.index + 1)?);
                    (distance_to(a, b, from) <= self.scale.pick).then_some(Handle {
                        at: a,
                        wire: s.wire,
                        grip: Grip::Segment(s.index),
                        red: true,
                    })
                })
            })?;
        let original = sheet.wire(grabbed.wire)?.vertices.clone();
        let anchor = match grabbed.grip {
            Grip::Vertex(i) | Grip::Segment(i) => original[i],
        };
        let targets = snap::hotspots::without(snap::hotspots::sheet(sheet), &[grabbed.wire]);
        let drag = HandleDrag {
            wire: grabbed.wire,
            grip: grabbed.grip,
            red: grabbed.red,
            vertices: original.clone(),
            original,
            anchor,
            lead: (anchor.x.0 as f64 - from.0, anchor.y.0 as f64 - from.1),
            at: anchor,
            targets,
        };
        Some(Interaction::Handle(self.drag_handle(drag, at)))
    }

    /// Moves the handle held in `drag` to the pointer at `at`.
    fn drag_handle(&self, mut drag: HandleDrag, at: P) -> HandleDrag {
        let led = (at.0 + drag.lead.0, at.1 + drag.lead.1);
        let target = snap::snap_at(self.shell, self.document, led, &drag.targets).at;
        drag.at = snap::snap_at(self.shell, self.document, at, &drag.targets).at;
        drag.vertices = drag.moved(target - drag.anchor);
        drag
    }

    /// A pointer event while a handle is held: it follows the pointer, and the release
    /// commits the Wire.
    pub(super) fn handle_step(&mut self, drag: HandleDrag, event: Pointer) -> Interaction {
        match event {
            Pointer::Drag(at) | Pointer::Hover(at) | Pointer::DragStart { at, .. } => {
                Interaction::Handle(self.drag_handle(drag, at))
            }
            Pointer::DragEnd(at) | Pointer::Click { at, .. } => {
                let drag = self.drag_handle(drag, at);
                self.commands.extend(drag.commit());
                Interaction::Idle
            }
        }
    }
}

/// Delete while a vertex is held: the Wire without it, as one Command, and the drag
/// ends. `None` when no vertex is held on `document` (Delete deletes the selection), or
/// the Wire would be left with fewer than two vertices.
pub fn delete_held_vertex(shell: &mut Shell, document: Uuid) -> Option<Vec<Invocation>> {
    let tab = shell.document_tab_mut(document)?;
    let Interaction::Handle(drag) = &tab.interaction else {
        return None;
    };
    let Grip::Vertex(i) = drag.grip else {
        return None;
    };
    let mut left = drag.original.clone();
    left.remove(i);
    let wire = drag.wire;
    super::cancel(shell, document);
    Some(if left.len() >= 2 {
        vec![set_vertices(wire, &left)]
    } else {
        Vec::new()
    })
}

/// Insert while a segment is held: a vertex where the pointer is, which the drag then
/// carries (the segment's ends go back where they were). `false` when no segment is held.
pub fn insert_held_vertex(shell: &mut Shell, document: Uuid) -> bool {
    let Some(tab) = shell.document_tab_mut(document) else {
        return false;
    };
    let Interaction::Handle(drag) = &mut tab.interaction else {
        return false;
    };
    let Grip::Segment(i) = drag.grip else {
        return false;
    };
    let at = drag.at;
    drag.original.insert(i + 1, at);
    drag.grip = Grip::Vertex(i + 1);
    drag.anchor = at;
    drag.lead = (0.0, 0.0);
    drag.vertices = drag.original.clone();
    true
}

/// The handles drawn while `drag` is in flight: the Wire's, where they are now.
pub(super) fn wire_handles_of(drag: &HandleDrag, out: &mut Vec<Handle>) {
    wire_handles(drag.wire, &drag.vertices, drag.red, out);
}
