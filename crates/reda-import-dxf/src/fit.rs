//! Smooth curves as arcs: each span of a curve is replaced by the arc through its ends and
//! its middle, and split in two until every sample of the curve between lies within the
//! tolerance of that arc (docs/spec/v0.3/dxf.md#conversion: 0.005 mm).

use crate::shape::{Piece, Xy};

/// Curve samples checked against each arc, besides its three points.
const SAMPLES: usize = 16;
/// The share of the tolerance the samples must keep to, so the curve between them, which
/// strays a little further, still keeps to all of it.
const MARGIN: f64 = 0.75;
/// How many times a span may be halved.
const MAX_DEPTH: u32 = 24;
/// The most pieces one curve becomes. A curve that needs more is far too large for a
/// board, and the import refuses it on its size.
const MAX_PIECES: usize = 1 << 16;

/// `curve` from `breaks[0]` to its last break as arcs and lines within `tolerance` of it.
/// Each span between breaks is fitted alone, so a corner at a break stays sharp.
pub(crate) fn fit(curve: &dyn Fn(f64) -> Xy, breaks: &[f64], tolerance: f64) -> Vec<Piece> {
    let mut fitter = Fitter {
        curve,
        tolerance,
        pieces: Vec::new(),
    };
    for span in breaks.windows(2) {
        let (t0, t1) = (span[0], span[1]);
        if t1 > t0 {
            fitter.span(t0, t1, curve(t0), curve(t1), 0);
        }
    }
    fitter.pieces
}

struct Fitter<'c> {
    curve: &'c dyn Fn(f64) -> Xy,
    tolerance: f64,
    pieces: Vec<Piece>,
}

impl Fitter<'_> {
    fn span(&mut self, t0: f64, t1: f64, p0: Xy, p1: Xy, depth: u32) {
        let tm = (t0 + t1) / 2.0;
        let pm = (self.curve)(tm);
        let finite = |p: Xy| p.x.is_finite() && p.y.is_finite();
        if !(finite(p0) && finite(pm) && finite(p1)) {
            // Coordinates past what a float holds: a broken file, with nothing to fit. The
            // pieces either side end here, so the import refuses the drawing on its size.
            self.pieces.push(Piece::Line { from: p0, to: p1 });
            return;
        }
        // Coordinates far past a board's lose the tolerance in rounding; a fit to a ten
        // millionth of their size is as close as they hold, and far below the tolerance on
        // anything within an import's reach.
        let size = [p0, pm, p1]
            .iter()
            .flat_map(|p| [p.x.abs(), p.y.abs()])
            .fold(0.0, f64::max);
        let tolerance = self.tolerance.max(size * 1e-7) * MARGIN;
        let stop = depth >= MAX_DEPTH || self.pieces.len() >= MAX_PIECES;
        // A span that comes back to where it started has no arc through its ends.
        let closed = p0 == p1 && pm != p0;
        if !closed || stop {
            let piece = through(p0, pm, p1);
            let fits = (1..SAMPLES).all(|i| {
                let t = t0 + (t1 - t0) * i as f64 / SAMPLES as f64;
                // Not-a-number distances never fit, so a broken curve stops at the limits.
                distance(piece, (self.curve)(t)) <= tolerance
            });
            if fits || stop {
                if p0 != p1 {
                    self.pieces.push(piece);
                }
                return;
            }
        }
        self.span(t0, tm, p0, pm, depth + 1);
        self.span(tm, t1, pm, p1, depth + 1);
    }
}

fn sub(a: Xy, b: Xy) -> Xy {
    Xy::new(a.x - b.x, a.y - b.y)
}

fn cross(a: Xy, b: Xy) -> f64 {
    a.x * b.y - a.y * b.x
}

fn length(a: Xy) -> f64 {
    a.x.hypot(a.y)
}

/// The arc from `a` through `m` to `b`, or the line from `a` to `b` when the three are in
/// line.
pub(crate) fn through(a: Xy, m: Xy, b: Xy) -> Piece {
    // Worked in units of the longer side from `a`, so huge coordinates don't overflow.
    let unit = length(sub(m, a)).max(length(sub(b, a)));
    let scaled = |p: Xy| Xy::new((p.x - a.x) / unit, (p.y - a.y) / unit);
    let (am, ab) = (scaled(m), scaled(b));
    let turn = cross(am, ab);
    if unit.is_nan() || unit <= 0.0 || turn.abs() <= 1e-12 {
        return Piece::Line { from: a, to: b };
    }
    // The circumcentre, from `a`.
    let (sm, sb) = (am.x * am.x + am.y * am.y, ab.x * ab.x + ab.y * ab.y);
    let d = 2.0 * turn;
    let cx = (ab.y * sm - am.y * sb) / d * unit;
    let cy = (am.x * sb - ab.x * sm) / d * unit;
    let centre = Xy::new(a.x + cx, a.y + cy);
    let angle = |p: Xy| (p.y - centre.y).atan2(p.x - centre.x);
    let (from, to) = (angle(a), angle(b));
    // a → m → b turns left on a counter-clockwise arc.
    let sweep = if turn > 0.0 {
        (to - from).rem_euclid(std::f64::consts::TAU)
    } else {
        -(from - to).rem_euclid(std::f64::consts::TAU)
    };
    Piece::Arc {
        from: a,
        to: b,
        sweep: sweep.to_degrees(),
        centre,
        radius: cx.hypot(cy),
    }
}

