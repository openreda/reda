//! Through-hole packages: DIP, and 1×N and 2×N pin headers and sockets.
//!
//! The hole is the lead's greatest diagonal plus 0.20 mm and the Pad the hole plus twice
//! a 0.25 mm annular allowance, the IPC-2222 / IPC-7251 Nominal (level B) values, each
//! rounded up to the 0.05 mm grid. Where that Pad would come closer than 0.25 mm to its
//! neighbour, as on 1.27 mm headers, it is shrunk, keeping at least a 0.10 mm ring.
//! Pin 1's Pad is square, the others round.
//!
//! Through-hole families stay at level B: the IPC sources REDA follows give no other
//! through-hole levels. Their names take the published through-hole form, ending in `B`;
//! the sources give examples rather than a rule, so the reading of each field is REDA's.
//!
//! Zero orientation: pin 1 at the upper left.

use reda_command::{CommandError, Kind, Length, Meta};
use reda_model::Uuid;
use reda_model::geom::{Nm, Point};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::gull_wing::{self, Side};
use super::ipc::{self, Tol};
use super::pattern::{
    Body, Finish, FootprintPlan, LandKind, PadSpec, Resolved, SILKSCREEN_CLEARANCE, check_absent,
    clip_silkscreen, rectangle, remove_absent,
};
use super::sections::{OutputsParams, ThPadsParams, ThResolved, ThTolerancesParams, check_outputs};
use super::{FootprintFamily, Target, hundredths};

/// Added to the lead's greatest diagonal for the hole.
pub const HOLE_ALLOWANCE: Nm = Nm(200_000);
/// Added to the hole for the Pad: twice the annular ring.
pub const LAND_ALLOWANCE: Nm = Nm(500_000);
/// The least copper gap between neighbouring Pads.
pub const MIN_LAND_GAP: Nm = Nm(250_000);
/// The least annular ring.
pub const MIN_ANNULAR_RING: Nm = Nm(100_000);
/// The courtyard excess around a through-hole part.
pub const COURTYARD_EXCESS: Nm = Nm(250_000);

/// A rectangular lead's width and thickness, each as least and greatest.
#[derive(Clone, Copy, Debug)]
struct LeadSection {
    width: Tol,
    thickness: Tol,
}

impl LeadSection {
    const fn mm(width: (f64, f64), thickness: (f64, f64)) -> LeadSection {
        LeadSection {
            width: Tol::mm(width.0, width.1),
            thickness: Tol::mm(thickness.0, thickness.1),
        }
    }

    /// The hole and Pad diameters for this lead with neighbours `pitch` away, or as
    /// `pads` overrides them.
    fn hole_and_pad(self, pitch: Nm, pads: &ThPadsParams) -> Result<(Nm, Nm), CommandError> {
        for (field, value) in [("/pads/hole", pads.hole), ("/pads/diameter", pads.diameter)] {
            if value.is_some_and(|v| v.nm().0 <= 0) {
                return Err(CommandError::invalid_params(
                    field,
                    "the value must be greater than zero",
                ));
            }
        }
        let (w, t) = (self.width.max.0 as f64, self.thickness.max.0 as f64);
        let diagonal = (w * w + t * t).sqrt().ceil() as i64;
        let hole = pads.hole.map_or_else(
            || ipc::round_up(Nm(diagonal + HOLE_ALLOWANCE.0)),
            Length::nm,
        );
        let spec = match pads.diameter {
            Some(diameter) => {
                if diameter.nm() <= hole {
                    return Err(CommandError::invalid_params(
                        "/pads/diameter",
                        "the Pad must be larger than its hole",
                    ));
                }
                return Ok((hole, diameter.nm()));
            }
            None => ipc::round_up(Nm(hole.0 + LAND_ALLOWANCE.0))
                .min(ipc::round_down(Nm(pitch.0 - MIN_LAND_GAP.0))),
        };
        if spec.0 < hole.0 + 2 * MIN_ANNULAR_RING.0 {
            return Err(CommandError::invalid_params(
                "/pitch",
                "the pitch leaves no room for a Pad around the hole",
            ));
        }
        Ok((hole, spec))
    }
}

