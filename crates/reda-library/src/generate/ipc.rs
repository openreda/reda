//! The IPC-7351B land-pattern equations at Nominal density.
//!
//! For a part with two opposite rows of terminations:
//!
//! - `Z = Lmin + 2·Jt + √(CL² + F² + P²)`, the distance over the outer (toe) land edges
//! - `G = Smax − 2·Jh − √(CS² + F² + P²)`, the distance between the inner (heel) land edges
//! - `X = Wmin + 2·Js + √(CW² + F² + P²)`, the land width
//!
//! where `L` is the terminal-to-terminal span, `S = L − 2T` the span between the inner
//! terminal ends, `W` the terminal width, `C` each dimension's tolerance, `F` the board
//! fabrication tolerance, `P` the placement tolerance, and `Jt`, `Jh`, `Js` the toe, heel
//! and side solder-fillet goals of the package family. `S` uses its RMS tolerance
//! `√(CL² + 2·CT²)`, as the standard does, rather than the worst case. `Z` and `X` are
//! rounded up and `G` down to the round-off grid.

use reda_model::geom::Nm;

/// The board fabrication tolerance `F`.
pub const FABRICATION: Nm = Nm(50_000);
/// The placement tolerance `P`.
pub const PLACEMENT: Nm = Nm(25_000);
/// Land sizes and spacings are rounded to this grid.
pub const ROUND_OFF: Nm = Nm(50_000);
/// The courtyard is rounded outwards to this grid.
pub const COURTYARD_GRID: Nm = Nm(50_000);

/// Millimetres to the nearest nanometre, for the built-in tables.
pub const fn mm(value: f64) -> Nm {
    let nm = value * 1e6;
    Nm(if nm >= 0.0 { nm + 0.5 } else { nm - 0.5 } as i64)
}

/// A dimension given as its least and greatest value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tol {
    pub min: Nm,
    pub max: Nm,
}

impl Tol {
    /// From millimetres, for the built-in package tables.
    pub const fn mm(min: f64, max: f64) -> Tol {
        Tol {
            min: mm(min),
            max: mm(max),
        }
    }

    pub fn nominal(self) -> Nm {
        Nm((self.min.0 + self.max.0) / 2)
    }

    fn tolerance(self) -> f64 {
        (self.max.0 - self.min.0) as f64
    }
}

/// The solder-fillet goals of a package family at Nominal density, and the courtyard
/// excess around it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Goals {
    pub toe: Nm,
    pub heel: Nm,
    pub side: Nm,
    pub courtyard: Nm,
}

impl Goals {
    /// Rectangular or square-end chips of 1608 metric (0603) and larger.
    pub const CHIP: Goals = Goals::mm(0.35, 0.0, 0.0, 0.25);
    /// Rectangular or square-end chips smaller than 1608 metric (0402, 0201).
    pub const SMALL_CHIP: Goals = Goals::mm(0.20, -0.05, 0.0, 0.15);
    /// Gull-wing leads at a pitch over 0.625 mm.
    pub const GULL_WING: Goals = Goals::mm(0.35, 0.35, 0.03, 0.25);
    /// Gull-wing leads at a pitch of 0.625 mm or less.
    pub const FINE_GULL_WING: Goals = Goals::mm(0.35, 0.35, -0.02, 0.25);
    /// Flat no-lead terminations on the body's underside (QFN, DFN/SON).
    pub const NO_LEAD: Goals = Goals::mm(0.30, 0.0, -0.04, 0.25);

    const fn mm(toe: f64, heel: f64, side: f64, courtyard: f64) -> Goals {
        Goals {
            toe: mm(toe),
            heel: mm(heel),
            side: mm(side),
            courtyard: mm(courtyard),
        }
    }

    /// The gull-wing goals for a lead pitch.
    pub fn gull_wing(pitch: Nm) -> Goals {
        if pitch.0 > 625_000 {
            Goals::GULL_WING
        } else {
            Goals::FINE_GULL_WING
        }
    }
}

/// How a package's leads meet the board.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Termination {
    /// Gull-wing leads, bent out from the body's sides.
    GullWing,
    /// Flat terminations on the body's underside, ending at its edge.
    NoLead,
}

impl Termination {
    /// The solder-fillet goals for leads at `pitch`.
    pub fn goals(self, pitch: Nm) -> Goals {
        match self {
            Termination::GullWing => Goals::gull_wing(pitch),
            Termination::NoLead => Goals::NO_LEAD,
        }
    }
}

/// The land span of one pair of opposite terminations, rounded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    /// `Z`: over the outer land edges.
    pub outer: Nm,
    /// `G`: between the inner land edges.
    pub inner: Nm,
}

impl Span {
    /// The land length, from heel to toe.
    pub fn land_length(self) -> Nm {
        Nm((self.outer.0 - self.inner.0) / 2)
    }

    /// The distance from the part's centre to each land's centre.
    pub fn land_offset(self) -> Nm {
        Nm((self.outer.0 + self.inner.0) / 4)
    }
}

fn rss(tolerance: f64) -> f64 {
    let (f, p) = (FABRICATION.0 as f64, PLACEMENT.0 as f64);
    (tolerance * tolerance + f * f + p * p).sqrt()
}

/// Rounds `value` (in nm) up or down to the grid, allowing for float noise.
fn round(value: f64, grid: Nm, up: bool) -> Nm {
    let steps = value / grid.0 as f64;
    let steps = if up {
        (steps - 1e-9).ceil()
    } else {
        (steps + 1e-9).floor()
    };
    Nm(steps as i64 * grid.0)
}

/// `Z` and `G` for terminations spanning `span` end to end, each `terminal` long, or why
/// they do not fit.
pub fn span(span: Tol, terminal: Tol, goals: Goals) -> Result<Span, String> {
    let s_min = (span.min.0 - 2 * terminal.max.0) as f64;
    let s_max = (span.max.0 - 2 * terminal.min.0) as f64;
    let s_rms = (span.tolerance().powi(2) + 2.0 * terminal.tolerance().powi(2)).sqrt();
    let s_max = s_max - ((s_max - s_min) - s_rms) / 2.0;
    let outer = span.min.0 as f64 + 2.0 * goals.toe.0 as f64 + rss(span.tolerance());
    let inner = s_max - 2.0 * goals.heel.0 as f64 - rss(s_rms);
    let span = Span {
        outer: round(outer, ROUND_OFF, true),
        inner: round(inner, ROUND_OFF, false),
    };
    if span.inner.0 <= 0 {
        return Err("the terminals are too long for the span: the lands would meet".into());
    }
    Ok(span)
}

/// `value` rounded up to the round-off grid.
pub fn round_up(value: Nm) -> Nm {
    round(value.0 as f64, ROUND_OFF, true)
}

/// `value` rounded down to the round-off grid.
pub fn round_down(value: Nm) -> Nm {
    round(value.0 as f64, ROUND_OFF, false)
}

/// `X` for a termination `width` wide.
pub fn land_width(width: Tol, goals: Goals) -> Nm {
    let x = width.min.0 as f64 + 2.0 * goals.side.0 as f64 + rss(width.tolerance());
    round(x, ROUND_OFF, true)
}

/// `value` rounded away from zero to the courtyard grid.
pub fn courtyard_round(value: Nm) -> Nm {
    let grid = COURTYARD_GRID.0;
    let steps = (value.0.abs() + grid - 1) / grid;
    Nm(value.0.signum() * steps * grid)
}
