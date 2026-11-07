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
use serde::{Deserialize, Serialize};

use super::gull_wing::{self, GullWing};
use super::ipc::{self, Density, Goals, Termination, Tol};
use super::pattern::{GeneratorWarning, LandKind, PadSpec};
use super::quad::{self, Quad};
use super::sections::{Applies, GoalsParams, OutputsParams, PadsParams, Smd, TolerancesParams};
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

    /// The IPC-7351 name: `<prefix><pitch>P<E>X<D>X<height>-<pins><density>`, width
    /// before length and counting the exposed pad as a pin. Sources disagree on both,
    /// so these are REDA's choices.
    fn ipc_name(&self, prefix: &str, leads: usize, density: Density) -> String {
        let pins = leads + usize::from(self.exposed_pad.is_some());
        format!(
            "{prefix}{}P{}X{}X{}-{pins}{}",
            hundredths(self.pitch),
            hundredths(self.body_width.nominal()),
            hundredths(self.body_length.nominal()),
            hundredths(self.height),
            density.letter()
        )
    }

    fn description(&self, title: &str, leads: usize, density: Density) -> String {
        format!(
            "{title}-{leads}, {} × {} mm body, {} mm pitch, IPC-7351 {}",
            self.body_width.nominal().0 as f64 / 1e6,
            self.body_length.nominal().0 as f64 / 1e6,
            self.pitch.0 as f64 / 1e6,
            density.title()
        )
    }

    /// The dimensions with the exposed pad sized as `pads` overrides it.
    fn with_exposed_pad(self, pads: &PadsParams) -> NoLead {
        let Some(size) = pads.exposed_pad else {
            return self;
        };
        let exact = |v: Length| Tol {
            min: v.nm(),
            max: v.nm(),
        };
        NoLead {
            exposed_pad: Some((exact(size.length), exact(size.width))),
            ..self
        }
    }

    /// The exposed pad's Pad, numbered `number`, at its nominal size.
    fn exposed_pad_spec(&self, number: usize) -> Option<PadSpec> {
        let (length, width) = self.exposed_pad?;
        Some(PadSpec {
            kind: LandKind::Exposed,
            ..PadSpec::smd(
                number.to_string(),
                Point::new(Nm(0), Nm(0)),
                width.nominal(),
                length.nominal(),
            )
        })
    }
}

/// A no-lead package's dimensions from a datasheet.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema)]
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
#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema)]
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

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NoLeadParams {
    /// The registered Library to write the Footprint into.
    pub library: Uuid,
    /// The Footprint's name. Omitted, it gets its IPC-7351 name, such as
    /// `QFN50P500X500X100-33N`.
    #[serde(default)]
    pub name: Option<String>,
    /// The Footprint's description. Omitted, it gets one naming the package and density.
    #[serde(default)]
    pub description: Option<String>,
    /// The number of terminals, not counting the exposed pad: one of the standard counts,
    /// or any count that fits the package with `dimensions`.
    pub pins: usize,
    /// A datasheet's dimensions, in place of the standard body.
    #[serde(default)]
    pub dimensions: Option<NoLeadDimensions>,
    #[serde(default)]
    pub goals: GoalsParams,
    #[serde(default)]
    pub tolerances: TolerancesParams,
    #[serde(default)]
    pub pads: PadsParams,
    #[serde(default)]
    pub outputs: OutputsParams,
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
        description: "Generates an IPC-7351 Footprint for a DFN (SON) with two rows of \
                      terminals and an exposed pad, from a standard body or the `dimensions` \
                      given, writes it into a registered Library and runs the Library check \
                      on it. \
                      `goals.density` picks the IPC density level, Nominal by default.",
        category: "library",
        icon: "footprint_generate",
        kind: Kind::File,
    };
    type Params = NoLeadParams;

    fn target(p: &NoLeadParams) -> Target {
        Target {
            library: p.library,
            name: p.name.clone(),
            description: p.description.clone(),
        }
    }

    fn plan(p: &NoLeadParams) -> Result<FootprintPlan, CommandError> {
        if p.pins < 2 || !p.pins.is_multiple_of(2) {
            return Err(CommandError::invalid_params(
                "/pins",
                "a DFN needs an even number of at least 2 terminals",
            ));
        }
        let smd = p.smd();
        smd.check(Applies {
            quad: false,
            exposed_pad: true,
        })?;
        let dims = standard_or("DFN", DFN, p.pins, p.dimensions)?.with_exposed_pad(&p.pads);
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
        let density = p.goals.density;
        let mut pattern = gull_wing::plan(
            &rows,
            &leads,
            Termination::NoLead,
            smd,
            dims.ipc_name("SON", p.pins, density),
            dims.description("DFN", p.pins, density),
        )?;
        add_exposed_pad(&mut pattern, &dims, p.pins + 1, &smd)?;
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
        description: "Generates an IPC-7351 Footprint for a QFN with terminals on four \
                      sides and an exposed pad, from a standard body or the `dimensions` \
                      given, writes it into a registered Library and runs the Library check \
                      on it. \
                      `goals.density` picks the IPC density level, Nominal by default.",
        category: "library",
        icon: "footprint_generate",
        kind: Kind::File,
    };
    type Params = NoLeadParams;

    fn target(p: &NoLeadParams) -> Target {
        Target {
            library: p.library,
            name: p.name.clone(),
            description: p.description.clone(),
        }
    }

    fn plan(p: &NoLeadParams) -> Result<FootprintPlan, CommandError> {
        if p.pins < 4 || !p.pins.is_multiple_of(4) {
            return Err(CommandError::invalid_params(
                "/pins",
                "a QFN needs the same number of terminals on each side: a multiple of 4",
            ));
        }
        let smd = p.smd();
        smd.check(Applies {
            quad: true,
            exposed_pad: true,
        })?;
        let dims = standard_or("QFN", QFN, p.pins, p.dimensions)?.with_exposed_pad(&p.pads);
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
            height: dims.height,
        };
        let density = p.goals.density;
        let mut pattern = quad::plan(
            &quad,
            Termination::NoLead,
            smd,
            dims.ipc_name("QFN", p.pins, density),
            dims.description("QFN", p.pins, density),
        )?;
        add_exposed_pad(&mut pattern, &dims, p.pins + 1, &smd)?;
        Ok(pattern)
    }
}

