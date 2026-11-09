//! The routing Tool (T) and the Via Tool (V): manual single Tracks with 45° or 90°
//! corners, and Vias placed mid-route or on their own for stitching.
//!
//! - A click starts a route on the active copper Layer (else the top copper), on the Net
//!   of the copper under it, as wide as that Net's Class says. Each further click places
//!   the corner path to the cursor; `/` cycles the corner mode, `pcb.route.width` changes
//!   the width mid-route, Backspace takes back the last corner, `pcb.route.add_via` (V)
//!   drops a Via at the cursor and carries on on the other side.
//! - The preview comes from [`route::propose`] in `reda-pcb`, a pure function of the
//!   Copper index: in `stop` (the default `pcb.route.obstacle_mode`) the path ahead is
//!   clipped at its last clear point, and a click places only up to there; nothing
//!   shoves. The route's width starts as [`route::width::start_width`] says.
//! - Reaching copper of the route's own Net ends the route, and so do Enter
//!   (`pcb.route.finish`) and a second click on the last corner. The route then commits
//!   as [`route::commit`] plans it: one `pcb.track.add`, or one `batch` (loop-removal
//!   deletes first) when there is more, so it is one undo step. Nothing runs before that.
//! - The route's preview depends on the copper under its corners: it registers them with
//!   [`Shell::begin_tool`], so an agent's edit to them cancels the route.
//!
//! The Via Tool previews a Via at the cursor, checked the same way, and each click runs
//! `pcb.via.add` there (taking the Net of the copper under it).

use reda_engine::model::geom::{Nm, Point};
use reda_engine::model::{LayerKind, PcbDocument, Project, RuleKind, Side, Uuid};
use reda_engine::pcb::route::{
    self, CornerMode, Posture, Proposed, Request, Status, Step, ViaChoice, ViaSize, WidthChoice,
};
use reda_engine::pcb::rules::net_rules;
use reda_engine::pcb::{ClearanceHit, Copper, Halo, Proposal, keepout};
use reda_engine::preferences::ClearanceDisplay;
use serde_json::json;

use super::route_commands::{ChooserKind, RouteMode};
use crate::shell::{Shell, ToolPreview};
use crate::snap::Snapped;
use crate::tools::Tool;
use crate::tr;

pub use route::Leg;

#[derive(Clone, Debug)]
struct Route {
    net: Option<Uuid>,
    /// Earlier legs and Vias.
    steps: Vec<Step>,
    /// The leg being routed; its last point is where the path ahead starts.
    leg: Leg,
    /// Objects whose copper is under the route's start, left out of the clearance check
    /// while the route has no Net.
    start_objects: Vec<Uuid>,
    /// Objects the preview depends on, for [`ToolPreview`].
    objects: Vec<Uuid>,
}

impl Route {
    fn last(&self) -> Point {
        *self.leg.points.last().expect("a leg has its start")
    }

    fn segments(&self) -> usize {
        self.steps
            .iter()
            .map(|s| match s {
                Step::Leg(l) => l.points.len() - 1,
                Step::Via(..) => 1,
            })
            .sum::<usize>()
            + self.leg.points.len()
            - 1
    }
}

/// The route in flight, as the canvas draws it and tests read it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Preview {
    /// The Layer being routed.
    pub layer: Uuid,
    pub net: Option<Uuid>,
    /// The width being routed.
    pub width: Nm,
    /// The current leg's placed points.
    pub placed: Vec<Point>,
    /// The path ahead, from the last placed point to the cursor.
    pub ahead: Vec<Point>,
    /// Whether the path ahead breaks a rule (clipped in `stop`, violating otherwise).
    pub blocked: bool,
    /// What the next click places, the look-ahead tail and the part out of reach, as
    /// `propose` says.
    pub proposed: Option<Proposed>,
    /// Earlier legs.
    pub legs: Vec<Leg>,
    /// The Vias placed.
    pub vias: Vec<Point>,
}

/// The Via the Via Tool would place at the cursor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ViaCursor {
    pub at: Point,
    pub diameter: Nm,
    pub blocked: bool,
}

/// A chooser open over the canvas, with its rows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Chooser {
    /// Shift+W: the Track widths.
    Width(Vec<WidthChoice>),
    /// Shift+V: the Via sizes.
    Via(Vec<ViaChoice>),
}

/// How far from the cursor the clearance display reaches in `near`.
pub const CLEARANCE_NEAR: Nm = Nm::from_mm(5);

