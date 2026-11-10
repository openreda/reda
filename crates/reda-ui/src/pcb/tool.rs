//! The PCB select-and-place Tool: what pointer presses, drags and the Space / L keys do
//! on the board.
//!
//! - A click picks from the stack under it (`selection.set`, see `selection::picking`);
//!   Shift+click toggles it; a click on nothing clears the selection unless Click Clears
//!   Selection is off.
//! - Dragging a Footprint, Free Pad or the Output Origin moves it (with the rest of the
//!   selection when it is selected). The move is drawn from a preview of the board
//!   (`reda_pcb::placement_preview`) and runs nothing until release, when it becomes one
//!   Command (`pcb.move`), or one `batch` when it was also turned or flipped in flight.
//! - Dragging routed copper (a Track segment, end or corner, a midpoint handle, a Via or a
//!   lone Free Pad) runs the drag gestures of [`super::copper_drag`]: drawn from the same
//!   plan as the drag Command and run as that one Command on release.
//! - Dragging from nothing draws a rubber band: left to right (as seen on screen) it
//!   selects what it encloses, right to left what it touches, unless Directional
//!   Rectangle is off.
//! - Space turns by 90° and L flips to the other side: in flight they join the move,
//!   otherwise they run `pcb.rotate` / `pcb.flip` on the selection.
//! - Moves snap the grabbed point (a Hotspot of the moved objects near the grab, else
//!   the object's position) by the shared [`crate::snap`] model: to the other objects'
//!   Hotspots, else to the snap grid.
//!
//! - A Footprint move (docs/spec/v0.1/pcb.md#moving-footprints) also snaps across the
//!   alignment guides to its neighbours' Pad centres and Courtyard edges, stops short of
//!   `courtyard_clearance` in `avoid`, shows the Courtyards it comes too close to, and
//!   draws the Ratsnest by `pcb.place.ratsnest_mode` (see [`super::placing`]).
//! - Reposition Selected brings queued Footprints to the cursor one at a time; with Move
//!   active a click picks a Footprint up, or on empty space opens Choose Component.
//!
//! Points are board nm. The Tool talks to the Shell only through its public API, so the
//! canvas and the tests drive it the same way.

use reda_engine::model::geom::{Millideg, Nm, Point};
use reda_engine::model::{Object, ObjectType, Project, Uuid};
use reda_engine::pcb::place::{self, ObstacleMode, Overlap, RatsnestMode};
use reda_engine::pcb::{
    Copper, Item, PlacementOp, placement_anchor, placement_preview,
    placement_preview_including_locked,
};
use reda_engine::render::{BoardColor, ColorKey, Primitive, SceneLayer};
use serde_json::{Value, json};

use super::copper_drag::{self, CopperDrag, DragMode};
use super::dimension_edit::{self, DimensionEdit};
use super::polygon_edit::{self, Grab, PolygonEdit};
use crate::shell::Shell;
use crate::snap::{self, Guide, Hit, Snapped, Target};
use crate::tools::Tool;

/// The Tool's state between events.
#[derive(Debug, Default)]
pub struct SelectTool {
    state: State,
    /// Where the cursor last snapped, for the snap marker.
    snap: Option<Snapped>,
    modifiers: egui::Modifiers,
    tolerance: Nm,
    dragged: bool,
    /// Where the pointer last was on the board.
    cursor: Option<Point>,
    /// Reposition Selected: the Footprints queued and the one on the cursor.
    queue: Option<Queue>,
    /// The alignment guides the move in flight snapped across.
    guide_lines: Vec<Guide>,
}

#[derive(Debug)]
struct Queue {
    footprints: Vec<Uuid>,
    index: usize,
}

#[derive(Debug, Default)]
enum State {
    #[default]
    Idle,
    /// A button is down and has not moved yet.
    Pressed {
        at: Point,
        hit: Option<Uuid>,
        toggle: bool,
    },
    Band {
        from: Point,
        to: Point,
    },
    Moving(Move),
    CopperDrag(CopperDrag),
    /// A press on the selected polygon's handles or edges (see [`super::polygon_edit`]).
    Polygon(PolygonPress),
    /// A press on the selected Dimension's handles (see [`super::dimension_edit`]).
    Dimension {
        edit: Box<DimensionEdit>,
        at: Point,
        dragged: bool,
    },
    /// The press can do nothing more until release (a refused move, a cancel).
    Spent,
    /// A press while Track Properties waits for a Net pick: release picks there.
    Picking {
        at: Point,
    },
}

#[derive(Debug)]
struct Move {
    document: Uuid,
    objects: Vec<Uuid>,
    /// The grab minus the reference point that snaps.
    offset: Point,
    /// Where the reference point is after `ops`.
    base: Point,
    /// Where the reference point is now (snapped).
    current: Point,
    /// The ops so far, before the final translation to `current`.
    ops: Vec<PlacementOp>,
    /// What the reference point can snap to: everything but the moved objects.
    targets: Vec<Target>,
    /// Picked up without a button held (Move Board Shape, Move, the queue): the pointer
    /// carries it until a click puts it down.
    carried: bool,
    /// The press that picked a carried move up is still down: its release keeps it.
    fresh: bool,
    /// Move Board: the click moves the whole board (`pcb.board.move`).
    whole_board: bool,
    /// The Footprints among `objects`, which guides, `avoid`, the live overlap and the
    /// Ratsnest mode work on.
    footprints: Vec<Uuid>,
    /// Alignment guides for the reference point.
    guides: Vec<Guide>,
    /// Pairs closer than `courtyard_clearance` where the move now stands.
    overlaps: Vec<Overlap>,
    /// The outlines of the Footprints in `overlaps`, where they now stand.
    overlap_outlines: Vec<Vec<Point>>,
}

