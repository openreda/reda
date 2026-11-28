//! What the reader keeps of each entity, in the file's own units and Y-up axes, seen from
//! above, and how each one breaks into straight and circular [`Piece`]s, the only two
//! shapes a drawing's graphics become on the board. Ellipses and splines are fitted with
//! arcs ([`crate::fit`]); text is kept as text.

use std::f64::consts::TAU;

use crate::fit::fit;
use crate::spline::Spline;
use crate::text::Text;

/// A point in drawing units.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Xy {
    pub x: f64,
    pub y: f64,
}

impl Xy {
    pub const fn new(x: f64, y: f64) -> Xy {
        Xy { x, y }
    }

    fn polar(centre: Xy, radius: f64, degrees: f64) -> Xy {
        let (sin, cos) = degrees.to_radians().sin_cos();
        Xy::new(centre.x + radius * cos, centre.y + radius * sin)
    }
}

/// One LWPOLYLINE vertex and the bulge of the segment that leaves it: the tangent of a
/// quarter of that segment's sweep, positive counter-clockwise, 0 for a straight segment.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vertex {
    pub at: Xy,
    pub bulge: f64,
}

/// A read entity's geometry.
#[derive(Clone, Debug, PartialEq)]
pub enum Shape {
    /// LINE.
    Line { from: Xy, to: Xy },
    /// ARC: counter-clockwise from `start` to `end`, both in degrees.
    Arc {
        centre: Xy,
        radius: f64,
        start: f64,
        end: f64,
    },
    /// CIRCLE.
    Circle { centre: Xy, radius: f64 },
    /// LWPOLYLINE or POLYLINE: `closed` joins the last vertex back to the first with its
    /// bulge.
    Polyline { vertices: Vec<Vertex>, closed: bool },
    /// An elliptical arc: `centre + u·cos t + v·sin t` for `t` from `start` to
    /// `start + sweep`, in radians, `sweep` up to a full turn. `u` and `v` are conjugate
    /// half-diameters: an ELLIPSE's major axis and minor axis, or a circle seen at a slant.
    Ellipse {
        centre: Xy,
        u: Xy,
        v: Xy,
        start: f64,
        sweep: f64,
    },
    /// SPLINE.
    Spline(Spline),
    /// TEXT or MTEXT, which becomes free text rather than graphics.
    Text(Text),
}

/// A straight or circular piece of a drawing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Piece {
    Line {
        from: Xy,
        to: Xy,
    },
    /// An arc from `from` to `to` about `centre`, sweeping `sweep` degrees,
    /// counter-clockwise when positive, never a full turn.
    Arc {
        from: Xy,
        to: Xy,
        sweep: f64,
        centre: Xy,
        radius: f64,
    },
}

impl Shape {
    /// The elliptical arc `centre + u·cos t + v·sin t` from `start` sweeping `sweep`
    /// radians (positive, a full turn when 0), as an [`Shape::Arc`] or [`Shape::Circle`]
    /// when it is circular.
    pub fn conic(centre: Xy, u: Xy, v: Xy, start: f64, sweep: f64) -> Shape {
        let sweep = sweep.rem_euclid(TAU);
        let sweep = if sweep == 0.0 || TAU - sweep < 1e-12 {
            TAU
        } else {
            sweep
        };
        let (ru, rv) = (u.x.hypot(u.y), v.x.hypot(v.y));
        let dot = u.x * v.x + u.y * v.y;
        let circular = ru > 0.0 && (ru - rv).abs() <= 1e-9 * ru && dot.abs() <= 1e-9 * ru * ru;
        if !circular {
            return Shape::Ellipse {
                centre,
                u,
                v,
                start,
                sweep,
            };
        }
        let radius = (ru + rv) / 2.0;
        if sweep == TAU {
            return Shape::Circle { centre, radius };
        }
        let base = u.y.atan2(u.x);
        // Clockwise when v is a quarter turn clockwise from u.
        let (from, to) = if u.x * v.y - u.y * v.x > 0.0 {
            (base + start, base + start + sweep)
        } else {
            (base - start - sweep, base - start)
        };
        Shape::Arc {
            centre,
            radius,
            start: from.to_degrees().rem_euclid(360.0),
            end: to.to_degrees().rem_euclid(360.0),
        }
    }

