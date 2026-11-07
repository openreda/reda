//! A generated Footprint before it has UUIDs, and turning it into a Footprint: the
//! courtyard, the Assembly outline and the text placement are derived here, the same way
//! for every family.

use reda_model::geom::{Millideg, Nm, Point};
use reda_model::{
    ConnectionStyle, Footprint, FootprintGraphic, FootprintLayer, FootprintText, Hole, Pad,
    PadKind, PadShape, Paste, PasteWindow, Ratio, Shape, TextStyle, Uuid, ZoneConnection,
};

use reda_command::CommandError;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::ipc::courtyard_round;
use super::sections::{
    OutputsParams, PadsParams, PasteArray, PasteKeyword, PasteSplit, SmdResolved, ThResolved,
};

/// The gap kept between Silkscreen lines and copper.
pub const SILKSCREEN_CLEARANCE: Nm = Nm(200_000);
/// An exposed pad at least this long and this wide gets an array of paste windows.
const PASTE_SPLIT: (Nm, Nm) = (Nm(2_100_000), Nm(1_600_000));
/// The largest cell of a paste array, along each axis.
const PASTE_CELL: Nm = Nm(1_500_000);
/// Paste windows are sized down to this grid.
const PASTE_GRID: Nm = Nm(10_000);

/// The shape of a surface-mount generator's Pads.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LeadShape {
    /// A rounded rectangle.
    #[default]
    RoundedRect,
    /// A plain rectangle.
    Rect,
}

/// How the Pads look and which outlines are drawn: what a family's `pads` and `outputs`
/// sections set beyond the land pattern itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Finish {
    pub shape: LeadShape,
    pub corner_ratio: Ratio,
    pub max_corner_radius: Nm,
    /// Paste windows' share of an exposed pad's area, in percent.
    pub paste_coverage: u32,
    /// The exposed pad's paste windows, when given.
    pub paste_array: Option<PasteSplit>,
    /// The Silkscreen lines' width; Silkscreen lines are drawn when `silkscreen`.
    pub silkscreen_width: Nm,
    pub silkscreen: bool,
    /// The courtyard's width, `None` when it is not drawn.
    pub courtyard: Option<Nm>,
    /// The Assembly outline's width, `None` when it is not drawn.
    pub assembly: Option<Nm>,
}

impl Finish {
    /// The outputs `outputs`, with the default Pads.
    pub fn outputs(outputs: &OutputsParams) -> Finish {
        let pads = PadsParams::default();
        Finish {
            shape: pads.shape,
            corner_ratio: pads.corner_ratio.ratio(),
            max_corner_radius: pads.max_corner_radius.nm(),
            paste_coverage: pads.paste_coverage,
            paste_array: None,
            silkscreen_width: outputs.silkscreen_width.nm(),
            silkscreen: outputs.silkscreen,
            courtyard: outputs.courtyard.then_some(outputs.courtyard_width.nm()),
            assembly: outputs.assembly.then_some(outputs.assembly_width.nm()),
        }
    }

    /// The corner ratio of a rounded `width` × `height` Pad or window: `corner_ratio`,
    /// lowered where that would make a radius over `max_corner_radius`.
    fn corner_ratio(&self, width: Nm, height: Nm) -> Ratio {
        self.corner_ratio
            .min(Ratio::from_length(self.max_corner_radius, width, height))
    }

    /// The shape and corner ratio of an SMD Pad or window `width` × `height`.
    fn smd_shape(&self, width: Nm, height: Nm) -> (PadShape, Option<Ratio>) {
        match self.shape {
            LeadShape::RoundedRect => (
                PadShape::RoundedRect,
                Some(self.corner_ratio(width, height)),
            ),
            LeadShape::Rect => (PadShape::Rect, None),
        }
    }
}

impl Default for Finish {
    fn default() -> Self {
        Finish::outputs(&OutputsParams::default())
    }
}

/// One Pad at rotation 0.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PadSpec {
    pub number: String,
    pub at: Point,
    /// Along x.
    pub width: Nm,
    /// Along y.
    pub height: Nm,
    pub kind: LandKind,
}

