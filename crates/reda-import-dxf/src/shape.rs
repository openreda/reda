//! What the reader keeps of each entity, in the file's own units and Y-up axes, and how
//! each one breaks into straight and circular [`Piece`]s, the only two shapes a drawing
//! becomes on the board.

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
    /// LWPOLYLINE: `closed` joins the last vertex back to the first with its bulge.
    Polyline { vertices: Vec<Vertex>, closed: bool },
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
    /// The pieces this shape is drawn with. A circle, and an arc of a full turn, become
    /// two half arcs; a polyline becomes one piece per segment, its bulged segments exact
    /// arcs. Segments of no length are left out.
    pub fn pieces(&self) -> Vec<Piece> {
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
fn bulged(from: Xy, to: Xy, bulge: f64) -> Piece {
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