    /// The pieces this shape is drawn with. A circle, and an arc of a full turn, become
    /// two half arcs; a polyline becomes one piece per segment, its bulged segments exact
    /// arcs. Ellipses and splines are fitted with arcs within `tolerance`, in drawing
    /// units. Segments of no length are left out, and text has no pieces.
    pub fn pieces(&self, tolerance: f64) -> Vec<Piece> {
        match *self {
            Shape::Line { from, to } => vec![Piece::Line { from, to }],
            Shape::Arc {
                centre,
                radius,
                start,
                end,
            } => {
                let sweep = (end - start).rem_euclid(360.0);
                // Equal angles draw the whole circle.
                let sweep = if sweep == 0.0 { 360.0 } else { sweep };
                arc_pieces(centre, radius, start, sweep)
            }
            Shape::Circle { centre, radius } => arc_pieces(centre, radius, 0.0, 360.0),
            Shape::Polyline {
                ref vertices,
                closed,
            } => {
                let mut pieces = Vec::new();
                let segments = if closed {
                    vertices.len()
                } else {
                    vertices.len().saturating_sub(1)
                };
                for i in 0..segments {
                    let (a, b) = (vertices[i], vertices[(i + 1) % vertices.len()]);
                    if a.at != b.at {
                        pieces.push(bulged(a.at, b.at, a.bulge));
                    }
                }
                pieces
            }
            Shape::Ellipse {
                centre,
                u,
                v,
                start,
                sweep,
            } => {
                let at = |t: f64| {
                    let (sin, cos) = t.sin_cos();
                    Xy::new(
                        centre.x + u.x * cos + v.x * sin,
                        centre.y + u.y * cos + v.y * sin,
                    )
                };
                // Quarter turns at most, so no span starts and ends at the same point.
                let spans = (sweep / (TAU / 4.0)).ceil().max(1.0) as usize;
                let breaks: Vec<f64> = (0..=spans)
                    .map(|i| start + sweep * i as f64 / spans as f64)
                    .collect();
                fit(&at, &breaks, tolerance)
            }
            Shape::Spline(ref spline) => fit(&|t| spline.point(t), &spline.breaks(), tolerance),
            Shape::Text(_) => Vec::new(),
        }
    }
}

/// An arc of `sweep` degrees (positive) from `start`, split in two halves when it is a
/// full turn.
fn arc_pieces(centre: Xy, radius: f64, start: f64, sweep: f64) -> Vec<Piece> {
    let piece = |start: f64, sweep: f64| Piece::Arc {
        from: Xy::polar(centre, radius, start),
        to: Xy::polar(centre, radius, start + sweep),
        sweep,
        centre,
        radius,
    };
    if sweep >= 360.0 {
        vec![piece(start, 180.0), piece(start + 180.0, 180.0)]
    } else {
        vec![piece(start, sweep)]
    }
}

/// The segment from `from` to `to` with `bulge`: a line, or the exact arc through both
/// ends sweeping 4·atan(bulge).
pub(crate) fn bulged(from: Xy, to: Xy, bulge: f64) -> Piece {
    if bulge == 0.0 {
        return Piece::Line { from, to };
    }
    let sweep = 4.0 * bulge.atan();
    let (dx, dy) = (to.x - from.x, to.y - from.y);
    let chord = dx.hypot(dy);
    // The centre lies on the chord's perpendicular bisector, to the left of the chord for
    // a counter-clockwise sweep under a half turn and to the right past it.
    let offset = chord / 2.0 / (sweep / 2.0).tan();
    let middle = Xy::new((from.x + to.x) / 2.0, (from.y + to.y) / 2.0);
    let centre = Xy::new(
        middle.x - dy / chord * offset,
        middle.y + dx / chord * offset,
    );
    Piece::Arc {
        from,
        to,
        sweep: sweep.to_degrees(),
        centre,
        radius: chord / 2.0 / (sweep / 2.0).sin().abs(),
    }
}
