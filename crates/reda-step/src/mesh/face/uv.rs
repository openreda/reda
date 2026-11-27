//! The trimming curve fitter: each bound of a face mapped into its surface's parameters as one
//! polygon. Each edge's points are inverted onto the surface in turn, each from the one before
//! (the freeform surfaces search from it), so an edge's trimming curve follows the edge rather
//! than jumping between solutions.
//!
//! - On a periodic parameter each run is unwrapped along itself, and the runs are joined end
//!   to end by whole periods: a seam edge used twice lands on both sides of the parameter
//!   rectangle.
//! - Where two runs meet at a singular point (where a parameter means nothing) the join is made
//!   from the other end, and the point is split into one point per side so the triangulation
//!   sees it as a segment.
//! - A bound that comes back one period off runs around the surface (a band's bound, or a cap's
//!   with no seam edge): it is kept open with its winding, for the face to close.

use super::{Bound3, UvPoint};
use crate::mesh::geometry::V3;
use crate::mesh::surface::{Lost, Surface};
use crate::records::RecordId;

/// The value of `x + k·period` nearest `near`.
pub(super) fn wrap_near(x: f64, near: f64, period: f64) -> f64 {
    x + ((near - x) / period).round() * period
}

/// A bound in parameter space: its points in order (the last not repeated), and how many
/// periods it runs around in `u` and `v` (0 for a bound that closes).
pub(super) struct UvLoop {
    pub points: Vec<UvPoint>,
    pub winding: [i64; 2],
}

/// One run's points in parameter space, unwrapped along the run. `None` for a parameter that
/// means nothing at that point, set later.
struct UvRun {
    c: [Vec<Option<f64>>; 2],
    pos: Vec<V3>,
    edge: RecordId,
}

impl UvRun {
    fn first(&self, k: usize) -> Option<f64> {
        self.c[k].iter().flatten().next().copied()
    }
    fn last(&self, k: usize) -> Option<f64> {
        self.c[k].iter().rev().flatten().next().copied()
    }
}

/// Maps a bound into parameter space.
pub(super) fn to_uv(surface: &Surface, bound: &Bound3) -> Result<UvLoop, String> {
    let periods = surface.periods();
    let period = |k: usize| if k == 0 { periods.0 } else { periods.1 };
    let mut seed: Option<(f64, f64)> = None;
    let mut runs: Vec<UvRun> = Vec::with_capacity(bound.runs.len());
    for run in &bound.runs {
        let mut c: [Vec<Option<f64>>; 2] = [
            Vec::with_capacity(run.points.len()),
            Vec::with_capacity(run.points.len()),
        ];
        let mut last: [Option<f64>; 2] = [None, None];
        for &p in &run.points {
            let (u, v) = surface.project(p, seed);
            let lost = surface.lost(p);
            // A singular point's lost parameter is arbitrary: search afresh after it.
            seed = if lost.is_some() { None } else { Some((u, v)) };
            for (k, x) in [u, v].into_iter().enumerate() {
                let lost_here = match lost {
                    Some(Lost::U) => k == 0,
                    Some(Lost::V) => k == 1,
                    None => false,
                };
                if lost_here {
                    c[k].push(None);
                    continue;
                }
                let x = match (period(k), last[k]) {
                    (Some(per), Some(l)) => wrap_near(x, l, per),
                    _ => x,
                };
                last[k] = Some(x);
                c[k].push(Some(x));
            }
        }
        runs.push(UvRun {
            c,
            pos: run.points.clone(),
            edge: run.edge,
        });
    }

    let n = runs.len();
    let mut winding = [0i64; 2];
    for (k, w) in winding.iter_mut().enumerate() {
        let Some(per) = period(k) else { continue };
        *w = join_runs(&mut runs, k, per)?;
    }

    // One polygon: each run's first point is the one before's last.
    let mut pts: Vec<([Option<f64>; 2], V3)> = Vec::new();
    for (i, r) in runs.iter().enumerate() {
        let skip = usize::from(i > 0);
        for j in skip..r.pos.len() {
            pts.push(([r.c[0][j], r.c[1][j]], r.pos[j]));
        }
    }
    pts.pop();
    if pts.len() < 2 || n == 0 {
        return Ok(UvLoop {
            points: Vec::new(),
            winding,
        });
    }
    // A singular point becomes one point on each side: its lost parameter taken from the
    // point before and from the point after (across the ends of a winding bound, a period on).
    let m = pts.len();
    let shift = |k: usize| winding[k] as f64 * period(k).unwrap_or(0.0);
    let neighbour = |i: usize, k: usize, back: bool| -> Option<f64> {
        (1..m).find_map(|d| {
            let (j, wrapped) = if back {
                ((i + m - d) % m, d > i)
            } else {
                ((i + d) % m, i + d >= m)
            };
            let x = pts[j].0[k]?;
            Some(match (wrapped, back) {
                (true, true) => x - shift(k),
                (true, false) => x + shift(k),
                _ => x,
            })
        })
    };
    let mut out = Vec::with_capacity(m + 4);
    for (i, &([u, v], pos)) in pts.iter().enumerate().take(m) {
        match (u, v) {
            (Some(u), Some(v)) => out.push(UvPoint { u, v, pos }),
            (None, None) => return Err("a bound point where neither parameter is defined".into()),
            (lu, lv) => {
                let k = if lu.is_none() { 0 } else { 1 };
                let (Some(before), Some(after)) = (neighbour(i, k, true), neighbour(i, k, false))
                else {
                    return Err("a bound that lies wholly on a pole".into());
                };
                let at = |x: f64| {
                    if k == 0 {
                        UvPoint {
                            u: x,
                            v: lv.expect("v is defined"),
                            pos,
                        }
                    } else {
                        UvPoint {
                            u: lu.expect("u is defined"),
                            v: x,
                            pos,
                        }
                    }
                };
                out.push(at(before));
                if (after - before).abs() > 1e-12 {
                    out.push(at(after));
                }
            }
        }
    }
    Ok(UvLoop {
        points: out,
        winding,
    })
}

