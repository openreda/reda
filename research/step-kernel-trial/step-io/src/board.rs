//! The trial board: a prism built face by face through `StepBuilder`. The
//! outline and the holes are 2D loops; each loop is swept into side faces
//! (planes under lines, cylinders under arcs) and the top and bottom planes
//! take the outline as their outer bound and the holes as inner bounds. No
//! booleans, and nothing here evaluates geometry.

use step_io::build::{CurveInput, FaceBoundInput, Frame, Part, SurfaceInput};
use step_io::model::{AdvancedFaceId, EdgeCurveId, ManifoldSolidBrepId};
use step_io::{AuthorError, Orientation, StepBuilder};

const Z: [f64; 3] = [0.0, 0.0, 1.0];
const X: [f64; 3] = [1.0, 0.0, 0.0];

enum Seg {
    Line {
        to: [f64; 2],
    },
    /// An arc about `c` to `to`; `ccw` as seen from +Z.
    Arc {
        c: [f64; 2],
        to: [f64; 2],
        ccw: bool,
    },
}

/// A closed 2D loop: the last segment ends at `start`. The outline runs
/// counter-clockwise and a hole clockwise (seen from +Z), so the material is
/// always on the left.
struct Loop2 {
    start: [f64; 2],
    segs: Vec<Seg>,
}

fn outline(w: f64, h: f64, r: f64) -> Loop2 {
    Loop2 {
        start: [r, 0.0],
        segs: vec![
            Seg::Line { to: [w - r, 0.0] },
            Seg::Arc {
                c: [w - r, r],
                to: [w, r],
                ccw: true,
            },
            Seg::Line { to: [w, h - r] },
            Seg::Arc {
                c: [w - r, h - r],
                to: [w - r, h],
                ccw: true,
            },
            Seg::Line { to: [r, h] },
            Seg::Arc {
                c: [r, h - r],
                to: [0.0, h - r],
                ccw: true,
            },
            Seg::Line { to: [0.0, r] },
            Seg::Arc {
                c: [r, r],
                to: [r, 0.0],
                ccw: true,
            },
        ],
    }
}

/// A round hole as two half circles, so no face needs a seam edge.
fn round_hole(c: [f64; 2], r: f64) -> Loop2 {
    Loop2 {
        start: [c[0] + r, c[1]],
        segs: vec![
            Seg::Arc {
                c,
                to: [c[0] - r, c[1]],
                ccw: false,
            },
            Seg::Arc {
                c,
                to: [c[0] + r, c[1]],
                ccw: false,
            },
        ],
    }
}

/// A slot along X: `len` between the end centres, `width` across.
fn slot(c: [f64; 2], len: f64, width: f64) -> Loop2 {
    let (r, half) = (width / 2.0, len / 2.0);
    let (left, right) = ([c[0] - half, c[1]], [c[0] + half, c[1]]);
    Loop2 {
        start: [left[0], left[1] + r],
        segs: vec![
            Seg::Line {
                to: [right[0], right[1] + r],
            },
            Seg::Arc {
                c: right,
                to: [right[0], right[1] - r],
                ccw: false,
            },
            Seg::Line {
                to: [left[0], left[1] - r],
            },
            Seg::Arc {
                c: left,
                to: [left[0], left[1] + r],
                ccw: false,
            },
        ],
    }
}

/// One swept loop: its bottom and top edges in loop order, and its side faces.
struct Swept {
    bottom: Vec<EdgeCurveId>,
    top: Vec<EdgeCurveId>,
    sides: Vec<AdvancedFaceId>,
}

