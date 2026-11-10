//! The Auto Wire router itself: an A* search for a 90° path over a grid, around boxes and
//! points that must not be touched, crossing Wires only at a price.
//!
//! The graph is the grid's lines in a window about the two ends (plus a line through each
//! end, so off-grid ends work). Every cost is an integer and ties break on node order, so a
//! route depends only on its inputs.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use reda_model::geom::{Bounds, Nm, Point};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// What a route goes around.
#[derive(Clone, Debug, Default)]
pub struct Obstacles {
    /// Areas a route never enters or runs along, edges included: Unit bodies with their
    /// Pins. A route keeps a grid step clear of them where it can.
    pub boxes: Vec<Bounds>,
    /// Points a route never passes over: Pin ends, Wire vertices, Net Labels, Ports, Power
    /// Ports and No-Connect Markers.
    pub points: Vec<Point>,
    /// Wire runs, as vertex lists. A route never runs along one, crosses one only at a
    /// price, and never turns on one.
    pub wires: Vec<Vec<Point>>,
}

/// How hard a route tries not to cross Wires: the Avoid cutting wires slider.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Avoid {
    /// A crossing costs as much as two grid steps of Wire.
    Low,
    /// A crossing costs as much as ten grid steps.
    #[default]
    Medium,
    /// A crossing costs as much as a hundred grid steps.
    High,
}

impl Avoid {
    fn steps(self) -> i64 {
        match self {
            Avoid::Low => 2,
            Avoid::Medium => 10,
            Avoid::High => 100,
        }
    }
}

/// How to route.
#[derive(Clone, Debug)]
pub struct Options {
    /// The grid the route's corners sit on (besides the two ends).
    pub grid: Nm,
    pub avoid: Avoid,
    /// Time Out After, in milliseconds. It is spent as a fixed amount of search work per
    /// millisecond rather than measured on a clock, so every machine gives the same answer.
    pub timeout_ms: u32,
}

/// Search steps one millisecond of Time Out After buys.
const STEPS_PER_MS: u64 = 400;
/// The most grid nodes a route may search over.
const MAX_NODES: usize = 1_000_000;
/// How far past the ends the search area reaches, in grid steps.
const MARGIN: i64 = 10;
/// How far past a box the search area grows when the box reaches into it, in grid steps.
const BOX_MARGIN: i64 = 3;
/// A corner's cost, in grid steps of Wire.
const BEND: i64 = 2;
/// The cost of a corner or crossing within a grid step of a box, in grid steps.
const NEAR: i64 = 1;

/// A found route.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Route {
    /// The route's vertices from start to end, corners only.
    pub points: Vec<Point>,
    /// How many Wires it crosses.
    pub crossings: u32,
}

/// Why no route came back.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RouteError {
    /// The two ends are the same point.
    SamePoint,
    /// The grid is not positive.
    BadGrid,
    /// The search area holds more grid nodes than the router takes.
    TooLarge,
    /// Every path is blocked.
    NoRoute,
    /// Time Out After ran out first.
    TimedOut,
}

/// The cheapest 90° route from `from` to `to` around `obstacles`: shortest, then fewest
/// corners, crossings and near passes, by the weights above. A box that holds an end
/// strictly inside is ignored, and an end may sit on a box edge, a point or a Wire.
pub fn route(
    obstacles: &Obstacles,
    from: Point,
    to: Point,
    options: &Options,
) -> Result<Route, RouteError> {
    if from == to {
        return Err(RouteError::SamePoint);
    }
    let g = options.grid.0;
    if g <= 0 {
        return Err(RouteError::BadGrid);
    }
    let boxes: Vec<Bounds> = obstacles
        .boxes
        .iter()
        .copied()
        .filter(|b| !strictly_inside(*b, from) && !strictly_inside(*b, to))
        .collect();
    let window = window(&boxes, from, to, g);
    let grid = Grid::new(window, from, to, g)?;
    let mut map = Map::new(&grid);
    let ends = [from, to];
    for b in &boxes {
        map.block_box(&grid, *b);
        map.mark_near(&grid, inflate(*b, g));
    }
    for &q in obstacles.points.iter().filter(|q| !ends.contains(q)) {
        map.block_box(&grid, Bounds::of_point(q));
    }
    for wire in &obstacles.wires {
        for &q in wire.iter().filter(|q| !ends.contains(q)) {
            map.block_box(&grid, Bounds::of_point(q));
        }
        for pair in wire.windows(2) {
            map.wire_segment(&grid, pair[0], pair[1]);
        }
    }
    let costs = Costs {
        bend: BEND * g,
        cross: options.avoid.steps() * g,
        near: NEAR * g,
    };
    let budget = u64::from(options.timeout_ms) * STEPS_PER_MS;
    search(&grid, &map, &costs, from, to, budget)
}