/// The routing and Via Tools' state between events.
#[derive(Debug)]
pub struct Router {
    route: Option<Route>,
    corner: CornerMode,
    posture: Posture,
    /// The cursor, snapped.
    cursor: Option<Snapped>,
    /// What the path ahead (or the Via at the cursor) hits.
    hits: Vec<ClearanceHit>,
    keepouts: Vec<Uuid>,
    via: Option<ViaCursor>,
    /// What `propose` said about the path ahead.
    proposed: Option<Proposed>,
    /// Look-ahead: a click places only the first of the two segments.
    lookahead: bool,
    /// The last width typed or chosen, for the `user` width mode.
    user_width: Option<Nm>,
    /// The Via size chosen with Shift+V; `None` is the Net's.
    via_size: Option<ViaSize>,
    chooser: Option<Chooser>,
    /// What the clearance display shades.
    halos: Vec<Halo>,
    /// The Tool's note, shown above the Routing Bar.
    note: Option<String>,
}

impl Default for Router {
    fn default() -> Router {
        Router {
            route: None,
            corner: CornerMode::default(),
            posture: Posture::default(),
            cursor: None,
            hits: Vec::new(),
            keepouts: Vec::new(),
            via: None,
            proposed: None,
            lookahead: true,
            user_width: None,
            via_size: None,
            chooser: None,
            halos: Vec::new(),
            note: None,
        }
    }
}

/// The title of the route's [`ToolPreview`].
fn title() -> String {
    tr!("tool-route")
}

impl Router {
    /// The corner mode (`/`).
    pub fn corner(&self) -> CornerMode {
        self.corner
    }

    /// Which leg comes first (Space).
    pub fn posture(&self) -> Posture {
        self.posture
    }

    /// Whether look-ahead (`1`) is on.
    pub fn lookahead(&self) -> bool {
        self.lookahead
    }

    /// Whether a route is in flight.
    pub fn routing(&self) -> bool {
        self.route.is_some()
    }

    /// The chooser open over the canvas.
    pub fn chooser(&self) -> Option<&Chooser> {
        self.chooser.as_ref()
    }

    /// Closes the chooser.
    pub fn close_chooser(&mut self) {
        self.chooser = None;
    }

    /// What the clearance display shades now: other-Net copper on the routed Layer,
    /// grown by its clearance.
    pub fn halos(&self) -> &[Halo] {
        &self.halos
    }

    /// The Tool's note, such as why a loop stays.
    pub fn note(&self) -> Option<&str> {
        self.note.as_deref()
    }

    /// Where the cursor last snapped, for the snap marker.
    pub fn snap(&self) -> Option<Snapped> {
        self.cursor
    }

    /// The Via the Via Tool would place at the cursor.
    pub fn via_cursor(&self) -> Option<ViaCursor> {
        self.via
    }

    /// The route in flight.
    pub fn preview(&self) -> Option<Preview> {
        let route = self.route.as_ref()?;
        let ahead = match self.cursor {
            Some(Snapped { at, .. }) => {
                route::corner_path(route.last(), at, self.corner, self.posture)
            }
            None => vec![route.last()],
        };
        Some(Preview {
            layer: route.leg.layer,
            net: route.net,
            width: route.leg.width,
            placed: route.leg.points.clone(),
            ahead,
            blocked: self
                .proposed
                .as_ref()
                .is_some_and(|p| p.status != Status::Clear)
                || !self.hits.is_empty()
                || !self.keepouts.is_empty(),
            proposed: self.proposed.clone(),
            legs: route
                .steps
                .iter()
                .filter_map(|s| match s {
                    Step::Leg(l) => Some(l.clone()),
                    Step::Via(..) => None,
                })
                .collect(),
            vias: route
                .steps
                .iter()
                .filter_map(|s| match s {
                    Step::Via(at, _) => Some(*at),
                    Step::Leg(_) => None,
                })
                .collect(),
        })
    }

    /// The cursor is at `at`: snaps it and checks the path ahead (or the Via) there.
    pub fn hover(&mut self, shell: &mut Shell, at: Point) {
        self.notice_cancel(shell);
        let at = self.snap_to(shell, at);
        self.check(shell, at);
    }

    /// A click at `at`: starts a route, places a corner, or ends the route; with the Via
    /// Tool, places a Via.
    pub fn click(&mut self, shell: &mut Shell, at: Point) {
        self.notice_cancel(shell);
        let at = self.snap_to(shell, at);
        if shell.tool() == Tool::Via {
            self.check(shell, at);
            self.place_via(shell, at);
            return;
        }
        if self.route.is_none() {
            self.start(shell, at);
            return;
        }
        self.check(shell, at);
        let route = self.route.as_ref().expect("routing");
        if at == route.last() {
            if route.segments() > 0 {
                self.finish(shell);
            }
            return;
        }
        let Some(proposed) = self.proposed.clone() else {
            return;
        };
        if !proposed.places() {
            if let Some(why) = self.why_blocked(shell) {
                shell.report(why);
            }
            return;
        }
        let end = *proposed.place.last().expect("placed");
        let route = self.route.as_mut().expect("routing");
        route.leg.points.extend_from_slice(&proposed.place[1..]);
        self.depend_on(shell, end);
        if proposed.ends_route {
            self.finish(shell);
        } else {
            self.check(shell, at);
        }
    }

