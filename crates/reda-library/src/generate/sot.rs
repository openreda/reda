//! Small-outline transistors: SOT-23 with 3, 5 or 6 leads, and SOT-223.

use reda_command::{CommandError, Kind, Meta};
use reda_model::Uuid;
use reda_model::geom::Nm;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::gull_wing::{self, GullWing, Lead, Side};
use super::ipc::{Termination, Tol};
use super::sections::{Applies, GoalsParams, OutputsParams, PadsParams, TolerancesParams};
use super::{FootprintFamily, FootprintPlan, Target};

pub struct Sot;

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SotParams {
    /// The registered Library to write the Footprint into.
    pub library: Uuid,
    /// The Footprint's name. Omitted, it gets its IPC-7351 name, such as
    /// `SOT95P237X112-3N`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The Footprint's description. Omitted, it gets one naming the package and density.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub package: SotPackage,
    /// Chamfer the Assembly outline's corner at pin 1. On by default.
    #[serde(default = "super::sections::yes")]
    pub chamfer: bool,
    #[serde(default)]
    pub goals: GoalsParams,
    #[serde(default)]
    pub tolerances: TolerancesParams,
    #[serde(default)]
    pub pads: PadsParams,
    #[serde(default)]
    pub outputs: OutputsParams,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
pub enum SotPackage {
    /// SOT-23 with 3 leads (JEDEC TO-236): pins 1 and 2 on the left, 3 on the right.
    #[serde(rename = "sot23_3")]
    Sot23_3,
    /// SOT-23 with 5 leads (JEDEC MO-178): 1–3 on the left, 4 and 5 on the right.
    #[serde(rename = "sot23_5")]
    Sot23_5,
    /// SOT-23 with 6 leads (JEDEC MO-178).
    #[serde(rename = "sot23_6")]
    Sot23_6,
    /// SOT-223 (JEDEC TO-261): pins 1–3 on the left, the tab (pin 4) on the right.
    #[serde(rename = "sot223")]
    Sot223,
}

/// Typical JEDEC outline dimensions (mm).
const TO_236: GullWing = GullWing::mm(
    0.95,
    (2.80, 3.04),
    (1.20, 1.40),
    (2.10, 2.64),
    (0.30, 0.50),
    (0.40, 0.60),
    1.12,
);
const MO_178: GullWing = GullWing::mm(
    0.95,
    (2.80, 3.00),
    (1.50, 1.70),
    (2.60, 3.00),
    (0.30, 0.50),
    (0.30, 0.60),
    1.45,
);
const TO_261: GullWing = GullWing::mm(
    2.30,
    (6.30, 6.70),
    (3.30, 3.70),
    (6.70, 7.30),
    (0.60, 0.80),
    (0.85, 1.05),
    1.80,
);
/// The SOT-223 tab's width.
const TO_261_TAB: Tol = Tol::mm(2.90, 3.10);

/// A lead `number` in column `side` at `row` pitches above the centre.
fn lead(number: u32, side: Side, row: i64, dims: &GullWing, width: Tol) -> Lead {
    Lead {
        number: number.to_string(),
        side,
        y: Nm(row * dims.pitch.0),
        width,
    }
}

impl FootprintFamily for Sot {
    const META: Meta = Meta {
        id: "library.footprint.generate.sot",
        title: "Generate SOT Footprint",
        description: "Generates an IPC-7351 Footprint for a SOT-23 with 3, 5 or 6 leads \
                      or a SOT-223, writes it into a registered Library and runs the Library \
                      check on it. `goals.density` picks the IPC density level, Nominal by \
                      default.",
        category: "library",
        icon: "footprint_generate",
        kind: Kind::File,
    };
    type Params = SotParams;

    fn target(p: &SotParams) -> Target {
        Target {
            library: p.library,
            name: p.name.clone(),
            description: p.description.clone(),
        }
    }

    fn plan(p: &SotParams) -> Result<FootprintPlan, CommandError> {
        use Side::{Left, Right};
        let smd = p.smd();
        smd.check(Applies::default())?;
        let (dims, title) = match p.package {
            SotPackage::Sot23_3 => (TO_236, "SOT-23-3"),
            SotPackage::Sot23_5 => (MO_178, "SOT-23-5"),
            SotPackage::Sot23_6 => (MO_178, "SOT-23-6"),
            SotPackage::Sot223 => (TO_261, "SOT-223"),
        };
        let b = dims.lead_width;
        let leads = match p.package {
            SotPackage::Sot23_3 => vec![
                lead(1, Left, 1, &dims, b),
                lead(2, Left, -1, &dims, b),
                lead(3, Right, 0, &dims, b),
            ],
            SotPackage::Sot23_5 => vec![
                lead(1, Left, 1, &dims, b),
                lead(2, Left, 0, &dims, b),
                lead(3, Left, -1, &dims, b),
                lead(4, Right, -1, &dims, b),
                lead(5, Right, 1, &dims, b),
            ],
            SotPackage::Sot23_6 => gull_wing::dual_row(6, dims.pitch, b),
            SotPackage::Sot223 => vec![
                lead(1, Left, 1, &dims, b),
                lead(2, Left, 0, &dims, b),
                lead(3, Left, -1, &dims, b),
                lead(4, Right, 0, &dims, TO_261_TAB),
            ],
        };
        let density = p.goals.density;
        let name = dims.ipc_name("SOT", leads.len(), density);
        let description = format!("{title}, IPC-7351 {}", density.title());
        let mut plan =
            gull_wing::plan(&dims, &leads, Termination::GullWing, smd, name, description)?;
        plan.body.chamfered = p.chamfer;
        Ok(plan)
    }
}