impl Move {
    /// Every op, the move to the cursor included.
    fn all_ops(&self) -> Vec<PlacementOp> {
        let mut ops = self.ops.clone();
        let by = self.current - self.base;
        if by != Point::new(Nm(0), Nm(0)) {
            ops.push(PlacementOp::Translate(by));
        }
        ops
    }

    /// Ends the move so far at the current point, so a turn or flip can follow.
    fn settle(&mut self) {
        self.ops = self.all_ops();
        self.base = self.current;
    }
}

/// A press on the selected polygon: a reshape once it drags.
#[derive(Debug)]
struct PolygonPress {
    edit: PolygonEdit,
    at: Point,
    /// `Mod` on an edge: a click inserts a vertex.
    insert: bool,
    dragged: bool,
}

/// The kinds of object the Tool can move.
fn movable(kind: ObjectType) -> bool {
    matches!(
        kind,
        ObjectType::FootprintInstance | ObjectType::FreePad | ObjectType::OutputOrigin
    )
}

/// The objects Move Board Shape picks up: the Board Outline and the selected Board
/// Cutouts.
fn board_shape(shell: &Shell) -> Vec<Uuid> {
    let Some(pcb) = shell.engine().project().and_then(|p| p.pcb()) else {
        return Vec::new();
    };
    let Some(outline) = pcb.outline() else {
        return Vec::new();
    };
    let mut objects = vec![outline.uuid];
    objects.extend(
        shell
            .selection()
            .objects
            .iter()
            .copied()
            .filter(|o| pcb.cutout(*o).is_some()),
    );
    objects
}

/// The PCB Document and the board's project, when one is open.
fn board(shell: &Shell) -> Option<(&Project, Uuid)> {
    let project = shell.engine().project()?;
    Some((project, project.pcb()?.uuid))
}

impl SelectTool {
    /// Where the cursor last snapped to, while a move is in flight or after a hover.
    pub fn snap(&self) -> Option<Snapped> {
        self.snap
    }

    /// The rubber band's corners while one is drawn.
    pub fn band(&self) -> Option<(Point, Point)> {
        match self.state {
            State::Band { from, to } => Some((from, to)),
            _ => None,
        }
    }

    /// Whether a move is in flight.
    pub fn moving(&self) -> bool {
        matches!(self.state, State::Moving(_))
    }

    /// The objects the move in flight carries; `None` with no move in flight.
    pub fn carried(&self) -> Option<&[Uuid]> {
        match &self.state {
            State::Moving(m) => Some(&m.objects),
            _ => None,
        }
    }

    /// Whether the move in flight carries Footprints.
    pub fn carries_footprints(&self, _shell: &Shell) -> bool {
        matches!(&self.state, State::Moving(m) if !m.footprints.is_empty())
    }

    /// Reposition Selected: the Footprint on the cursor (1-based) and how many are queued.
    pub fn queue(&self) -> Option<(usize, usize)> {
        self.queue
            .as_ref()
            .map(|q| (q.index + 1, q.footprints.len()))
    }

    /// The pairs of Footprints closer than `courtyard_clearance` where the move now stands
    /// (the live overlap), with the outlines to draw in the violation colour.
    pub fn overlaps(&self) -> (&[Overlap], &[Vec<Point>]) {
        match &self.state {
            State::Moving(m) => (&m.overlaps, &m.overlap_outlines),
            _ => (&[], &[]),
        }
    }

    /// The alignment guides the move in flight is snapped across.
    pub fn guide_lines(&self) -> &[Guide] {
        &self.guide_lines
    }

    /// Draws the move in flight anew, after a placement mode changed.
    pub fn refresh(&mut self, shell: &mut Shell) {
        if let State::Moving(mut m) = std::mem::take(&mut self.state) {
            m.guides = guides(shell, &m, None);
            let to = self.cursor.unwrap_or(m.current + m.offset);
            self.track(shell, &mut m, to, true);
            self.state = State::Moving(m);
        }
    }

    /// Reposition Selected: queues `footprints` and brings the first to the cursor.
    pub fn start_queue(&mut self, shell: &mut Shell, footprints: Vec<Uuid>) {
        self.cancel(shell);
        self.state = State::Idle;
        self.queue = Some(Queue {
            footprints,
            index: 0,
        });
        self.bring(shell);
    }

    /// Tab during Reposition Selected: leaves the Footprint on the cursor where it was and
    /// brings the next.
    pub fn skip(&mut self, shell: &mut Shell) {
        if self.queue.is_none() {
            return;
        }
        if let State::Moving(m) = std::mem::take(&mut self.state) {
            shell.redraw(m.document);
        }
        self.next_in_queue(shell);
    }

    fn next_in_queue(&mut self, shell: &mut Shell) {
        if let Some(q) = &mut self.queue {
            q.index += 1;
        }
        self.bring(shell);
    }

