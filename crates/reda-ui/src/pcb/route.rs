//! The routing Tool (T) and the Via Tool (V): manual single Tracks with 45° or 90°
//! corners, and Vias placed mid-route or on their own for stitching.
//!
//! - A click starts a route on the active copper Layer (else the top copper), on the Net
//!   of the copper under it, as wide as that Net's Class says. Each further click places
//!   the corner path to the cursor; `/` cycles the corner mode, `pcb.route.width` changes
//!   the width mid-route, Backspace takes back the last corner, `pcb.route.add_via` (V)
//!   drops a Via at the cursor and carries on on the other side.
//! - The live clearance check (the engine's [`Copper::clearance`] and Keepout queries)
//!   flags the path ahead and refuses to place it; nothing shoves.
//! - Reaching copper of the route's own Net ends the route, and so do Enter
//!   (`pcb.route.finish`) and a second click on the last corner. The route then commits as
//!   one `pcb.track.add`, or one `batch` when it changed Layer or width, so it is one undo
//!   step. Nothing runs before that.
//! - The route's preview depends on the copper under its corners: it registers them with
//!   [`Shell::begin_tool`], so an agent's edit to them cancels the route.
//!
//! The Via Tool previews a Via at the cursor, checked the same way, and each click runs
//! `pcb.via.add` there (taking the Net of the copper under it).

use reda_engine::model::geom::{Nm, Point};
use reda_engine::model::{LayerKind, PcbDocument, Project, Side, TrackGeometry, Uuid};
use reda_engine::pcb::net_rules::{clearance_between, net_rules};
use reda_engine::pcb::placed::placed_pads;
use reda_engine::pcb::{ClearanceHit, Copper, Item, Proposal, RatsnestLine, zone};
use serde_json::{Value, json};

use super::snap::{self, SnapKind};
use super::tool::DEFAULT_SNAP_RADIUS;
use crate::shell::{Shell, ToolPreview};
use crate::tools::Tool;
use crate::tr;

/// How a route turns the corner between its last point and the cursor.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Corner {
    /// Straight along the longer axis, then 45° into the cursor.
    #[default]
    Straight45,
    /// 45° first, then straight into the cursor.
    Diagonal45,
    /// Horizontal, then vertical.
    Horizontal90,
    /// Vertical, then horizontal.
    Vertical90,
    /// PROTOTYPE (issue 136): straight to the cursor at any angle.
    Any,
}

/// PROTOTYPE (issue 136): what a route does at an obstacle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Obstacle {
    Stop,
    Ignore,
}

/// PROTOTYPE (issue 136): how much of the clearance boundary shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClearanceShow {
    Off,
    Near,
    All,
}

/// PROTOTYPE (issue 136): the switches being tried.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Opts {
    pub lookahead: bool,
    pub obstacle: Obstacle,
    pub auto_terminate: bool,
    pub loop_removal: bool,
    pub clearance: ClearanceShow,
    pub miter: bool,
}

impl Default for Opts {
    fn default() -> Self {
        Opts {
            lookahead: true,
            obstacle: Obstacle::Stop,
            auto_terminate: true,
            loop_removal: true,
            clearance: ClearanceShow::Near,
            miter: true,
        }
    }
}

impl Corner {
    /// The mode `/` goes to next: 45, 90, any angle.
    pub fn next(self) -> Corner {
        match self {
            Corner::Straight45 | Corner::Diagonal45 => Corner::Horizontal90,
            Corner::Horizontal90 | Corner::Vertical90 => Corner::Any,
            Corner::Any => Corner::Straight45,
        }
    }

    /// Space: which leg comes first.
    pub fn flipped(self) -> Corner {
        match self {
            Corner::Straight45 => Corner::Diagonal45,
            Corner::Diagonal45 => Corner::Straight45,
            Corner::Horizontal90 => Corner::Vertical90,
            Corner::Vertical90 => Corner::Horizontal90,
            Corner::Any => Corner::Any,
        }
    }
}

/// The path from `from` to `to` turning `corner`'s way: `from`, the corner when there is
/// one, and `to`.
pub fn corner_path(from: Point, to: Point, corner: Corner) -> Vec<Point> {
    let (dx, dy) = (to.x.0 - from.x.0, to.y.0 - from.y.0);
    let (sx, sy) = (dx.signum(), dy.signum());
    let d = dx.abs().min(dy.abs());
    let bend = match corner {
        Corner::Straight45 if dx.abs() >= dy.abs() => Point::new(Nm(to.x.0 - sx * d), from.y),
        Corner::Straight45 => Point::new(from.x, Nm(to.y.0 - sy * d)),
        Corner::Diagonal45 => Point::new(Nm(from.x.0 + sx * d), Nm(from.y.0 + sy * d)),
        Corner::Horizontal90 => Point::new(to.x, from.y),
        Corner::Vertical90 => Point::new(from.x, to.y),
        Corner::Any => to,
    };
    let mut path = vec![from];
    for p in [bend, to] {
        if *path.last().expect("from") != p {
            path.push(p);
        }
    }
    path
}

/// A stretch of route on one Layer at one width.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Leg {
    pub layer: Uuid,
    pub width: Nm,
    pub points: Vec<Point>,
}

