//! Two-terminal rectangular chips (resistors, capacitors, inductors), 0201 to 2512.
//!
//! Zero orientation: the body lies along x with terminal 1 on the left.

use reda_command::{CommandError, Kind, Length, Meta};
use reda_model::Uuid;
use reda_model::geom::{Nm, Point};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::ipc::{self, Goals, Tol};
use super::{FootprintFamily, FootprintPlan, Target, TolParam, hundredths};
use crate::generate::pattern::{Body, PadSpec};

pub struct Chip;

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ChipParams {
    /// The registered Library to write the Footprint into.
    pub library: Uuid,
    /// The Footprint's name. Omitted, it gets its IPC-7351 name, such as `RESC1608X55N`.
    #[serde(default)]
    pub name: Option<String>,
    /// A standard EIA size, with typical body dimensions. Give `size` or `dimensions`.
    #[serde(default)]
    pub size: Option<ChipSize>,
    /// The body dimensions from a datasheet, for a chip of no standard size.
    #[serde(default)]
    pub dimensions: Option<ChipDimensions>,
    /// What the chip is, which sets the name prefix (`RESC`, `CAPC`, `INDC`).
    #[serde(default)]
    pub class: ChipClass,
}

/// A chip's body dimensions.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ChipDimensions {
    /// The body length, terminal end to terminal end.
    pub length: TolParam,
    /// The body width.
    pub width: TolParam,
    /// The length of each terminal along the body.
    pub terminal: TolParam,
    /// The greatest body height.
    pub height: Length,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ChipClass {
    #[default]
    Resistor,
    Capacitor,
    Inductor,
}

impl ChipClass {
    fn prefix(self) -> &'static str {
        match self {
            ChipClass::Resistor => "RESC",
            ChipClass::Capacitor => "CAPC",
            ChipClass::Inductor => "INDC",
        }
    }

    fn noun(self) -> &'static str {
        match self {
            ChipClass::Resistor => "Chip resistor",
            ChipClass::Capacitor => "Chip capacitor",
            ChipClass::Inductor => "Chip inductor",
        }
    }
}

/// A standard chip size by its imperial (EIA) code.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
pub enum ChipSize {
    #[serde(rename = "0201")]
    S0201,
    #[serde(rename = "0402")]
    S0402,
    #[serde(rename = "0603")]
    S0603,
    #[serde(rename = "0805")]
    S0805,
    #[serde(rename = "1206")]
    S1206,
    #[serde(rename = "1210")]
    S1210,
    #[serde(rename = "1812")]
    S1812,
    #[serde(rename = "2010")]
    S2010,
    #[serde(rename = "2512")]
    S2512,
}

/// A standard size's typical body: imperial code, metric code, length, width, terminal
/// length, greatest height (mm).
struct Standard {
    imperial: &'static str,
    metric: &'static str,
    length: Tol,
    width: Tol,
    terminal: Tol,
    height: Nm,
}

const fn standard(
    imperial: &'static str,
    metric: &'static str,
    length: (f64, f64),
    width: (f64, f64),
    terminal: (f64, f64),
    height: f64,
) -> Standard {
    Standard {
        imperial,
        metric,
        length: Tol::mm(length.0, length.1),
        width: Tol::mm(width.0, width.1),
        terminal: Tol::mm(terminal.0, terminal.1),
        height: ipc::mm(height),
    }
}