fn strictly_inside(b: Bounds, p: Point) -> bool {
    b.min.x < p.x && p.x < b.max.x && b.min.y < p.y && p.y < b.max.y
}

fn inflate(b: Bounds, by: i64) -> Bounds {
    Bounds {
        min: Point::new(b.min.x - Nm(by), b.min.y - Nm(by)),
        max: Point::new(b.max.x + Nm(by), b.max.y + Nm(by)),
    }
}

fn overlaps(a: Bounds, b: Bounds) -> bool {
    a.min.x <= b.max.x && b.min.x <= a.max.x && a.min.y <= b.max.y && b.min.y <= a.max.y
}

/// The search area: the ends' box plus a margin, grown to take in every box that reaches
/// into it (so a route can get round it), until nothing more reaches in.
fn window(boxes: &[Bounds], from: Point, to: Point, g: i64) -> Bounds {
    let mut window = inflate(
        Bounds::of_points([from, to]).expect("two points"),
        MARGIN * g,
    );
    let mut taken = vec![false; boxes.len()];
    loop {
        let mut grew = false;
        for (b, taken) in boxes.iter().zip(&mut taken) {
            if !*taken && overlaps(*b, window) {
                *taken = true;
                grew = true;
                window = window.union(inflate(*b, BOX_MARGIN * g));
            }
        }
        if !grew {
            return window;
        }
    }
}

/// The grid lines of the search area.
struct Grid {
    xs: Vec<i64>,
    ys: Vec<i64>,
}

impl Grid {
    fn new(window: Bounds, from: Point, to: Point, g: i64) -> Result<Grid, RouteError> {
        let lines = |lo: i64, hi: i64, ends: [i64; 2]| -> Result<Vec<i64>, RouteError> {
            let first = lo.div_euclid(g) + 1;
            let last = hi.div_euclid(g);
            let count = usize::try_from((last - first + 1).max(0)).unwrap_or(usize::MAX);
            if count > MAX_NODES {
                return Err(RouteError::TooLarge);
            }
            let mut v: Vec<i64> = (first..=last).map(|k| k * g).collect();
            v.extend(ends);
            v.sort_unstable();
            v.dedup();
            Ok(v)
        };
        let grid = Grid {
            xs: lines(window.min.x.0, window.max.x.0, [from.x.0, to.x.0])?,
            ys: lines(window.min.y.0, window.max.y.0, [from.y.0, to.y.0])?,
        };
        if grid.xs.len().saturating_mul(grid.ys.len()) > MAX_NODES {
            return Err(RouteError::TooLarge);
        }
        Ok(grid)
    }

    fn nx(&self) -> usize {
        self.xs.len()
    }

    fn ny(&self) -> usize {
        self.ys.len()
    }

    fn node(&self, i: usize, j: usize) -> usize {
        j * self.nx() + i
    }

    fn node_at(&self, p: Point) -> usize {
        let i = self.xs.binary_search(&p.x.0).expect("an end's line");
        let j = self.ys.binary_search(&p.y.0).expect("an end's line");
        self.node(i, j)
    }

    fn point(&self, n: usize) -> Point {
        Point::new(Nm(self.xs[n % self.nx()]), Nm(self.ys[n / self.nx()]))
    }
}