    /// Carries the queue's current Footprint at the cursor; ends the queue past its last.
    fn bring(&mut self, shell: &mut Shell) {
        loop {
            let Some(q) = &self.queue else {
                return;
            };
            let Some(&footprint) = q.footprints.get(q.index) else {
                self.queue = None;
                return;
            };
            if self.carry(shell, footprint, false) {
                return;
            }
            if let Some(q) = &mut self.queue {
                q.index += 1;
            }
        }
    }

    /// Choose Component's buttons: floats `footprint` on the cursor, from where it stands
    /// (`jump`) or moved to the cursor.
    pub fn pick_up(&mut self, shell: &mut Shell, footprint: Uuid, jump: bool) {
        self.cancel(shell);
        self.state = State::Idle;
        self.queue = None;
        if jump {
            self.cursor = board(shell)
                .and_then(|(p, _)| p.pcb())
                .and_then(|pcb| pcb.footprint(footprint))
                .map(|f| f.at);
        }
        self.carry(shell, footprint, false);
    }

    /// Picks up `footprint` by its origin, carried until a click puts it down, at the
    /// cursor. Returns whether it is in flight.
    fn carry(&mut self, shell: &mut Shell, footprint: Uuid, fresh: bool) -> bool {
        let Some((project, document)) = board(shell) else {
            return false;
        };
        let pcb = project.pcb().expect("a board");
        let Some(at) = pcb.footprint(footprint).map(|f| f.at) else {
            return false;
        };
        let targets = snap::hotspots::without(snap::hotspots::board(pcb), &[footprint]);
        let mut m = Move {
            document,
            objects: vec![footprint],
            offset: Point::default(),
            base: at,
            current: at,
            ops: Vec::new(),
            targets,
            carried: true,
            fresh,
            whole_board: false,
            footprints: vec![footprint],
            guides: Vec::new(),
            overlaps: Vec::new(),
            overlap_outlines: Vec::new(),
        };
        m.guides = guides(shell, &m, None);
        let to = self.cursor.unwrap_or(at);
        self.track(shell, &mut m, to, true);
        self.state = State::Moving(m);
        true
    }

    /// Move Board Shape: picks up the Board Outline (with the selected Board Cutouts) by
    /// the middle of the outline's box, which then follows the pointer until a click puts
    /// it down.
    ///
    /// With `whole` (Move Board) every Board Cutout comes too, turns are left out, and
    /// the click runs `pcb.board.move`, which moves everything on the board.
    pub fn carry_board_shape(&mut self, shell: &mut Shell, whole: bool) {
        let mut objects = board_shape(shell);
        if whole && let Some(pcb) = shell.engine().project().and_then(|p| p.pcb()) {
            objects.truncate(1);
            objects.extend(pcb.cutouts().map(|c| c.uuid));
        }
        let Some((project, document)) = board(shell) else {
            return;
        };
        let Some(pcb) = project.pcb() else {
            return;
        };
        if objects.is_empty() {
            return;
        }
        let Ok(middle) = placement_anchor(pcb, &objects[..1]) else {
            return;
        };
        let at = middle;
        let targets = snap::hotspots::without(snap::hotspots::board(pcb), &objects);
        self.state = State::Moving(Move {
            document,
            objects,
            offset: Point::default(),
            base: at,
            current: at,
            ops: Vec::new(),
            targets,
            carried: true,
            fresh: false,
            whole_board: whole,
            footprints: Vec::new(),
            guides: Vec::new(),
            overlaps: Vec::new(),
            overlap_outlines: Vec::new(),
        });
        let State::Moving(m) = &mut self.state else {
            unreachable!("just set");
        };
        show(shell, m);
    }

    /// The cursor is at `at` with no button down: snaps it for the marker, and moves what
    /// it carries.
    pub fn hover_mut(&mut self, shell: &mut Shell, at: Point) -> Snapped {
        self.cursor = Some(at);
        if let State::Moving(mut m) = std::mem::take(&mut self.state) {
            self.track(shell, &mut m, at, false);
            self.state = State::Moving(m);
            return self.snap.expect("tracking snaps");
        }
        self.hover(shell, at)
    }

    /// The cursor is at `at` with no button down: snaps it for the marker.
    pub fn hover(&mut self, shell: &Shell, at: Point) -> Snapped {
        let targets = board_hotspots(shell);
        let snapped = snap_board(shell, at, &targets);
        self.snap = Some(snapped);
        snapped
    }

    /// A button went down at `at`; `tolerance` is how near counts as on an object.
    pub fn press(&mut self, shell: &Shell, at: Point, modifiers: egui::Modifiers, tolerance: Nm) {
        if matches!(&self.state, State::Moving(m) if m.carried) {
            // The click that puts a carried move down: release places it.
            return;
        }
        self.modifiers = modifiers;
        self.tolerance = tolerance;
        self.dragged = false;
        self.cursor = Some(at);
        if !shell.net_pick().is_empty() {
            // The Net row's canvas pick takes this press.
            self.state = State::Picking { at };
            return;
        }
        if shell.tool() == Tool::Select
            && let Some(edit) = dimension_edit::grab_at(shell, at, tolerance)
        {
            self.state = State::Dimension {
                edit: Box::new(edit),
                at,
                dragged: false,
            };
            return;
        }
        if shell.tool() == Tool::Select
            && let Some((object, vertices)) = polygon_edit::selected(shell)
            && let Some(grab) = polygon_edit::grab_at(&vertices, at, tolerance)
            && let Some((_, document)) = board(shell)
        {
            let insert = modifiers.command && !matches!(grab, Grab::Corner(_));
            let edit = polygon_edit::start(shell, document, object, vertices, grab, at);
            self.state = State::Polygon(PolygonPress {
                edit,
                at,
                insert,
                dragged: false,
            });
            return;
        }
        let hit = board(shell).and_then(|(_, pcb)| {
            crate::selection::picking::hit(shell, pcb, xy(at), tolerance.0 as f64)
        });
        if shell.tool() == Tool::Move {
            self.state = State::Pressed {
                at,
                hit,
                toggle: modifiers.command,
            };
            return;
        }
        self.state = State::Pressed {
            at,
            hit,
            toggle: modifiers.command,
        };
    }

