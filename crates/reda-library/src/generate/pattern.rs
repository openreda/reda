//! A generated land pattern before it has UUIDs, and turning it into a Footprint: the
//! courtyard, the Assembly outline and the text placement are derived here, the same way
//! for every family.

use reda_model::geom::{Millideg, Nm, Point};
use reda_model::{
    Footprint, FootprintGraphic, FootprintLayer, FootprintText, Pad, PadKind, PadShape, Shape, Uuid,
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
/// The largest corner radius of a land.
const MAX_CORNER_RADIUS: Nm = Nm(250_000);

/// One SMD land, a rounded rectangle at rotation 0.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Land {
    pub number: String,
    pub at: Point,
    /// Along x.
    pub width: Nm,
    /// Along y.
    pub height: Nm,
}

impl Land {
    /// The highest y the land's copper reaches.
    pub fn top(&self) -> Nm {
        Nm(self.at.y.0 + self.height.0 / 2)
    }
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
pub struct LandPattern {
    /// The IPC-7351 name, used unless the caller names the Footprint.
    pub name: String,
    pub description: String,
    pub lands: Vec<Land>,
    pub body: Body,
    /// Silkscreen lines, as start and end points.
    pub silkscreen: Vec<(Point, Point)>,
    /// The courtyard excess around the lands and body.
    pub courtyard_excess: Nm,
}

impl LandPattern {
    /// The courtyard rectangle's upper-right corner; it is symmetric about the origin.
    fn courtyard_corner(&self) -> Point {
        let (mut x, mut y) = (self.body.max.0.0 / 2, self.body.max.1.0 / 2);
        for land in &self.lands {
            x = x.max(land.at.x.0.abs() + land.width.0 / 2);
            y = y.max(land.at.y.0.abs() + land.height.0 / 2);
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
            .lands
            .iter()
            .map(|land| Pad {
                uuid: new_id(),
                number: land.number.clone(),
                kind: PadKind::Smd,
                shape: PadShape::RoundedRect,
                at: land.at,
                rotation: Millideg(0),
                width: land.width,
                height: land.height,
                corner_radius: Some(corner_radius(land)),
                hole: None,
                mask_margin: None,
                paste_margin: None,
            })
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
        let designator = FootprintText {
            at: Point::new(
                Nm(0),
                Nm(corner.y.0 + SILKSCREEN_CLEARANCE.0 + FootprintText::DESIGNATOR.size.0 / 2),
            ),
            ..FootprintText::DESIGNATOR
        };
        let value = FootprintText {
            size: value_size(self.body),
            ..FootprintText::VALUE
        };
        let mut footprint = Footprint {
            uuid,
            name,
            description: self.description,
            pads,
            graphics,
            designator,
            value,
        };
        footprint.pads.sort_by_key(|p| p.uuid);
        footprint.graphics.sort_by_key(|g| g.uuid);
        footprint
    }
}

/// A quarter of the land's shorter side, at most 0.25 mm.
fn corner_radius(land: &Land) -> Nm {
    Nm((land.width.0.min(land.height.0) / 4).min(MAX_CORNER_RADIUS.0))
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

/// The value text fits the body: half its shorter side, at most 1 mm, on a 0.01 mm grid.
fn value_size(body: Body) -> Nm {
    let size = (body.nominal.0.0.min(body.nominal.1.0) / 2).min(FootprintText::VALUE.size.0);
    Nm(size / 10_000 * 10_000)
}