    /// V while routing: places the path to the cursor, drops a Via there and carries on on
    /// the other side.
    pub fn add_via(&mut self, shell: &mut Shell) {
        self.place_via_here(shell);
    }

    /// E: places the path to the cursor and a Via there, and ends the route.
    pub fn via_end(&mut self, shell: &mut Shell) {
        if self.place_via_here(shell) {
            self.finish(shell);
        }
    }

    /// The Via the route places next: the size chosen with Shift+V, else the Net's.
    fn via_diameter(&self, pcb: &PcbDocument, net: Option<Uuid>) -> Nm {
        self.via_size.map_or_else(
            || net_rules(pcb, net).via_diameter.default_size(),
            |s| s.diameter,
        )
    }

    /// Places the path to the cursor and a Via there; `false` when nothing is in flight or
    /// the Via is refused (the status bar says why).
    fn place_via_here(&mut self, shell: &mut Shell) -> bool {
        self.notice_cancel(shell);
        let (Some(route), Some(Snapped { at, .. })) = (&self.route, self.cursor) else {
            return false;
        };
        let net = route.net;
        self.check(shell, at);
        if let Some(why) = self.why_blocked(shell) {
            shell.report(why);
            return false;
        }
        let Some((project, _)) = board(shell) else {
            return false;
        };
        let pcb = project.pcb().expect("a board");
        let diameter = self.via_diameter(pcb, net);
        let proposal = Proposal::Via { at, diameter, net };
        if let Some(why) = self.blocked_by(shell, &proposal) {
            shell.report(why);
            return false;
        }
        let other = other_side(pcb, self.route.as_ref().expect("routing").leg.layer);
        let via_size = self.via_size;
        let (corner, posture) = (self.corner, self.posture);
        let route = self.route.as_mut().expect("routing");
        let path = route::corner_path(route.last(), at, corner, posture);
        route.leg.points.extend_from_slice(&path[1..]);
        let next = Leg {
            layer: other,
            width: route.leg.width,
            points: vec![at],
        };
        let done = std::mem::replace(&mut route.leg, next);
        if done.points.len() > 1 {
            route.steps.push(Step::Leg(done));
        }
        route.steps.push(Step::Via(at, via_size));
        self.depend_on(shell, at);
        let _ = shell.run_reporting("pcb.layer.activate", json!({ "layer": other }));
        self.check(shell, at);
        true
    }

    /// Backspace: takes back the last corner (or Via); with nothing left, drops the route.
    pub fn undo_segment(&mut self, shell: &mut Shell) {
        self.notice_cancel(shell);
        let Some(route) = self.route.as_mut() else {
            return;
        };
        if route.leg.points.len() > 1 {
            route.leg.points.pop();
        } else if let Some(Step::Via(..)) = route.steps.last() {
            route.steps.pop();
            let at = route.last();
            route.leg = match route.steps.pop() {
                Some(Step::Leg(leg)) => leg,
                _ => Leg {
                    layer: other_side_of(shell, route.leg.layer),
                    width: route.leg.width,
                    points: vec![at],
                },
            };
            let layer = route.leg.layer;
            let _ = shell.run_reporting("pcb.layer.activate", json!({ "layer": layer }));
        } else if route.steps.is_empty() {
            self.cancel(shell);
            return;
        } else if let Some(Step::Leg(leg)) = route.steps.pop() {
            // A width change left a one-point leg: go back into the earlier one.
            route.leg = leg;
            route.leg.points.pop();
        }
        if let Some(Snapped { at, .. }) = self.cursor {
            self.check(shell, at);
        }
    }

    /// `/`: the next corner mode.
    pub fn cycle_corner(&mut self, shell: &mut Shell) {
        self.corner = self.corner.next();
        self.recheck(shell);
    }

    /// Checks the path ahead again at the cursor, after a mode changed.
    pub fn recheck(&mut self, shell: &mut Shell) {
        self.notice_cancel(shell);
        if let Some(Snapped { at, .. }) = self.cursor {
            self.check(shell, at);
        }
    }