    /// The pointer moved to `to` with the button down.
    pub fn drag(&mut self, shell: &mut Shell, to: Point) {
        self.dragged = true;
        self.cursor = Some(to);
        if shell.tool() == Tool::Move
            && let State::Pressed { at, hit, .. } = self.state
        {
            self.state = State::Spent;
            self.move_tool_pick(shell, at, hit);
            if let State::Moving(mut m) = std::mem::take(&mut self.state) {
                self.track(shell, &mut m, to, false);
                self.state = State::Moving(m);
            }
            return;
        }
        match std::mem::take(&mut self.state) {
            State::Pressed { at, hit, toggle } => match hit.map(|id| {
                board(shell)
                    .and_then(|(_, doc)| crate::selection::rules::child(shell, doc, id))
                    .map_or(id, |c| c.owner())
            }) {
                None => self.state = State::Band { from: at, to },
                Some(object) => match self.copper_grab(shell, object, at) {
                    Some(grab) => self.start_copper_drag(shell, at, object, grab, to),
                    None => self.start_move(shell, at, object, toggle, to),
                },
            },
            State::CopperDrag(mut d) => {
                d.follow(shell, to);
                self.state = State::CopperDrag(d);
            }
            State::Polygon(mut p) => {
                p.dragged = true;
                p.edit.follow(shell, to);
                self.guide_lines = p.edit.guide_lines().to_vec();
                self.snap = p.edit.snap().or(self.snap);
                self.state = State::Polygon(p);
            }
            State::Dimension { mut edit, at, .. } => {
                self.snap = Some(edit.follow(shell, to));
                self.state = State::Dimension {
                    edit,
                    at,
                    dragged: true,
                };
            }
            State::Band { from, .. } => self.state = State::Band { from, to },
            State::Moving(mut m) => {
                self.track(shell, &mut m, to, false);
                self.state = State::Moving(m);
            }
            other => self.state = other,
        }
    }

    /// The button came up.
    pub fn release(&mut self, shell: &mut Shell) {
        match std::mem::take(&mut self.state) {
            State::Pressed { at, hit, .. } if shell.tool() == Tool::Move => {
                self.move_tool_pick(shell, at, hit);
                if let State::Moving(m) = &mut self.state {
                    m.fresh = false;
                }
            }
            State::Pressed { at, hit, .. } => {
                let Some((_, document)) = board(shell) else {
                    return;
                };
                if self.dragged {
                    // Dragging what cannot move just selects it, as a drag selects first.
                    let select = hit.and_then(|hit| {
                        crate::selection::picking::drag_start(
                            shell,
                            document,
                            hit,
                            self.modifiers,
                            |_| false,
                        )
                        .select
                    });
                    if let Some((id, params)) = select {
                        let _ = shell.run_reporting(&id, params);
                    }
                } else if (shell.modifier_roles().net_highlight_board).held(&self.modifiers) {
                    self.highlight_net(shell, document, hit);
                } else {
                    self.click(shell, document, at);
                }
            }
            State::Polygon(p) => self.release_polygon(shell, p),
            State::Dimension { edit, at, dragged } => {
                if !dragged {
                    self.click(shell, edit.document, at);
                } else {
                    let ran = edit
                        .command()
                        .is_some_and(|(id, params)| shell.run_reporting(id, params).is_ok());
                    if !ran {
                        shell.redraw(edit.document);
                    }
                }
            }
            State::Band { from, to } => {
                let Some((_, document)) = board(shell) else {
                    return;
                };
                if let Some((id, params)) = crate::selection::picking::box_selection(
                    shell,
                    document,
                    xy(from),
                    xy(to),
                    self.modifiers,
                ) {
                    let _ = shell.run_reporting(&id, params);
                }
            }
            State::Moving(mut m) if m.fresh => {
                m.fresh = false;
                self.state = State::Moving(m);
            }
            State::Moving(m) if m.whole_board => {
                self.snap = None;
                let by = m.current - m.base;
                let params = json!({"dx": by.x.0, "dy": by.y.0});
                if by == Point::default() || shell.run_reporting("pcb.board.move", params).is_err()
                {
                    shell.redraw(m.document);
                }
            }
            State::Moving(m) => {
                self.snap = None;
                self.guide_lines.clear();
                let ops = m.all_ops();
                let steps: Vec<(String, Value)> = ops
                    .iter()
                    .map(|op| {
                        let s = step(&m.objects, *op);
                        (
                            s["id"].as_str().expect("a step id").to_owned(),
                            s["params"].clone(),
                        )
                    })
                    .collect();
                if !super::placing::commit(shell, steps) {
                    shell.redraw(m.document);
                }
                if self.queue.is_some() {
                    self.next_in_queue(shell);
                } else if shell.tool() == Tool::Move {
                    let _ = shell.run_reporting("tool.activate", json!({"tool": "select"}));
                }
            }
            State::CopperDrag(d) => {
                shell.end_tool();
                let ran = d
                    .command(shell)
                    .is_some_and(|(id, params)| shell.run_reporting(id, params).is_ok());
                if !ran {
                    shell.redraw(d.document);
                }
            }
            State::Picking { at } => {
                let tracks = shell.net_pick().to_vec();
                let net = shell
                    .engine()
                    .project()
                    .and_then(|p| crate::properties::track::net_at(p, &tracks, at));
                let _ = shell.run_reporting("view.properties.pick_net", json!({"tracks": []}));
                match net {
                    Some(net) => {
                        let params = json!({"tracks": tracks, "net": net});
                        let _ = shell.run_reporting("pcb.track.set", params);
                    }
                    None => shell.report(crate::tr!("properties-pick-net-none")),
                }
            }
            State::Idle | State::Spent => {}
        }
    }