/// What kind of Pad a Pad becomes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LandKind {
    /// An SMD lead Pad.
    Smd,
    /// An exposed (thermal) pad under the body: an SMD Pad the Zone fill runs straight
    /// into.
    Exposed,
    /// A plated through-hole Pad with a round hole `drill` across: square when `square`
    /// (pin 1), otherwise round or oval.
    ThroughHole { drill: Nm, square: bool },
}

impl PadSpec {
    /// An SMD Pad.
    pub fn smd(number: impl Into<String>, at: Point, width: Nm, height: Nm) -> PadSpec {
        PadSpec {
            number: number.into(),
            at,
            width,
            height,
            kind: LandKind::Smd,
        }
    }

    /// The highest y the Pad's copper reaches.
    pub fn top(&self) -> Nm {
        Nm(self.at.y.0 + self.height.0 / 2)
    }

    /// The Pad's copper as `(x min, y min, x max, y max)`.
    fn bounds(&self) -> (i64, i64, i64, i64) {
        let (x, y) = (self.at.x.0, self.at.y.0);
        let (w, h) = (self.width.0 / 2, self.height.0 / 2);
        (x - w, y - h, x + w, y + h)
    }

    fn pad(&self, uuid: Uuid, finish: &Finish) -> Pad {
        let (kind, shape, corner_ratio, hole) = match self.kind {
            LandKind::Smd | LandKind::Exposed => {
                let (shape, ratio) = finish.smd_shape(self.width, self.height);
                (PadKind::Smd, shape, ratio, None)
            }
            LandKind::ThroughHole { drill, square } => {
                let shape = if square {
                    PadShape::Rect
                } else if self.width == self.height {
                    PadShape::Circle
                } else {
                    PadShape::Oval
                };
                let hole = Hole::new(drill, drill);
                (PadKind::PlatedThroughHole, shape, None, Some(hole))
            }
        };
        Pad {
            uuid,
            number: self.number.clone(),
            kind,
            shape,
            at: self.at,
            rotation: Millideg(0),
            width: self.width,
            height: self.height,
            corner_ratio,
            chamfer_ratio: None,
            corners: None,
            offset: None,
            hole,
            side: None,
            mask: None,
            paste: match self.kind {
                LandKind::Exposed => paste_windows(self.width, self.height, finish),
                _ => None,
            },
            zone_connection: (self.kind == LandKind::Exposed).then(|| ZoneConnection {
                style: Some(ConnectionStyle::Solid),
                ..ZoneConnection::default()
            }),
        }
    }
}

/// The parts of the axis-parallel Silkscreen `lines`, `width` wide, that keep
/// [`SILKSCREEN_CLEARANCE`] from every Pad's copper, measured from the line's edge;
/// pieces no longer than the line width are dropped.
pub fn clip_silkscreen(
    lines: &[(Point, Point)],
    pad_specs: &[PadSpec],
    width: Nm,
) -> Vec<(Point, Point)> {
    let keep = SILKSCREEN_CLEARANCE.0 + width.0 / 2;
    let mut out = vec![];
    for (start, end) in lines {
        let horizontal = start.y == end.y;
        // The line as an interval along its own axis, at `across` on the other one.
        let (across, a, b) = if horizontal {
            (start.y.0, start.x.0.min(end.x.0), start.x.0.max(end.x.0))
        } else {
            (start.x.0, start.y.0.min(end.y.0), start.y.0.max(end.y.0))
        };
        let mut pieces = vec![(a, b)];
        for spec in pad_specs {
            let (x0, y0, x1, y1) = spec.bounds();
            let (along, near) = if horizontal {
                ((x0 - keep, x1 + keep), (y0 - keep, y1 + keep))
            } else {
                ((y0 - keep, y1 + keep), (x0 - keep, x1 + keep))
            };
            if across <= near.0 || across >= near.1 {
                continue;
            }
            pieces = pieces
                .into_iter()
                .flat_map(|(a, b)| {
                    let mut left = vec![];
                    if a < along.0 {
                        left.push((a, b.min(along.0)));
                    }
                    if b > along.1 {
                        left.push((a.max(along.1), b));
                    }
                    left
                })
                .collect();
        }
        for (a, b) in pieces {
            if b - a <= width.0 {
                continue;
            }
            out.push(if horizontal {
                (Point::new(Nm(a), Nm(across)), Point::new(Nm(b), Nm(across)))
            } else {
                (Point::new(Nm(across), Nm(a)), Point::new(Nm(across), Nm(b)))
            });
        }
    }
    out
}