/// Shifts whole runs by periods along parameter `k` so each starts where the one before ends,
/// and returns how many periods the bound then runs around.
fn join_runs(runs: &mut [UvRun], k: usize, per: f64) -> Result<i64, String> {
    let n = runs.len();
    let mut shift: Vec<Option<f64>> = vec![None; n];
    let Some(seed) = (0..n).find(|&i| runs[i].first(k).is_some()) else {
        return Err("a bound that lies wholly on a pole".into());
    };
    shift[seed] = Some(0.0);
    // A join can be made where the runs meet at a point with a meaningful parameter.
    let joinable = |a: &UvRun, b: &UvRun| -> Option<(f64, f64)> {
        match (
            a.c[k].last().copied().flatten(),
            b.c[k].first().copied().flatten(),
        ) {
            (Some(end), Some(start)) => Some((end, start)),
            _ => None,
        }
    };
    loop {
        let mut changed = false;
        for a in 0..n {
            let (i, j) = (a, (a + 1) % n);
            if let Some((end, start)) = joinable(&runs[i], &runs[j]) {
                match (shift[i], shift[j]) {
                    (Some(si), None) => {
                        shift[j] = Some(wrap_near(start, end + si, per) - start);
                        changed = true;
                    }
                    (None, Some(sj)) => {
                        shift[i] = Some(wrap_near(end, start + sj, per) - end);
                        changed = true;
                    }
                    _ => {}
                }
            }
        }
        if !changed {
            break;
        }
    }
    // Runs joined to the rest only through singular points: a seam's second use goes a
    // period from its first; anything else stays where it is nearest the rest of the bound.
    for j in 0..n {
        if shift[j].is_some() {
            continue;
        }
        let Some(x0) = runs[j].first(k) else {
            shift[j] = Some(0.0);
            continue;
        };
        let partner =
            (0..n).find(|&m| m != j && shift[m].is_some() && runs[m].edge == runs[j].edge);
        let resolved: Vec<f64> = (0..n)
            .filter_map(|m| Some(runs[m].first(k)? + shift[m]?))
            .collect();
        let mean = resolved.iter().sum::<f64>() / resolved.len() as f64;
        shift[j] = Some(match partner {
            Some(m) => {
                let at = runs[m].last(k).expect("resolved runs have values")
                    + shift[m].expect("resolved");
                let same = wrap_near(x0, at, per) - x0;
                let others: Vec<f64> = (0..n)
                    .filter(|&o| o != m && o != j)
                    .filter_map(|o| Some(runs[o].first(k)? + shift[o]?))
                    .collect();
                // Towards the rest of the bound; for a bound of nothing but the seam,
                // either side is the same surface.
                let rest = others.iter().sum::<f64>() / others.len() as f64;
                let toward = if others.is_empty() || rest >= at {
                    1.0
                } else {
                    -1.0
                };
                same + toward * per
            }
            None => wrap_near(x0, mean, per) - x0,
        });
    }
    for (r, s) in runs.iter_mut().zip(&shift) {
        let s = s.expect("every run is resolved");
        r.c[k].iter_mut().flatten().for_each(|x| *x += s);
    }
    // How far the bound comes back from where it started, in periods; the joins themselves
    // all meet, so any difference is in the closing join or across a singular point.
    let mut total = 0.0;
    for a in 0..n {
        let (i, j) = (a, (a + 1) % n);
        if let Some((end, start)) = joinable(&runs[i], &runs[j]) {
            total += end - start;
        }
    }
    let periods = total / per;
    let w = periods.round();
    if (periods - w).abs() > 1e-6 {
        return Err("its bound's runs do not meet".into());
    }
    Ok(w as i64)
}

#[cfg(test)]
mod tests;
