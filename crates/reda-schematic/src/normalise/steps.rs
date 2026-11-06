//! The normalisation steps, on a working copy of one sheet's Wires.

use std::collections::{BTreeMap, BTreeSet};

use reda_model::geom::{Nm, Point};
use reda_model::{SchematicSheet, Uuid};

use super::Lineage;
use crate::connectivity::placed_pins;

/// One sheet's Wires being normalised, with what they may connect to.
pub(crate) struct Work {
    /// Every Wire, by UUID, as its vertices.
    pub wires: BTreeMap<Uuid, Vec<Point>>,
    /// The Wires on the sheet before normalising.
    known: BTreeSet<Uuid>,
    /// Each Wire with the Wires before it whose drawing it holds.
    origins: BTreeMap<Uuid, BTreeSet<Uuid>>,
    /// The Wires being normalised.
    involved: BTreeSet<Uuid>,
    /// The Wires the edit itself, or a step since, changed: the ones that move first
    /// when converting a 4-way.
    changed: BTreeSet<Uuid>,
    /// Pin points of Units the edit placed, moved or reshaped: the Wires through them are
    /// involved too.
    moved: BTreeSet<Point>,
    /// Every Pin's connection point.
    pins: BTreeSet<Point>,
    /// Ports and Power Ports: a Wire passing over one has a Junction there.
    ports: BTreeSet<Point>,
    /// Net Labels: they connect along a Wire without a Junction.
    labels: BTreeSet<Point>,
    /// No-Connect Markers.
    markers: BTreeSet<Point>,
}

/// A Wire leaving a point: towards its neighbouring vertex `toward`, and whether that
/// point is the Wire's first (`Some(true)`) or last (`Some(false)`) vertex.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Arm {
    wire: Uuid,
    toward: Point,
    end: Option<bool>,
}

impl Work {
    /// A working copy of `sheet`'s Wires, normalising `involved` and the Wires through
    /// `moved`.
    pub fn new(
        sheet: &SchematicSheet,
        involved: BTreeSet<Uuid>,
        changed: BTreeSet<Uuid>,
        moved: BTreeSet<Point>,
    ) -> Work {
        let wires: BTreeMap<Uuid, Vec<Point>> = sheet
            .wires()
            .map(|w| (w.uuid, w.vertices.clone()))
            .collect();
        Work {
            known: wires.keys().copied().collect(),
            origins: wires.keys().map(|&u| (u, BTreeSet::from([u]))).collect(),
            wires,
            involved,
            changed,
            moved,
            pins: placed_pins(sheet).into_iter().map(|(_, _, p)| p).collect(),
            ports: (sheet.ports().map(|p| p.at))
                .chain(sheet.power_ports().map(|p| p.at))
                .collect(),
            labels: sheet.net_labels().map(|l| l.at).collect(),
            markers: sheet.no_connects().map(|m| m.at).collect(),
        }
    }

    /// Where each Wire came from.
    pub fn lineage(&self) -> Lineage {
        Lineage {
            known: self.known.clone(),
            origins: (self.origins.iter())
                .filter(|(u, _)| self.wires.contains_key(u))
                .map(|(&u, from)| (u, from.clone()))
                .collect(),
        }
    }

    /// Drops the zero-length segments of the involved Wires, and the Wires left with no
    /// length.
    pub fn drop_zero_length(&mut self) {
        for uuid in self.involved.clone() {
            let Some(v) = self.wires.get(&uuid) else {
                continue;
            };
            match dedup(v.clone()) {
                Some(v) => {
                    self.wires.insert(uuid, v);
                }
                None => self.remove(uuid),
            }
        }
    }

    fn remove(&mut self, uuid: Uuid) {
        self.wires.remove(&uuid);
        self.origins.remove(&uuid);
        self.involved.remove(&uuid);
        self.changed.remove(&uuid);
    }

    /// Replaces Wire `uuid` by `pieces`, in order along it: the first keeps the UUID, the
    /// rest get new ones. All of them are involved.
    fn replace(&mut self, uuid: Uuid, pieces: Vec<Vec<Point>>, new_id: &mut dyn FnMut() -> Uuid) {
        let from = self.origins.get(&uuid).cloned().unwrap_or_default();
        self.remove(uuid);
        for (i, piece) in pieces.into_iter().enumerate() {
            let id = if i == 0 { uuid } else { new_id() };
            self.wires.insert(id, piece);
            self.origins.insert(id, from.clone());
            self.involved.insert(id);
            self.changed.insert(id);
        }
    }

