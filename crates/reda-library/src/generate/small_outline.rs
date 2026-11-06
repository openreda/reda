//! Dual-row small-outline ICs: SOIC (narrow and wide), SSOP and TSSOP. Each is its own
//! Command with its own table of standard bodies; all three take a datasheet's
//! `dimensions` for any other body.

use reda_command::{CommandError, Kind, Meta};
use reda_model::Uuid;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::gull_wing::{self, GullWing, GullWingDimensions};
use super::ipc::{Termination, Tol};
use super::{FootprintFamily, FootprintPlan, Target};

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SoicParams {
    /// The registered Library to write the Footprint into.
    pub library: Uuid,
    /// The Footprint's name. Omitted, it gets its IPC-7351 name, such as
    /// `SOIC127P600X175-8N`.
    #[serde(default)]
    pub name: Option<String>,
    /// The number of leads: 8, 14 or 16 for the narrow (3.9 mm) body; 16, 18, 20, 24 or 28
    /// for the wide (7.5 mm) one. Any even number of at least 4 with `dimensions`.
    pub pins: usize,
    /// The wide (7.5 mm, JEDEC MS-013) body instead of the narrow one (MS-012).
    #[serde(default)]
    pub wide: bool,
    /// A datasheet's dimensions, in place of the standard body.
    #[serde(default)]
    pub dimensions: Option<GullWingDimensions>,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SmallOutlineParams {
    /// The registered Library to write the Footprint into.
    pub library: Uuid,
    /// The Footprint's name. Omitted, it gets its IPC-7351 name, such as
    /// `SOP65P640X120-14N`.
    #[serde(default)]
    pub name: Option<String>,
    /// The number of leads: one of the standard counts, or any even number of at least 4
    /// with `dimensions`.
    pub pins: usize,
    /// A datasheet's dimensions, in place of the standard body.
    #[serde(default)]
    pub dimensions: Option<GullWingDimensions>,
}

/// A standard body: its lead geometry, and its length (D) for each lead count.
struct Standard {
    title: &'static str,
    leads: GullWing,
    lengths: &'static [(usize, (f64, f64))],
}

const SOIC_NARROW: Standard = Standard {
    title: "SOIC",
    leads: GullWing::mm(
        1.27,
        (0.0, 0.0),
        (3.80, 4.00),
        (5.80, 6.20),
        (0.31, 0.51),
        (0.40, 1.27),
        1.75,
    ),
    lengths: &[(8, (4.80, 5.00)), (14, (8.55, 8.75)), (16, (9.80, 10.00))],
};
const SOIC_WIDE: Standard = Standard {
    title: "SOIC wide",
    leads: GullWing::mm(
        1.27,
        (0.0, 0.0),
        (7.40, 7.60),
        (10.00, 10.65),
        (0.31, 0.51),
        (0.40, 1.27),
        2.65,
    ),
    lengths: &[
        (16, (10.10, 10.50)),
        (18, (11.35, 11.75)),
        (20, (12.60, 13.00)),
        (24, (15.20, 15.60)),
        (28, (17.70, 18.10)),
    ],
};
const SSOP: Standard = Standard {
    title: "SSOP",
    leads: GullWing::mm(
        0.65,
        (0.0, 0.0),
        (5.00, 5.60),
        (7.40, 8.20),
        (0.22, 0.38),
        (0.55, 0.95),
        2.00,
    ),
    lengths: &[
        (14, (6.00, 6.40)),
        (16, (6.00, 6.40)),
        (20, (7.00, 7.40)),
        (24, (8.00, 8.40)),
        (28, (10.00, 10.40)),
    ],
};
const TSSOP: Standard = Standard {
    title: "TSSOP",
    leads: GullWing::mm(
        0.65,
        (0.0, 0.0),
        (4.30, 4.50),
        (6.20, 6.60),
        (0.19, 0.30),
        (0.45, 0.75),
        1.20,
    ),
    lengths: &[
        (8, (2.90, 3.10)),
        (14, (4.90, 5.10)),
        (16, (4.90, 5.10)),
        (20, (6.40, 6.60)),
        (24, (7.70, 7.90)),
        (28, (9.60, 9.80)),
    ],
};

/// The Footprint of a `pins`-lead package: a standard body, or `dimensions`.
fn small_outline(
    standard: &Standard,
    prefix: &str,
    pins: usize,
    dimensions: Option<GullWingDimensions>,
) -> Result<FootprintPlan, CommandError> {
    if pins < 4 || !pins.is_multiple_of(2) {
        return Err(CommandError::invalid_params(
            "/pins",
            "a dual-row package needs an even number of at least 4 leads",
        ));
    }
    let dims = match dimensions {
        Some(d) => d.dims("/dimensions")?,
        None => {
            let Some((_, (min, max))) = standard.lengths.iter().find(|(n, _)| *n == pins) else {
                let counts: Vec<String> = standard
                    .lengths
                    .iter()
                    .map(|(n, _)| n.to_string())
                    .collect();
                return Err(CommandError::invalid_params(
                    "/pins",
                    format!(
                        "the standard {} body comes with {} leads; give `dimensions` for {pins}",
                        standard.title,
                        counts.join(", ")
                    ),
                ));
            };
            GullWing {
                body_length: Tol::mm(*min, *max),
                ..standard.leads
            }
        }
    };
    let leads = gull_wing::dual_row(pins, dims.pitch, dims.lead_width);
    let name = dims.ipc_name(prefix, pins);
    let description = format!(
        "{}-{pins}, {} mm pitch, IPC-7351 Nominal",
        standard.title,
        dims.pitch.0 as f64 / 1e6
    );
    gull_wing::plan(&dims, &leads, Termination::GullWing, name, description)
}

pub struct Soic;

impl FootprintFamily for Soic {
    const META: Meta = Meta {
        id: "library.footprint.generate.soic",
        title: "Generate SOIC Footprint",
        description: "Generates an IPC-7351 Nominal Footprint for a 1.27 mm pitch SOIC with \
                      a narrow or wide standard body, or the `dimensions` given, writes it \
                      into a registered Library and runs the Library check on it.",
        category: "library",
        icon: "footprint_generate",
        kind: Kind::File,
    };
    type Params = SoicParams;

    fn target(p: &SoicParams) -> Target {
        Target {
            library: p.library,
            name: p.name.clone(),
        }
    }

    fn plan(p: &SoicParams) -> Result<FootprintPlan, CommandError> {
        let standard = if p.wide { &SOIC_WIDE } else { &SOIC_NARROW };
        small_outline(standard, "SOIC", p.pins, p.dimensions)
    }
}

pub struct Ssop;

impl FootprintFamily for Ssop {
    const META: Meta = Meta {
        id: "library.footprint.generate.ssop",
        title: "Generate SSOP Footprint",
        description: "Generates an IPC-7351 Nominal Footprint for a 0.65 mm pitch SSOP with \
                      the standard 5.3 mm body (JEDEC MO-150), or the `dimensions` given, \
                      writes it into a registered Library and runs the Library check on it.",
        category: "library",
        icon: "footprint_generate",
        kind: Kind::File,
    };
    type Params = SmallOutlineParams;

    fn target(p: &SmallOutlineParams) -> Target {
        Target {
            library: p.library,
            name: p.name.clone(),
        }
    }

    fn plan(p: &SmallOutlineParams) -> Result<FootprintPlan, CommandError> {
        small_outline(&SSOP, "SOP", p.pins, p.dimensions)
    }
}

pub struct Tssop;

impl FootprintFamily for Tssop {
    const META: Meta = Meta {
        id: "library.footprint.generate.tssop",
        title: "Generate TSSOP Footprint",
        description: "Generates an IPC-7351 Nominal Footprint for a 0.65 mm pitch TSSOP with \
                      the standard 4.4 mm body (JEDEC MO-153), or the `dimensions` given, \
                      writes it into a registered Library and runs the Library check on it.",
        category: "library",
        icon: "footprint_generate",
        kind: Kind::File,
    };
    type Params = SmallOutlineParams;

    fn target(p: &SmallOutlineParams) -> Target {
        Target {
            library: p.library,
            name: p.name.clone(),
        }
    }

    fn plan(p: &SmallOutlineParams) -> Result<FootprintPlan, CommandError> {
        small_outline(&TSSOP, "SOP", p.pins, p.dimensions)
    }
}