/// What a route has placed so far, in order.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Step {
    Leg(Leg),
    Via(Point),
}

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
    /// PROTOTYPE: the Ratsnest line this route started from.
    connection: Option<RatsnestLine>,
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
                Step::Via(_) => 1,
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
    /// Whether the path ahead breaks clearance (it then cannot be placed).
    pub blocked: bool,
    /// Earlier legs.
    pub legs: Vec<Leg>,
    /// The Vias placed.
    pub vias: Vec<Point>,
    /// PROTOTYPE: what the next click places.
    pub commit: Vec<Point>,
    /// PROTOTYPE: the look-ahead tail, which the next click doesn't place.
    pub hollow: Vec<Point>,
    /// PROTOTYPE: from the clip point to the cursor, unreachable.
    pub ghost: Vec<Point>,
    /// PROTOTYPE: the path ahead breaks clearance and Ignore lets it.
    pub violating: bool,
    /// PROTOTYPE: the old copper that goes if the route ends here.
    pub removal: Vec<super::proto_loop::Part>,
    /// PROTOTYPE: the connection's far end.
    pub target: Option<Point>,
}

/// The Via the Via Tool would place at the cursor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ViaCursor {
    pub at: Point,
    pub diameter: Nm,
    pub blocked: bool,
}

/// The routing and Via Tools' state between events.
#[derive(Debug, Default)]
pub struct Router {
    route: Option<Route>,
    corner: Corner,
    /// The cursor, snapped.
    cursor: Option<(Point, SnapKind)>,
    radius: Option<Nm>,
    /// What the path ahead (or the Via at the cursor) hits.
    hits: Vec<ClearanceHit>,
    keepouts: Vec<Uuid>,
    via: Option<ViaCursor>,
    opts: Opts,
    commit: Vec<Point>,
    hollow: Vec<Point>,
    ghost: Vec<Point>,
    removal: Vec<super::proto_loop::Part>,
    note: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ViaThen {
    OtherSide,
    SameLayer,
    End,
}

fn dist(a: Point, b: Point) -> f64 {
    ((a.x.0 - b.x.0) as f64).hypot((a.y.0 - b.y.0) as f64)
}

/// PROTOTYPE: the Ratsnest line nearest `p`, within `reach`.
fn nearest_line(lines: &[RatsnestLine], p: Point, reach: Nm) -> Option<RatsnestLine> {
    let off = |l: &RatsnestLine| {
        let (dx, dy) = ((l.to.x.0 - l.from.x.0) as f64, (l.to.y.0 - l.from.y.0) as f64);
        let len2 = dx * dx + dy * dy;
        if len2 == 0.0 {
            return dist(p, l.from);
        }
        let t = (((p.x.0 - l.from.x.0) as f64 * dx + (p.y.0 - l.from.y.0) as f64 * dy) / len2)
            .clamp(0.0, 1.0);
        ((p.x.0 as f64 - l.from.x.0 as f64 - t * dx).powi(2)
            + (p.y.0 as f64 - l.from.y.0 as f64 - t * dy).powi(2))
        .sqrt()
    };
    lines
        .iter()
        .filter(|l| off(l) <= reach.0 as f64)
        .min_by(|a, b| off(a).total_cmp(&off(b)))
        .copied()
}

/// The title of the route's [`ToolPreview`].
fn title() -> String {
    tr!("tool-route")
}

impl Router {
    /// Sets how far electrical points catch the cursor (a few pixels at the zoom).
    pub fn set_snap_radius(&mut self, radius: Nm) {
        self.radius = Some(radius);
    }

    /// PROTOTYPE: the switches being tried.
    pub fn opts(&self) -> Opts {
        self.opts
    }

    /// PROTOTYPE: the new in-Tool keys.
    pub fn proto(&mut self, shell: &mut Shell, what: &str) {
        self.notice_cancel(shell);
        match what {
            "posture" => self.corner = self.corner.flipped(),
            "lookahead" => self.opts.lookahead = !self.opts.lookahead,
            "obstacle" => {
                self.opts.obstacle = match self.opts.obstacle {
                    Obstacle::Stop => Obstacle::Ignore,
                    Obstacle::Ignore => Obstacle::Stop,
                }
            }
            "auto" => self.opts.auto_terminate = !self.opts.auto_terminate,
            "loops" => self.opts.loop_removal = !self.opts.loop_removal,
            "miter" => self.opts.miter = !self.opts.miter,
            "clearance" => {
                self.opts.clearance = match self.opts.clearance {
                    ClearanceShow::Off => ClearanceShow::Near,
                    ClearanceShow::Near => ClearanceShow::All,
                    ClearanceShow::All => ClearanceShow::Off,
                }
            }
            "esc" => {
                match &self.route {
                    Some(route) if route.segments() > 0 => self.finish(shell),
                    Some(_) => self.cancel(shell),
                    None => {
                        let _ = shell.run_reporting("tool.activate", json!({"tool": "select"}));
                    }
                }
                return;
            }
            "via_same" => return self.place_via_in_route(shell, ViaThen::SameLayer),
            "via_end" => return self.place_via_in_route(shell, ViaThen::End),
            "other_end" => self.swap_end(shell),
            "next_connection" => self.next_connection(shell),
            "start_layer" => self.next_start_layer(shell),
            _ => {}
        }
        if let Some((at, _)) = self.cursor {
            self.check(shell, at);
        }
    }