/// A 2-terminal part's polarity mark: a Silkscreen bar `width` wide across the outer end
/// of `pad` (terminal 1, on the left), [`SILKSCREEN_CLEARANCE`] beyond its copper,
/// reaching `half_height` either side of the axis, or past the Pad if that is taller.
pub fn polarity_mark(pad: &PadSpec, half_height: Nm, width: Nm) -> Vec<(Point, Point)> {
    let (x0, _, _, y1) = pad.bounds();
    let x = x0 - SILKSCREEN_CLEARANCE.0 - width.0 / 2;
    let y = half_height.0.max(y1 + width.0 / 2);
    vec![(Point::new(Nm(x), Nm(y)), Point::new(Nm(x), Nm(-y)))]
}

/// Refuses `absent` unless it names distinct leads of the `leads` a package has, leaving
/// at least one.
pub fn check_absent(absent: &[u32], leads: usize) -> Result<(), CommandError> {
    for (i, &n) in absent.iter().enumerate() {
        let pointer = format!("/absent_pins/{i}");
        if n == 0 || n as usize > leads {
            return Err(CommandError::invalid_params(
                &pointer,
                format!("the package's leads are numbered 1 to {leads}"),
            ));
        }
        if absent[..i].contains(&n) {
            return Err(CommandError::invalid_params(
                &pointer,
                format!("pin {n} is listed twice"),
            ));
        }
    }
    if absent.len() >= leads {
        return Err(CommandError::invalid_params(
            "/absent_pins",
            "leave at least one lead",
        ));
    }
    Ok(())
}

/// Leaves out the Pads numbered in `absent`; the others keep their numbers.
pub fn remove_absent(plan: &mut FootprintPlan, absent: &[u32]) {
    plan.pad_specs
        .retain(|spec| !absent.iter().any(|n| n.to_string() == spec.number));
}

/// The four sides of the rectangle with corner `(x, y)`, symmetric about the origin.
pub fn rectangle(x: Nm, y: Nm) -> Vec<(Point, Point)> {
    let p = |x: i64, y: i64| Point::new(Nm(x), Nm(y));
    let (x, y) = (x.0, y.0);
    vec![
        (p(-x, y), p(x, y)),
        (p(x, y), p(x, -y)),
        (p(x, -y), p(-x, -y)),
        (p(-x, -y), p(-x, y)),
    ]
}

/// The package body, centred on the origin.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Body {
    /// The nominal size along x and y, drawn as the Assembly outline.
    pub nominal: (Nm, Nm),
    /// The greatest size along x and y, which the courtyard encloses.
    pub max: (Nm, Nm),
    /// Whether pin 1 is marked, by a chamfer on the Assembly outline's upper-left corner.
    pub chamfered: bool,
}

/// Everything a family computes. Coordinates have y pointing up, and pin 1 sits on the
/// left (chips) or at the upper left (packages with rows of leads), the IPC-7351 zero
/// orientation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FootprintPlan {
    /// The IPC-7351 name, used unless the caller names the Footprint.
    pub name: String,
    pub description: String,
    pub pad_specs: Vec<PadSpec>,
    pub body: Body,
    /// Silkscreen lines, as start and end points.
    pub silkscreen: Vec<(Point, Point)>,
    /// The courtyard excess around the Pads and body.
    pub courtyard_excess: Nm,
    /// The package's greatest height, when the family knows it.
    pub height: Option<Nm>,
    /// What the generator changed from the IPC-7351 land pattern, or why the land
    /// pattern may not be what the caller expects.
    pub warnings: Vec<GeneratorWarning>,
    pub finish: Finish,
    /// The values the params may leave to be calculated, as calculated.
    pub resolved: Resolved,
}

/// What a plan used for the values its params may leave to be calculated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(
    clippy::large_enum_variant,
    reason = "one per plan, copied a handful of times"
)]
pub enum Resolved {
    Smd(SmdResolved),
    Th(ThResolved),
}

impl Resolved {
    pub fn smd(self) -> SmdResolved {
        match self {
            Resolved::Smd(r) => r,
            Resolved::Th(_) => unreachable!("a surface-mount plan"),
        }
    }