/// Adds the exposed pad's Pad, if the package has one, and trims the lead Pads' heels
/// where they come closer to it than [`EXPOSED_PAD_CLEARANCE`], with a warning for each
/// pair of opposite sides trimmed.
fn add_exposed_pad(
    pattern: &mut FootprintPlan,
    dims: &NoLead,
    number: usize,
    smd: &Smd<'_>,
) -> Result<(), CommandError> {
    let Some(exposed) = dims.exposed_pad_spec(number) else {
        return Ok(());
    };
    if !smd.pads.trim {
        pattern.pad_specs.push(exposed);
        return Ok(());
    }
    let grid = smd.goals(Goals::no_lead(smd.goals.density)).place;
    // Along x (the left and right sides) and then along y (top and bottom).
    for along_x in [true, false] {
        let size = if along_x {
            exposed.width
        } else {
            exposed.height
        };
        let edge = Nm(ipc::round_up_to(Nm(size.0 + 2 * EXPOSED_PAD_CLEARANCE.0), grid).0 / 2);
        let mut trimmed = vec![];
        let mut by = Nm(0);
        for spec in &mut pattern.pad_specs {
            let (at, length) = if along_x {
                (spec.at.x.0, &mut spec.width)
            } else {
                (spec.at.y.0, &mut spec.height)
            };
            let (across, other) = if along_x {
                (spec.at.y.0.abs(), at.abs())
            } else {
                (spec.at.x.0.abs(), at.abs())
            };
            if other <= across {
                continue;
            }
            let (outer, inner) = (at.abs() + length.0 / 2, at.abs() - length.0 / 2);
            if inner >= edge.0 {
                continue;
            }
            if outer <= edge.0 {
                return Err(CommandError::invalid_params(
                    "/dimensions/exposed_pad",
                    "the exposed pad leaves no room for the lead Pads",
                ));
            }
            by = by.max(Nm(edge.0 - inner));
            *length = Nm(outer - edge.0);
            let centre = Nm(at.signum() * (outer + edge.0) / 2);
            if along_x {
                spec.at.x = centre;
            } else {
                spec.at.y = centre;
            }
            trimmed.push(spec.number.clone());
        }
        if !trimmed.is_empty() {
            pattern.warnings.push(GeneratorWarning::trimmed(
                trimmed,
                format!(
                    "the Pads are trimmed by {} mm at the heel to keep {} mm from the exposed pad",
                    by.0 as f64 / 1e6,
                    EXPOSED_PAD_CLEARANCE.0 as f64 / 1e6
                ),
            ));
        }
    }
    pattern.pad_specs.push(exposed);
    Ok(())
}