    /// PROTOTYPE: moves a route that hasn't placed anything to `at` on `layer`.
    fn restart(&mut self, shell: &mut Shell, at: Point, layer: Uuid) {
        let route = self.route.as_mut().expect("routing");
        route.leg.points = vec![at];
        route.leg.layer = layer;
        let _ = shell.run_reporting("pcb.layer.activate", json!({ "layer": layer }));
    }

    fn end_layer(&self, shell: &Shell, at: Point, fallback: Uuid) -> Uuid {
        let Some(pcb) = board(shell).and_then(|(p, _)| p.pcb()) else {
            return fallback;
        };
        let Some(copper) = self.copper(shell) else {
            return fallback;
        };
        if !copper.items_at(fallback, at).is_empty() {
            return fallback;
        }
        pcb.copper_layers()
            .map(|l| l.uuid)
            .find(|l| !copper.items_at(*l, at).is_empty())
            .unwrap_or(fallback)
    }

    fn swap_end(&mut self, shell: &mut Shell) {
        let Some(route) = &self.route else { return };
        if route.segments() > 0 {
            return;
        }
        let Some(mut line) = route.connection else {
            return;
        };
        let here = route.last();
        let layer = route.leg.layer;
        std::mem::swap(&mut line.from, &mut line.to);
        std::mem::swap(&mut line.from_item, &mut line.to_item);
        let (at, target) = if line.from == here {
            (line.to, line.from)
        } else {
            (line.from, line.to)
        };
        let _ = target;
        let next = self.end_layer(shell, at, layer);
        self.restart(shell, at, next);
    }

    fn next_connection(&mut self, shell: &mut Shell) {
        let Some(route) = &self.route else { return };
        if route.segments() > 0 {
            return;
        }
        let (Some(net), Some(current)) = (route.net, route.connection) else {
            return;
        };
        let here = route.last();
        let Some(copper) = self.copper(shell) else {
            return;
        };
        let lines = copper.net_ratsnest(net);
        let Some(i) = lines.iter().position(|l| *l == current) else {
            return;
        };
        let Some(next) = (1..lines.len())
            .map(|k| lines[(i + k) % lines.len()])
            .find(|l| l.from == here || l.to == here)
        else {
            shell.report("no other connection leaves here".into());
            return;
        };
        self.route.as_mut().expect("routing").connection = Some(next);
    }

    fn next_start_layer(&mut self, shell: &mut Shell) {
        let Some(route) = &self.route else { return };
        if route.segments() > 0 {
            return;
        }
        let (here, layer) = (route.last(), route.leg.layer);
        let Some(pcb) = board(shell).and_then(|(p, _)| p.pcb()) else {
            return;
        };
        let Some(copper) = self.copper(shell) else {
            return;
        };
        let layers: Vec<Uuid> = pcb
            .copper_layers()
            .map(|l| l.uuid)
            .filter(|l| !copper.items_at(*l, here).is_empty())
            .collect();
        let Some(i) = layers.iter().position(|l| *l == layer) else {
            return;
        };
        let next = layers[(i + 1) % layers.len()];
        self.restart(shell, here, next);
    }

    /// PROTOTYPE: the routing bar's buttons.
    fn tiles(&self) -> Vec<Tile> {
        let o = self.opts;
        let state = |b: bool| if b { "On" } else { "Off" };
        let (corner, corner_name) = match self.corner {
            Corner::Straight45 => ("route_corner_45", "45, straight first"),
            Corner::Diagonal45 => ("route_corner_45_diagonal", "45, diagonal first"),
            Corner::Horizontal90 => ("route_corner_90_horizontal", "90, horizontal first"),
            Corner::Vertical90 => ("route_corner_90_vertical", "90, vertical first"),
            Corner::Any => ("route_corner_any", "any angle"),
        };
        let (obstacle, obstacle_name) = match o.obstacle {
            Obstacle::Stop => ("route_obstacle_stop", "stop at clearance"),
            Obstacle::Ignore => ("route_obstacle_ignore", "ignore, and flag the violation"),
        };
        vec![
            Tile { icon: corner, on: false, title: "Corner mode", key: "/", command: "tool.cycle_corner", tip: format!("Now: {corner_name}. Space flips the posture.") },
            Tile { icon: "route_lookahead", on: o.lookahead, title: "Look-ahead", key: "1", command: "proto.track:lookahead", tip: format!("{}: a click places only the first segment.", state(o.lookahead)) },
            Tile { icon: obstacle, on: o.obstacle == Obstacle::Stop, title: "Obstacles", key: "Shift+R", command: "proto.track:obstacle", tip: format!("Now: {obstacle_name}.") },
            Tile { icon: "route_auto_end", on: o.auto_terminate, title: "Auto-terminate", key: "Shift+A", command: "proto.track:auto", tip: format!("{}: the route ends on same-Net copper.", state(o.auto_terminate)) },
            Tile { icon: "route_loop_removal", on: o.loop_removal, title: "Loop removal", key: "Shift+L", command: "proto.track:loops", tip: format!("{}: a new route replaces the old path it duplicates.", state(o.loop_removal)) },
            Tile { icon: "clearance", on: o.clearance != ClearanceShow::Off, title: "Show clearance", key: "C", command: "proto.track:clearance", tip: format!("Now: {:?}.", o.clearance) },
            Tile { icon: "route_miter", on: o.miter, title: "Drag miters", key: "Shift+M", command: "proto.track:miter", tip: format!("{}: dragging adds a 45 degree connector where one is needed.", state(o.miter)) },
        ]
    }