impl ChipSize {
    /// Typical thick-film chip dimensions, as most manufacturers publish them.
    fn standard(self) -> Standard {
        match self {
            ChipSize::S0201 => standard(
                "0201",
                "0603",
                (0.57, 0.63),
                (0.27, 0.33),
                (0.10, 0.20),
                0.26,
            ),
            ChipSize::S0402 => standard(
                "0402",
                "1005",
                (0.95, 1.05),
                (0.45, 0.55),
                (0.15, 0.35),
                0.40,
            ),
            ChipSize::S0603 => standard(
                "0603",
                "1608",
                (1.45, 1.75),
                (0.65, 0.95),
                (0.10, 0.50),
                0.55,
            ),
            ChipSize::S0805 => standard(
                "0805",
                "2012",
                (1.80, 2.20),
                (1.05, 1.45),
                (0.20, 0.60),
                0.60,
            ),
            ChipSize::S1206 => standard(
                "1206",
                "3216",
                (3.00, 3.40),
                (1.40, 1.80),
                (0.25, 0.75),
                0.65,
            ),
            ChipSize::S1210 => standard(
                "1210",
                "3225",
                (3.00, 3.40),
                (2.30, 2.70),
                (0.25, 0.75),
                0.65,
            ),
            ChipSize::S1812 => standard(
                "1812",
                "4532",
                (4.30, 4.70),
                (3.00, 3.40),
                (0.25, 0.75),
                0.65,
            ),
            ChipSize::S2010 => standard(
                "2010",
                "5025",
                (4.80, 5.20),
                (2.30, 2.70),
                (0.35, 0.85),
                0.65,
            ),
            ChipSize::S2512 => standard(
                "2512",
                "6332",
                (6.15, 6.55),
                (3.00, 3.40),
                (0.35, 0.85),
                0.65,
            ),
        }
    }
}

impl FootprintFamily for Chip {
    const META: Meta = Meta {
        id: "library.footprint.generate.chip",
        title: "Generate Chip Footprint",
        description: "Generates an IPC-7351 Nominal Footprint for a two-terminal \
                      rectangular chip (resistor, capacitor or inductor) of a standard size \
                      from 0201 to 2512, or of the body `dimensions` given, writes it into a \
                      registered Library and runs the Library check on it.",
        category: "library",
        icon: "footprint_generate",
        kind: Kind::File,
    };
    type Params = ChipParams;

    fn target(p: &ChipParams) -> Target {
        Target {
            library: p.library,
            name: p.name.clone(),
        }
    }

    fn plan(p: &ChipParams) -> Result<FootprintPlan, CommandError> {
        let (length, width, terminal, height, size) = match (p.size, p.dimensions) {
            (Some(size), None) => {
                let s = size.standard();
                (s.length, s.width, s.terminal, s.height, Some(s))
            }
            (None, Some(d)) => (
                d.length.tol("/dimensions/length")?,
                d.width.tol("/dimensions/width")?,
                d.terminal.tol("/dimensions/terminal")?,
                d.height.nm(),
                None,
            ),
            (Some(_), Some(_)) => {
                return Err(CommandError::invalid_params(
                    "/dimensions",
                    "give `size` or `dimensions`, not both",
                ));
            }
            (None, None) => {
                return Err(CommandError::invalid_params(
                    "/size",
                    "give a standard `size` or the body `dimensions`",
                ));
            }
        };
        let goals = if length.nominal().0 >= 1_500_000 {
            Goals::CHIP
        } else {
            Goals::SMALL_CHIP
        };
        let span = ipc::span(length, terminal, goals)
            .map_err(|why| CommandError::invalid_params("/dimensions/terminal", why))?;
        let pad_width = ipc::pad_width(width, goals);
        let offset = span.pad_offset();
        let spec = |number: &str, x: i64| {
            PadSpec::smd(
                number,
                Point::new(Nm(x), Nm(0)),
                span.pad_length(),
                pad_width,
            )
        };
        let code = match &size {
            Some(s) => s.metric.to_owned(),
            None => format!(
                "{:02}{:02}",
                (length.nominal().0 + 50_000) / 100_000,
                (width.nominal().0 + 50_000) / 100_000
            ),
        };
        let name = format!("{}{code}X{}N", p.class.prefix(), hundredths(height));
        let description = match &size {
            Some(s) => format!(
                "{}, {} ({} metric), IPC-7351 Nominal",
                p.class.noun(),
                s.imperial,
                s.metric
            ),
            None => format!("{}, {code} metric, IPC-7351 Nominal", p.class.noun()),
        };
        Ok(FootprintPlan {
            name,
            description,
            pad_specs: vec![spec("1", -offset.0), spec("2", offset.0)],
            body: Body {
                nominal: (length.nominal(), width.nominal()),
                max: (length.max, width.max),
                polarised: false,
            },
            silkscreen: vec![],
            courtyard_excess: goals.courtyard,
            height: Some(height),
        })
    }
}
