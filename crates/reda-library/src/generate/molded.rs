//! Molded 2-terminal bodies: SOD-123, SOD-323, and the DO-214 SMA, SMB and SMC, or any
//! molded diode, polarised capacitor or inductor from a datasheet's dimensions.
//!
//! IPC-7351B's table 3-13 (inward flat ribbon L-leads) gives the goals for all of them.
//! Zero orientation: the body lies along x with terminal 1 (the cathode, or the positive
//! end) on the left, which the polarity mark marks.

use reda_command::{CommandError, Kind, Length, Meta};
use reda_model::Uuid;
use reda_model::geom::{Nm, Point};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::ipc::{self, Goals, Tol};
use super::pattern::{Body, FootprintPlan, PadSpec, Resolved, clip_silkscreen, polarity_mark};
use super::sections::{
    Applies, GoalsParams, OutputsParams, PadsParams, SmdResolved, TolerancesParams,
};
use super::{FootprintFamily, Target, TolParam, hundredths, tenths};

pub struct Molded;

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MoldedParams {
    /// The registered Library to write the Footprint into.
    pub library: Uuid,
    /// The Footprint's name. Omitted, it gets its IPC-7351 name, such as `SOD3716X135N`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The Footprint's description. Omitted, it gets one naming the package and density.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// A standard body, with typical dimensions. Give `package` or `dimensions`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package: Option<MoldedPackage>,
    /// The body dimensions from a datasheet, for a body of no standard size.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dimensions: Option<MoldedDimensions>,
    /// What the part is, which sets the name prefix of a body from `dimensions` or a
    /// DO-214 body (`DIOM`, `CAPM`, `INDM`). SOD bodies are always `SOD`.
    #[serde(default)]
    pub class: MoldedClass,
    /// Chamfer the Assembly outline's corner at terminal 1. Off by default.
    #[serde(default)]
    pub chamfer: bool,
    /// Mark terminal 1 with a Silkscreen bar beyond its Pad. On by default.
    #[serde(default = "super::sections::yes")]
    pub polarity_mark: bool,
    #[serde(default)]
    pub goals: GoalsParams,
    #[serde(default)]
    pub tolerances: TolerancesParams,
    #[serde(default)]
    pub pads: PadsParams,
    #[serde(default)]
    pub outputs: OutputsParams,
}

/// A molded body's dimensions.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MoldedDimensions {
    /// The span over the terminals, end to end.
    pub span: TolParam,
    /// The body's length along the span, without the terminals.
    pub body_length: TolParam,
    /// The body's width across the span.
    pub body_width: TolParam,
    /// Each terminal's width.
    pub terminal_width: TolParam,
    /// The length of each terminal's flat, along the span.
    pub terminal_length: TolParam,
    /// The greatest height.
    pub height: Length,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MoldedPackage {
    Sod123,
    Sod323,
    /// DO-214AC.
    Sma,
    /// DO-214AA.
    Smb,
    /// DO-214AB.
    Smc,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MoldedClass {
    #[default]
    Diode,
    Capacitor,
    Inductor,
}

impl MoldedClass {
    fn prefix(self) -> &'static str {
        match self {
            MoldedClass::Diode => "DIOM",
            MoldedClass::Capacitor => "CAPM",
            MoldedClass::Inductor => "INDM",
        }
    }

    fn noun(self) -> &'static str {
        match self {
            MoldedClass::Diode => "molded diode",
            MoldedClass::Capacitor => "molded capacitor",
            MoldedClass::Inductor => "molded inductor",
        }
    }
}

/// A molded body's dimensions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Body2 {
    span: Tol,
    body_length: Tol,
    body_width: Tol,
    terminal_width: Tol,
    terminal_length: Tol,
    height: Nm,
}

const fn body(
    span: (f64, f64),
    body_length: (f64, f64),
    body_width: (f64, f64),
    terminal_width: (f64, f64),
    terminal_length: (f64, f64),
    height: f64,
) -> Body2 {
    Body2 {
        span: Tol::mm(span.0, span.1),
        body_length: Tol::mm(body_length.0, body_length.1),
        body_width: Tol::mm(body_width.0, body_width.1),
        terminal_width: Tol::mm(terminal_width.0, terminal_width.1),
        terminal_length: Tol::mm(terminal_length.0, terminal_length.1),
        height: ipc::mm(height),
    }
}