/// A round Pad, square for pin 1.
fn spec(number: usize, at: Point, diameter: Nm, drill: Nm) -> PadSpec {
    PadSpec {
        number: number.to_string(),
        at,
        width: diameter,
        height: diameter,
        kind: LandKind::ThroughHole {
            drill,
            square: number == 1,
        },
    }
}

/// The distance from a Pad's centre at which Silkscreen centre lines `width` wide may
/// run.
fn silkscreen_offset(diameter: Nm, width: Nm) -> i64 {
    diameter.0 / 2 + SILKSCREEN_CLEARANCE.0 + width.0 / 2
}

/// The courtyard excess, or its override.
fn courtyard_excess(tolerances: &ThTolerancesParams) -> Result<Nm, CommandError> {
    match tolerances.courtyard_excess {
        Some(excess) if excess.nm().0 < 0 => Err(CommandError::invalid_params(
            "/tolerances/courtyard_excess",
            "the value must not be negative",
        )),
        Some(excess) => Ok(excess.nm()),
        None => Ok(COURTYARD_EXCESS),
    }
}

/// MS-001 / MS-011 DIP leads.
const DIP_LEAD: LeadSection = LeadSection::mm((0.36, 0.56), (0.20, 0.36));
const DIP_PITCH: Nm = ipc::mm(2.54);
/// The greatest seated height (A), 0.210 in.
const DIP_HEIGHT: Nm = ipc::mm(5.33);

pub struct Dip;

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DipParams {
    /// The registered Library to write the Footprint into.
    pub library: Uuid,
    /// The Footprint's name. Omitted, it is
    /// `DIP<row spacing>W<lead width>P<pitch>L<body length>H<height>Q<pins>B`, such as
    /// `DIP762W46P254L1016H533Q8B`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The Footprint's description. Omitted, it gets one naming the package and density.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// The number of leads: even, 4 to 64.
    pub pins: usize,
    /// Rows 15.24 mm (600 mil, JEDEC MS-011) apart instead of 7.62 mm (300 mil, MS-001).
    #[serde(default)]
    pub wide: bool,
    /// Lead positions left without a Pad, by number; the other Pads keep their numbers.
    #[serde(default)]
    pub absent_pins: Vec<u32>,
    /// Chamfer the Assembly outline's corner at pin 1. On by default.
    #[serde(default = "super::sections::yes")]
    pub chamfer: bool,
    #[serde(default)]
    pub tolerances: ThTolerancesParams,
    #[serde(default)]
    pub pads: ThPadsParams,
    #[serde(default)]
    pub outputs: OutputsParams,
}

impl FootprintFamily for Dip {
    const META: Meta = Meta {
        id: "library.footprint.generate.dip",
        title: "Generate DIP Footprint",
        description: "Generates an IPC-7251 Nominal through-hole Footprint for a 2.54 mm \
                      pitch DIP with rows 7.62 or 15.24 mm apart, writes it into a registered \
                      Library and runs the Library check on it.",
        category: "library",
        icon: "footprint_generate",
        kind: Kind::File,
    };
    type Params = DipParams;

    fn target(p: &DipParams) -> Target {
        Target {
            library: p.library,
            name: p.name.clone(),
            description: p.description.clone(),
        }
    }