    /// A click at `at` on `document`: picks from the stack there.
    fn click(&mut self, shell: &mut Shell, document: Uuid, at: Point) {
        if let Some((id, params)) = crate::selection::picking::click(
            shell,
            document,
            xy(at),
            self.tolerance.0 as f64,
            self.modifiers,
        ) {
            let _ = shell.run_reporting(&id, params);
        }
    }

    /// The release ending press `p` on the selected polygon: a click picks (or with `Mod`
    /// on an edge inserts a vertex); a drag runs its one Command, or puts back a shape
    /// that crosses itself.
    fn release_polygon(&mut self, shell: &mut Shell, p: PolygonPress) {
        let document = p.edit.document;
        self.guide_lines.clear();
        if !p.dragged {
            if p.insert
                && let Grab::Edge(i) | Grab::Midpoint(i) = p.edit.grab()
            {
                let (id, params) =
                    polygon_edit::insert(p.edit.object(), p.edit.vertices(), i, p.at);
                let _ = shell.run_reporting(id, params);
            } else {
                self.click(shell, document, p.at);
            }
            return;
        }
        match p.edit.command() {
            Some((id, params)) => {
                if shell.run_reporting(id, params).is_err() {
                    shell.redraw(document);
                }
            }
            None => {
                shell.redraw(document);
                if !p.edit.valid() {
                    shell.report(crate::tr!("pcb-polygon-reverted"));
                }
            }
        }
    }

    /// The Dimension handle drag in flight, once the press has dragged.
    pub fn dimension_edit(&self) -> Option<&DimensionEdit> {
        match &self.state {
            State::Dimension {
                edit,
                dragged: true,
                ..
            } => Some(edit.as_ref()),
            _ => None,
        }
    }

    /// The polygon reshape in flight, once the press has dragged.
    pub fn polygon_edit(&self) -> Option<&PolygonEdit> {
        match &self.state {
            State::Polygon(p) if p.dragged => Some(&p.edit),
            _ => None,
        }
    }

    /// Del while holding a polygon's vertex: removes it (one Command) and ends the drag.
    /// Returns whether a vertex was held.
    pub fn delete_held_vertex(&mut self, shell: &mut Shell) -> bool {
        let State::Polygon(p) = &self.state else {
            return false;
        };
        if !p.edit.holds_vertex() {
            return false;
        }
        let State::Polygon(p) = std::mem::replace(&mut self.state, State::Spent) else {
            unreachable!("checked above");
        };
        self.guide_lines.clear();
        shell.redraw(p.edit.document);
        match p.edit.without_held() {
            Some((id, params)) => {
                let _ = shell.run_reporting(id, params);
            }
            None => shell.report(crate::tr!("pcb-polygon-too-few")),
        }
        true
    }

    /// Esc: drops a move or rubber band in flight, leaving the board as it was.
    pub fn cancel(&mut self, shell: &mut Shell) {
        if !shell.net_pick().is_empty() {
            let _ = shell.run_reporting("view.properties.pick_net", json!({"tracks": []}));
        }
        if let State::Moving(m) = &self.state {
            shell.redraw(m.document);
        }
        if let State::Polygon(p) = &self.state {
            shell.redraw(p.edit.document);
        }
        if let State::Dimension { edit, .. } = &self.state {
            shell.redraw(edit.document);
        }
        if let State::CopperDrag(d) = &self.state {
            shell.end_tool();
            shell.redraw(d.document);
        }
        if !matches!(self.state, State::Idle) {
            self.state = State::Spent;
        }
        self.snap = None;
        self.queue = None;
        self.guide_lines.clear();
    }

    /// Space: turns the move in flight by 90°, or else the selection (`pcb.rotate`).
    pub fn rotate(&mut self, shell: &mut Shell) {
        if let State::Polygon(p) = &mut self.state {
            // Space while reshaping a polygon cycles the handle's modes.
            if p.dragged {
                p.edit.cycle(shell);
                self.guide_lines = p.edit.guide_lines().to_vec();
            }
            return;
        }
        self.turn(shell, |at| PlacementOp::Rotate(at, Millideg::from_deg(90)));
    }

    /// L: flips the move in flight to the other side, or else the selection (`pcb.flip`).
    pub fn flip(&mut self, shell: &mut Shell) {
        self.turn(shell, |at| PlacementOp::Flip(at.x));
    }

