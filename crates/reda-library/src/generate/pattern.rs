//! A generated Footprint before it has UUIDs, and turning it into a Footprint: the
//! courtyard, the Assembly outline and the text placement are derived here, the same way
//! for every family.

use reda_model::geom::{Millideg, Nm, Point};
use reda_model::{
    Footprint, FootprintGraphic, FootprintLayer, FootprintText, Hole, Pad, PadKind, PadShape,
    Shape, TextStyle, Uuid,
};

use super::ipc::courtyard_round;

/// Courtyard outlines are drawn this wide.
pub const COURTYARD_WIDTH: Nm = Nm(50_000);
/// Assembly outlines are drawn this wide.
pub const ASSEMBLY_WIDTH: Nm = Nm(100_000);
/// Silkscreen lines are drawn this wide.
pub const SILKSCREEN_WIDTH: Nm = Nm(120_000);
/// The gap kept between Silkscreen lines and copper.
pub const SILKSCREEN_CLEARANCE: Nm = Nm(200_000);
/// The largest corner radius of a Pad.
const MAX_CORNER_RADIUS: Nm = Nm(250_000);

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
    /// An SMD Pad, a rounded rectangle.
    Smd,
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

    fn pad(&self, uuid: Uuid) -> Pad {
        let (kind, shape, corner_radius, hole) = match self.kind {
            LandKind::Smd => (
                PadKind::Smd,
                PadShape::RoundedRect,
                Some(corner_radius(self)),
                None,
            ),
            LandKind::ThroughHole { drill, square } => {
                let shape = if square {
                    PadShape::Rect
                } else if self.width == self.height {
                    PadShape::Circle
                } else {
                    PadShape::Oval
                };
                let hole = Hole {
                    width: drill,
                    height: drill,
                };
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
            corner_radius,
            hole,
            mask_margin: None,
            paste_margin: None,
        }
    }
}

/// The parts of the axis-parallel Silkscreen `lines` that keep [`SILKSCREEN_CLEARANCE`]
/// from every Pad's copper, measured from the line's edge; pieces no longer than the
/// line width are dropped.
pub fn clip_silkscreen(lines: &[(Point, Point)], pad_specs: &[PadSpec]) -> Vec<(Point, Point)> {
    let keep = SILKSCREEN_CLEARANCE.0 + SILKSCREEN_WIDTH.0 / 2;
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
            if b - a <= SILKSCREEN_WIDTH.0 {
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
    pub polarised: bool,
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
}

impl FootprintPlan {
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
            .map(|spec| spec.pad(new_id()))
            .collect();
        let corner = self.courtyard_corner();
        let mut graphics = vec![FootprintGraphic {
            uuid: new_id(),
            layer: FootprintLayer::CourtyardTop,
            shape: Shape::Rect {
                start: Point::new(Nm(-corner.x.0), Nm(-corner.y.0)),
                end: corner,
            },
            width: COURTYARD_WIDTH,
            fill: false,
        }];
        graphics.push(FootprintGraphic {
            uuid: new_id(),
            layer: FootprintLayer::AssemblyTop,
            shape: assembly_outline(self.body),
            width: ASSEMBLY_WIDTH,
            fill: false,
        });
        for (start, end) in &self.silkscreen {
            graphics.push(FootprintGraphic {
                uuid: new_id(),
                layer: FootprintLayer::SilkscreenTop,
                shape: Shape::Line {
                    start: *start,
                    end: *end,
                },
                width: SILKSCREEN_WIDTH,
                fill: false,
            });
        }
        // The texts take their UUIDs last, after the Pads and graphics.
        let mut designator = FootprintText::designator(new_id());
        designator.at = Point::new(
            Nm(0),
            Nm(corner.y.0 + SILKSCREEN_CLEARANCE.0 + designator.style.height.0 / 2),
        );
        let mut value = FootprintText::value(new_id());
        value.style = TextStyle::of_height(value_size(self.body, value.style.height));
        let mut footprint = Footprint {
            uuid,
            name,
            description: self.description,
            height: self.height,
            pads,
            graphics,
            texts: vec![designator, value],
            generator: None,
            provenance: None,
        };
        footprint.pads.sort_by_key(|p| p.uuid);
        footprint.graphics.sort_by_key(|g| g.uuid);
        footprint.texts.sort_by_key(|t| t.uuid);
        footprint
    }
}

/// A quarter of the Pad's shorter side, at most 0.25 mm.
fn corner_radius(spec: &PadSpec) -> Nm {
    Nm((spec.width.0.min(spec.height.0) / 4).min(MAX_CORNER_RADIUS.0))
}

/// The nominal body outline; a polarised body has its upper-left corner chamfered by a
/// quarter of its shorter side, at most 1 mm.
fn assembly_outline(body: Body) -> Shape {
    let (x, y) = (body.nominal.0.0 / 2, body.nominal.1.0 / 2);
    if !body.polarised {
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
