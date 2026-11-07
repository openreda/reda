//! The IPC-7351B Pad equations, at the three density levels.
//!
//! For a part with two opposite rows of terminations:
//!
//! - `Z = Lmin + 2·Jt + √(CL² + F² + P²)`, the distance over the outer (toe) Pad edges
//! - `G = Smax − 2·Jh − √(CS² + F² + P²)`, the distance between the inner (heel) Pad edges
//! - `X = Wmin + 2·Js + √(CW² + F² + P²)`, the Pad width
//!
//! where `L` is the terminal-to-terminal span, `S = L − 2T` the span between the inner
//! terminal ends, `W` the terminal width, `C` each dimension's tolerance, `F` the board
//! fabrication tolerance, `P` the placement tolerance, and `Jt`, `Jh`, `Js` the toe, heel
//! and side solder-fillet goals of the package family. `S` uses its RMS tolerance
//! `√(CL² + 2·CT²)`, as the standard does, rather than the worst case. `Z`, `G` and `X`
//! are rounded to the nearest point of the round-off grid: 0.05 mm, or 0.02 mm on chips
//! smaller than 1608.

use reda_model::geom::Nm;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The board fabrication tolerance `F`, as one total value.
pub const FABRICATION: Nm = Nm(100_000);
/// The placement tolerance `P`, as one total value.
pub const PLACEMENT: Nm = Nm(50_000);
/// Pad sizes and spacings are rounded to this grid.
pub const ROUND_OFF: Nm = Nm(50_000);
/// Pad sizes and spacings of chips smaller than 1608 are rounded to this grid.
pub const SMALL_CHIP_ROUND_OFF: Nm = Nm(20_000);
/// The courtyard is rounded outwards to this grid.
pub const COURTYARD_GRID: Nm = Nm(50_000);

/// How much copper and room a surface-mount Footprint gets: the IPC-7351 density levels
/// A (Most), B (Nominal) and C (Least).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Density {
    /// Level A, for low-density boards and hand or wave soldering.
    Most,
    /// Level B, for most boards.
    #[default]
    Nominal,
    /// Level C, for high-density boards.
    Least,
}

impl Density {
    /// The last letter of an IPC-7351 name: `M`, `N` or `L`.
    pub fn letter(self) -> char {
        match self {
            Density::Most => 'M',
            Density::Nominal => 'N',
            Density::Least => 'L',
        }
    }

    /// The level's name in a description: `Most`, `Nominal` or `Least`.
    pub fn title(self) -> &'static str {
        match self {
            Density::Most => "Most",
            Density::Nominal => "Nominal",
            Density::Least => "Least",
        }
    }

    /// The value for this level out of `most / nominal / least`.
    const fn pick(self, values: [f64; 3]) -> f64 {
        match self {
            Density::Most => values[0],
            Density::Nominal => values[1],
            Density::Least => values[2],
        }
    }
}

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

/// The solder-fillet goals of a package family at one density level, the courtyard excess
/// around it, the grids its Pads are rounded to and the tolerances the equations take.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Goals {
    pub toe: Nm,
    pub heel: Nm,
    pub side: Nm,
    pub courtyard: Nm,
    /// The grid the Pad width (X) is rounded to.
    pub round_off: Nm,
    /// The grid the outer and inner Pad edges (Z, G) are rounded to.
    pub place: Nm,
    /// `F`.
    pub fabrication: Nm,
    /// `P`.
    pub placement: Nm,
    /// `CL`, when given rather than calculated from the span's range.
    pub span_tolerance: Option<Nm>,
    /// `CS`, when given rather than calculated.
    pub inner_tolerance: Option<Nm>,
    /// `CW`, when given rather than calculated from the width's range.
    pub width_tolerance: Option<Nm>,
}

/// One family's row of IPC-7351B's goal tables: toe, heel, side and courtyard excess as
/// Most / Nominal / Least (mm), and the round-off grid.
struct Table {
    toe: [f64; 3],
    heel: [f64; 3],
    side: [f64; 3],
    courtyard: [f64; 3],
    round_off: Nm,
}

/// Table 3-5: rectangular or square-end chips of 1608 metric (0603) and larger.
const CHIP: Table = Table {
    toe: [0.55, 0.35, 0.15],
    heel: [0.0, 0.0, 0.0],
    side: [0.05, 0.0, -0.05],
    courtyard: [0.50, 0.25, 0.10],
    round_off: ROUND_OFF,
};
/// Table 3-6: chips smaller than 1608 metric (0402, 0201).
const SMALL_CHIP: Table = Table {
    toe: [0.30, 0.20, 0.10],
    heel: [0.0, 0.0, 0.0],
    side: [0.05, 0.0, -0.05],
    courtyard: [0.20, 0.15, 0.10],
    round_off: SMALL_CHIP_ROUND_OFF,
};
/// Table 3-2: gull-wing leads at a pitch over 0.625 mm.
const GULL_WING: Table = Table {
    toe: [0.55, 0.35, 0.15],
    heel: [0.45, 0.35, 0.25],
    side: [0.05, 0.03, 0.01],
    courtyard: [0.50, 0.25, 0.10],
    round_off: ROUND_OFF,
};
/// Table 3-3: gull-wing leads at a pitch of 0.625 mm or less.
const FINE_GULL_WING: Table = Table {
    side: [0.01, -0.02, -0.04],
    ..GULL_WING
};
/// Tables 3-15 and 3-16: flat no-lead terminations on the body's underside (QFN, SON).
const NO_LEAD: Table = Table {
    toe: [0.40, 0.30, 0.20],
    heel: [0.0, 0.0, 0.0],
    side: [-0.04, -0.04, -0.04],
    courtyard: [0.50, 0.25, 0.10],
    round_off: ROUND_OFF,
};