    /// Adds `op` (made for the point it turns about) to the move in flight, or runs it on
    /// the selection about its anchor.
    fn turn(&mut self, shell: &mut Shell, op: impl Fn(Point) -> PlacementOp) {
        match &mut self.state {
            // `pcb.board.move` only translates.
            State::Moving(m) if m.whole_board => {}
            State::Moving(m) => {
                m.settle();
                m.ops.push(op(m.current));
                let ops = m.all_ops();
                let turned = applied(shell, &m.objects, &ops);
                m.guides = guides(shell, m, turned.as_ref());
                show(shell, m);
            }
            _ => {
                let objects = placeable_selection(shell);
                if objects.is_empty() {
                    return;
                }
                let Some(anchor) = board(shell)
                    .and_then(|(p, _)| p.pcb())
                    .and_then(|pcb| placement_anchor(pcb, &objects).ok())
                else {
                    return;
                };
                let s = step(&objects, op(anchor));
                let id = s["id"].as_str().expect("a step id").to_owned();
                super::placing::commit(shell, vec![(id, s["params"].clone())]);
            }
        }
    }

    fn start_move(&mut self, shell: &mut Shell, at: Point, object: Uuid, toggle: bool, to: Point) {
        let Some((project, document)) = board(shell) else {
            return;
        };
        let pcb = project.pcb().expect("a board");
        let kind = project.object(object).map(|o| o.object_type());
        let drag =
            crate::selection::picking::drag_start(shell, document, object, self.modifiers, |id| {
                project.object(id).is_some_and(|x| movable(x.object_type()))
            });
        let objects = drag.objects;
        if !kind.is_some_and(movable) || objects.is_empty() {
            // Dragging what cannot move just selects it.
            self.state = State::Pressed {
                at,
                hit: Some(object),
                toggle,
            };
            return;
        }
        let all = snap::hotspots::board(pcb);
        let own: Vec<Target> = all
            .iter()
            .filter(|t| objects.contains(&t.object))
            .copied()
            .collect();
        let others: Vec<Target> = all
            .into_iter()
            .filter(|t| !objects.contains(&t.object))
            .collect();
        let capture = shell.preferences().pcb.snap_distance.nm();
        let reference = snap::nearest(&own, at, capture)
            .map(|t| t.at())
            .or_else(|| match project.object(object) {
                Some(Object::FootprintInstance(f)) => Some(f.object.at),
                Some(Object::FreePad(p)) => Some(p.object.at),
                Some(Object::OutputOrigin(o)) => Some(o.object.at),
                _ => None,
            })
            .unwrap_or(at);
        let protect = shell.preferences().editing.protect_locked_objects;
        let failed = if protect {
            placement_preview(pcb, &objects, &[]).err()
        } else {
            placement_preview_including_locked(pcb, &objects, &[]).err()
        };
        let footprints: Vec<Uuid> = objects
            .iter()
            .copied()
            .filter(|o| pcb.footprint(*o).is_some())
            .collect();
        if let Some((id, params)) = drag.select {
            let _ = shell.run_reporting(&id, params);
        }
        if let Some(e) = failed {
            shell.report(crate::tr!("error-command", error = e.message));
            self.state = State::Spent;
            return;
        }
        let mut m = Move {
            document,
            objects,
            offset: at - reference,
            base: reference,
            current: reference,
            ops: Vec::new(),
            targets: others,
            carried: false,
            fresh: false,
            whole_board: false,
            footprints,
            guides: Vec::new(),
            overlaps: Vec::new(),
            overlap_outlines: Vec::new(),
        };
        m.guides = guides(shell, &m, None);
        self.track(shell, &mut m, to, true);
        self.state = State::Moving(m);
    }

    /// The Move Tool's press at `at` on `hit`: picks up what it hits (with the selection
    /// when it is selected), carried until the next click; on empty space it opens Choose
    /// Component.
    fn move_tool_pick(&mut self, shell: &mut Shell, at: Point, hit: Option<Uuid>) {
        let owner = hit.map(|id| {
            board(shell)
                .and_then(|(_, doc)| crate::selection::rules::child(shell, doc, id))
                .map_or(id, |c| c.owner())
        });
        let movable_hit = owner.filter(|o| {
            board(shell)
                .and_then(|(p, _)| p.object(*o))
                .is_some_and(|x| movable(x.object_type()))
                && board(shell)
                    .is_some_and(|(_, doc)| crate::selection::rules::pickable(shell, doc, *o))
        });
        let Some(object) = movable_hit else {
            self.state = State::Spent;
            let _ = shell.run_reporting("pcb.place.choose", json!({}));
            return;
        };
        self.start_move(shell, at, object, false, at);
        if let State::Moving(m) = &mut self.state {
            m.carried = true;
            m.fresh = true;
        }
    }