impl MoldedPackage {
    /// Typical dimensions, as most manufacturers publish them, and the package's title.
    fn standard(self) -> (Body2, &'static str) {
        match self {
            MoldedPackage::Sod123 => (
                body(
                    (3.55, 3.85),
                    (2.55, 2.85),
                    (1.40, 1.80),
                    (0.50, 0.70),
                    (0.25, 0.45),
                    1.35,
                ),
                "SOD-123",
            ),
            MoldedPackage::Sod323 => (
                body(
                    (2.30, 2.70),
                    (1.60, 1.80),
                    (1.15, 1.35),
                    (0.25, 0.40),
                    (0.20, 0.40),
                    1.10,
                ),
                "SOD-323",
            ),
            MoldedPackage::Sma => (
                body(
                    (4.80, 5.60),
                    (4.00, 4.60),
                    (2.29, 2.92),
                    (1.27, 1.63),
                    (0.76, 1.52),
                    2.44,
                ),
                "SMA (DO-214AC)",
            ),
            MoldedPackage::Smb => (
                body(
                    (5.21, 5.59),
                    (4.06, 4.57),
                    (3.30, 3.94),
                    (1.96, 2.21),
                    (0.76, 1.52),
                    2.44,
                ),
                "SMB (DO-214AA)",
            ),
            MoldedPackage::Smc => (
                body(
                    (7.75, 8.13),
                    (6.60, 7.11),
                    (5.59, 6.22),
                    (2.90, 3.20),
                    (0.76, 1.52),
                    2.44,
                ),
                "SMC (DO-214AB)",
            ),
        }
    }

    fn is_sod(self) -> bool {
        matches!(self, MoldedPackage::Sod123 | MoldedPackage::Sod323)
    }
}

impl MoldedDimensions {
    fn dims(self) -> Result<Body2, CommandError> {
        let at = |field: &str| format!("/dimensions/{field}");
        let dims = Body2 {
            span: self.span.tol(&at("span"))?,
            body_length: self.body_length.tol(&at("body_length"))?,
            body_width: self.body_width.tol(&at("body_width"))?,
            terminal_width: self.terminal_width.tol(&at("terminal_width"))?,
            terminal_length: self.terminal_length.tol(&at("terminal_length"))?,
            height: self.height.nm(),
        };
        if dims.height.0 <= 0 {
            return Err(CommandError::invalid_params(
                &at("height"),
                "the dimension must be greater than zero",
            ));
        }
        if dims.body_length.min > dims.span.max {
            return Err(CommandError::invalid_params(
                &at("body_length"),
                "the body cannot be longer than the span over its terminals",
            ));
        }
        Ok(dims)
    }

    pub(crate) fn of(dims: &Body2) -> MoldedDimensions {
        let tol = TolParam::of;
        MoldedDimensions {
            span: tol(dims.span),
            body_length: tol(dims.body_length),
            body_width: tol(dims.body_width),
            terminal_width: tol(dims.terminal_width),
            terminal_length: tol(dims.terminal_length),
            height: Length(dims.height),
        }
    }
}

impl MoldedParams {
    /// The body's dimensions and title.
    pub(crate) fn body(&self) -> Result<(Body2, Option<&'static str>), CommandError> {
        match (self.package, self.dimensions) {
            (Some(package), None) => {
                let (dims, title) = package.standard();
                Ok((dims, Some(title)))
            }
            (None, Some(d)) => Ok((d.dims()?, None)),
            (Some(_), Some(_)) => Err(CommandError::invalid_params(
                "/dimensions",
                "give `package` or `dimensions`, not both",
            )),
            (None, None) => Err(CommandError::invalid_params(
                "/package",
                "give a standard `package` or the body `dimensions`",
            )),
        }
    }
}

