//! Packages with two opposite columns of gull-wing leads: SOT-23, SOT-223, SOIC, SSOP and
//! TSSOP share this land pattern.
//!
//! Zero orientation: the lead columns run along y, pin 1 at the upper left, numbered down
//! the left column and back up the right one.

use reda_command::{CommandError, Length};
use reda_model::geom::{Nm, Point};
use schemars::JsonSchema;
use serde::Deserialize;

use super::ipc::{self, Goals, Tol};
use super::pattern::{Body, Land, LandPattern, SILKSCREEN_CLEARANCE, SILKSCREEN_WIDTH};
use super::{TolParam, hundredths};

/// A gull-wing package's dimensions, with the JEDEC letters in brackets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GullWing {
    /// The lead pitch (e).
    pub pitch: Nm,
    /// The body length along the lead columns (D).
    pub body_length: Tol,
    /// The body width between the lead columns (E1).
    pub body_width: Tol,
    /// The span over the lead tips (E).
    pub lead_span: Tol,
    /// The lead width (b).
    pub lead_width: Tol,
    /// The length of the lead's flat foot (L).
    pub lead_length: Tol,
    /// The greatest height (A).
    pub height: Nm,
}

impl GullWing {
    pub const fn mm(
        pitch: f64,
        body_length: (f64, f64),
        body_width: (f64, f64),
        lead_span: (f64, f64),
        lead_width: (f64, f64),
        lead_length: (f64, f64),
        height: f64,
    ) -> GullWing {
        GullWing {
            pitch: ipc::mm(pitch),
            body_length: Tol::mm(body_length.0, body_length.1),
            body_width: Tol::mm(body_width.0, body_width.1),
            lead_span: Tol::mm(lead_span.0, lead_span.1),
            lead_width: Tol::mm(lead_width.0, lead_width.1),
            lead_length: Tol::mm(lead_length.0, lead_length.1),
            height: ipc::mm(height),
        }
    }

    /// The IPC-7351 name: `<prefix><pitch>P<lead span>X<height>-<pins>N`, in hundredths
    /// of a millimetre.
    pub fn ipc_name(&self, prefix: &str, pins: usize) -> String {
        format!(
            "{prefix}{}P{}X{}-{pins}N",
            hundredths(self.pitch),
            hundredths(self.lead_span.nominal()),
            hundredths(self.height)
        )
    }
}

/// A gull-wing package's dimensions from a datasheet.
#[derive(Clone, Copy, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GullWingDimensions {
    /// The lead pitch (e).
    pub pitch: Length,
    /// The body length along the lead rows (D).
    pub body_length: TolParam,
    /// The body width between the lead rows, without the leads (E1).
    pub body_width: TolParam,
    /// The span over the lead tips (E).
    pub lead_span: TolParam,
    /// The lead width (b).
    pub lead_width: TolParam,
    /// The length of the lead's flat foot (L).
    pub lead_length: TolParam,
    /// The greatest height (A).
    pub height: Length,
}

impl GullWingDimensions {
    /// The dimensions, or `invalid_params` under `pointer`.
    pub fn dims(self, pointer: &str) -> Result<GullWing, CommandError> {
        let at = |field: &str| format!("{pointer}/{field}");
        let positive = |value: Nm, field: &str| {
            if value.0 > 0 {
                Ok(value)
            } else {
                Err(CommandError::invalid_params(
                    &at(field),
                    "the dimension must be greater than zero",
                ))
            }
        };
        let dims = GullWing {
            pitch: positive(self.pitch.nm(), "pitch")?,
            body_length: self.body_length.tol(&at("body_length"))?,
            body_width: self.body_width.tol(&at("body_width"))?,
            lead_span: self.lead_span.tol(&at("lead_span"))?,
            lead_width: self.lead_width.tol(&at("lead_width"))?,
            lead_length: self.lead_length.tol(&at("lead_length"))?,
            height: positive(self.height.nm(), "height")?,
        };
        if dims.lead_span.min <= dims.body_width.max {
            return Err(CommandError::invalid_params(
                &at("lead_span"),
                "the lead span must be wider than the body",
            ));
        }
        Ok(dims)
    }
}

/// Which lead column a lead is in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
}

/// One lead: its number, column, centre height and width.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lead {
    pub number: String,
    pub side: Side,
    pub y: Nm,
    pub width: Tol,
}

/// The leads of a dual-row package with `pins` leads, all `width` wide: `1..pins/2` down
/// the left column and the rest back up the right.
pub fn dual_row(pins: usize, pitch: Nm, width: Tol) -> Vec<Lead> {
    let per_side = pins / 2;
    let top = |i: usize| Nm((per_side as i64 - 1 - 2 * i as i64) * pitch.0 / 2);
    (0..pins)
        .map(|n| {
            let (side, i) = if n < per_side {
                (Side::Left, n)
            } else {
                (Side::Right, pins - 1 - n)
            };
            Lead {
                number: (n + 1).to_string(),
                side,
                y: top(i),
                width,
            }
        })
        .collect()
}

/// The land pattern for `leads` of a package with dimensions `dims`. Silkscreen lines run
/// along the body's top and bottom edges; the top one reaches out over pin 1 when it
/// clears the copper.
pub fn land_pattern(
    dims: &GullWing,
    leads: &[Lead],
    name: String,
    description: String,
) -> Result<LandPattern, CommandError> {
    let goals = Goals::gull_wing(dims.pitch);
    let span = ipc::span(dims.lead_span, dims.lead_length, goals)
        .map_err(|why| CommandError::invalid_params("/dimensions/lead_length", why))?;
    let offset = span.land_offset();
    let lands: Vec<Land> = leads
        .iter()
        .map(|lead| Land {
            number: lead.number.clone(),
            at: Point::new(
                match lead.side {
                    Side::Left => Nm(-offset.0),
                    Side::Right => offset,
                },
                lead.y,
            ),
            width: span.land_length(),
            height: ipc::land_width(lead.width, goals),
        })
        .collect();
    let line_y = dims.body_length.max.0 / 2 + SILKSCREEN_WIDTH.0 / 2;
    let copper_top = lands.iter().map(|l| l.top().0).max().unwrap_or(0);
    let clear = line_y - SILKSCREEN_WIDTH.0 / 2 - SILKSCREEN_CLEARANCE.0 >= copper_top;
    let p = |x: i64, y: i64| Point::new(Nm(x), Nm(y));
    let silkscreen = if clear {
        let x = dims.body_width.max.0 / 2;
        vec![
            (p(-span.outer.0 / 2, line_y), p(x, line_y)),
            (p(-x, -line_y), p(x, -line_y)),
        ]
    } else {
        let x = span.inner.0 / 2 - SILKSCREEN_CLEARANCE.0 - SILKSCREEN_WIDTH.0 / 2;
        if x > 0 {
            vec![
                (p(-x, line_y), p(x, line_y)),
                (p(-x, -line_y), p(x, -line_y)),
            ]
        } else {
            vec![]
        }
    };
    Ok(LandPattern {
        name,
        description,
        lands,
        body: Body {
            nominal: (dims.body_width.nominal(), dims.body_length.nominal()),
            max: (dims.body_width.max, dims.body_length.max),
            polarised: true,
        },
        silkscreen,
        courtyard_excess: goals.courtyard,
    })
}