    /// A key or Routing Bar button: toggles or steps `mode`. Modes kept in Preferences
    /// are set through `app.preferences.set`.
    pub fn change_mode(&mut self, shell: &mut Shell, mode: RouteMode) {
        let prefs = shell.preferences().pcb.route.clone();
        let set = |shell: &mut Shell, key: &str, value: serde_json::Value| {
            let _ = shell.run_reporting(
                "app.preferences.set",
                json!({ "key": format!("pcb.route.{key}"), "value": value }),
            );
        };
        match mode {
            RouteMode::Lookahead => self.lookahead = !self.lookahead,
            RouteMode::Posture => self.posture = self.posture.flipped(),
            RouteMode::ObstacleMode => {
                set(shell, "obstacle_mode", json!(prefs.obstacle_mode.next()));
            }
            RouteMode::AutoTerminate => set(shell, "auto_terminate", json!(!prefs.auto_terminate)),
            RouteMode::LoopRemoval => set(shell, "loop_removal", json!(!prefs.loop_removal)),
            RouteMode::ClearanceDisplay => set(
                shell,
                "clearance_display",
                json!(prefs.clearance_display.next()),
            ),
            RouteMode::WidthMode => {
                let Some((project, _)) = board(shell) else {
                    return;
                };
                let pcb = project.pcb().expect("a board");
                let net = self.route.as_ref().and_then(|r| r.net);
                let next = route::width::next_mode(pcb, net, prefs.width_mode, self.user_width);
                let width = route::width::mode_width(pcb, net, next, self.user_width);
                set(shell, "width_mode", json!(next));
                if let Some(width) = width {
                    self.apply_width(shell, width);
                }
            }
        }
        self.recheck(shell);
    }

    /// Esc or right-click: closes an open chooser, else ends the route and keeps what is
    /// placed; with only the start point, drops it; with no route, returns to Select.
    pub fn end(&mut self, shell: &mut Shell) {
        self.notice_cancel(shell);
        if self.chooser.take().is_some() {
            return;
        }
        match &self.route {
            None => {
                let _ = shell.run_reporting("tool.activate:select", json!({}));
            }
            Some(route) if route.segments() == 0 => self.cancel(shell),
            Some(_) => self.finish(shell),
        }
    }

    /// Shift+W or Shift+V: opens a chooser for the route in flight.
    pub fn open_chooser(&mut self, shell: &Shell, kind: ChooserKind) {
        let Some((project, _)) = board(shell) else {
            return;
        };
        let pcb = project.pcb().expect("a board");
        let net = self.route.as_ref().and_then(|r| r.net);
        self.chooser = Some(match kind {
            ChooserKind::Width => Chooser::Width(route::width::width_choices(
                pcb,
                net,
                &shell.preferences().pcb.route.favourites(),
            )),
            ChooserKind::Via => Chooser::Via(route::width::via_choices(pcb, net)),
        });
    }

    /// The Via size chosen or typed, clipped to the Net's range, for the Vias placed from
    /// here on.
    pub fn set_via_size(&mut self, shell: &mut Shell, diameter: Nm, hole: Nm) {
        self.chooser = None;
        let Some((project, _)) = board(shell) else {
            return;
        };
        let net = self.route.as_ref().and_then(|r| r.net);
        let rules = net_rules(project.pcb().expect("a board"), net);
        let size = ViaSize {
            diameter: rules.via_diameter.clip(diameter),
            hole: rules.via_hole.clip(hole),
        };
        if size.diameter != diameter || size.hole != hole {
            let units = shell.board_view().units;
            shell.report(tr!(
                "route-via-clipped",
                diameter = super::view::format_length(size.diameter, units),
                hole = super::view::format_length(size.hole, units)
            ));
        }
        self.via_size = Some(size);
        self.recheck(shell);
    }

    /// Changes the width from here on: `width`, or with `step` the board's next or
    /// previous Track width (its rule and Net Classes').
    pub fn set_width(&mut self, shell: &mut Shell, width: Option<Nm>, step: Option<i32>) {
        self.notice_cancel(shell);
        self.chooser = None;
        let Some(current) = self.route.as_ref().map(|r| r.leg.width) else {
            return;
        };
        let width = match (width, step) {
            (Some(w), _) => {
                let Some((project, _)) = board(shell) else {
                    return;
                };
                let net = self.route.as_ref().and_then(|r| r.net);
                let clipped = route::width::clip_width(project.pcb().expect("a board"), net, w);
                if let Some(limit) = clipped.limit {
                    let units = shell.board_view().units;
                    shell.report(tr!(
                        "route-width-clipped",
                        width = super::view::format_length(clipped.width, units),
                        limit = format!("{limit:?}").to_lowercase()
                    ));
                }
                self.user_width = Some(clipped.width);
                clipped.width
            }
            (None, Some(step)) => {
                let Some((project, _)) = board(shell) else {
                    return;
                };
                let widths = widths(project.pcb().expect("a board"));
                let found = if step > 0 {
                    widths.iter().copied().find(|w| *w > current)
                } else {
                    widths.iter().rev().copied().find(|w| *w < current)
                };
                found.unwrap_or(current)
            }
            (None, None) => return,
        };
        self.apply_width(shell, width);
    }