    /// Whether anything but Wires `except` is at `p`: another Wire through it, a Pin, a
    /// Net Label, a Port, a Power Port or a No-Connect Marker.
    fn occupied(&self, p: Point, except: &[Uuid]) -> bool {
        self.pins.contains(&p)
            || self.ports.contains(&p)
            || self.labels.contains(&p)
            || self.markers.contains(&p)
            || (self.wires.iter()).any(|(u, v)| !except.contains(u) && contains(v, p))
    }

    /// Components Cut Wires for one placed Unit with Pins at `pins`: each Wire two or more
    /// of them land on partway along is split at each, and the pieces running straight
    /// from one Pin to the next are removed, so the part isn't shorted. A piece that turns
    /// a corner, or that something else connects to (another Wire's end, a Pin, a Net
    /// Label, a Port or a Power Port), stays, because removing it would orphan that branch.
    pub fn split_wires_at_pins(&mut self, pins: &[Point], new_id: &mut dyn FnMut() -> Uuid) {
        let candidates: Vec<Uuid> = (self.wires.iter())
            .filter(|(_, v)| pins.iter().any(|&p| passes_over(v, p)))
            .map(|(&u, _)| u)
            .collect();
        for uuid in candidates {
            let v = self.wires[&uuid].clone();
            let cuts: BTreeSet<Point> = pins
                .iter()
                .copied()
                .filter(|&p| passes_over(&v, p))
                .collect();
            let pieces = split(&v, &cuts);
            let last = pieces.len() - 1;
            let keep: Vec<bool> = (pieces.iter().enumerate())
                .map(|(i, piece)| {
                    let (a, b) = (piece[0], piece[piece.len() - 1]);
                    i == 0 || i == last || !straight(piece) || self.connects_inside(uuid, a, b)
                })
                .collect();
            if keep.iter().all(|&k| k) {
                continue;
            }
            let kept = (pieces.into_iter().zip(keep))
                .filter(|(_, k)| *k)
                .filter_map(|(piece, _)| dedup(piece))
                .collect();
            self.replace(uuid, kept, new_id);
        }
    }

    /// Whether something connects strictly between `a` and `b` on Wire `uuid`: another
    /// Wire's end, a Pin, a Net Label, a Port or a Power Port.
    fn connects_inside(&self, uuid: Uuid, a: Point, b: Point) -> bool {
        let inside = |p: Point| p != a && p != b && p.on_segment(a, b);
        (self.wires.iter()).any(|(u, v)| *u != uuid && ends(v).into_iter().any(inside))
            || self.pins.iter().copied().any(inside)
            || self.labels.iter().copied().any(inside)
            || self.ports.iter().copied().any(inside)
    }

    /// Optimize Wires: removes the collinear vertices of the involved Wires, then merges
    /// involved Wires with the Wires they overlap or continue, until none is left to merge.
    /// The lowest UUID survives.
    pub fn optimize(&mut self) {
        for uuid in self.involved.clone() {
            if let Some(v) = self.wires.get(&uuid) {
                let tidied = tidy(v.clone());
                self.wires.insert(uuid, tidied);
            }
        }
        loop {
            let found = self.involved.iter().find_map(|&x| {
                (self.wires.keys())
                    .filter(|&&y| y != x)
                    .find_map(|&y| self.merge(x, y).map(|m| (x, y, m)))
            });
            let Some((x, y, merged)) = found else {
                break;
            };
            let (keep, gone) = (x.min(y), x.max(y));
            let from = self.origins.get(&gone).cloned().unwrap_or_default();
            self.remove(gone);
            self.origins.entry(keep).or_default().extend(from);
            self.wires.insert(keep, tidy(merged));
            self.involved.insert(keep);
            self.changed.insert(keep);
        }
    }

