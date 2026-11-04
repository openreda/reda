//! No-lead packages with an exposed pad: DFN (two rows, IPC-7351 `SON`) and QFN (four
//! sides). Their terminations sit on the body's underside and end at its edge, so the
//! span over the terminations is the body itself.
//!
//! Zero orientation: pin 1 at the top of the left side, numbered counter-clockwise; the
//! exposed pad takes the number after the last lead.

use reda_command::{CommandError, Kind, Length, Meta};
use reda_model::Uuid;
use reda_model::geom::{Nm, Point};
use schemars::JsonSchema;
use serde::Deserialize;

use super::gull_wing::{self, GullWing};
use super::ipc::{self, Termination, Tol};
use super::pattern::PadSpec;
use super::quad::{self, Quad};
use super::{FootprintFamily, FootprintPlan, Target, TolParam, hundredths};

/// The least gap kept between the exposed pad's Pad and the lead Pads.
pub const EXPOSED_PAD_CLEARANCE: Nm = Nm(200_000);

/// A no-lead package's dimensions, with the JEDEC letters in brackets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NoLead {
    /// The terminal pitch (e).
    pub pitch: Nm,
    /// The body length along y, the side pin 1 is on (D).
    pub body_length: Tol,
    /// The body width along x (E).
    pub body_width: Tol,
    /// The terminal width (b).
    pub lead_width: Tol,
    /// The terminal length, in from the body edge (L).
    pub lead_length: Tol,
    /// The greatest height (A).
    pub height: Nm,
    /// The exposed pad's length along y (D2) and width along x (E2).
    pub exposed_pad: Option<(Tol, Tol)>,
}

impl NoLead {
    #[allow(clippy::too_many_arguments, reason = "a table row of JEDEC dimensions")]
    const fn mm(
        pitch: f64,
        body: (f64, f64),
        lead_width: (f64, f64),
        lead_length: (f64, f64),
        height: f64,
        pad_length: (f64, f64),
        pad_width: (f64, f64),
    ) -> NoLead {
        NoLead {
            pitch: ipc::mm(pitch),
            body_length: Tol::mm(body.0, body.1),
            body_width: Tol::mm(body.0, body.1),
            lead_width: Tol::mm(lead_width.0, lead_width.1),
            lead_length: Tol::mm(lead_length.0, lead_length.1),
            height: ipc::mm(height),
            exposed_pad: Some((
                Tol::mm(pad_length.0, pad_length.1),
                Tol::mm(pad_width.0, pad_width.1),
            )),
        }
    }

    /// The IPC-7351 name: `<prefix><pitch>P<E>X<D>X<height>-<pins>N`, counting the
    /// exposed pad as a pin.
    fn ipc_name(&self, prefix: &str, leads: usize) -> String {
        let pins = leads + usize::from(self.exposed_pad.is_some());
        format!(
            "{prefix}{}P{}X{}X{}-{pins}N",
            hundredths(self.pitch),
            hundredths(self.body_width.nominal()),
            hundredths(self.body_length.nominal()),
            hundredths(self.height)
        )
    }

    fn description(&self, title: &str, leads: usize) -> String {
        format!(
            "{title}-{leads}, {} × {} mm body, {} mm pitch, IPC-7351 Nominal",
            self.body_width.nominal().0 as f64 / 1e6,
            self.body_length.nominal().0 as f64 / 1e6,
            self.pitch.0 as f64 / 1e6
        )
    }

    /// The exposed pad's Pad, numbered `number`, no larger than `room` along x and y.
    fn exposed_pad_spec(&self, number: usize, room: (Nm, Nm)) -> Option<PadSpec> {
        let (length, width) = self.exposed_pad?;
        let fit = |nominal: Nm, room: Nm| Nm(nominal.0.min(room.0 - 2 * EXPOSED_PAD_CLEARANCE.0));
        Some(PadSpec::smd(
            number.to_string(),
            Point::new(Nm(0), Nm(0)),
            fit(width.nominal(), room.0),
            fit(length.nominal(), room.1),
        ))
    }
}

/// A no-lead package's dimensions from a datasheet.
#[derive(Clone, Copy, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NoLeadDimensions {
    /// The terminal pitch (e).
    pub pitch: Length,
    /// The body length along the side pin 1 is on (D).
    pub body_length: TolParam,
    /// The body width across it (E).
    pub body_width: TolParam,
    /// The terminal width (b).
    pub lead_width: TolParam,
    /// The terminal length, in from the body edge (L).
    pub lead_length: TolParam,
    /// The greatest height (A).
    pub height: Length,
    /// The exposed pad. Omitted, the package has none.
    #[serde(default)]
    pub exposed_pad: Option<ExposedPadDimensions>,
}

/// An exposed pad's size.
#[derive(Clone, Copy, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExposedPadDimensions {
    /// Along the body length (D2).
    pub length: TolParam,
    /// Along the body width (E2).
    pub width: TolParam,
}