    /// Follows the cursor at `to` with the move `m`, redrawing the preview when it changed
    /// (or when `force`d). The reference point snaps to Hotspots, the alignment guides and
    /// the grid; in `avoid` a Footprint-only move stops at the last clear point.
    fn track(&mut self, shell: &mut Shell, m: &mut Move, to: Point, force: bool) {
        let Some((project, _)) = board(shell) else {
            return;
        };
        let mut rules = snap::rules(shell, m.document);
        rules.guides &= shell.preferences().pcb.place.alignment_guides;
        let snapped = snap::snap(xy(to - m.offset), &m.targets, &m.guides, &rules);
        let mut current = snapped.at;
        let avoid = shell.preferences().pcb.place.obstacle_mode == ObstacleMode::Avoid;
        if avoid && !m.footprints.is_empty() && m.footprints.len() == m.objects.len() {
            let settled = (!m.ops.is_empty())
                .then(|| applied(shell, &m.objects, &m.ops))
                .flatten();
            let pcb = settled
                .as_ref()
                .and_then(|p| p.pcb())
                .or_else(|| project.pcb());
            if let Some(pcb) = pcb {
                current = m.base + place::avoid(pcb, &m.footprints, current - m.base);
            }
        }
        self.guide_lines = if snapped.hit == Hit::Guide {
            m.guides
                .iter()
                .copied()
                .filter(|g| match g {
                    Guide::X(x) => *x == snapped.at.x,
                    Guide::Y(y) => *y == snapped.at.y,
                })
                .collect()
        } else {
            Vec::new()
        };
        self.snap = Some(snapped);
        if force || current != m.current {
            m.current = current;
            show(shell, m);
        }
    }
}

impl SelectTool {
    /// The copper drag gesture a press at `at` on `object` starts, if any.
    fn copper_grab(&self, shell: &Shell, object: Uuid, at: Point) -> Option<copper_drag::Grab> {
        if shell.tool() != Tool::Select {
            return None;
        }
        let pcb = board(shell)?.0.pcb()?;
        copper_drag::grab_at(shell, pcb, object, at, self.modifiers, self.tolerance)
    }

    fn start_copper_drag(
        &mut self,
        shell: &mut Shell,
        at: Point,
        target: Uuid,
        grab: copper_drag::Grab,
        to: Point,
    ) {
        let Some((_, document)) = board(shell) else {
            return;
        };
        let detach = self.modifiers.command && !self.modifiers.shift;
        let Some(mut d) = CopperDrag::start(shell, document, target, grab, at, detach) else {
            self.state = State::Spent;
            return;
        };
        d.follow(shell, to);
        self.state = State::CopperDrag(d);
    }

    /// The copper drag in flight.
    pub fn copper_drag(&self) -> Option<&CopperDrag> {
        match &self.state {
            State::CopperDrag(d) => Some(d),
            _ => None,
        }
    }

    /// A drag key (`pcb.drag.mode`): flips the option on the copper drag in flight, or
    /// steps the obstacle mode shared with routing.
    pub fn drag_mode(&mut self, shell: &mut Shell, mode: DragMode) {
        let State::CopperDrag(mut d) = std::mem::take(&mut self.state) else {
            return;
        };
        if mode == DragMode::ObstacleMode {
            let next = shell.preferences().pcb.route.obstacle_mode.next();
            let _ = shell.run_reporting(
                "app.preferences.set",
                json!({"key": "pcb.route.obstacle_mode", "value": next}),
            );
            d.refresh(shell, self.cursor);
        } else {
            d.change_mode(shell, mode);
        }
        self.state = State::CopperDrag(d);
    }

    /// A `Mod`+click (the `net_highlight_board` role) without movement: highlights the
    /// Net of what is under the pointer, or with Shift adds it; on empty space it clears
    /// the highlight. A `Mod`+drag still detaches, and `Mod`+Shift+drag breaks a Track.
    fn highlight_net(&self, shell: &mut Shell, document: Uuid, hit: Option<Uuid>) {
        let add = self.modifiers.shift;
        let Some(hit) = hit else {
            if !add {
                let _ = shell.run_reporting("view.highlight.clear", json!({}));
            }
            return;
        };
        let params = match crate::selection::rules::child(shell, document, hit) {
            Some(reda_engine::render::Child::Pad { footprint, pad }) => {
                json!({"objects": [], "pads": [{"footprint": footprint, "pad": pad}], "add": add})
            }
            _ => json!({"objects": [hit], "add": add}),
        };
        // An object on no Net (a Footprint's body, the outline) highlights nothing.
        let _ = shell.run("view.highlight.net_of", params);
    }
}

/// `p` as the shared canvas functions take it: nm, y up.
fn xy(p: Point) -> (f64, f64) {
    (p.x.0 as f64, p.y.0 as f64)
}

/// Every Hotspot of the open board.
pub(crate) fn board_hotspots(shell: &Shell) -> Vec<Target> {
    board(shell)
        .and_then(|(p, _)| p.pcb())
        .map(snap::hotspots::board)
        .unwrap_or_default()
}

/// `at` snapped on the board by its Rules onto `targets`.
pub(crate) fn snap_board(shell: &Shell, at: Point, targets: &[Target]) -> Snapped {
    let xy = (at.x.0 as f64, at.y.0 as f64);
    match board(shell) {
        Some((_, document)) => snap::snap_at(shell, document, xy, targets),
        None => snap::snap(
            xy,
            &[],
            &[],
            &snap::Rules::grid_only(Some(snap::Grid {
                pitch: crate::grid::pcb_default(shell.board_view().units),
                origin: Point::default(),
            })),
        ),
    }
}

/// `at` on the board's grid level alone.
pub(crate) fn board_to_grid(shell: &Shell, at: Point) -> Point {
    let xy = (at.x.0 as f64, at.y.0 as f64);
    match board(shell) {
        Some((_, document)) => snap::to_grid(shell, document, xy),
        None => snap_board(shell, at, &[]).at,
    }
}