    pub fn th(self) -> ThResolved {
        match self {
            Resolved::Th(r) => r,
            Resolved::Smd(_) => unreachable!("a through-hole plan"),
        }
    }
}

/// Something a generator reports about the Footprint it made.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct GeneratorWarning {
    pub code: WarningCode,
    pub message: String,
    /// The Pads it concerns, by number.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pads: Vec<String>,
    /// The params it concerns, as JSON pointers.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub params: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WarningCode {
    /// Pads were cut back: gull-wing Pads so they don't run under the body, or lead Pads
    /// so they keep clear of the exposed pad.
    Trimmed,
    /// An overridden value gives a land pattern other than IPC-7351's.
    NotIpcCompliant,
}

impl GeneratorWarning {
    /// A [`WarningCode::Trimmed`] warning for `pads`.
    pub fn trimmed(pads: Vec<String>, message: String) -> GeneratorWarning {
        GeneratorWarning {
            code: WarningCode::Trimmed,
            message,
            pads,
            params: vec![],
        }
    }
}

impl FootprintPlan {
    /// Whether `other` has the same lead Pads and courtyard excess: the land pattern
    /// IPC-7351 governs. The exposed pad is the package's own size, not an IPC value.
    pub fn same_land_pattern(&self, other: &FootprintPlan) -> bool {
        let leads = |plan: &FootprintPlan| -> Vec<PadSpec> {
            plan.pad_specs
                .iter()
                .filter(|s| s.kind != LandKind::Exposed)
                .cloned()
                .collect()
        };
        leads(self) == leads(other) && self.courtyard_excess == other.courtyard_excess
    }

    /// The courtyard rectangle's upper-right corner; it is symmetric about the origin.
    fn courtyard_corner(&self) -> Point {
        let (mut x, mut y) = (self.body.max.0.0 / 2, self.body.max.1.0 / 2);
        for spec in &self.pad_specs {
            x = x.max(spec.at.x.0.abs() + spec.width.0 / 2);
            y = y.max(spec.at.y.0.abs() + spec.height.0 / 2);
        }
        let excess = self.courtyard_excess.0;
        Point::new(
            courtyard_round(Nm(x + excess)),
            courtyard_round(Nm(y + excess)),
        )
    }

    /// The Footprint called `name`, taking UUIDs from `new_id` in a fixed order.
    pub fn into_footprint(self, name: String, mut new_id: impl FnMut() -> Uuid) -> Footprint {
        let uuid = new_id();
        let pads: Vec<Pad> = self
            .pad_specs
            .iter()
            .map(|spec| spec.pad(new_id(), &self.finish))
            .collect();
        let corner = self.courtyard_corner();
        let mut graphics = vec![];
        if let Some(width) = self.finish.courtyard {
            graphics.push(FootprintGraphic {
                uuid: new_id(),
                layer: FootprintLayer::CourtyardTop,
                shape: Shape::Rect {
                    start: Point::new(Nm(-corner.x.0), Nm(-corner.y.0)),
                    end: corner,
                },
                width,
                fill: false,
            });
        }
        if let Some(width) = self.finish.assembly {
            graphics.push(FootprintGraphic {
                uuid: new_id(),
                layer: FootprintLayer::AssemblyTop,
                shape: assembly_outline(self.body),
                width,
                fill: false,
            });
        }
        if self.finish.silkscreen {
            for (start, end) in &self.silkscreen {
                graphics.push(FootprintGraphic {
                    uuid: new_id(),
                    layer: FootprintLayer::SilkscreenTop,
                    shape: Shape::Line {
                        start: *start,
                        end: *end,
                    },
                    width: self.finish.silkscreen_width,
                    fill: false,
                });
            }
        }
        // The texts take their UUIDs last, after the Pads and graphics.
        let mut designator = FootprintText::designator(new_id());
        designator.at = Point::new(
            Nm(0),
            Nm(corner.y.0 + SILKSCREEN_CLEARANCE.0 + designator.style.height.0 / 2),
        );
        let size = value_size(self.body, designator.style.height);
        let mut value = FootprintText::value(new_id());
        value.style = TextStyle::of_height(size);
        value.at = Point::new(Nm(0), Nm(-corner.y.0 - SILKSCREEN_CLEARANCE.0 - size.0 / 2));
        // The Assembly Designator, in the middle of the body.
        let mut assembly = FootprintText::designator(new_id());
        assembly.layer = FootprintLayer::AssemblyTop;
        assembly.style = TextStyle::of_height(size);
        let mut footprint = Footprint {
            uuid,
            name,
            description: self.description,
            height: self.height,
            pads,
            graphics,
            texts: vec![designator, value, assembly],
            generator: None,
            provenance: None,
        };
        footprint.pads.sort_by_key(|p| p.uuid);
        footprint.graphics.sort_by_key(|g| g.uuid);
        footprint.texts.sort_by_key(|t| t.uuid);
        footprint
    }
}