    /// Wires `x` and `y` as one, when they can merge: one lies along the other, they
    /// overlap end to end along one line, or they meet end to end in line with nothing
    /// else at that point. Never when an end that the merge swallows is where a third Wire
    /// passes, which would disconnect it.
    fn merge(&self, x: Uuid, y: Uuid) -> Option<Vec<Point>> {
        let (xv, yv) = (&self.wires[&x], &self.wires[&y]);
        let merged = if lies_along(yv, xv) {
            Some(xv.clone())
        } else if lies_along(xv, yv) {
            Some(yv.clone())
        } else {
            [false, true].into_iter().find_map(|flip_x| {
                let xs = oriented(xv, flip_x);
                [false, true].into_iter().find_map(|flip_y| {
                    let ys = oriented(yv, flip_y);
                    let joined = self.join(&xs, &ys, x, y)?;
                    Some(oriented(&joined, flip_x))
                })
            })
        }?;
        let kept = ends(&merged);
        let swallowed = ends(xv)
            .into_iter()
            .chain(ends(yv))
            .filter(|e| !kept.contains(e));
        for e in swallowed {
            if (self.wires.iter()).any(|(u, v)| *u != x && *u != y && passes_over(v, e)) {
                return None;
            }
        }
        Some(merged)
    }

    /// `xs` followed by `ys` as one path, when `xs` ends where `ys` starts in line with
    /// nothing else there, or `xs`'s last segment overlaps `ys`'s first along one line.
    fn join(&self, xs: &[Point], ys: &[Point], x: Uuid, y: Uuid) -> Option<Vec<Point>> {
        let n = xs.len();
        let (a, ex) = (xs[n - 2], xs[n - 1]);
        let (ey, b) = (ys[0], ys[1]);
        let joined = || xs[..n - 1].iter().chain(&ys[1..]).copied().collect();
        if ex == ey {
            let (d, e) = (ex - a, b - ey);
            let in_line = cross(d, e) == 0 && dot(d, e) > 0;
            return (in_line && !self.occupied(ex, &[x, y]))
                .then(|| xs.iter().chain(&ys[1..]).copied().collect());
        }
        let d = ex - a;
        let collinear = cross(d, ey - a) == 0 && cross(d, b - a) == 0;
        if !collinear || dot(d, b - ey) <= 0 {
            return None;
        }
        let (along_ey, along_b, length) = (dot(d, ey - a), dot(d, b - a), dot(d, d));
        (0 <= along_ey && along_ey < length && along_b >= length).then(joined)
    }

    /// The points where an involved Wire ends, or where a Wire ends on an involved Wire.
    pub fn involved_ends(&self) -> BTreeSet<Point> {
        let mut out = BTreeSet::new();
        for (uuid, v) in &self.wires {
            let involved = self.involved.contains(uuid);
            for e in ends(v) {
                if involved || (self.involved.iter()).any(|i| contains(&self.wires[i], e)) {
                    out.insert(e);
                }
            }
        }
        out
    }

    /// Every 4-way on the sheet: a point where four or more Wire branches meet and at
    /// least one Wire ends (two Wires only crossing don't connect).
    pub fn four_ways(&self) -> Vec<Point> {
        let ends: BTreeSet<Point> = self.wires.values().flat_map(|v| ends(v)).collect();
        ends.into_iter()
            .filter(|&p| self.arms(p).len() >= 4)
            .collect()
    }

    /// The Wires every 4-way at `p` is made of.
    pub fn wires_at(&self, p: Point) -> BTreeSet<Uuid> {
        self.arms(p).into_iter().map(|a| a.wire).collect()
    }

    /// Every Wire branch leaving `p`.
    fn arms(&self, p: Point) -> Vec<Arm> {
        let mut arms = vec![];
        for (&wire, v) in &self.wires {
            let n = v.len();
            if v[0] == p {
                arms.push(Arm {
                    wire,
                    toward: v[1],
                    end: Some(true),
                });
            }
            if v[n - 1] == p {
                arms.push(Arm {
                    wire,
                    toward: v[n - 2],
                    end: Some(false),
                });
            }
            for i in 1..n - 1 {
                if v[i] == p {
                    arms.push(Arm {
                        wire,
                        toward: v[i - 1],
                        end: None,
                    });
                    arms.push(Arm {
                        wire,
                        toward: v[i + 1],
                        end: None,
                    });
                }
            }
            for s in v.windows(2) {
                if p != s[0] && p != s[1] && p.on_segment(s[0], s[1]) {
                    arms.push(Arm {
                        wire,
                        toward: s[0],
                        end: None,
                    });
                    arms.push(Arm {
                        wire,
                        toward: s[1],
                        end: None,
                    });
                }
            }
        }
        arms
    }