/// The selected objects that can be placed.
fn placeable_selection(shell: &Shell) -> Vec<Uuid> {
    let Some(project) = shell.engine().project() else {
        return Vec::new();
    };
    shell
        .selection()
        .objects
        .iter()
        .copied()
        .filter(|o| project.object(*o).is_some_and(|x| movable(x.object_type())))
        .collect()
}

/// The open Project with `objects` after `ops`.
fn applied(shell: &Shell, objects: &[Uuid], ops: &[PlacementOp]) -> Option<Project> {
    let project = shell.engine().project()?;
    // A locked object moves in the preview only when the move will ask first.
    let placed = placement_preview_including_locked(project.pcb()?, objects, ops).ok()?;
    let mut project = project.clone();
    let mut tx = project.transaction();
    for object in placed {
        tx.replace(object).ok()?;
    }
    tx.commit().ok()?;
    Some(project)
}

/// Draws the board with the move `m` where it now stands, with the live overlap and the
/// Ratsnest of a Footprint move.
fn show(shell: &mut Shell, m: &mut Move) {
    let Some(project) = applied(shell, &m.objects, &m.all_ops()) else {
        return;
    };
    shell.show_preview(m.document, &project);
    if m.footprints.is_empty() {
        return;
    }
    let Some(pcb) = project.pcb() else {
        return;
    };
    m.overlaps = place::overlaps(pcb, &m.footprints, Point::default());
    let mut shown: Vec<Uuid> = m.overlaps.iter().flat_map(|o| [o.moved, o.other]).collect();
    shown.sort();
    shown.dedup();
    m.overlap_outlines = shown
        .iter()
        .flat_map(|f| place::selection_outlines(pcb, *f))
        .collect();
    ratsnest_in_move(shell, m.document, &project, &m.footprints);
}

/// The alignment guides of the move `m` held at its current point, from `turned` (the
/// board after its turns) or else the open board. None unless it carries Footprints.
fn guides(shell: &Shell, m: &Move, turned: Option<&Project>) -> Vec<Guide> {
    if m.footprints.is_empty() {
        return Vec::new();
    }
    let Some(pcb) = turned
        .and_then(|p| p.pcb())
        .or_else(|| shell.engine().project().and_then(|p| p.pcb()))
    else {
        return Vec::new();
    };
    let g = place::alignment_guides(pcb, &m.footprints, m.current);
    g.x.into_iter()
        .map(Guide::X)
        .chain(g.y.into_iter().map(Guide::Y))
        .collect()
}

/// During a Footprint move the Ratsnest shows by `pcb.place.ratsnest_mode`: Breaks keeps
/// only the lines with an end on a moved Footprint, Hidden none, and Pad To Pad draws a
/// line from each moved Pad to the nearest Pad on its Net (see [`place::pad_to_pad`]).
fn ratsnest_in_move(shell: &mut Shell, document: Uuid, project: &Project, footprints: &[Uuid]) {
    let mode = shell.preferences().pcb.place.ratsnest_mode;
    let smart = shell.preferences().pcb.ratsnest.smart_track_ends;
    let keep: Vec<(Point, Point)> = match mode {
        RatsnestMode::Breaks => {
            let ends = if smart {
                reda_engine::pcb::RatsnestEnds::SmartTrackEnds
            } else {
                reda_engine::pcb::RatsnestEnds::Pads
            };
            let moved = |item: Item| matches!(item, Item::Pad { footprint, .. } if footprints.contains(&footprint));
            Copper::of(project)
                .map(|c| c.ratsnest_with(ends))
                .unwrap_or_default()
                .into_iter()
                .filter(|l| moved(l.from_item) || moved(l.to_item))
                .map(|l| (l.from, l.to))
                .collect()
        }
        RatsnestMode::Hidden | RatsnestMode::PadToPad => Vec::new(),
    };
    let added: Vec<place::PadLine> = match (mode, project.pcb()) {
        (RatsnestMode::PadToPad, Some(pcb)) => place::pad_to_pad(pcb, footprints),
        _ => Vec::new(),
    };
    let Some(tab) = shell.document_tab_mut(document) else {
        return;
    };
    let scene = std::sync::Arc::make_mut(&mut tab.scene.scene);
    scene.items.retain(|item| {
        item.layer != SceneLayer::Ratsnest
            || matches!(item.primitive, Primitive::Hairline { a, b }
                if keep.contains(&(a, b)) || keep.contains(&(b, a)))
    });
    for line in added {
        scene.items.push(reda_engine::render::Item {
            object: line.net,
            child: None,
            layer: SceneLayer::Ratsnest,
            color: ColorKey::Board(BoardColor::Ratsnest),
            primitive: Primitive::Hairline {
                a: line.from,
                b: line.to,
            },
            look: Default::default(),
        });
    }
}

/// The Command (as a `batch` step) that does `op` to `objects`.
fn step(objects: &[Uuid], op: PlacementOp) -> Value {
    let point = |p: Point| json!({"x": p.x, "y": p.y});
    match op {
        PlacementOp::Translate(by) => {
            json!({"id": "pcb.move", "params": {"objects": objects, "by": point(by)}})
        }
        PlacementOp::Rotate(center, angle) => json!({"id": "pcb.rotate", "params": {
            "objects": objects, "angle": angle.0, "center": point(center)
        }}),
        PlacementOp::Flip(x) => json!({"id": "pcb.flip", "params": {
            "objects": objects, "center": {"x": x, "y": 0}
        }}),
    }
}
