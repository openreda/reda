//! `pcb.board.move`: moves the whole board, the Board Outline, its Board Cutouts, the
//! Output Origin and every object with any part inside the outline. Locks don't stop it,
//! and moved objects keep their locks: a board moves as a whole.

use reda_command::{
    AvailabilityContext, Command, CommandError, Context, Fonts, Kind, Length, Meta, Registry,
    Unavailable,
};
use reda_model::geom::grown::{Core, P, Prim};
use reda_model::geom::{Bounds, Millideg, Nm, Point, Vertex};
use reda_model::{GraphicShape, InDocument, KeepoutShape, Object, PcbDocument, TrackGeometry};
use schemars::JsonSchema;
use serde::Deserialize;

use super::Placed;
use crate::commands::{needs_pcb, pcb};
use crate::copper::shape::{BoardShape as _, Shape, arc_points};
use crate::placed::{footprint_bounds, placed_graphics, placed_pads};
use crate::zone::poly::{self, Area};

pub(super) fn register_all(registry: &mut Registry) {
    registry.add(BoardMove);
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BoardMoveParams {
    /// How far to move it along x.
    pub dx: Length,
    /// How far to move it along y.
    pub dy: Length,
}

pub struct BoardMove;

impl Command for BoardMove {
    const META: Meta = Meta {
        id: "pcb.board.move",
        title: "Move Board",
        description: "Moves the whole board by `dx`, `dy`: the Board Outline, its Board \
                      Cutouts, the Output Origin and every object with any part inside the \
                      outline. A Track crossing the edge moves whole; objects wholly \
                      outside, such as unplaced Footprints, stay. Locked objects move too \
                      and stay locked.",
        category: "pcb",
        icon: "move",
        kind: Kind::Edit,
    };
    type Params = BoardMoveParams;
    type Output = Placed;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: BoardMoveParams) -> Result<Placed, CommandError> {
        let by = Point::new(p.dx.nm(), p.dy.nm());
        let moved = moved(pcb(cx)?, cx.fonts(), by)?;
        let objects = moved.iter().map(Object::uuid).collect();
        let tx = cx.tx()?;
        for object in moved {
            tx.replace(object)?;
        }
        Ok(Placed { objects })
    }
}

/// The board's area for telling what is inside it.
struct Board {
    area: Area,
    edge: Shape,
}

impl Board {
    /// Whether any part of `shape` lies inside the board: a point of its core inside, or
    /// its width reaching over the edge.
    fn holds(&self, shape: &Shape) -> bool {
        let inside = shape.prims.iter().any(|prim| {
            prim.core
                .points()
                .into_iter()
                .any(|p| poly::inside(&self.area, Point::new(Nm(p[0]), Nm(p[1]))))
        });
        inside || self.edge.distance(shape) < -reda_model::geom::grown::TOUCH
    }

    /// Whether an area overlaps the board with some area.
    fn overlaps(&self, area: &Area) -> bool {
        poly::overlaps(&self.area, area)
    }

    fn holds_contour(&self, vertices: &[Vertex]) -> bool {
        !vertices.is_empty() && self.overlaps(&poly::polygon(poly::contour(vertices)))
    }
}

fn xy(p: Point) -> P {
    [p.x.0, p.y.0]
}

/// A filled box, as a shape.
fn boxed(b: Bounds) -> Prim {
    let (lo, hi) = (b.min, b.max);
    let corners = [
        [lo.x.0, lo.y.0],
        [hi.x.0, lo.y.0],
        [hi.x.0, hi.y.0],
        [lo.x.0, hi.y.0],
    ];
    Prim {
        core: Core::Poly(corners.to_vec()),
        radius: 0,
    }
}

/// A line or arc of pen `width` as a shape.
fn stroke(from: Point, to: Point, sweep: Millideg, width: Nm) -> Shape {
    let points = if sweep == Millideg::ZERO {
        vec![from, to]
    } else {
        arc_points(from, to, sweep)
    };
    Shape::chain(&points, width.0 / 2)
}

fn graphic_shape(shape: &GraphicShape, width: Nm) -> Shape {
    match *shape {
        GraphicShape::Line { from, to } => stroke(from, to, Millideg::ZERO, width),
        GraphicShape::Arc { from, to, sweep } => stroke(from, to, sweep, width),
        GraphicShape::Rect { from, to, filled } => {
            let corners = [
                from,
                Point::new(to.x, from.y),
                to,
                Point::new(from.x, to.y),
                from,
            ];
            let mut shape = Shape::chain(&corners, width.0 / 2);
            if filled {
                shape.prims.push(Prim {
                    core: Core::Poly(corners[..4].iter().map(|p| xy(*p)).collect()),
                    radius: 0,
                });
            }
            shape
        }
    }
}

/// Free text as drawn: its strokes and filled glyphs, or its anchor when it draws nothing.
fn text_shape(text: &reda_model::PcbText, fonts: &Fonts) -> Shape {
    let drawn = crate::text_geometry(text, fonts);
    let mut prims: Vec<Prim> = drawn
        .strokes
        .iter()
        .map(|s| Prim {
            core: Core::Seg(xy(s.a), xy(s.b)),
            radius: drawn.stroke_width.0 / 2,
        })
        .collect();
    for tri in drawn.fill.indices.chunks(3) {
        let corner = |i: u32| xy(drawn.fill.points[i as usize]);
        if let [a, b, c] = *tri {
            prims.push(Prim {
                core: Core::Poly(vec![corner(a), corner(b), corner(c)]),
                radius: 0,
            });
        }
    }
    if prims.is_empty() {
        prims.push(Prim {
            core: Core::Seg(xy(text.position), xy(text.position)),
            radius: 0,
        });
    }
    Shape { prims }
}