/// How far `p` is from `piece`.
pub(crate) fn distance(piece: Piece, p: Xy) -> f64 {
    match piece {
        Piece::Line { from, to } => {
            let (d, ap) = (sub(to, from), sub(p, from));
            let len2 = d.x * d.x + d.y * d.y;
            let s = if len2 > 0.0 {
                ((ap.x * d.x + ap.y * d.y) / len2).clamp(0.0, 1.0)
            } else {
                0.0
            };
            length(sub(p, Xy::new(from.x + d.x * s, from.y + d.y * s)))
        }
        Piece::Arc {
            from,
            to,
            sweep,
            centre,
            radius,
        } => {
            let start = (from.y - centre.y).atan2(from.x - centre.x);
            let at = (p.y - centre.y).atan2(p.x - centre.x);
            let sweep = sweep.to_radians();
            let along = if sweep >= 0.0 {
                (at - start).rem_euclid(std::f64::consts::TAU)
            } else {
                (start - at).rem_euclid(std::f64::consts::TAU)
            };
            if along <= sweep.abs() {
                (length(sub(p, centre)) - radius).abs()
            } else {
                length(sub(p, from)).min(length(sub(p, to)))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn three_points_on_a_circle_give_its_arc_turning_their_way() {
        let Piece::Arc {
            sweep,
            centre,
            radius,
            ..
        } = through(Xy::new(1.0, 0.0), Xy::new(0.0, 1.0), Xy::new(-1.0, 0.0))
        else {
            panic!("not an arc")
        };
        assert!(
            (sweep - 180.0).abs() < 1e-9 && radius == 1.0,
            "{sweep} {radius}"
        );
        assert!(centre.x.abs() < 1e-12 && centre.y.abs() < 1e-12);
        let Piece::Arc { sweep, .. } =
            through(Xy::new(1.0, 0.0), Xy::new(0.0, -1.0), Xy::new(-1.0, 0.0))
        else {
            panic!("not an arc")
        };
        assert!((sweep + 180.0).abs() < 1e-9, "{sweep}");
    }

    #[test]
    fn three_points_in_line_give_a_line() {
        let piece = through(Xy::new(0.0, 0.0), Xy::new(1.0, 1.0), Xy::new(2.0, 2.0));
        assert!(matches!(piece, Piece::Line { .. }));
    }

    #[test]
    fn a_parabola_is_fitted_within_the_tolerance() {
        let curve = |t: f64| Xy::new(t, t * t);
        let pieces = fit(&curve, &[-1.0, 1.0], 1e-4);
        assert!(pieces.len() > 1);
        for i in 0..=1000 {
            let p = curve(-1.0 + 2.0 * i as f64 / 1000.0);
            let d = pieces
                .iter()
                .map(|q| distance(*q, p))
                .fold(f64::INFINITY, f64::min);
            assert!(d <= 1e-4, "{d} at {p:?}");
        }
    }

    #[test]
    fn a_curve_that_never_fits_stops_at_the_piece_limit() {
        // Jumps between 0 and 1 at every millionth of its parameter.
        let jagged = |t: f64| Xy::new(t, ((t * 1e6) as i64 % 2) as f64);
        let pieces = fit(&jagged, &[0.0, 1.0], 1e-3);
        assert!(
            pieces.len() <= MAX_PIECES + MAX_DEPTH as usize + 1,
            "{}",
            pieces.len()
        );
    }

    #[test]
    fn a_curve_past_what_floats_hold_is_not_fitted() {
        let pieces = fit(&|t| Xy::new(t, f64::NAN), &[0.0, 1.0], 1e-3);
        assert_eq!(pieces.len(), 1);
        let huge = |t: f64| Xy::new(1e300 * t.cos(), 1e300 * t.sin());
        assert!(fit(&huge, &[0.0, 1.5], 1e-3).len() < 100);
    }
}