    /// Routes on at `width` from the last placed vertex.
    fn apply_width(&mut self, shell: &mut Shell, width: Nm) {
        let Some(route) = self.route.as_mut() else {
            return;
        };
        if width == route.leg.width {
            return;
        }
        if route.leg.points.len() > 1 {
            let next = Leg {
                layer: route.leg.layer,
                width,
                points: vec![route.last()],
            };
            let done = std::mem::replace(&mut route.leg, next);
            route.steps.push(Step::Leg(done));
        } else {
            route.leg.width = width;
        }
        if let Some(Snapped { at, .. }) = self.cursor {
            self.check(shell, at);
        }
    }

    /// Enter: commits what is placed (the path ahead is dropped).
    pub fn finish(&mut self, shell: &mut Shell) {
        self.notice_cancel(shell);
        let Some(route) = self.route.take() else {
            return;
        };
        shell.end_tool();
        self.hits.clear();
        self.keepouts.clear();
        self.proposed = None;
        let net = route.net;
        let mut steps = route.steps;
        steps.push(Step::Leg(route.leg));
        let (Some((project, _)), Some(copper)) = (board(shell), self.copper(shell)) else {
            return;
        };
        let loops = shell.preferences().pcb.route.loops();
        let plan = route::commit(
            &copper,
            project.pcb().expect("a board"),
            net,
            &steps,
            &loops,
        );
        self.chooser = None;
        self.halos.clear();
        self.note = plan.note.map(note_text);
        if let Some(note) = &self.note {
            shell.report(note.clone());
        }
        if let Some((id, params)) = plan.command() {
            let _ = shell.run_reporting(&id, params);
        }
    }

    /// Esc, or another Tool: drops the route (or the Via at the cursor) unplaced.
    pub fn cancel(&mut self, shell: &mut Shell) {
        if self.route.take().is_some() && shell.tool_preview().is_some_and(|t| t.title == title()) {
            shell.end_tool();
        }
        self.hits.clear();
        self.keepouts.clear();
        self.via = None;
        self.proposed = None;
        self.chooser = None;
        self.halos.clear();
        self.note = None;
    }

    /// Drops the route when an agent's edit cancelled its preview.
    fn notice_cancel(&mut self, shell: &Shell) {
        if self.route.is_some() && shell.tool_preview().is_none_or(|t| t.title != title()) {
            self.route = None;
            self.hits.clear();
            self.keepouts.clear();
            self.proposed = None;
        }
    }

    fn snap_to(&mut self, shell: &Shell, at: Point) -> Point {
        let targets = super::tool::board_hotspots(shell);
        let snapped = super::tool::snap_board(shell, at, &targets);
        self.cursor = Some(snapped);
        snapped.at
    }

    /// The board's copper index, kept in the Project between edits.
    fn copper(&self, shell: &Shell) -> Option<std::sync::Arc<Copper>> {
        let (project, _) = board(shell)?;
        Copper::of(project)
    }

    /// Checks the path ahead to `at` (or, with the Via Tool, a Via at `at`).
    fn check(&mut self, shell: &Shell, at: Point) {
        self.hits.clear();
        self.keepouts.clear();
        if shell.tool() == Tool::Via && self.route.is_none() {
            let Some((project, _)) = board(shell) else {
                return;
            };
            let pcb = project.pcb().expect("a board");
            let net = self
                .copper(shell)
                .and_then(|c| pcb.copper_layers().find_map(|l| c.net_at(l.uuid, at)));
            let diameter = net_rules(pcb, net).via_diameter.default_size();
            let proposal = Proposal::Via { at, diameter, net };
            let (hits, keepouts) = self.query(shell, &proposal, &[]);
            self.via = Some(ViaCursor {
                at,
                diameter,
                blocked: !hits.is_empty() || !keepouts.is_empty(),
            });
            self.hits = hits;
            self.keepouts = keepouts;
            return;
        }
        self.via = None;
        self.proposed = None;
        self.halos.clear();
        let Some(route) = &self.route else {
            return;
        };
        let (Some((project, _)), Some(copper)) = (board(shell), self.copper(shell)) else {
            return;
        };
        let prefs = &shell.preferences().pcb.route;
        let (corner, posture) = (self.corner, self.posture);
        let origin = route
            .steps
            .iter()
            .find_map(|s| match s {
                Step::Leg(l) => Some(l),
                Step::Via(..) => None,
            })
            .unwrap_or(&route.leg);
        let narrowest = route
            .steps
            .iter()
            .filter_map(|s| match s {
                Step::Leg(l) => Some(l.width),
                Step::Via(..) => None,
            })
            .min();
        let request = Request {
            mode: prefs.obstacle_mode,
            corner,
            posture,
            lookahead: self.lookahead,
            origin: Some(route::RouteEnd {
                layer: origin.layer,
                at: origin.points[0],
            }),
            narrowest,
            auto_terminate: prefs.auto_terminate,
            loops: prefs.loops(),
            ignore: if route.net.is_none() {
                route.start_objects.clone()
            } else {
                Vec::new()
            },
            ..Request::new(
                route.last(),
                at,
                route.leg.layer,
                route.leg.width,
                route.net,
            )
        };
        let pcb = project.pcb().expect("a board");
        let proposed = route::propose(&copper, pcb, &request);
        self.halos = match prefs.clearance_display {
            ClearanceDisplay::Off => Vec::new(),
            ClearanceDisplay::Near => {
                copper.halos(pcb, route.leg.layer, route.net, Some((at, CLEARANCE_NEAR)))
            }
            ClearanceDisplay::All => copper.halos(pcb, route.leg.layer, route.net, None),
        };
        self.note = proposed.note.map(note_text);
        self.hits = proposed.obstacles.copper.clone();
        self.keepouts = proposed.obstacles.keepouts.clone();
        self.proposed = Some(proposed);
    }