impl NoLeadDimensions {
    fn dims(self) -> Result<NoLead, CommandError> {
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
        let exposed_pad = match self.exposed_pad {
            Some(pad) => Some((
                pad.length.tol(&at("exposed_pad/length"))?,
                pad.width.tol(&at("exposed_pad/width"))?,
            )),
            None => None,
        };
        Ok(NoLead {
            pitch: positive(self.pitch, "pitch")?,
            body_length: self.body_length.tol(&at("body_length"))?,
            body_width: self.body_width.tol(&at("body_width"))?,
            lead_width: self.lead_width.tol(&at("lead_width"))?,
            lead_length: self.lead_length.tol(&at("lead_length"))?,
            height: positive(self.height, "height")?,
            exposed_pad,
        })
    }
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NoLeadParams {
    /// The registered Library to write the Footprint into.
    pub library: Uuid,
    /// The Footprint's name. Omitted, it gets its IPC-7351 name, such as
    /// `QFN50P500X500X100-33N`.
    #[serde(default)]
    pub name: Option<String>,
    /// The number of terminals, not counting the exposed pad: one of the standard counts,
    /// or any count that fits the package with `dimensions`.
    pub pins: usize,
    /// A datasheet's dimensions, in place of the standard body.
    #[serde(default)]
    pub dimensions: Option<NoLeadDimensions>,
}

/// The dimensions for `pins`: the datasheet's, or the standard body from `table`.
fn standard_or(
    title: &str,
    table: &[(usize, NoLead)],
    pins: usize,
    dimensions: Option<NoLeadDimensions>,
) -> Result<NoLead, CommandError> {
    if let Some(d) = dimensions {
        return d.dims();
    }
    match table.iter().find(|(n, _)| *n == pins) {
        Some((_, dims)) => Ok(*dims),
        None => {
            let counts: Vec<String> = table.iter().map(|(n, _)| n.to_string()).collect();
            Err(CommandError::invalid_params(
                "/pins",
                format!(
                    "the standard {title} bodies come with {} terminals; give `dimensions` for \
                     {pins}",
                    counts.join(", ")
                ),
            ))
        }
    }
}

/// Typical JEDEC MO-229 bodies, 0.80 mm high.
const DFN: &[(usize, NoLead)] = &[
    (
        6,
        NoLead::mm(
            0.65,
            (1.90, 2.10),
            (0.25, 0.35),
            (0.25, 0.45),
            0.80,
            (1.50, 1.70),
            (0.80, 1.00),
        ),
    ),
    (
        8,
        NoLead::mm(
            0.65,
            (2.90, 3.10),
            (0.25, 0.35),
            (0.30, 0.50),
            0.80,
            (2.30, 2.50),
            (1.40, 1.60),
        ),
    ),
    (
        10,
        NoLead::mm(
            0.50,
            (2.90, 3.10),
            (0.18, 0.30),
            (0.30, 0.50),
            0.80,
            (2.30, 2.50),
            (1.55, 1.75),
        ),
    ),
];

pub struct Dfn;

impl FootprintFamily for Dfn {
    const META: Meta = Meta {
        id: "library.footprint.generate.dfn",
        title: "Generate DFN Footprint",
        description: "Generates an IPC-7351 Nominal Footprint for a DFN (SON) with two rows of \
                      terminals and an exposed pad, from a standard body or the `dimensions` \
                      given, writes it into a registered Library and runs the Library check \
                      on it.",
        category: "library",
        icon: "footprint_generate",
        kind: Kind::File,
    };
    type Params = NoLeadParams;

    fn target(p: &NoLeadParams) -> Target {
        Target {
            library: p.library,
            name: p.name.clone(),
        }
    }