impl FootprintFamily for Molded {
    const META: Meta = Meta {
        id: "library.footprint.generate.molded",
        title: "Generate Molded Footprint",
        description: "Generates an IPC-7351 Footprint for a molded 2-terminal body (SOD-123, \
                      SOD-323, SMA, SMB, SMC, or the `dimensions` given), terminal 1 on the \
                      left with a Silkscreen polarity mark, writes it into a registered \
                      Library and runs the Library check on it. `goals.density` picks the \
                      IPC density level, Nominal by default.",
        category: "library",
        icon: "footprint_generate",
        kind: Kind::File,
    };
    type Params = MoldedParams;

    fn target(p: &MoldedParams) -> Target {
        Target {
            library: p.library,
            name: p.name.clone(),
            description: p.description.clone(),
        }
    }

    fn resolve_package(p: &MoldedParams) -> Result<MoldedParams, CommandError> {
        let (dims, _) = p.body()?;
        Ok(MoldedParams {
            package: None,
            dimensions: Some(MoldedDimensions::of(&dims)),
            ..p.clone()
        })
    }

    fn plan(p: &MoldedParams) -> Result<FootprintPlan, CommandError> {
        let (dims, title) = p.body()?;
        let smd = p.smd();
        smd.check(Applies::default())?;
        let density = p.goals.density;
        let goals = smd.goals(Goals::molded(density));
        let span = ipc::span(dims.span, dims.terminal_length, goals)
            .map_err(|why| CommandError::invalid_params("/dimensions/terminal_length", why))?;
        let span = smd.span(span, false)?;
        let pad_width = smd.pad_width(ipc::pad_width(dims.terminal_width, goals));
        let (cl, cs) = ipc::tolerances(dims.span, dims.terminal_length, goals);
        let resolved = Resolved::Smd(SmdResolved {
            goals,
            tolerances: (cl, cs, ipc::width_tolerance(dims.terminal_width, goals)),
            span,
            y: None,
            pad_width,
            exposed_pad: None,
        });
        let offset = span.pad_offset();
        let spec = |number: &str, x: i64| {
            PadSpec::smd(
                number,
                Point::new(Nm(x), Nm(0)),
                span.pad_length(),
                pad_width,
            )
        };
        let pad_specs = vec![spec("1", -offset.0), spec("2", offset.0)];
        let prefix = match p.package {
            Some(package) if package.is_sod() => "SOD",
            _ => p.class.prefix(),
        };
        let name = format!(
            "{prefix}{}{}X{}{}",
            tenths(dims.span.nominal()),
            tenths(dims.body_width.nominal()),
            hundredths(dims.height),
            density.letter()
        );
        let description = match title {
            Some(title) => format!("{title} {}, IPC-7351 {}", p.class.noun(), density.title()),
            None => format!(
                "{}{}, {}, IPC-7351 {}",
                tenths(dims.span.nominal()),
                tenths(dims.body_width.nominal()),
                p.class.noun(),
                density.title()
            ),
        };
        let finish = smd.finish();
        let silk = finish.silkscreen_width;
        // Lines along the body's long edges, clear of the copper, and the polarity bar.
        let y = dims.body_width.max.0 / 2 + silk.0 / 2;
        let x = dims.body_length.max.0 / 2;
        let at = |x: i64, y: i64| Point::new(Nm(x), Nm(y));
        let mut silkscreen = clip_silkscreen(
            &[(at(-x, y), at(x, y)), (at(-x, -y), at(x, -y))],
            &pad_specs,
            silk,
        );
        if p.polarity_mark {
            silkscreen.extend(polarity_mark(&pad_specs[0], Nm(y), silk));
        }
        Ok(FootprintPlan {
            name,
            description,
            pad_specs,
            body: Body {
                nominal: (dims.body_length.nominal(), dims.body_width.nominal()),
                // The terminals reach past the body, out to the span.
                max: (dims.span.max, dims.body_width.max),
                chamfered: p.chamfer,
            },
            silkscreen,
            courtyard_excess: goals.courtyard,
            height: Some(dims.height),
            warnings: vec![],
            finish,
            resolved,
        })
    }
}