/// The split paste of an exposed pad `width` × `height`: an array of windows covering
/// about `finish.paste_coverage` of its area, each centred in an equal cell and scaled by
/// the same factor along both axes. Unless `finish` gives the array, a pad of at least
/// [`PASTE_SPLIT`] either way gets cells no larger than [`PASTE_CELL`], and a smaller pad
/// `None`, so it inherits.
fn paste_windows(width: Nm, height: Nm, finish: &Finish) -> Option<Paste> {
    let (columns, rows) = match paste_split(width, height, finish) {
        PasteSplit::Array(a) => (i64::from(a.columns), i64::from(a.rows)),
        PasteSplit::Keyword(PasteKeyword::Inherit) => return None,
    };
    let (cell_x, cell_y) = (width.0 / columns, height.0 / rows);
    let scale = (f64::from(finish.paste_coverage) / 100.0).sqrt();
    let side = |cell: i64| Nm((cell as f64 * scale) as i64 / PASTE_GRID.0 * PASTE_GRID.0);
    let (w, h) = (side(cell_x), side(cell_y));
    let centre = |i: i64, n: i64, cell: i64| Nm((2 * i - (n - 1)) * cell / 2);
    let mut windows = vec![];
    for row in 0..rows {
        for column in 0..columns {
            windows.push(PasteWindow {
                at: Point::new(centre(column, columns, cell_x), centre(row, rows, cell_y)),
                width: w,
                height: h,
                corner_ratio: finish.smd_shape(w, h).1,
            });
        }
    }
    Some(Paste::Windows { windows })
}

/// The paste of an exposed pad `width` × `height`: as `finish` gives it, or else an array
/// of cells no larger than [`PASTE_CELL`] on a pad of at least [`PASTE_SPLIT`] either way,
/// and none on a smaller one.
pub fn paste_split(width: Nm, height: Nm, finish: &Finish) -> PasteSplit {
    if let Some(split) = finish.paste_array {
        return split;
    }
    let (long, short) = (width.max(height), width.min(height));
    if long < PASTE_SPLIT.0 || short < PASTE_SPLIT.1 {
        return PasteSplit::Keyword(PasteKeyword::Inherit);
    }
    let count = |side: Nm| ((side.0 + PASTE_CELL.0 - 1) / PASTE_CELL.0).max(1) as u32;
    PasteSplit::Array(PasteArray {
        columns: count(width),
        rows: count(height),
    })
}

/// The nominal body outline; a polarised body has its upper-left corner chamfered by a
/// quarter of its shorter side, at most 1 mm.
fn assembly_outline(body: Body) -> Shape {
    let (x, y) = (body.nominal.0.0 / 2, body.nominal.1.0 / 2);
    if !body.chamfered {
        return Shape::Rect {
            start: Point::new(Nm(-x), Nm(-y)),
            end: Point::new(Nm(x), Nm(y)),
        };
    }
    let chamfer = (x.min(y) / 2).min(1_000_000);
    let p = |x: i64, y: i64| Point::new(Nm(x), Nm(y));
    Shape::Polygon {
        points: vec![
            p(-x + chamfer, y),
            p(x, y),
            p(x, -y),
            p(-x, -y),
            p(-x, y - chamfer),
        ],
    }
}

/// The value text fits the body: half its shorter side, at most `largest`, on a 0.01 mm
/// grid.
fn value_size(body: Body, largest: Nm) -> Nm {
    let size = (body.nominal.0.0.min(body.nominal.1.0) / 2).min(largest.0);
    Nm(size / 10_000 * 10_000)
}