/// Indices of `lines` within `lo..=hi`.
fn closed(lines: &[i64], lo: i64, hi: i64) -> std::ops::Range<usize> {
    lines.partition_point(|&v| v < lo)..lines.partition_point(|&v| v <= hi)
}

/// Indices `i` of the gaps `(lines[i], lines[i + 1])`, their ends left out, that meet
/// `lo..=hi` (for `lo < hi`, the same gaps as meet `lo..hi`).
fn gaps(lines: &[i64], lo: i64, hi: i64) -> std::ops::Range<usize> {
    let after_lo = lines.partition_point(|&v| v <= lo);
    let before_hi = lines.partition_point(|&v| v < hi);
    let start = after_lo.saturating_sub(1);
    let end = before_hi.min(lines.len().saturating_sub(1));
    start..end.max(start)
}

/// The index of the gap strictly holding `num / den` (`den > 0`), if any.
fn gap_holding(lines: &[i64], num: i128, den: i128) -> Option<usize> {
    let above = lines.partition_point(|&v| i128::from(v) * den <= num);
    (above > 0 && above < lines.len() && i128::from(lines[above - 1]) * den != num)
        .then(|| above - 1)
}

/// What each node and grid edge holds. Horizontal edge `(i, j)` joins nodes `(i, j)` and
/// `(i + 1, j)`; vertical edge `(i, j)` joins `(i, j)` and `(i, j + 1)`.
struct Map {
    blocked: Vec<bool>,
    near: Vec<bool>,
    on_wire: Vec<bool>,
    h_blocked: Vec<bool>,
    h_cross: Vec<u16>,
    v_blocked: Vec<bool>,
    v_cross: Vec<u16>,
}

impl Map {
    fn new(grid: &Grid) -> Map {
        let (nx, ny) = (grid.nx(), grid.ny());
        let h = nx.saturating_sub(1) * ny;
        let v = nx * ny.saturating_sub(1);
        Map {
            blocked: vec![false; nx * ny],
            near: vec![false; nx * ny],
            on_wire: vec![false; nx * ny],
            h_blocked: vec![false; h],
            h_cross: vec![0; h],
            v_blocked: vec![false; v],
            v_cross: vec![0; v],
        }
    }

    fn h(grid: &Grid, i: usize, j: usize) -> usize {
        j * (grid.nx() - 1) + i
    }

    fn v(grid: &Grid, i: usize, j: usize) -> usize {
        i * (grid.ny() - 1) + j
    }

    /// Blocks the nodes in `b` and the edges that touch it between their ends.
    fn block_box(&mut self, grid: &Grid, b: Bounds) {
        let (x0, x1, y0, y1) = (b.min.x.0, b.max.x.0, b.min.y.0, b.max.y.0);
        for j in closed(&grid.ys, y0, y1) {
            for i in closed(&grid.xs, x0, x1) {
                self.blocked[grid.node(i, j)] = true;
            }
            for i in gaps(&grid.xs, x0, x1) {
                self.h_blocked[Map::h(grid, i, j)] = true;
            }
        }
        for i in closed(&grid.xs, x0, x1) {
            for j in gaps(&grid.ys, y0, y1) {
                self.v_blocked[Map::v(grid, i, j)] = true;
            }
        }
    }

    fn mark_near(&mut self, grid: &Grid, b: Bounds) {
        for j in closed(&grid.ys, b.min.y.0, b.max.y.0) {
            for i in closed(&grid.xs, b.min.x.0, b.max.x.0) {
                self.near[grid.node(i, j)] = true;
            }
        }
    }