    /// Converts the 4-ways among `points` into two adjacent 3-ways: a Wire ending there
    /// (one the edit changed first, else the lowest UUID) is moved `offset` along a crossing
    /// horizontal or vertical branch, turning a mitred corner `offset` before the point.
    /// Returns the points converted.
    pub fn convert_cross(&mut self, points: &BTreeSet<Point>, offset: Nm) -> Vec<Point> {
        let mut converted = vec![];
        for &p in points {
            let arms = self.arms(p);
            if arms.len() < 4 || !arms.iter().any(|a| a.end.is_some()) {
                continue;
            }
            let mut movers: Vec<Arm> = arms.iter().copied().filter(|a| a.end.is_some()).collect();
            movers.sort_by_key(|a| (!self.changed.contains(&a.wire), a.wire));
            let mut targets = arms.clone();
            targets.sort_by_key(|a| (direction_key(a.toward - p), a.wire));
            let moved = movers.into_iter().find_map(|m| {
                let arrival = p - m.toward;
                targets.iter().find_map(|t| {
                    let d = t.toward - p;
                    let axis = (d.x.0 == 0) != (d.y.0 == 0);
                    let reach = d.x.0.abs() + d.y.0.abs();
                    if *t == m || !axis || cross(arrival, d) == 0 || reach <= offset.0 {
                        return None;
                    }
                    let step =
                        Point::new(Nm(d.x.0.signum() * offset.0), Nm(d.y.0.signum() * offset.0));
                    let to = p + step;
                    if self.occupied(to, &[t.wire]) {
                        return None;
                    }
                    Some((m, mitred(&self.wires[&m.wire], m, p, to, offset)?))
                })
            });
            if let Some((m, vertices)) = moved {
                self.wires.insert(m.wire, vertices);
                self.involved.insert(m.wire);
                self.changed.insert(m.wire);
                converted.push(p);
            }
        }
        converted
    }

    /// Break Wires At Autojunctions: splits each involved Wire, and each Wire touching
    /// one or through a moved Pin, at every Junction on its interior: where another Wire
    /// ends on it, or it passes over a Pin, a Port or a Power Port.
    pub fn break_at_junctions(&mut self, new_id: &mut dyn FnMut() -> Uuid) {
        let targets: Vec<Uuid> = (self.wires.iter())
            .filter(|(u, v)| {
                self.involved.contains(u)
                    || self.moved.iter().any(|&p| contains(v, p))
                    || self.involved.iter().any(|i| touches(v, &self.wires[i]))
            })
            .map(|(&u, _)| u)
            .collect();
        for uuid in targets {
            let v = self.wires[&uuid].clone();
            let other_ends = (self.wires.iter())
                .filter(|(u, _)| **u != uuid)
                .flat_map(|(_, w)| ends(w));
            let cuts: BTreeSet<Point> = (other_ends)
                .chain(self.pins.iter().copied())
                .chain(self.ports.iter().copied())
                .filter(|&p| passes_over(&v, p))
                .collect();
            if cuts.is_empty() {
                continue;
            }
            let pieces = split(&v, &cuts).into_iter().filter_map(dedup).collect();
            self.replace(uuid, pieces, new_id);
        }
    }
}

/// Wire `v` with the end that arm `m` leaves `p` from moved to `to`: it turns a corner
/// `offset` before `p` (or at its neighbouring vertex, when that is nearer) and runs
/// straight to `to`.
fn mitred(v: &[Point], m: Arm, p: Point, to: Point, offset: Nm) -> Option<Vec<Point>> {
    let q = m.toward;
    let (dx, dy) = ((p.x - q.x).0 as f64, (p.y - q.y).0 as f64);
    let length = dx.hypot(dy);
    let corner = if length <= offset.0 as f64 {
        q
    } else {
        let k = offset.0 as f64 / length;
        Point::new(
            Nm(p.x.0 - (dx * k).round() as i64),
            Nm(p.y.0 - (dy * k).round() as i64),
        )
    };
    let n = v.len();
    let vertices: Vec<Point> = match m.end {
        Some(true) => [to, corner]
            .into_iter()
            .chain(v[1..].iter().copied())
            .collect(),
        Some(false) => v[..n - 1].iter().copied().chain([corner, to]).collect(),
        None => return None,
    };
    dedup(vertices)
}

