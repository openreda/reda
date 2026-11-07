//! Packages with leads on all four sides: QFP (gull-wing) and QFN (no-lead) share this
//! Footprint.
//!
//! Zero orientation: pin 1 at the top of the left side, numbered counter-clockwise: down
//! the left side, along the bottom, up the right side and back along the top.

use reda_command::CommandError;
use reda_model::geom::{Nm, Point};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::gull_wing::{body_trim_warning, trim_to_body};
use super::ipc::{self, Termination, Tol};
use super::pattern::{Body, FootprintPlan, PadSpec, Resolved, clip_silkscreen, rectangle};
use super::sections::{Smd, SmdResolved};

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
    /// Where pin 1 is.
    pub pin1: Pin1Location,
}

/// Where a four-sided package's pin 1 is; the numbering runs counter-clockwise from it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Pin1Location {
    /// At the top of the left side, the IPC-7351 zero orientation.
    #[default]
    Corner,
    /// In the middle of the top side (left of the middle on an even count), as on a
    /// PLCC.
    TopCentre,
}

/// Centres of `n` leads at `pitch`, symmetric about zero, in increasing order.
fn centres(n: usize, pitch: Nm) -> impl DoubleEndedIterator<Item = Nm> {
    (0..n).map(move |i| Nm((2 * i as i64 - (n as i64 - 1)) * pitch.0 / 2))
}

/// The Footprint of a four-sided package terminated as `termination`, with the sections `smd`. Silkscreen
/// marks the body's corners where they clear the copper, all but the upper-left one,
/// whose absence marks pin 1.
pub fn plan(
    q: &Quad,
    termination: Termination,
    smd: Smd<'_>,
    name: String,
    description: String,
) -> Result<FootprintPlan, CommandError> {
    let table = termination.goals(q.pitch, smd.goals.density);
    let goals = smd.goals(table);
    let goals_y = smd.goals_y(table);
    // Each span before and after its heels were trimmed back to keep out from under the
    // body, and how far.
    type Trimmed = (ipc::Span, ipc::Span, Option<Nm>);
    let span = |tips: Tol, body: Tol, along_y: bool| -> Result<Trimmed, CommandError> {
        let goals = if along_y { goals_y } else { goals };
        let span = ipc::span(tips, q.lead_length, goals)
            .map_err(|why| CommandError::invalid_params("/dimensions/lead_length", why))?;
        let span = smd.span(span, along_y)?;
        if termination == Termination::NoLead || !smd.pads.trim {
            return Ok((span, span, None));
        }
        Ok(match trim_to_body(span, body.max, goals.place)? {
            Some(trimmed) => (
                span,
                trimmed,
                Some(Nm((trimmed.inner.0 - span.inner.0) / 2)),
            ),
            None => (span, span, None),
        })
    };
    let ((ix, sx, trim_x), (iy, sy, trim_y)) = (
        span(q.span_x, q.body_x, false)?,
        span(q.span_y, q.body_y, true)?,
    );
    let (cl, cs) = ipc::tolerances(q.span_x, q.lead_length, goals);
    let (ox, oy) = (sx.pad_offset().0, sy.pad_offset().0);
    let x = smd.pad_width(ipc::pad_width(q.lead_width, goals));
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
    let mut warnings = vec![];
    // Pads 1..rows and the third side run along x; the other two sides along y.
    let numbers = |sides: [usize; 2]| -> Vec<String> {
        let (r, c) = (q.rows, q.columns);
        let starts = [0, r, r + c, 2 * r + c];
        let lengths = [r, c, r, c];
        sides
            .iter()
            .flat_map(|&s| (starts[s]..starts[s] + lengths[s]).map(|i| (i + 1).to_string()))
            .collect()
    };
    if let Some(by) = trim_x {
        warnings.push(body_trim_warning(numbers([0, 2]), by));
    }
    if let Some(by) = trim_y {
        warnings.push(body_trim_warning(numbers([1, 3]), by));
    }
    if q.pin1 == Pin1Location::TopCentre {
        // Position k, counted from the corner, takes the number counted from the middle
        // of the top side, which starts at 2·rows + columns + columns / 2.
        let count = pad_specs.len();
        let start = 2 * q.rows + q.columns + q.columns / 2;
        let renumber = |number: &str| -> String {
            let k = number.parse::<usize>().expect("numbered by position") - 1;
            ((k + count - start) % count + 1).to_string()
        };
        for spec in &mut pad_specs {
            spec.number = renumber(&spec.number);
        }
        for warning in &mut warnings {
            for number in &mut warning.pads {
                *number = renumber(number);
            }
        }
    }
    let finish = smd.finish();
    let silk = finish.silkscreen_width;
    let corner = p(
        q.body_x.max.0 / 2 + silk.0 / 2,
        q.body_y.max.0 / 2 + silk.0 / 2,
    );
    let pin_1 = p(-corner.x.0, corner.y.0);
    let silkscreen = clip_silkscreen(&rectangle(corner.x, corner.y), &pad_specs, silk)
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
            chamfered: true,
        },
        silkscreen,
        courtyard_excess: goals.courtyard,
        height: Some(q.height),
        warnings,
        finish,
        resolved: Resolved::Smd(SmdResolved {
            goals,
            tolerances: (cl, cs, ipc::width_tolerance(q.lead_width, goals)),
            span: ix,
            y: Some((ipc::tolerances(q.span_y, q.lead_length, goals_y), iy)),
            pad_width: x,
            exposed_pad: None,
        }),
    })
}