fn sweep(b: &mut StepBuilder, l: &Loop2, z0: f64, z1: f64) -> Result<Swept, AuthorError> {
    let n = l.segs.len();
    // The start point of each segment.
    let mut pts = vec![l.start];
    for seg in &l.segs[..n - 1] {
        pts.push(match seg {
            Seg::Line { to } | Seg::Arc { to, .. } => *to,
        });
    }
    let mut vb = Vec::new();
    let mut vt = Vec::new();
    let mut vertical = Vec::new();
    for p in &pts {
        let (lo, hi) = (b.vertex([p[0], p[1], z0])?, b.vertex([p[0], p[1], z1])?);
        vertical.push(b.edge(lo, hi, CurveInput::Line)?);
        vb.push(lo);
        vt.push(hi);
    }
    let mut out = Swept {
        bottom: Vec::new(),
        top: Vec::new(),
        sides: Vec::new(),
    };
    for (i, seg) in l.segs.iter().enumerate() {
        let j = (i + 1) % n;
        let (from, to) = (pts[i], pts[j]);
        // Each level's curve runs with the loop, so every edge is used
        // forward along the loop.
        let curve = |z: f64| match seg {
            Seg::Line { .. } => CurveInput::Line,
            Seg::Arc { c, ccw, .. } => {
                let r = ((to[0] - c[0]).powi(2) + (to[1] - c[1]).powi(2)).sqrt();
                let axis = if *ccw { Z } else { [0.0, 0.0, -1.0] };
                CurveInput::Circle(
                    Frame {
                        origin: [c[0], c[1], z],
                        axis,
                        ref_dir: X,
                    },
                    r,
                )
            }
        };
        let eb = b.edge(vb[i], vb[j], curve(z0))?;
        let et = b.edge(vt[i], vt[j], curve(z1))?;
        // The side face, with its normal away from the material (to the right
        // of the direction of travel): along the bottom, up, back along the
        // top, down.
        let (surface, orientation) = match seg {
            Seg::Line { .. } => {
                let d = [to[0] - from[0], to[1] - from[1]];
                let len = (d[0] * d[0] + d[1] * d[1]).sqrt();
                let d = [d[0] / len, d[1] / len];
                let plane = Frame {
                    origin: [from[0], from[1], z0],
                    axis: [d[1], -d[0], 0.0],
                    ref_dir: [d[0], d[1], 0.0],
                };
                (SurfaceInput::Plane(plane), Orientation::Forward)
            }
            Seg::Arc { c, ccw, .. } => {
                let r = ((to[0] - c[0]).powi(2) + (to[1] - c[1]).powi(2)).sqrt();
                let frame = Frame {
                    origin: [c[0], c[1], z0],
                    axis: Z,
                    ref_dir: X,
                };
                // A cylinder's normal points away from its axis: outward for
                // an outline corner, into the material for a hole wall.
                let o = if *ccw {
                    Orientation::Forward
                } else {
                    Orientation::Reversed
                };
                (SurfaceInput::Cylinder(frame, r), o)
            }
        };
        let bound = FaceBoundInput::outer(vec![
            (eb, Orientation::Forward),
            (vertical[j], Orientation::Forward),
            (et, Orientation::Reversed),
            (vertical[i], Orientation::Reversed),
        ]);
        out.sides.push(b.face(surface, orientation, vec![bound])?);
        out.bottom.push(eb);
        out.top.push(et);
    }
    Ok(out)
}

/// The board of SPEC.md: 50 x 30 x 1.6 mm, 3 mm corners, four 3.2 mm holes and
/// one 2 x 8 mm slot. Returns the solid and its face count.
pub fn build(b: &mut StepBuilder, part: Part) -> Result<(ManifoldSolidBrepId, usize), AuthorError> {
    let (w, h, r, z0, z1) = (50.0, 30.0, 3.0, 0.0, 1.6);
    let mut loops = vec![outline(w, h, r)];
    for c in [
        [4.0, 4.0],
        [w - 4.0, 4.0],
        [w - 4.0, h - 4.0],
        [4.0, h - 4.0],
    ] {
        loops.push(round_hole(c, 1.6));
    }
    loops.push(slot([25.0, 15.0], 6.0, 2.0));

    let mut faces = Vec::new();
    let mut top_bounds = Vec::new();
    let mut bottom_bounds = Vec::new();
    for (i, l) in loops.iter().enumerate() {
        let swept = sweep(b, l, z0, z1)?;
        faces.extend(swept.sides);
        let top: Vec<_> = swept
            .top
            .iter()
            .map(|&e| (e, Orientation::Forward))
            .collect();
        // Seen from below the loops run the other way.
        let bottom: Vec<_> = swept
            .bottom
            .iter()
            .rev()
            .map(|&e| (e, Orientation::Reversed))
            .collect();
        if i == 0 {
            top_bounds.push(FaceBoundInput::outer(top));
            bottom_bounds.push(FaceBoundInput::outer(bottom));
        } else {
            top_bounds.push(FaceBoundInput::inner(top));
            bottom_bounds.push(FaceBoundInput::inner(bottom));
        }
    }
    let top_plane = Frame {
        origin: [0.0, 0.0, z1],
        axis: Z,
        ref_dir: X,
    };
    let bottom_plane = Frame {
        origin: [0.0, 0.0, z0],
        axis: [0.0, 0.0, -1.0],
        ref_dir: X,
    };
    faces.push(b.face(
        SurfaceInput::Plane(top_plane),
        Orientation::Forward,
        top_bounds,
    )?);
    faces.push(b.face(
        SurfaceInput::Plane(bottom_plane),
        Orientation::Forward,
        bottom_bounds,
    )?);
    let count = faces.len();
    Ok((b.solid(part, "PCB", faces)?, count))
}