    fn plan(p: &DipParams) -> Result<FootprintPlan, CommandError> {
        if !(4..=64).contains(&p.pins) || !p.pins.is_multiple_of(2) {
            return Err(CommandError::invalid_params(
                "/pins",
                "a DIP needs an even number of 4 to 64 leads",
            ));
        }
        let (row_spacing, body_width) = if p.wide {
            (ipc::mm(15.24), Tol::mm(12.32, 14.73))
        } else {
            (ipc::mm(7.62), Tol::mm(6.10, 7.11))
        };
        // A typical body is as long as its leads take: half the count times the pitch.
        let length = Nm(p.pins as i64 / 2 * DIP_PITCH.0);
        check_absent(&p.absent_pins, p.pins)?;
        check_outputs(&p.outputs)?;
        let finish = Finish::outputs(&p.outputs);
        let silk = finish.silkscreen_width;
        let (hole, diameter) = DIP_LEAD.hole_and_pad(DIP_PITCH, &p.pads)?;
        let x = row_spacing.0 / 2;
        let pad_specs: Vec<PadSpec> = gull_wing::dual_row(p.pins, DIP_PITCH, DIP_LEAD.width)
            .iter()
            .enumerate()
            .map(|(i, lead)| {
                let x = match lead.side {
                    Side::Left => -x,
                    Side::Right => x,
                };
                spec(i + 1, Point::new(Nm(x), lead.y), diameter, hole)
            })
            .collect();
        let outline = rectangle(
            Nm(x - silkscreen_offset(diameter, silk)),
            Nm(length.0 / 2 + silk.0 / 2),
        );
        let silkscreen = clip_silkscreen(&outline, &pad_specs, silk);
        let excess = courtyard_excess(&p.tolerances)?;
        let mut plan = FootprintPlan {
            name: format!(
                "DIP{}W{}P{}L{}H{}Q{}B",
                hundredths(row_spacing),
                hundredths(DIP_LEAD.width.nominal()),
                hundredths(DIP_PITCH),
                hundredths(length),
                hundredths(DIP_HEIGHT),
                p.pins - p.absent_pins.len()
            ),
            description: format!(
                "DIP-{}, {} mm row spacing, IPC-7251 Nominal",
                p.pins,
                row_spacing.0 as f64 / 1e6
            ),
            pad_specs,
            body: Body {
                nominal: (body_width.nominal(), length),
                max: (body_width.max, length),
                chamfered: p.chamfer,
            },
            silkscreen,
            courtyard_excess: excess,
            height: Some(DIP_HEIGHT),
            warnings: vec![],
            finish,
            resolved: Resolved::Th(ThResolved {
                courtyard_excess: excess,
                hole,
                diameter,
            }),
        };
        remove_absent(&mut plan, &p.absent_pins);
        Ok(plan)
    }
}

pub struct Header;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
pub enum HeaderPitch {
    /// 2.54 mm (0.1 in), with 0.64 mm square posts.
    #[serde(rename = "2.54")]
    P254,
    /// 1.27 mm (0.05 in), with 0.40 mm square posts.
    #[serde(rename = "1.27")]
    P127,
}

impl HeaderPitch {
    fn pitch(self) -> Nm {
        match self {
            HeaderPitch::P254 => ipc::mm(2.54),
            HeaderPitch::P127 => ipc::mm(1.27),
        }
    }

    /// The typical height of a straight header or socket above the board.
    fn height(self) -> Nm {
        match self {
            HeaderPitch::P254 => ipc::mm(8.50),
            HeaderPitch::P127 => ipc::mm(4.00),
        }
    }