/// Orders branch directions: right, up, left, down, then the rest.
fn direction_key(d: Point) -> (u8, i64, i64) {
    let rank = match (d.x.0.signum(), d.y.0.signum()) {
        (1, 0) => 0,
        (0, 1) => 1,
        (-1, 0) => 2,
        (0, -1) => 3,
        _ => 4,
    };
    (rank, d.x.0, d.y.0)
}

fn ends(v: &[Point]) -> [Point; 2] {
    [v[0], v[v.len() - 1]]
}

fn contains(v: &[Point], p: Point) -> bool {
    v.windows(2).any(|s| p.on_segment(s[0], s[1]))
}

fn passes_over(v: &[Point], p: Point) -> bool {
    !ends(v).contains(&p) && contains(v, p)
}

/// Whether an end of one of `a` and `b` lies on the other.
fn touches(a: &[Point], b: &[Point]) -> bool {
    ends(a).iter().any(|&e| contains(b, e)) || ends(b).iter().any(|&e| contains(a, e))
}

fn cross(a: Point, b: Point) -> i128 {
    i128::from(a.x.0) * i128::from(b.y.0) - i128::from(a.y.0) * i128::from(b.x.0)
}

fn dot(a: Point, b: Point) -> i128 {
    i128::from(a.x.0) * i128::from(b.x.0) + i128::from(a.y.0) * i128::from(b.y.0)
}

/// `v` reversed when `flip`.
fn oriented(v: &[Point], flip: bool) -> Vec<Point> {
    let mut out = v.to_vec();
    if flip {
        out.reverse();
    }
    out
}

/// Whether every segment of `inner` lies within one segment of `outer`.
fn lies_along(inner: &[Point], outer: &[Point]) -> bool {
    inner.windows(2).all(|s| {
        outer
            .windows(2)
            .any(|o| s[0].on_segment(o[0], o[1]) && s[1].on_segment(o[0], o[1]))
    })
}

/// Whether every vertex of `piece` lies on the line from its first to its last.
fn straight(piece: &[Point]) -> bool {
    let (a, b) = (piece[0], piece[piece.len() - 1]);
    piece.iter().all(|p| p.on_segment(a, b))
}

/// `v` without repeated vertices, or `None` with fewer than two left.
fn dedup(mut v: Vec<Point>) -> Option<Vec<Point>> {
    v.dedup();
    (v.len() >= 2).then_some(v)
}

/// `v` without repeated or collinear interior vertices.
fn tidy(mut v: Vec<Point>) -> Vec<Point> {
    v.dedup();
    let mut i = 1;
    while i + 1 < v.len() {
        if v[i].on_segment(v[i - 1], v[i + 1]) {
            v.remove(i);
            i = i.saturating_sub(1).max(1);
        } else {
            i += 1;
        }
    }
    v.dedup();
    v
}

/// `v` cut at each of `cuts` on its interior, as the vertex lists of its pieces in order
/// along it.
fn split(v: &[Point], cuts: &BTreeSet<Point>) -> Vec<Vec<Point>> {
    let [start, end] = ends(v);
    let mut pieces = vec![];
    let mut current = vec![start];
    for s in v.windows(2) {
        let (a, b) = (s[0], s[1]);
        let mut inside: Vec<Point> = (cuts.iter().copied())
            .filter(|&p| p != a && p != b && p.on_segment(a, b))
            .collect();
        inside.sort_by_key(|&p| dot(p - a, p - a));
        for p in inside.into_iter().chain([b]) {
            current.push(p);
            if p != start && p != end && cuts.contains(&p) {
                pieces.push(std::mem::replace(&mut current, vec![p]));
            }
        }
    }
    pieces.push(current);
    pieces
}
