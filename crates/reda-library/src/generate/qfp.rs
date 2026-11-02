//! Quad flat packages: gull-wing leads on four sides, the same count on each.

use reda_command::{CommandError, Kind, Length, Meta};
use reda_model::Uuid;
use reda_model::geom::Nm;
use schemars::JsonSchema;
use serde::Deserialize;

use super::ipc::{self, Termination, Tol};
use super::quad::{self, Quad};
use super::{FootprintFamily, LandPattern, Target, TolParam, hundredths};

pub struct Qfp;

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct QfpParams {
    /// The registered Library to write the Footprint into.
    pub library: Uuid,
    /// The Footprint's name. Omitted, it gets its IPC-7351 name, such as
    /// `QFP50P900X900X160-48N`.
    #[serde(default)]
    pub name: Option<String>,
    /// The number of leads, the same on each side: 32 or 44 (0.8 mm pitch), 48, 64, 80,
    /// 100 or 144 (0.5 mm pitch) for the standard LQFP bodies, or any multiple of 4 with
    /// `dimensions`.
    pub pins: usize,
    /// A datasheet's dimensions, in place of the standard body.
    #[serde(default)]
    pub dimensions: Option<QfpDimensions>,
}

/// A QFP's dimensions from a datasheet, with the JEDEC letters in brackets.
#[derive(Clone, Copy, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct QfpDimensions {
    /// The lead pitch (e).
    pub pitch: Length,
    /// The body length along the side pin 1 is on, without the leads (D1).
    pub body_length: TolParam,
    /// The body width across it, without the leads (E1).
    pub body_width: TolParam,
    /// The span over the lead tips along the body length (D).
    pub lead_span_length: TolParam,
    /// The span over the lead tips along the body width (E).
    pub lead_span_width: TolParam,
    /// The lead width (b).
    pub lead_width: TolParam,
    /// The length of the lead's flat foot (L).
    pub lead_length: TolParam,
    /// The greatest height (A).
    pub height: Length,
}

/// A QFP's dimensions; the body and span along x (E1, E) and y (D1, D).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct QfpBody {
    pitch: Nm,
    body_x: Tol,
    body_y: Tol,
    span_x: Tol,
    span_y: Tol,
    lead_width: Tol,
    lead_length: Tol,
    height: Nm,
}

impl QfpBody {
    /// A square LQFP body (MS-026), 1.60 mm high, with a foot of 0.45–0.75 mm.
    const fn lqfp(pitch: f64, body: (f64, f64), span: (f64, f64), lead_width: (f64, f64)) -> Self {
        QfpBody {
            pitch: ipc::mm(pitch),
            body_x: Tol::mm(body.0, body.1),
            body_y: Tol::mm(body.0, body.1),
            span_x: Tol::mm(span.0, span.1),
            span_y: Tol::mm(span.0, span.1),
            lead_width: Tol::mm(lead_width.0, lead_width.1),
            lead_length: Tol::mm(0.45, 0.75),
            height: ipc::mm(1.60),
        }
    }
}

/// Typical JEDEC MS-026 LQFP bodies.
const LQFP: &[(usize, QfpBody)] = &[
    (
        32,
        QfpBody::lqfp(0.80, (6.90, 7.10), (8.80, 9.20), (0.30, 0.45)),
    ),
    (
        44,
        QfpBody::lqfp(0.80, (9.90, 10.10), (11.80, 12.20), (0.30, 0.45)),
    ),
    (
        48,
        QfpBody::lqfp(0.50, (6.90, 7.10), (8.80, 9.20), (0.17, 0.27)),
    ),
    (
        64,
        QfpBody::lqfp(0.50, (9.90, 10.10), (11.80, 12.20), (0.17, 0.27)),
    ),
    (
        80,
        QfpBody::lqfp(0.50, (11.90, 12.10), (13.80, 14.20), (0.17, 0.27)),
    ),
    (
        100,
        QfpBody::lqfp(0.50, (13.90, 14.10), (15.80, 16.20), (0.17, 0.27)),
    ),
    (
        144,
        QfpBody::lqfp(0.50, (19.90, 20.10), (21.80, 22.20), (0.17, 0.27)),
    ),
];