    /// The corner mode.
    pub fn corner(&self) -> Corner {
        self.corner
    }

    /// Where the cursor last snapped, for the snap marker.
    pub fn snap(&self) -> Option<(Point, SnapKind)> {
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
            Some((at, _)) => corner_path(route.last(), at, self.corner),
            None => vec![route.last()],
        };
        Some(Preview {
            layer: route.leg.layer,
            net: route.net,
            width: route.leg.width,
            placed: route.leg.points.clone(),
            ahead,
            blocked: !self.ghost.is_empty(),
            commit: self.commit.clone(),
            hollow: self.hollow.clone(),
            ghost: self.ghost.clone(),
            violating: self.ghost.is_empty()
                && (!self.hits.is_empty() || !self.keepouts.is_empty()),
            removal: self.removal.clone(),
            target: route.connection.map(|l| if l.from == route.last() { l.to } else { l.from }),
            legs: route
                .steps
                .iter()
                .filter_map(|s| match s {
                    Step::Leg(l) => Some(l.clone()),
                    Step::Via(_) => None,
                })
                .collect(),
            vias: route
                .steps
                .iter()
                .filter_map(|s| match s {
                    Step::Via(at) => Some(*at),
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
        let raw = at;
        let at = self.snap_to(shell, at);
        if shell.tool() == Tool::Via {
            self.check(shell, at);
            self.place_via(shell, at);
            return;
        }
        if self.route.is_none() {
            self.start(shell, at, raw);
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
        let reach = self.ghost.is_empty();
        if self.commit.len() < 2 {
            if let Some(why) = self.why_blocked(shell) {
                shell.report(why);
            }
            return;
        }
        let ends = reach && self.opts.auto_terminate && self.ends_here(shell, at);
        let commit = self.commit.clone();
        let last = *commit.last().expect("two points");
        let route = self.route.as_mut().expect("routing");
        route.leg.points.extend_from_slice(&commit[1..]);
        self.depend_on(shell, last);
        if ends {
            self.finish(shell);
        } else {
            self.check(shell, at);
        }
    }

    /// V while routing: places the path to the cursor, drops a Via there and carries on on
    /// the other side.
    pub fn add_via(&mut self, shell: &mut Shell) {
        self.place_via_in_route(shell, ViaThen::OtherSide);
    }

    fn place_via_in_route(&mut self, shell: &mut Shell, then: ViaThen) {
        self.notice_cancel(shell);
        let (Some(route), Some((at, _))) = (&self.route, self.cursor) else {
            return;
        };
        let net = route.net;
        self.check(shell, at);
        if let Some(why) = self.why_blocked(shell) {
            shell.report(why);
            return;
        }
        let Some((project, _)) = board(shell) else {
            return;
        };
        let pcb = project.pcb().expect("a board");
        let diameter = net_rules(pcb, net).via_diameter;
        let proposal = Proposal::Via { at, diameter, net };
        if let Some(why) = self.blocked_by(shell, &proposal) {
            shell.report(why);
            return;
        }
        let current = self.route.as_ref().expect("routing").leg.layer;
        let other = match then {
            ViaThen::OtherSide => other_side(pcb, current),
            ViaThen::SameLayer | ViaThen::End => current,
        };
        let route = self.route.as_mut().expect("routing");
        let path = corner_path(route.last(), at, self.corner);
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
        route.steps.push(Step::Via(at));
        self.depend_on(shell, at);
        if then == ViaThen::End {
            self.finish(shell);
            return;
        }
        let _ = shell.run_reporting("pcb.layer.activate", json!({ "layer": other }));
        self.check(shell, at);
    }

    // (the Via keys share `place_via_in_route`)

    /// Backspace: takes back the last corner (or Via); with nothing left, drops the route.
    pub fn undo_segment(&mut self, shell: &mut Shell) {
        self.notice_cancel(shell);
        let Some(route) = self.route.as_mut() else {
            return;
        };
        if route.leg.points.len() > 1 {
            route.leg.points.pop();
        } else if let Some(Step::Via(_)) = route.steps.last() {
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
        if let Some((at, _)) = self.cursor {
            self.check(shell, at);
        }
    }

    /// `/`: the next corner mode.
    pub fn cycle_corner(&mut self, shell: &mut Shell) {
        self.corner = self.corner.next();
        if let Some((at, _)) = self.cursor {
            self.check(shell, at);
        }
    }

    /// Changes the width from here on: `width`, or with `step` the board's next or
    /// previous Track width (its rule and Net Classes').
    pub fn set_width(&mut self, shell: &mut Shell, width: Option<Nm>, step: Option<i32>) {
        self.notice_cancel(shell);
        let Some(current) = self.route.as_ref().map(|r| r.leg.width) else {
            return;
        };
        let width = match (width, step) {
            (Some(w), _) => w,
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
        let route = self.route.as_mut().expect("routing");
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
        if let Some((at, _)) = self.cursor {
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
        self.commit.clear();
        self.hollow.clear();
        self.ghost.clear();
        self.removal.clear();
        let mut steps: Vec<Value> = Vec::new();
        let net = route.net;
        let plan = self.loop_plan(shell, &route, route.last(), route.leg.layer);
        self.note = plan.note.clone();
        if !plan.delete_tracks.is_empty() {
            steps.push(json!({"id": "pcb.track.delete", "params": {"tracks": plan.delete_tracks}}));
        }
        if !plan.delete_vias.is_empty() {
            steps.push(json!({"id": "pcb.via.delete", "params": {"vias": plan.delete_vias}}));
        }
        for part in &plan.keep {
            let mut params = json!({
                "layer": part.layer,
                "points": [{"x": part.from.x, "y": part.from.y}, {"x": part.to.x, "y": part.to.y}],
                "width": part.width.0,
            });
            if let Some(net) = part.net {
                params["net"] = json!(net);
            }
            steps.push(json!({"id": "pcb.track.add", "params": params}));
        }
        let mut parts = route.steps;
        parts.push(Step::Leg(route.leg));
        // A Via at the end of the route with nothing after it still counts.
        for part in parts {
            match part {
                Step::Leg(leg) if leg.points.len() > 1 => {
                    let points: Vec<Value> = leg
                        .points
                        .iter()
                        .map(|p| json!({"x": p.x, "y": p.y}))
                        .collect();
                    let mut params = json!({
                        "layer": leg.layer, "points": points, "width": leg.width.0
                    });
                    if let Some(net) = net {
                        params["net"] = json!(net);
                    }
                    steps.push(json!({"id": "pcb.track.add", "params": params}));
                }
                Step::Leg(_) => {}
                Step::Via(at) => {
                    let mut params = json!({"at": {"x": at.x, "y": at.y}});
                    if let Some(net) = net {
                        params["net"] = json!(net);
                    }
                    steps.push(json!({"id": "pcb.via.add", "params": params}));
                }
            }
        }
        match steps.len() {
            0 => {}
            1 => {
                let id = steps[0]["id"].as_str().expect("a step id").to_owned();
                let _ = shell.run_reporting(&id, steps[0]["params"].clone());
            }
            _ => {
                let _ = shell.run_reporting("batch", json!({ "commands": steps }));
            }
        }
    }

    /// PROTOTYPE: what old copper goes if the route ends at `at` on `layer`.
    fn loop_plan(
        &self,
        shell: &Shell,
        route: &Route,
        at: Point,
        layer: Uuid,
    ) -> super::proto_loop::LoopPlan {
        let none = super::proto_loop::LoopPlan::default();
        if !self.opts.loop_removal {
            return none;
        }
        let Some(net) = route.net else {
            return none;
        };
        let (Some(pcb), Some(copper)) =
            (board(shell).and_then(|(p, _)| p.pcb()), self.copper(shell))
        else {
            return none;
        };
        let first = route
            .steps
            .iter()
            .find_map(|s| match s {
                Step::Leg(l) => Some(l),
                Step::Via(_) => None,
            })
            .unwrap_or(&route.leg);
        let width = route
            .steps
            .iter()
            .filter_map(|s| match s {
                Step::Leg(l) => Some(l.width),
                Step::Via(_) => None,
            })
            .chain([route.leg.width])
            .min()
            .expect("the leg");
        super::proto_loop::plan(pcb, &copper, net, (first.layer, first.points[0]), (layer, at), width, true)
    }

    /// Esc, or another Tool: drops the route (or the Via at the cursor) unplaced.
    pub fn cancel(&mut self, shell: &mut Shell) {
        if self.route.take().is_some() && shell.tool_preview().is_some_and(|t| t.title == title()) {
            shell.end_tool();
        }
        self.hits.clear();
        self.keepouts.clear();
        self.via = None;
        self.commit.clear();
        self.hollow.clear();
        self.ghost.clear();
        self.removal.clear();
    }

    /// Drops the route when an agent's edit cancelled its preview.
    fn notice_cancel(&mut self, shell: &Shell) {
        if self.route.is_some() && shell.tool_preview().is_none_or(|t| t.title != title()) {
            self.route = None;
            self.hits.clear();
            self.keepouts.clear();
        }
    }

    fn snap_to(&mut self, shell: &Shell, at: Point) -> Point {
        let grid = match board(shell) {
            Some((_, pcb)) => shell.grid(pcb),
            None => crate::grid::pcb_default(shell.board_view().units),
        };
        let targets = board(shell)
            .and_then(|(p, _)| p.pcb())
            .filter(|_| shell.preferences().pcb.electrical_snap)
            .map(snap::targets)
            .unwrap_or_default();
        let snapped = snap::snap(
            at,
            grid,
            &targets,
            self.radius.unwrap_or(DEFAULT_SNAP_RADIUS),
        );
        self.cursor = Some(snapped);
        snapped.0
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
            let diameter = net_rules(pcb, net).via_diameter;
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
        let Some(route) = &self.route else {
            return;
        };
        let route = route.clone();
        let path = corner_path(route.last(), at, self.corner);
        let (layer, width, net) = (route.leg.layer, route.leg.width, route.net);
        let ignore = if net.is_none() {
            route.start_objects.clone()
        } else {
            Vec::new()
        };
        let proposal = |from: Point, to: Point| Proposal::Track {
            layer,
            geometry: TrackGeometry::Line { from, to },
            width,
            net,
        };
        let mut reached = vec![path[0]];
        let mut ghost: Vec<Point> = Vec::new();
        for (i, pair) in path.windows(2).enumerate() {
            let (hits, keepouts) = self.query(shell, &proposal(pair[0], pair[1]), &ignore);
            let clear = hits.is_empty() && keepouts.is_empty();
            self.hits.extend(hits);
            self.keepouts.extend(keepouts);
            if clear || self.opts.obstacle == Obstacle::Ignore {
                reached.push(pair[1]);
                continue;
            }
            // Stop: the longest clear part of this segment.
            let (mut lo, mut hi) = (0.0f64, 1.0f64);
            let along = |t: f64| {
                Point::new(
                    Nm((pair[0].x.0 as f64 + (pair[1].x.0 - pair[0].x.0) as f64 * t).round() as i64),
                    Nm((pair[0].y.0 as f64 + (pair[1].y.0 - pair[0].y.0) as f64 * t).round() as i64),
                )
            };
            for _ in 0..18 {
                let mid = (lo + hi) / 2.0;
                let (h, k) = self.query(shell, &proposal(pair[0], along(mid)), &ignore);
                if h.is_empty() && k.is_empty() {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            let clip = along(lo);
            if clip != pair[0] {
                reached.push(clip);
            }
            ghost.push(clip);
            ghost.extend_from_slice(&path[i + 1..]);
            break;
        }
        let reach = ghost.is_empty();
        let on_net = reach && self.ends_here(shell, at);
        let ends = on_net && self.opts.auto_terminate;
        if reached.len() > 2 && self.opts.lookahead && !ends {
            self.commit = reached[..2].to_vec();
            self.hollow = reached[1..].to_vec();
        } else {
            self.commit = reached;
            self.hollow = Vec::new();
        }
        self.ghost = ghost;
        self.note = None;
        self.removal = Vec::new();
        if on_net {
            let plan = self.loop_plan(shell, &route, at, layer);
            self.removal = plan.removed;
            self.note = plan.note;
        } else if reach && self.in_zone_only(shell, at) {
            self.note = Some("in a Zone of this Net: not connected, end with E (Via) or Enter".into());
        }
    }

    /// PROTOTYPE: the cursor is on a same-Net Zone and on no other copper of the Net.
    fn in_zone_only(&self, shell: &Shell, at: Point) -> bool {
        let Some(route) = &self.route else { return false };
        let (Some(net), Some(copper)) = (route.net, self.copper(shell)) else {
            return false;
        };
        copper
            .items_at(route.leg.layer, at)
            .into_iter()
            .any(|i| matches!(i, Item::Zone { .. }) && copper.net(i) == Some(net))
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
        let keepouts = zone::keepouts_hit(pcb, proposal);
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
        None
    }

    fn blocked_by(&mut self, shell: &Shell, proposal: &Proposal) -> Option<String> {
        let (hits, keepouts) = self.query(shell, proposal, &[]);
        self.hits = hits;
        self.keepouts = keepouts;
        self.why_blocked(shell)
    }

    /// Starts a route at `at` on the active copper Layer.
    fn start(&mut self, shell: &mut Shell, at: Point, raw: Point) {
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
        // PROTOTYPE: copper on any Layer first (taking its Layer), then a Ratsnest line.
        let on_copper: Vec<Uuid> = pcb
            .copper_layers()
            .map(|l| l.uuid)
            .filter(|l| !copper.items_at(*l, at).is_empty())
            .collect();
        let mut connection = None;
        let mut at = at;
        let mut layer = if on_copper.contains(&layer) {
            layer
        } else {
            on_copper.first().copied().unwrap_or(layer)
        };
        if on_copper.is_empty() {
            let reach = self.radius.unwrap_or(DEFAULT_SNAP_RADIUS);
            if let Some(line) = nearest_line(&copper.ratsnest(), raw, reach) {
                let near_from = dist(raw, line.from) <= dist(raw, line.to);
                at = if near_from { line.from } else { line.to };
                connection = Some(line);
                layer = pcb
                    .copper_layers()
                    .map(|l| l.uuid)
                    .find(|l| !copper.items_at(*l, at).is_empty())
                    .unwrap_or(layer);
            }
        }
        let items = copper.items_at(layer, at);
        let net = connection.map(|l| l.net).or_else(|| copper.net_at(layer, at));
        let start_objects: Vec<Uuid> = items.iter().map(|i| i.object()).collect();
        let picked = items.iter().find_map(|i| match i {
            Item::Track { uuid } => pcb.track(*uuid).map(|t| t.width),
            _ => None,
        });
        let project = shell.engine().project().expect("a Project");
        let width = picked.unwrap_or(net_rules(project.pcb().expect("a board"), net).track_width);
        let _ = shell.run_reporting("pcb.layer.activate", json!({ "layer": layer }));
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
            connection,
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

    /// Whether `at` is on copper of the route's own Net (other than where it started), so
    /// placing the path there ends the route.
    fn ends_here(&mut self, shell: &Shell, at: Point) -> bool {
        let Some((layer, net)) = self.route.as_ref().map(|r| (r.leg.layer, r.net)) else {
            return false;
        };
        let Some(net) = net else {
            return false;
        };
        let Some(copper) = self.copper(shell) else {
            return false;
        };
        copper
            .items_at(layer, at)
            .into_iter()
            .any(|i| !matches!(i, Item::Zone { .. }) && copper.net(i) == Some(net))
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

/// The board's Track widths to step through: its rule, every Net Class's and (PROTOTYPE
/// stand-in for the real cycle keys) every width already on the board, sorted.
fn widths(pcb: &PcbDocument) -> Vec<Nm> {
    let mut out: Vec<Nm> = std::iter::once(pcb.setup().rules.track_width)
        .chain(pcb.net_classes().filter_map(|c| c.track_width))
        .chain(pcb.tracks().map(|t| t.width))
        .collect();
    out.sort();
    out.dedup();
    out
}

/// The outer copper Layer on the other side from `layer`.
fn other_side(pcb: &PcbDocument, layer: Uuid) -> Uuid {
    let copper: Vec<Uuid> = pcb.copper_layers().map(|l| l.uuid).collect();
    match copper.first() {
        Some(first) if *first == layer => *copper.last().expect("a first"),
        Some(first) => *first,
        None => layer,
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
    let via = |at: Point, diameter: Nm, color: Color32| {
        painter.circle_stroke(screen(at), points(diameter) / 2.0, Stroke::new(2.0, color));
    };
    // PROTOTYPE: a Ratsnest line stays one pixel at every zoom, so it never reads as copper.
    let shown = &shell.board_view().layers.ratsnest;
    if !shown.hidden && let Some(copper) = shell.engine().project().and_then(Copper::of) {
        let color = if ui.visuals().dark_mode {
            Color32::from_rgb(0xd0, 0xd4, 0xdc)
        } else {
            Color32::from_rgb(0x38, 0x3c, 0x44)
        };
        for line in copper.ratsnest() {
            if !shown.hidden_nets.contains(&line.net) {
                painter.line_segment([screen(line.from), screen(line.to)], Stroke::new(1.0, color));
            }
        }
    }
    if shell.tool() == Tool::Route {
        draw_clearance(&painter, shell, input, router);
    }
    if let Some(p) = router.preview() {
        let placed = accent.gamma_multiply(0.8);
        for part in &p.removal {
            path(
                &[part.from, part.to],
                part.width,
                refused.gamma_multiply(0.45),
            );
        }
        for leg in &p.legs {
            path(&leg.points, leg.width, placed);
        }
        path(&p.placed, p.width, placed);
        // What the next click places, then the hollow tail, then the unreachable ghost.
        let commit = if p.violating { refused } else { accent };
        path(&p.commit, p.width, commit.gamma_multiply(0.6));
        for pair in p.hollow.windows(2) {
            let (a, b) = (screen(pair[0]), screen(pair[1]));
            let n = (b - a).normalized().rot90() * points(p.width) / 2.0;
            let edge = Stroke::new(1.0, if p.violating { refused } else { accent });
            painter.line_segment([a + n, b + n], edge);
            painter.line_segment([a - n, b - n], edge);
        }
        for pair in p.ghost.windows(2) {
            painter.line_segment(
                [screen(pair[0]), screen(pair[1])],
                Stroke::new(1.0, refused),
            );
        }
        if let Some(end) = p.ghost.last() {
            let c = screen(*end);
            let k = 5.0;
            let stroke = Stroke::new(2.0, refused);
            painter.line_segment([c + egui::vec2(-k, -k), c + egui::vec2(k, k)], stroke);
            painter.line_segment([c + egui::vec2(-k, k), c + egui::vec2(k, -k)], stroke);
        }
        if let Some(target) = p.target {
            let start = p.placed.last().copied().unwrap_or(target);
            painter.line_segment(
                [screen(start), screen(target)],
                Stroke::new(1.0, Color32::from_rgb(230, 200, 60)),
            );
        }
        let diameter = board(shell)
            .and_then(|(project, _)| project.pcb())
            .map_or(Nm::from_um(600), |pcb| net_rules(pcb, p.net).via_diameter);
        for at in p.vias {
            via(at, diameter, placed);
        }
    } else if shell.tool() == Tool::Via
        && let Some(v) = router.via_cursor()
    {
        via(v.at, v.diameter, if v.blocked { refused } else { accent });
    }
}

/// PROTOTYPE: one button on the routing bar.
struct Tile {
    icon: &'static str,
    on: bool,
    title: &'static str,
    key: &'static str,
    command: &'static str,
    tip: String,
}

/// PROTOTYPE: a bar at the bottom centre of the canvas while the Track Tool is active, in
/// the Active Bar's frame and buttons, with each button's key beneath it.
pub fn bar(ui: &egui::Ui, shell: &mut Shell, input: &crate::canvas::CanvasInputs) {
    use egui::{Align2, RichText, Vec2, vec2};
    use crate::{chrome, icons};
    if shell.tool() != Tool::Route {
        return;
    }
    let router = shell.router();
    let tiles = router.tiles();
    let note = router.note.clone();
    let width = router.route.as_ref().map(|r| format!("{} mm", r.leg.width.0 as f64 / 1e6));
    let help = "V Via and Layer   Shift+V Via, same Layer   E Via, end route\n\
                O other end   N next connection   L start Layer\n\
                Backspace undo   Enter finish   Esc end, Esc again leave";
    let theme = ui.ctx().theme();
    let clicked = egui::Area::new(egui::Id::new("proto-routing-bar"))
        .order(egui::Order::Middle)
        .fixed_pos(input.rect.center_bottom() - vec2(0.0, 36.0))
        .pivot(Align2::CENTER_BOTTOM)
        .constrain_to(input.rect.shrink(8.0))
        .show(ui.ctx(), |ui| {
            let mut clicked = None;
            ui.vertical(|ui| {
                if let Some(note) = &note {
                    egui::Frame::window(ui.style())
                        .inner_margin(egui::Margin::same(4))
                        .show(ui, |ui| {
                            ui.colored_label(ui.visuals().warn_fg_color, note);
                        });
                }
                egui::Frame::window(ui.style())
                    .inner_margin(egui::Margin::same(3))
                    .corner_radius(egui::CornerRadius::same(5))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing = Vec2::splat(chrome::BAR_SPACING);
                            if let Some(width) = &width {
                                let b = egui::Button::new(RichText::new(width).strong())
                                    .frame_when_inactive(false)
                                    .min_size(Vec2::splat(chrome::BAR_BUTTON));
                                let response = ui.add(b).on_hover_ui(|ui| {
                                    ui.strong("Track width (W)");
                                    ui.label("Next width. Shift+W goes back.");
                                });
                                if response.clicked() {
                                    clicked = Some("pcb.route.width:next");
                                }
                            }
                            for tile in &tiles {
                                let Some(image) = icons::image(tile.icon, icons::TOOLBAR, theme, true) else {
                                    continue;
                                };
                                let response = chrome::icon_button(
                                    ui,
                                    image,
                                    icons::TOOLBAR,
                                    chrome::BAR_BUTTON,
                                    tile.on,
                                    true,
                                )
                                .on_hover_ui(|ui| {
                                    ui.strong(format!("{} ({})", tile.title, tile.key));
                                    ui.label(&tile.tip);
                                });
                                if response.clicked() {
                                    clicked = Some(tile.command);
                                }
                            }
                            if let Some(image) = icons::image("route_keys", icons::TOOLBAR, theme, true) {
                                let _ = chrome::icon_button(
                                    ui,
                                    image,
                                    icons::TOOLBAR,
                                    chrome::BAR_BUTTON,
                                    false,
                                    true,
                                )
                                .on_hover_ui(|ui| {
                                    ui.strong("Other routing keys");
                                    ui.label(help);
                                });
                            }
                        });
                    });
            });
            clicked
        })
        .inner;
    if let Some(command) = clicked {
        let _ = shell.run_reporting(command, json!({}));
    }
}

/// PROTOTYPE: other-Net copper grown by its clearance, near the cursor or everywhere.
fn draw_clearance(
    painter: &egui::Painter,
    shell: &Shell,
    input: &crate::canvas::CanvasInputs,
    router: &Router,
) {
    const NEAR: i64 = 5_000_000;
    let show = router.opts.clearance;
    if show == ClearanceShow::Off {
        return;
    }
    let Some((project, _)) = board(shell) else {
        return;
    };
    let Some(pcb) = project.pcb() else { return };
    let Some(cursor) = router.cursor.map(|c| c.0) else {
        return;
    };
    let (net, layer) = match &router.route {
        Some(r) => (r.net, Some(r.leg.layer)),
        None => (None, shell.board_view().active_layer(project)),
    };
    let Some(layer) = layer else { return };
    let side = pcb.layers().layer(layer).and_then(|l| l.side);
    let color = egui::Color32::from_rgb(220, 70, 60).gamma_multiply(0.16);
    let screen = |p: Point| input.screen_point((p.x.0 as f64, p.y.0 as f64));
    let size = |n: i64| (n as f64 / input.camera.nm_per_point) as f32;
    let near = |p: Point, reach: i64| {
        show == ClearanceShow::All || dist(p, cursor) <= (NEAR + reach) as f64
    };
    let disc = |at: Point, radius: i64, other: Option<Uuid>| {
        if other.is_some() && other == net {
            return;
        }
        let c = clearance_between(pcb, net, other).0;
        if near(at, radius + c) {
            painter.circle_filled(screen(at), size(radius + c), color);
        }
    };
    for v in pcb.vias() {
        disc(v.at, v.diameter.0 / 2, v.net);
    }
    for f in pcb.footprints() {
        for pad in placed_pads(pcb, f) {
            if pad.side.is_none() || pad.side == side {
                disc(pad.at, pad.width.0.max(pad.height.0) / 2, pad.net);
            }
        }
    }
    for t in pcb.tracks().filter(|t| t.layer == layer) {
        if t.net.is_some() && t.net == net {
            continue;
        }
        let c = clearance_between(pcb, net, t.net).0;
        let (a, b) = t.geometry.ends();
        let reach = dist(a, b) as i64 + c;
        if near(a, reach) {
            let w = size(t.width.0 + 2 * c);
            painter.line_segment([screen(a), screen(b)], egui::Stroke::new(w, color));
            painter.circle_filled(screen(a), w / 2.0, color);
            painter.circle_filled(screen(b), w / 2.0, color);
        }
    }
}