    fn post(self) -> LeadSection {
        match self {
            HeaderPitch::P254 => LeadSection::mm((0.64, 0.64), (0.64, 0.64)),
            HeaderPitch::P127 => LeadSection::mm((0.40, 0.40), (0.40, 0.40)),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HeaderParams {
    /// The registered Library to write the Footprint into.
    pub library: Uuid,
    /// The Footprint's name. Omitted, it is
    /// `HDRV<row span>W<post width>P<pitch>R<rows>L<length>T<width>H<height>B`, such as
    /// `HDRV254W64P254R2L1270T508H850B`, or `SKTV…` for a socket.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The Footprint's description. Omitted, it gets one naming the package and density.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// 1 or 2 rows.
    pub rows: usize,
    /// The pins in each row, 1 to 50.
    pub pins_per_row: usize,
    pub pitch: HeaderPitch,
    /// A socket (female) rather than a pin header (male). The Footprint is the same.
    #[serde(default)]
    pub socket: bool,
    /// Lead positions left without a Pad, by number; the other Pads keep their numbers.
    #[serde(default)]
    pub absent_pins: Vec<u32>,
    /// Chamfer the Assembly outline's corner at pin 1. On by default.
    #[serde(default = "super::sections::yes")]
    pub chamfer: bool,
    #[serde(default)]
    pub tolerances: ThTolerancesParams,
    #[serde(default)]
    pub pads: ThPadsParams,
    #[serde(default)]
    pub outputs: OutputsParams,
}

impl FootprintFamily for Header {
    const META: Meta = Meta {
        id: "library.footprint.generate.header",
        title: "Generate Header Footprint",
        description: "Generates a through-hole Footprint for a straight 1×N or 2×N pin header \
                      or socket at 2.54 or 1.27 mm pitch, writes it into a registered Library \
                      and runs the Library check on it.",
        category: "library",
        icon: "footprint_generate",
        kind: Kind::File,
    };
    type Params = HeaderParams;

    fn target(p: &HeaderParams) -> Target {
        Target {
            library: p.library,
            name: p.name.clone(),
            description: p.description.clone(),
        }
    }

    fn plan(p: &HeaderParams) -> Result<FootprintPlan, CommandError> {
        if !(1..=2).contains(&p.rows) {
            return Err(CommandError::invalid_params(
                "/rows",
                "a header has 1 or 2 rows",
            ));
        }
        if !(1..=50).contains(&p.pins_per_row) {
            return Err(CommandError::invalid_params(
                "/pins_per_row",
                "a header row has 1 to 50 pins",
            ));
        }
        let pitch = p.pitch.pitch();
        check_absent(&p.absent_pins, p.rows * p.pins_per_row)?;
        check_outputs(&p.outputs)?;
        let finish = Finish::outputs(&p.outputs);
        let silk = finish.silkscreen_width;
        let (hole, diameter) = p.pitch.post().hole_and_pad(pitch, &p.pads)?;
        let (rows, count) = (p.rows as i64, p.pins_per_row as i64);
        // Pins count across the rows first: 1 and 2 side by side at the top.
        let mut pad_specs = vec![];
        for i in 0..count {
            for r in 0..rows {
                let x = (2 * r - (rows - 1)) * pitch.0 / 2;
                let y = ((count - 1) - 2 * i) * pitch.0 / 2;
                let number = (i * rows + r + 1) as usize;
                pad_specs.push(spec(number, Point::new(Nm(x), Nm(y)), diameter, hole));
            }
        }
        let body = (Nm(rows * pitch.0), Nm(count * pitch.0));
        // The body's outline, pushed out where it would come too close to the outer Pads,
        // whose centres are `outer` from the middle.
        let outline = |outer: i64, side: Nm| {
            Nm((side.0 / 2 + silk.0 / 2).max(outer + silkscreen_offset(diameter, silk)))
        };
        let silkscreen = clip_silkscreen(
            &rectangle(
                outline((rows - 1) * pitch.0 / 2, body.0),
                outline((count - 1) * pitch.0 / 2, body.1),
            ),
            &pad_specs,
            silk,
        );
        let (prefix, noun) = if p.socket {
            ("SKTV", "Socket")
        } else {
            ("HDRV", "Pin header")
        };
        let excess = courtyard_excess(&p.tolerances)?;
        let mut plan = FootprintPlan {
            name: format!(
                "{prefix}{}W{}P{}R{rows}L{}T{}H{}B",
                hundredths(Nm((rows - 1) * pitch.0)),
                hundredths(p.pitch.post().width.nominal()),
                hundredths(pitch),
                hundredths(body.1),
                hundredths(body.0),
                hundredths(p.pitch.height())
            ),
            description: format!(
                "{noun}, {rows} × {count}, {} mm pitch",
                pitch.0 as f64 / 1e6
            ),
            pad_specs,
            body: Body {
                nominal: body,
                max: body,
                chamfered: p.chamfer,
            },
            silkscreen,
            courtyard_excess: excess,
            height: Some(p.pitch.height()),
            warnings: vec![],
            finish,
            resolved: Resolved::Th(ThResolved {
                courtyard_excess: excess,
                hole,
                diameter,
            }),
        };
        remove_absent(&mut plan, &p.absent_pins);
        Ok(plan)
    }
}