    /// Records Wire segment `a`–`b`: edges along it are blocked, nodes inside it are on a
    /// Wire, and edges it crosses between their ends count a crossing. Its ends are
    /// blocked as points by the caller.
    fn wire_segment(&mut self, grid: &Grid, a: Point, b: Point) {
        let (a, b) = if (a.x, a.y) <= (b.x, b.y) {
            (a, b)
        } else {
            (b, a)
        };
        let (ax, ay, bx, by) = (a.x.0, a.y.0, b.x.0, b.y.0);
        if ay == by {
            if let Ok(j) = grid.ys.binary_search(&ay) {
                for i in gaps(&grid.xs, ax, bx) {
                    self.h_blocked[Map::h(grid, i, j)] = true;
                }
                for i in open(&grid.xs, ax, bx) {
                    self.on_wire[grid.node(i, j)] = true;
                }
            } else if let Some(j) = gap_holding(&grid.ys, i128::from(ay), 1) {
                for i in open(&grid.xs, ax, bx) {
                    self.v_cross[Map::v(grid, i, j)] += 1;
                }
            }
            return;
        }
        if ax == bx {
            let (ay, by) = (ay.min(by), ay.max(by));
            if let Ok(i) = grid.xs.binary_search(&ax) {
                for j in gaps(&grid.ys, ay, by) {
                    self.v_blocked[Map::v(grid, i, j)] = true;
                }
                for j in open(&grid.ys, ay, by) {
                    self.on_wire[grid.node(i, j)] = true;
                }
            } else if let Some(i) = gap_holding(&grid.xs, i128::from(ax), 1) {
                for j in open(&grid.ys, ay, by) {
                    self.h_cross[Map::h(grid, i, j)] += 1;
                }
            }
            return;
        }
        // At an angle: where it meets each vertical line, then each horizontal line.
        let (dx, dy) = (i128::from(bx - ax), i128::from(by - ay));
        for i in open(&grid.xs, ax, bx) {
            let num = i128::from(ay) * dx + dy * i128::from(grid.xs[i] - ax);
            self.angled_meet(grid, num, dx, |j| (i, j), true);
        }
        let (ylo, yhi) = (ay.min(by), ay.max(by));
        let (dy_abs, sign) = (dy.abs(), dy.signum());
        for j in open(&grid.ys, ylo, yhi) {
            // x = ax + dx * (y - ay) / dy, over the positive denominator |dy|.
            let num = i128::from(ax) * dy_abs + dx * sign * i128::from(grid.ys[j] - ay);
            self.angled_meet(grid, num, dy_abs, |i| (i, j), false);
        }
    }

    /// An angled Wire meets a grid line at `num / den` along it: on a node, or between two.
    fn angled_meet(
        &mut self,
        grid: &Grid,
        num: i128,
        den: i128,
        at: impl Fn(usize) -> (usize, usize),
        vertical_line: bool,
    ) {
        let lines = if vertical_line { &grid.ys } else { &grid.xs };
        if num % den == 0
            && let Ok(k) = lines.binary_search(&i64::try_from(num / den).unwrap_or(i64::MAX))
        {
            let (i, j) = at(k);
            self.on_wire[grid.node(i, j)] = true;
            return;
        }
        if let Some(k) = gap_holding(lines, num, den) {
            let (i, j) = at(k);
            if vertical_line {
                self.v_cross[Map::v(grid, i, j)] += 1;
            } else {
                self.h_cross[Map::h(grid, i, j)] += 1;
            }
        }
    }
}

/// Indices of `lines` strictly between `lo` and `hi`.
fn open(lines: &[i64], lo: i64, hi: i64) -> std::ops::Range<usize> {
    lines.partition_point(|&v| v <= lo)..lines.partition_point(|&v| v < hi)
}

struct Costs {
    bend: i64,
    cross: i64,
    near: i64,
}

/// Directions: +x, −x, +y, −y, and "none yet" at the start.
const START: usize = 4;

fn reverse(d: usize) -> usize {
    d ^ 1
}