    fn query(
        &mut self,
        shell: &Shell,
        proposal: &Proposal,
        ignore: &[Uuid],
    ) -> (Vec<ClearanceHit>, Vec<Uuid>) {
        let Some(pcb) = board(shell).and_then(|(p, _)| p.pcb()) else {
            return (Vec::new(), Vec::new());
        };
        let keepouts = keepout::keepouts_hit(pcb, proposal);
        let hits = self
            .copper(shell)
            .map(|c| c.clearance(pcb, proposal, ignore))
            .unwrap_or_default();
        (hits, keepouts)
    }

    /// Why the last check refused, for the status bar.
    fn why_blocked(&self, shell: &Shell) -> Option<String> {
        if let Some(hit) = self.hits.first() {
            let units = shell.board_view().units;
            return Some(tr!(
                "route-blocked",
                actual = super::view::format_length(hit.actual, units),
                required = super::view::format_length(hit.required, units)
            ));
        }
        if !self.keepouts.is_empty() {
            return Some(tr!("route-keepout"));
        }
        if self
            .proposed
            .as_ref()
            .is_some_and(|p| !p.obstacles.edges.is_empty())
        {
            return Some(tr!("route-edge"));
        }
        None
    }

    fn blocked_by(&mut self, shell: &Shell, proposal: &Proposal) -> Option<String> {
        let (hits, keepouts) = self.query(shell, proposal, &[]);
        self.hits = hits;
        self.keepouts = keepouts;
        self.why_blocked(shell)
    }

    /// Starts a route at `at` on the active copper Layer.
    fn start(&mut self, shell: &mut Shell, at: Point) {
        let Some((project, _)) = board(shell) else {
            return;
        };
        let pcb = project.pcb().expect("a board");
        let active = shell.board_view().active_layer(project).filter(|l| {
            pcb.layers()
                .layer(*l)
                .is_some_and(|x| x.kind == LayerKind::Copper)
        });
        let Some(layer) = active.or_else(|| {
            pcb.layers()
                .find(LayerKind::Copper, Some(Side::Top))
                .map(|l| l.uuid)
        }) else {
            return;
        };
        let Some(copper) = self.copper(shell) else {
            return;
        };
        let items = copper.items_at(layer, at);
        let net = copper.net_at(layer, at);
        let start_objects: Vec<Uuid> = items.iter().map(|i| i.object()).collect();
        let prefs = &shell.preferences().pcb.route;
        let width = route::width::start_width(
            pcb,
            &copper,
            net,
            (layer, at),
            prefs.width_mode,
            self.user_width,
            prefs.pickup_width,
        );
        self.route = Some(Route {
            net,
            steps: Vec::new(),
            leg: Leg {
                layer,
                width,
                points: vec![at],
            },
            objects: start_objects.clone(),
            start_objects,
        });
        shell.begin_tool(ToolPreview::new(title(), self.objects()));
        self.check(shell, at);
    }

    fn objects(&self) -> Vec<Uuid> {
        self.route
            .as_ref()
            .map(|r| r.objects.clone())
            .unwrap_or_default()
    }

    /// Adds the copper under `at` to what the preview depends on.
    fn depend_on(&mut self, shell: &mut Shell, at: Point) {
        let Some(layer) = self.route.as_ref().map(|r| r.leg.layer) else {
            return;
        };
        let found: Vec<Uuid> = self
            .copper(shell)
            .map(|c| c.items_at(layer, at).iter().map(|i| i.object()).collect())
            .unwrap_or_default();
        let route = self.route.as_mut().expect("routing");
        for o in found {
            if !route.objects.contains(&o) {
                route.objects.push(o);
            }
        }
        shell.begin_tool(ToolPreview::new(title(), self.objects()));
    }