    fn plan(p: &NoLeadParams) -> Result<FootprintPlan, CommandError> {
        if p.pins < 2 || !p.pins.is_multiple_of(2) {
            return Err(CommandError::invalid_params(
                "/pins",
                "a DFN needs an even number of at least 2 terminals",
            ));
        }
        let dims = standard_or("DFN", DFN, p.pins, p.dimensions)?;
        let rows = GullWing {
            pitch: dims.pitch,
            body_length: dims.body_length,
            body_width: dims.body_width,
            lead_span: dims.body_width,
            lead_width: dims.lead_width,
            lead_length: dims.lead_length,
            height: dims.height,
        };
        let leads = gull_wing::dual_row(p.pins, dims.pitch, dims.lead_width);
        let mut pattern = gull_wing::plan(
            &rows,
            &leads,
            Termination::NoLead,
            dims.ipc_name("SON", p.pins),
            dims.description("DFN", p.pins),
        )?;
        add_exposed_pad(&mut pattern, &dims, p.pins + 1)?;
        Ok(pattern)
    }
}

/// Typical JEDEC MO-220 bodies, 1.00 mm high.
const QFN: &[(usize, NoLead)] = &[
    (
        16,
        NoLead::mm(
            0.50,
            (2.90, 3.10),
            (0.18, 0.30),
            (0.30, 0.50),
            1.00,
            (1.55, 1.75),
            (1.55, 1.75),
        ),
    ),
    (
        20,
        NoLead::mm(
            0.50,
            (3.90, 4.10),
            (0.18, 0.30),
            (0.30, 0.50),
            1.00,
            (2.55, 2.75),
            (2.55, 2.75),
        ),
    ),
    (
        24,
        NoLead::mm(
            0.50,
            (3.90, 4.10),
            (0.18, 0.30),
            (0.30, 0.50),
            1.00,
            (2.55, 2.75),
            (2.55, 2.75),
        ),
    ),
    (
        28,
        NoLead::mm(
            0.50,
            (4.90, 5.10),
            (0.18, 0.30),
            (0.30, 0.50),
            1.00,
            (3.15, 3.35),
            (3.15, 3.35),
        ),
    ),
    (
        32,
        NoLead::mm(
            0.50,
            (4.90, 5.10),
            (0.18, 0.30),
            (0.30, 0.50),
            1.00,
            (3.35, 3.55),
            (3.35, 3.55),
        ),
    ),
    (
        40,
        NoLead::mm(
            0.50,
            (5.90, 6.10),
            (0.18, 0.30),
            (0.30, 0.50),
            1.00,
            (4.05, 4.25),
            (4.05, 4.25),
        ),
    ),
    (
        48,
        NoLead::mm(
            0.50,
            (6.90, 7.10),
            (0.18, 0.30),
            (0.30, 0.50),
            1.00,
            (5.05, 5.25),
            (5.05, 5.25),
        ),
    ),
    (
        64,
        NoLead::mm(
            0.50,
            (8.90, 9.10),
            (0.18, 0.30),
            (0.30, 0.50),
            1.00,
            (7.05, 7.25),
            (7.05, 7.25),
        ),
    ),
];

pub struct Qfn;

impl FootprintFamily for Qfn {
    const META: Meta = Meta {
        id: "library.footprint.generate.qfn",
        title: "Generate QFN Footprint",
        description: "Generates an IPC-7351 Nominal Footprint for a QFN with terminals on four \
                      sides and an exposed pad, from a standard body or the `dimensions` \
                      given, writes it into a registered Library and runs the Library check \
                      on it.",
        category: "library",
        icon: "footprint_generate",
        kind: Kind::File,
    };
    type Params = NoLeadParams;

    fn target(p: &NoLeadParams) -> Target {
        Target {
            library: p.library,
            name: p.name.clone(),
        }
    }

    fn plan(p: &NoLeadParams) -> Result<FootprintPlan, CommandError> {
        if p.pins < 4 || !p.pins.is_multiple_of(4) {
            return Err(CommandError::invalid_params(
                "/pins",
                "a QFN needs the same number of terminals on each side: a multiple of 4",
            ));
        }
        let dims = standard_or("QFN", QFN, p.pins, p.dimensions)?;
        let quad = Quad {
            pitch: dims.pitch,
            rows: p.pins / 4,
            columns: p.pins / 4,
            span_x: dims.body_width,
            span_y: dims.body_length,
            body_x: dims.body_width,
            body_y: dims.body_length,
            lead_width: dims.lead_width,
            lead_length: dims.lead_length,
        };
        let mut pattern = quad::plan(
            &quad,
            Termination::NoLead,
            dims.ipc_name("QFN", p.pins),
            dims.description("QFN", p.pins),
        )?;
        add_exposed_pad(&mut pattern, &dims, p.pins + 1)?;
        Ok(pattern)
    }
}

/// Adds the exposed pad's Pad, if the package has one, between the lead Pads.
fn add_exposed_pad(
    pattern: &mut FootprintPlan,
    dims: &NoLead,
    number: usize,
) -> Result<(), CommandError> {
    // The room left between the Pads on opposite sides, along x and along y.
    let mut room = (dims.body_width.max, dims.body_length.max);
    for spec in &pattern.pad_specs {
        let (x, y) = (spec.at.x.0.abs(), spec.at.y.0.abs());
        if x - spec.width.0 / 2 > 0 && spec.height.0 < spec.width.0 {
            room.0 = room.0.min(Nm(2 * (x - spec.width.0 / 2)));
        }
        if y - spec.height.0 / 2 > 0 && spec.width.0 < spec.height.0 {
            room.1 = room.1.min(Nm(2 * (y - spec.height.0 / 2)));
        }
    }
    if let Some(pad) = dims.exposed_pad_spec(number, room) {
        if pad.width.0 <= 0 || pad.height.0 <= 0 {
            return Err(CommandError::invalid_params(
                "/dimensions/exposed_pad",
                "the Pads leave no room for the exposed pad",
            ));
        }
        pattern.pad_specs.push(pad);
    }
    Ok(())
}