fn search(
    grid: &Grid,
    map: &Map,
    costs: &Costs,
    from: Point,
    to: Point,
    budget: u64,
) -> Result<Route, RouteError> {
    let (nx, ny) = (grid.nx(), grid.ny());
    let start = grid.node_at(from);
    let end = grid.node_at(to);
    let (ex, ey) = (to.x.0, to.y.0);
    let heuristic = |n: usize| {
        let q = grid.point(n);
        (q.x.0 - ex).abs() + (q.y.0 - ey).abs()
    };
    let states = nx * ny * 5;
    let mut best = vec![i64::MAX; states];
    let mut prev = vec![usize::MAX; states];
    let mut heap = BinaryHeap::new();
    let first = start * 5 + START;
    best[first] = 0;
    heap.push(Reverse((heuristic(start), heuristic(start), first)));
    let mut steps = 0u64;
    while let Some(Reverse((_, _, state))) = heap.pop() {
        let (n, d) = (state / 5, state % 5);
        let g = best[state];
        if n == end {
            return Ok(path(grid, map, &prev, state, start));
        }
        steps += 1;
        if steps > budget {
            return Err(RouteError::TimedOut);
        }
        let (i, j) = (n % nx, n / nx);
        for nd in 0..4 {
            if d != START && nd == reverse(d) {
                continue;
            }
            if map.on_wire[n] && n != start && nd != d {
                continue;
            }
            let Some((m, length, edge_blocked, edge_cross)) = step(grid, map, i, j, nd) else {
                continue;
            };
            if edge_blocked || (map.blocked[m] && m != end) {
                continue;
            }
            let mut cost = g + length + i64::from(edge_cross) * costs.cross;
            if d != START && nd != d {
                cost += costs.bend;
                if map.near[n] {
                    cost += costs.near;
                }
            }
            if m != end {
                if map.on_wire[m] {
                    cost += costs.cross;
                }
                if map.near[m] {
                    cost += costs.near;
                }
            }
            let next = m * 5 + nd;
            if cost < best[next] {
                best[next] = cost;
                prev[next] = state;
                let h = heuristic(m);
                heap.push(Reverse((cost + h, h, next)));
            }
        }
    }
    Err(RouteError::NoRoute)
}

/// The neighbour of node `(i, j)` in direction `d`, the edge's length, whether it is
/// blocked, and the Wires it crosses.
fn step(grid: &Grid, map: &Map, i: usize, j: usize, d: usize) -> Option<(usize, i64, bool, u16)> {
    let (nx, ny) = (grid.nx(), grid.ny());
    match d {
        0 if i + 1 < nx => {
            let e = Map::h(grid, i, j);
            let len = grid.xs[i + 1] - grid.xs[i];
            Some((grid.node(i + 1, j), len, map.h_blocked[e], map.h_cross[e]))
        }
        1 if i > 0 => {
            let e = Map::h(grid, i - 1, j);
            let len = grid.xs[i] - grid.xs[i - 1];
            Some((grid.node(i - 1, j), len, map.h_blocked[e], map.h_cross[e]))
        }
        2 if j + 1 < ny => {
            let e = Map::v(grid, i, j);
            let len = grid.ys[j + 1] - grid.ys[j];
            Some((grid.node(i, j + 1), len, map.v_blocked[e], map.v_cross[e]))
        }
        3 if j > 0 => {
            let e = Map::v(grid, i, j - 1);
            let len = grid.ys[j] - grid.ys[j - 1];
            Some((grid.node(i, j - 1), len, map.v_blocked[e], map.v_cross[e]))
        }
        _ => None,
    }
}

/// The route back from `state` to the start, corners only, with its crossings.
fn path(grid: &Grid, map: &Map, prev: &[usize], state: usize, start: usize) -> Route {
    let mut states = vec![state];
    let mut s = state;
    while s / 5 != start || s % 5 != START {
        s = prev[s];
        states.push(s);
    }
    states.reverse();
    let mut crossings = 0u32;
    let mut points = vec![grid.point(start)];
    for (k, pair) in states.windows(2).enumerate() {
        let (a, b) = (pair[0], pair[1]);
        let (n, d) = (a / 5, b % 5);
        let (i, j) = (n % grid.nx(), n / grid.nx());
        let (m, _, _, cross) = step(grid, map, i, j, d).expect("a step taken");
        crossings += u32::from(cross);
        let last = k + 2 == states.len();
        if !last && map.on_wire[m] {
            crossings += 1;
        }
        let turns = states.get(k + 2).is_none_or(|c| c % 5 != d);
        if turns {
            points.push(grid.point(m));
        }
    }
    Route { points, crossings }
}