    /// The Via Tool's click: a stitching Via at `at` unless it breaks clearance.
    fn place_via(&mut self, shell: &mut Shell, at: Point) {
        if let Some(why) = self.why_blocked(shell) {
            shell.report(why);
            return;
        }
        let _ = shell.run_reporting("pcb.via.add", json!({"at": {"x": at.x, "y": at.y}}));
        self.check(shell, at);
    }
}

/// Whether a right-click on the board ends the route rather than opening the context
/// menu: while the Track Tool is active (docs/spec/v0.1/pcb.md#placing-and-ending).
pub fn takes_right_click(shell: &Shell) -> bool {
    shell.tool() == Tool::Route
}

/// The status-bar text for a `propose` or `commit` note.
fn note_text(note: route::Note) -> String {
    match note {
        route::Note::LoopNarrower => tr!("route-loop-narrower"),
        route::Note::LoopBranch => tr!("route-loop-branch"),
        route::Note::LoopThroughVias => tr!("route-loop-through-vias"),
        route::Note::LoopLocked => tr!("route-loop-locked"),
        route::Note::ZoneNotConnected => tr!("route-zone-not-connected"),
    }
}

/// The board's Track widths to step through: the preferred width of every `track_width`
/// record, the board-wide one's and every enabled Net Class's, sorted.
fn widths(pcb: &PcbDocument) -> Vec<Nm> {
    let mut out: Vec<Nm> = pcb
        .rules()
        .filter(|r| r.kind == RuleKind::TrackWidth && (r.enabled || r.scope.is_board_wide()))
        .filter_map(|r| r.values.preferred)
        .collect();
    out.sort();
    out.dedup();
    out
}

/// The copper Layer a Via switches to from `layer`: the other outer copper from an outer
/// Layer, and the next copper Layer down from inner copper. It walks the copper Layers, so
/// inner copper is never skipped as a route's next Layer.
fn other_side(pcb: &PcbDocument, layer: Uuid) -> Uuid {
    let copper: Vec<Uuid> = pcb.copper_layers().map(|l| l.uuid).collect();
    let (Some(&first), Some(&last)) = (copper.first(), copper.last()) else {
        return layer;
    };
    match copper.iter().position(|l| *l == layer) {
        Some(0) => last,
        Some(i) if i + 1 == copper.len() => first,
        Some(i) => copper[i + 1],
        None => first,
    }
}

fn other_side_of(shell: &Shell, layer: Uuid) -> Uuid {
    board(shell)
        .and_then(|(p, _)| p.pcb())
        .map_or(layer, |pcb| other_side(pcb, layer))
}

/// The open Project and its PCB Document's UUID.
fn board(shell: &Shell) -> Option<(&Project, Uuid)> {
    let project = shell.engine().project()?;
    Some((project, project.pcb()?.uuid))
}

/// How a piece of the route's preview is drawn (docs/spec/v0.1/pcb.md#preview).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Look {
    /// Held by the Tool: solid.
    Placed,
    /// What the next click places: translucent, not hatched.
    Proposed,
    /// Proposed but breaking a rule (`ignore`, `highlight`): translucent in the violation
    /// colour.
    Violating,
    /// The look-ahead tail the next click doesn't place: hollow.
    LookAhead,
    /// Out of reach in `stop`: a thin ghost to the cursor, an X at its end.
    Blocked,
    /// What loop removal deletes when the route ends: red.
    Removed,
}

impl Look {
    /// The colour, from the theme's `accent` and violation colour `refused`.
    pub fn color(self, accent: egui::Color32, refused: egui::Color32) -> egui::Color32 {
        match self {
            Look::Placed => accent,
            Look::Proposed => accent.gamma_multiply(0.55),
            Look::Violating => refused.gamma_multiply(0.55),
            Look::LookAhead => accent,
            Look::Blocked => refused.gamma_multiply(0.7),
            Look::Removed => refused,
        }
    }

    /// Drawn as an outline at its width rather than filled.
    pub fn hollow(self) -> bool {
        matches!(self, Look::LookAhead | Look::Removed)
    }
}

/// One piece of the preview: a path at a width, drawn its [`Look`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreviewStroke {
    pub look: Look,
    pub points: Vec<Point>,
    /// The copper width; zero draws a hairline.
    pub width: Nm,
}