fn shift(vertices: &mut [Vertex], by: Point) {
    for v in vertices {
        let p = v.point() + by;
        (v.x, v.y) = (p.x, p.y);
    }
}

/// Every object the move changes, moved by `by`.
fn moved(pcb: &PcbDocument, fonts: &Fonts, by: Point) -> Result<Vec<Object>, CommandError> {
    let doc = pcb.uuid;
    let outline = pcb
        .outline()
        .ok_or_else(|| CommandError::not_found("the board has no Board Outline to move"))?;
    let area = poly::polygon(poly::contour(&outline.vertices));
    let board = Board {
        edge: poly::boundary(&area),
        area,
    };
    let mut out = Vec::new();

    let mut o = outline.clone();
    shift(&mut o.vertices, by);
    out.push(Object::BoardOutline(InDocument::new(doc, o)));
    for c in pcb.cutouts() {
        let mut c = c.clone();
        shift(&mut c.vertices, by);
        out.push(Object::BoardCutout(InDocument::new(doc, c)));
    }
    if let Some(origin) = pcb.output_origin() {
        let mut origin = origin.clone();
        origin.at = origin.at + by;
        out.push(Object::OutputOrigin(InDocument::new(doc, origin)));
    }
    for f in pcb.footprints() {
        let pads = placed_pads(pcb, f).into_iter().map(|p| boxed(p.bounds()));
        let drawing = placed_graphics(pcb, f)
            .into_iter()
            .filter_map(|g| g.shape.bounds().map(boxed));
        let mut prims: Vec<Prim> = pads.chain(drawing).collect();
        if prims.is_empty() {
            prims.push(boxed(footprint_bounds(pcb, f)));
        }
        if board.holds(&Shape { prims }) {
            let mut f = f.clone();
            f.at = f.at + by;
            out.push(Object::FootprintInstance(InDocument::new(doc, f)));
        }
    }
    for t in pcb.tracks() {
        if board.holds(&Shape::track(&t.geometry, t.width)) {
            let mut t = t.clone();
            t.geometry = match t.geometry {
                TrackGeometry::Line { from, to } => TrackGeometry::Line {
                    from: from + by,
                    to: to + by,
                },
                TrackGeometry::Arc { from, to, sweep } => TrackGeometry::Arc {
                    from: from + by,
                    to: to + by,
                    sweep,
                },
            };
            out.push(Object::Track(InDocument::new(doc, t)));
        }
    }
    for v in pcb.vias() {
        if board.holds(&Shape::disc(v.at, v.diameter)) {
            let mut v = v.clone();
            v.at = v.at + by;
            out.push(Object::Via(InDocument::new(doc, v)));
        }
    }
    for p in pcb.free_pads() {
        if board.holds(&Shape::free_pad(p)) {
            let mut p = p.clone();
            p.at = p.at + by;
            out.push(Object::FreePad(InDocument::new(doc, p)));
        }
    }
    // A Zone that follows the Board Outline has no vertices of its own and follows it.
    for z in pcb.zones() {
        if board.holds_contour(&z.vertices) {
            let mut z = z.clone();
            shift(&mut z.vertices, by);
            out.push(Object::Zone(InDocument::new(doc, z)));
        }
    }
    for c in pcb.zone_cutouts() {
        if board.holds_contour(&c.vertices) {
            let mut c = c.clone();
            shift(&mut c.vertices, by);
            out.push(Object::ZoneCutout(InDocument::new(doc, c)));
        }
    }
    for k in pcb.keepouts() {
        if board.overlaps(&crate::keepout::area(k)) {
            let mut k = k.clone();
            match &mut k.shape {
                KeepoutShape::Polygon { vertices } | KeepoutShape::Line { vertices, .. } => {
                    shift(vertices, by)
                }
            }
            out.push(Object::Keepout(InDocument::new(doc, k)));
        }
    }
    for t in pcb.texts() {
        if board.holds(&text_shape(t, fonts)) {
            let mut t = t.clone();
            t.position = t.position + by;
            out.push(Object::PcbText(InDocument::new(doc, t)));
        }
    }
    for g in pcb.graphics() {
        if board.holds(&graphic_shape(&g.shape, g.width)) {
            let mut g = g.clone();
            g.shape = match g.shape {
                GraphicShape::Line { from, to } => GraphicShape::Line {
                    from: from + by,
                    to: to + by,
                },
                GraphicShape::Arc { from, to, sweep } => GraphicShape::Arc {
                    from: from + by,
                    to: to + by,
                    sweep,
                },
                GraphicShape::Rect { from, to, filled } => GraphicShape::Rect {
                    from: from + by,
                    to: to + by,
                    filled,
                },
            };
            out.push(Object::PcbGraphic(InDocument::new(doc, g)));
        }
    }
    for d in pcb.dimensions() {
        let drawn = crate::dimension::drawing(d, fonts);
        let mut shape = Shape { prims: Vec::new() };
        for line in &drawn.lines {
            shape
                .prims
                .extend(stroke(line.from, line.to, Millideg::ZERO, line.width).prims);
        }
        if let Some(text) = &drawn.text {
            shape.prims.extend(text_shape(text, fonts).prims);
        }
        if board.holds(&shape) {
            let mut d = d.clone();
            super::move_dimension(&mut d, super::Op::Translate(by));
            out.push(Object::Dimension(InDocument::new(doc, d)));
        }
    }
    Ok(out)
}