impl QfpDimensions {
    fn dims(self) -> Result<QfpBody, CommandError> {
        let at = |field: &str| format!("/dimensions/{field}");
        let positive = |value: Length, field: &str| {
            if value.nm().0 > 0 {
                Ok(value.nm())
            } else {
                Err(CommandError::invalid_params(
                    &at(field),
                    "the dimension must be greater than zero",
                ))
            }
        };
        let dims = QfpBody {
            pitch: positive(self.pitch, "pitch")?,
            body_x: self.body_width.tol(&at("body_width"))?,
            body_y: self.body_length.tol(&at("body_length"))?,
            span_x: self.lead_span_width.tol(&at("lead_span_width"))?,
            span_y: self.lead_span_length.tol(&at("lead_span_length"))?,
            lead_width: self.lead_width.tol(&at("lead_width"))?,
            lead_length: self.lead_length.tol(&at("lead_length"))?,
            height: positive(self.height, "height")?,
        };
        if dims.span_x.min <= dims.body_x.max {
            return Err(CommandError::invalid_params(
                &at("lead_span_width"),
                "the lead span must be wider than the body",
            ));
        }
        if dims.span_y.min <= dims.body_y.max {
            return Err(CommandError::invalid_params(
                &at("lead_span_length"),
                "the lead span must be longer than the body",
            ));
        }
        Ok(dims)
    }
}

impl FootprintFamily for Qfp {
    const META: Meta = Meta {
        id: "library.footprint.generate.qfp",
        title: "Generate QFP Footprint",
        description: "Generates an IPC-7351 Nominal Footprint for a QFP with the same number \
                      of gull-wing leads on each side, from a standard LQFP body or the \
                      `dimensions` given, writes it into a registered Library and runs the \
                      Library check on it.",
        category: "library",
        icon: "footprint_generate",
        kind: Kind::File,
    };
    type Params = QfpParams;

    fn target(p: &QfpParams) -> Target {
        Target {
            library: p.library,
            name: p.name.clone(),
        }
    }

    fn land_pattern(p: &QfpParams) -> Result<LandPattern, CommandError> {
        if p.pins < 8 || !p.pins.is_multiple_of(4) {
            return Err(CommandError::invalid_params(
                "/pins",
                "a QFP needs the same number of leads on each side: a multiple of 4, at least 8",
            ));
        }
        let dims = match p.dimensions {
            Some(d) => d.dims()?,
            None => match LQFP.iter().find(|(n, _)| *n == p.pins) {
                Some((_, dims)) => *dims,
                None => {
                    let counts: Vec<String> = LQFP.iter().map(|(n, _)| n.to_string()).collect();
                    return Err(CommandError::invalid_params(
                        "/pins",
                        format!(
                            "the standard LQFP bodies come with {} leads; give `dimensions` \
                             for {}",
                            counts.join(", "),
                            p.pins
                        ),
                    ));
                }
            },
        };
        let quad = Quad {
            pitch: dims.pitch,
            rows: p.pins / 4,
            columns: p.pins / 4,
            span_x: dims.span_x,
            span_y: dims.span_y,
            body_x: dims.body_x,
            body_y: dims.body_y,
            lead_width: dims.lead_width,
            lead_length: dims.lead_length,
        };
        let name = format!(
            "QFP{}P{}X{}X{}-{}N",
            hundredths(dims.pitch),
            hundredths(dims.span_x.nominal()),
            hundredths(dims.span_y.nominal()),
            hundredths(dims.height),
            p.pins
        );
        let description = format!(
            "QFP-{}, {} × {} mm body, {} mm pitch, IPC-7351 Nominal",
            p.pins,
            dims.body_x.nominal().0 as f64 / 1e6,
            dims.body_y.nominal().0 as f64 / 1e6,
            dims.pitch.0 as f64 / 1e6
        );
        quad::land_pattern(&quad, Termination::GullWing, name, description)
    }
}