/// What the canvas draws for `preview`, in paint order: the old path loop removal
/// deletes, placed legs, the proposed segments, the look-ahead tail and the ghost.
pub fn strokes(preview: &Preview) -> Vec<PreviewStroke> {
    let mut out = Vec::new();
    let mut push = |look, points: &[Point], width| {
        if points.len() > 1 {
            out.push(PreviewStroke {
                look,
                points: points.to_vec(),
                width,
            });
        }
    };
    if let Some(proposed) = &preview.proposed {
        for piece in &proposed.removes.removed {
            push(Look::Removed, &[piece.from, piece.to], piece.width);
        }
    }
    for leg in &preview.legs {
        push(Look::Placed, &leg.points, leg.width);
    }
    push(Look::Placed, &preview.placed, preview.width);
    match &preview.proposed {
        Some(proposed) => {
            let look = if proposed.status == Status::Violating {
                Look::Violating
            } else {
                Look::Proposed
            };
            push(look, &proposed.place, preview.width);
            push(Look::LookAhead, &proposed.look_ahead, preview.width);
            push(Look::Blocked, &proposed.blocked, Nm::ZERO);
        }
        None => push(Look::Proposed, &preview.ahead, preview.width),
    }
    out
}

/// Draws the route in flight (placed legs, the path ahead in red while it breaks
/// clearance, its Vias) or the Via Tool's Via at the cursor, over the board canvas.
pub fn draw(ui: &egui::Ui, shell: &Shell, input: &crate::canvas::CanvasInputs) {
    use egui::{Color32, Stroke};
    let router = shell.router();
    let painter = ui.painter_at(input.rect);
    let accent = ui.visuals().selection.stroke.color;
    let refused = ui.visuals().error_fg_color;
    let screen = |p: Point| input.screen_point((p.x.0 as f64, p.y.0 as f64));
    let points = |n: Nm| (n.0 as f64 / input.camera.nm_per_point).max(1.0) as f32;
    let path = |pts: &[Point], width: Nm, color: Color32| {
        for pair in pts.windows(2) {
            painter.line_segment(
                [screen(pair[0]), screen(pair[1])],
                Stroke::new(points(width), color),
            );
        }
    };
    // A hollow path: the two edges of each segment at `width` apart.
    let outline = |pts: &[Point], width: Nm, color: Color32| {
        let half = points(width) / 2.0;
        for pair in pts.windows(2) {
            let (a, b) = (screen(pair[0]), screen(pair[1]));
            let along = (b - a).normalized();
            let side = egui::vec2(-along.y, along.x) * half;
            let stroke = Stroke::new(1.0, color);
            painter.line_segment([a + side, b + side], stroke);
            painter.line_segment([a - side, b - side], stroke);
            painter.circle_stroke(a, half, stroke);
            painter.circle_stroke(b, half, stroke);
        }
    };
    let via = |at: Point, diameter: Nm, color: Color32| {
        painter.circle_stroke(screen(at), points(diameter) / 2.0, Stroke::new(2.0, color));
    };
    for halo in router.halos() {
        let shade = refused.gamma_multiply(0.12);
        for prim in &halo.shape.prims {
            let r = points(Nm(prim.radius));
            let at = |p: [i64; 2]| screen(Point::new(Nm(p[0]), Nm(p[1])));
            match &prim.core {
                reda_engine::model::geom::grown::Core::Seg(a, b) => {
                    painter.line_segment([at(*a), at(*b)], Stroke::new(2.0 * r, shade));
                    painter.circle_filled(at(*a), r, shade);
                    painter.circle_filled(at(*b), r, shade);
                }
                reda_engine::model::geom::grown::Core::Poly(corners) => {
                    let ring: Vec<egui::Pos2> = corners.iter().map(|c| at(*c)).collect();
                    painter.add(egui::Shape::convex_polygon(
                        ring.clone(),
                        shade,
                        Stroke::new(2.0 * r, shade),
                    ));
                }
            }
        }
    }
    if let Some(p) = router.preview() {
        let placed = accent.gamma_multiply(0.8);
        for stroke in strokes(&p) {
            let color = stroke.look.color(accent, refused);
            if stroke.look.hollow() {
                outline(&stroke.points, stroke.width, color);
            } else {
                path(&stroke.points, stroke.width, color);
            }
            if stroke.look == Look::Blocked
                && let Some(end) = stroke.points.last()
            {
                let c = screen(*end);
                let d = 4.0;
                let x = Stroke::new(1.5, color);
                painter.line_segment([c + egui::vec2(-d, -d), c + egui::vec2(d, d)], x);
                painter.line_segment([c + egui::vec2(-d, d), c + egui::vec2(d, -d)], x);
            }
        }
        let diameter = board(shell)
            .and_then(|(project, _)| project.pcb())
            .map_or(Nm::from_um(600), |pcb| {
                net_rules(pcb, p.net).via_diameter.default_size()
            });
        for at in p.vias {
            via(at, diameter, placed);
        }
    } else if shell.tool() == Tool::Via
        && let Some(v) = router.via_cursor()
    {
        via(v.at, v.diameter, if v.blocked { refused } else { accent });
    }
}
