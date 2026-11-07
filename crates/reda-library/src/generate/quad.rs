//! Packages with leads on all four sides: QFP (gull-wing) and QFN (no-lead) share this
//! Footprint.
//!
//! Zero orientation: pin 1 at the top of the left side, numbered counter-clockwise: down
//! the left side, along the bottom, up the right side and back along the top.

use reda_command::CommandError;
use reda_model::geom::{Nm, Point};

use super::gull_wing::keep_body_middle_clear;
use super::ipc::{self, Density, Termination, Tol};
use super::pattern::{Body, FootprintPlan, PadSpec, SILKSCREEN_WIDTH, clip_silkscreen, rectangle};

/// A four-sided package's dimensions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Quad {
    /// The lead pitch (e).
    pub pitch: Nm,
    /// The leads on each of the left and right sides.
    pub rows: usize,
    /// The leads on each of the top and bottom sides.
    pub columns: usize,
    /// The span over the left and right leads' tips, along x.
    pub span_x: Tol,
    /// The span over the top and bottom leads' tips, along y.
    pub span_y: Tol,
    /// The body along x.
    pub body_x: Tol,
    /// The body along y.
    pub body_y: Tol,
    /// The lead width (b).
    pub lead_width: Tol,
    /// The length of the lead's foot or terminal (L).
    pub lead_length: Tol,
    /// The greatest height (A).
    pub height: Nm,
}

/// Centres of `n` leads at `pitch`, symmetric about zero, in increasing order.
fn centres(n: usize, pitch: Nm) -> impl DoubleEndedIterator<Item = Nm> {
    (0..n).map(move |i| Nm((2 * i as i64 - (n as i64 - 1)) * pitch.0 / 2))
}

/// The Footprint of a four-sided package terminated as `termination`, at `density`. Silkscreen
/// marks the body's corners where they clear the copper, all but the upper-left one,
/// whose absence marks pin 1.
pub fn plan(
    q: &Quad,
    termination: Termination,
    density: Density,
    name: String,
    description: String,
) -> Result<FootprintPlan, CommandError> {
    let goals = termination.goals(q.pitch, density);
    let span = |tips: Tol, body: Tol| {
        let span = ipc::span(tips, q.lead_length, goals)
            .map_err(|why| CommandError::invalid_params("/dimensions/lead_length", why))?;
        match termination {
            Termination::GullWing => keep_body_middle_clear(span, body.max),
            Termination::NoLead => Ok(span),
        }
    };
    let (sx, sy) = (span(q.span_x, q.body_x)?, span(q.span_y, q.body_y)?);
    let (ox, oy) = (sx.pad_offset().0, sy.pad_offset().0);
    let x = ipc::pad_width(q.lead_width, goals);
    let reach = |n: usize| Nm((n as i64 - 1) * q.pitch.0 / 2 + x.0 / 2);
    if reach(q.rows).0 * 2 >= sy.inner.0 || reach(q.columns).0 * 2 >= sx.inner.0 {
        return Err(CommandError::invalid_params(
            "/pins",
            "the leads do not fit along the body: the Pads at the corners would meet",
        ));
    }
    let mut pad_specs = vec![];
    let mut n = 0;
    let mut add = |at: Point, width: Nm, height: Nm| {
        n += 1;
        pad_specs.push(PadSpec::smd(n.to_string(), at, width, height));
    };
    let p = |x: i64, y: i64| Point::new(Nm(x), Nm(y));
    for y in centres(q.rows, q.pitch).rev() {
        add(p(-ox, y.0), sx.pad_length(), x);
    }
    for x_at in centres(q.columns, q.pitch) {
        add(p(x_at.0, -oy), x, sy.pad_length());
    }
    for y in centres(q.rows, q.pitch) {
        add(p(ox, y.0), sx.pad_length(), x);
    }
    for x_at in centres(q.columns, q.pitch).rev() {
        add(p(x_at.0, oy), x, sy.pad_length());
    }
    let corner = p(
        q.body_x.max.0 / 2 + SILKSCREEN_WIDTH.0 / 2,
        q.body_y.max.0 / 2 + SILKSCREEN_WIDTH.0 / 2,
    );
    let pin_1 = p(-corner.x.0, corner.y.0);
    let silkscreen = clip_silkscreen(&rectangle(corner.x, corner.y), &pad_specs)
        .into_iter()
        .filter(|(start, end)| *start != pin_1 && *end != pin_1)
        .collect();
    Ok(FootprintPlan {
        name,
        description,
        pad_specs,
        body: Body {
            nominal: (q.body_x.nominal(), q.body_y.nominal()),
            max: (q.body_x.max, q.body_y.max),
            polarised: true,
        },
        silkscreen,
        courtyard_excess: goals.courtyard,
        height: Some(q.height),
    })
}