impl Table {
    const fn at(&self, density: Density) -> Goals {
        Goals {
            toe: mm(density.pick(self.toe)),
            heel: mm(density.pick(self.heel)),
            side: mm(density.pick(self.side)),
            courtyard: mm(density.pick(self.courtyard)),
            round_off: self.round_off,
            place: self.round_off,
            fabrication: FABRICATION,
            placement: PLACEMENT,
            span_tolerance: None,
            inner_tolerance: None,
            width_tolerance: None,
        }
    }
}

/// A chip's body is smaller than 1608 metric when its nominal length is under 1.6 mm or
/// its nominal width under 0.8 mm.
pub fn is_small_chip(length: Tol, width: Tol) -> bool {
    length.nominal().0 < 1_600_000 || width.nominal().0 < 800_000
}

impl Goals {
    /// Two-terminal chips `length` by `width`, at `density`.
    pub fn chip(length: Tol, width: Tol, density: Density) -> Goals {
        if is_small_chip(length, width) {
            SMALL_CHIP.at(density)
        } else {
            CHIP.at(density)
        }
    }

    /// Gull-wing leads at `pitch`, at `density`.
    pub fn gull_wing(pitch: Nm, density: Density) -> Goals {
        if pitch.0 > 625_000 {
            GULL_WING.at(density)
        } else {
            FINE_GULL_WING.at(density)
        }
    }

    /// Flat no-lead terminations, at `density`.
    pub fn no_lead(density: Density) -> Goals {
        NO_LEAD.at(density)
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
    /// The solder-fillet goals for leads at `pitch`, at `density`.
    pub fn goals(self, pitch: Nm, density: Density) -> Goals {
        match self {
            Termination::GullWing => Goals::gull_wing(pitch, density),
            Termination::NoLead => Goals::no_lead(density),
        }
    }
}

/// The Pad span of one pair of opposite terminations, rounded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    /// `Z`: over the outer Pad edges.
    pub outer: Nm,
    /// `G`: between the inner Pad edges.
    pub inner: Nm,
}

impl Span {
    /// The Pad length, from heel to toe.
    pub fn pad_length(self) -> Nm {
        Nm((self.outer.0 - self.inner.0) / 2)
    }

    /// The distance from the part's centre to each Pad's centre.
    pub fn pad_offset(self) -> Nm {
        Nm((self.outer.0 + self.inner.0) / 4)
    }
}

/// `√(C² + F² + P²)`.
fn rss(tolerance: f64, goals: Goals) -> f64 {
    let (f, p) = (goals.fabrication.0 as f64, goals.placement.0 as f64);
    (tolerance * tolerance + f * f + p * p).sqrt()
}

/// A tolerance as given, or as calculated.
fn given_or(given: Option<Nm>, calculated: f64) -> f64 {
    given.map_or(calculated, |t| t.0 as f64)
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

/// Rounds `value` (in nm) to the nearest point of the grid, halves away from zero,
/// allowing for float noise.
fn round_nearest(value: f64, grid: Nm) -> Nm {
    let steps = value / grid.0 as f64;
    let steps = (steps + 1e-9 * steps.signum()).round();
    Nm(steps as i64 * grid.0)
}

/// `Z` and `G` for terminations spanning `span` end to end, each `terminal` long, or why
/// they do not fit.
pub fn span(span: Tol, terminal: Tol, goals: Goals) -> Result<Span, String> {
    let s_min = (span.min.0 - 2 * terminal.max.0) as f64;
    let s_max = (span.max.0 - 2 * terminal.min.0) as f64;
    let s_rms = given_or(
        goals.inner_tolerance,
        (span.tolerance().powi(2) + 2.0 * terminal.tolerance().powi(2)).sqrt(),
    );
    let s_max = s_max - ((s_max - s_min) - s_rms) / 2.0;
    let c_l = given_or(goals.span_tolerance, span.tolerance());
    let outer = span.min.0 as f64 + 2.0 * goals.toe.0 as f64 + rss(c_l, goals);
    let inner = s_max - 2.0 * goals.heel.0 as f64 - rss(s_rms, goals);
    let span = Span {
        outer: round_nearest(outer, goals.place),
        inner: round_nearest(inner, goals.place),
    };
    if span.inner.0 <= 0 {
        return Err("the terminals are too long for the span: the Pads would meet".into());
    }
    Ok(span)
}

/// `value` rounded up to the 0.05 mm round-off grid.
pub fn round_up(value: Nm) -> Nm {
    round(value.0 as f64, ROUND_OFF, true)
}

/// `value` rounded up to `grid`.
pub fn round_up_to(value: Nm, grid: Nm) -> Nm {
    round(value.0 as f64, grid, true)
}

/// `value` rounded down to the 0.05 mm round-off grid.
pub fn round_down(value: Nm) -> Nm {
    round(value.0 as f64, ROUND_OFF, false)
}

/// `X` for a termination `width` wide.
pub fn pad_width(width: Tol, goals: Goals) -> Nm {
    let c_w = given_or(goals.width_tolerance, width.tolerance());
    let x = width.min.0 as f64 + 2.0 * goals.side.0 as f64 + rss(c_w, goals);
    round_nearest(x, goals.round_off)
}

/// `value` rounded away from zero to the courtyard grid.
pub fn courtyard_round(value: Nm) -> Nm {
    let grid = COURTYARD_GRID.0;
    let steps = (value.0.abs() + grid - 1) / grid;
    Nm(value.0.signum() * steps * grid)
}
