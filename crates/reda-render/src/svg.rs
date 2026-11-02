//! Writing a Scene as SVG.
//!
//! The SVG's user unit is the millimetre and its y axis points down, so every Document
//! point (x, y) is written as (x, -y) in mm. Glyphs are filled outlines; no font is used.

use std::fmt::Write;

use reda_model::Uuid;
use reda_model::geom::Point;

use crate::color::{Palette, Rgba};
use crate::outline::{Seg, mesh_contours, mm, polygons, rect};
use crate::scene::{Primitive, Scene};
use crate::viewport::{Viewport, ink};

/// The Scene as an SVG document showing `view`. Items whose object is in `highlight` keep
/// their colour and everything else is dimmed; an empty `highlight` dims nothing.
///
/// Every element carries its object's UUID as `data-object`.
pub fn to_svg(scene: &Scene, palette: &Palette, view: &Viewport, highlight: &[Uuid]) -> String {
    let mut out = String::new();
    let (w, h) = (view.max.x - view.min.x, view.max.y - view.min.y);
    let _ = writeln!(
        out,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" viewBox=\"{} {} {} {}\">",
        view.width,
        view.height,
        mm(view.min.x.0),
        mm(-view.max.y.0),
        mm(w.0),
        mm(h.0)
    );
    for item in &scene.items {
        let color = hex(ink(item, palette, highlight));
        let _ = write!(out, "<");
        match &item.primitive {
            Primitive::Capsule { a, b, width } => {
                let _ = write!(
                    out,
                    "path d=\"M{} {}L{} {}\" fill=\"none\" stroke=\"{color}\" stroke-width=\"{}\" stroke-linecap=\"round\"",
                    mm(a.x.0),
                    mm(-a.y.0),
                    mm(b.x.0),
                    mm(-b.y.0),
                    mm(width.0)
                );
            }
            Primitive::Rect {
                center,
                size,
                rotation,
                corner_radius,
                stroke,
            } => {
                let d = path(&rect(*center, *size, *rotation, *corner_radius));
                match stroke {
                    None => {
                        let _ = write!(out, "path d=\"{d}\" fill=\"{color}\"");
                    }
                    Some(width) => {
                        let _ = write!(
                            out,
                            "path d=\"{d}\" fill=\"none\" stroke=\"{color}\" stroke-width=\"{}\"",
                            mm(width.0)
                        );
                    }
                }
            }
            Primitive::Ring {
                center,
                outer,
                inner,
            } => {
                if inner.0 <= 0 {
                    let _ = write!(
                        out,
                        "circle cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"{color}\"",
                        mm(center.x.0),
                        mm(-center.y.0),
                        mm(outer.0)
                    );
                } else {
                    let _ = write!(
                        out,
                        "circle cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"none\" stroke=\"{color}\" stroke-width=\"{}\"",
                        mm(center.x.0),
                        mm(-center.y.0),
                        mm((outer.0 + inner.0) / 2),
                        mm(outer.0 - inner.0)
                    );
                }
            }
            Primitive::Triangles(mesh) => {
                let d = path(&polygons(&mesh_contours(mesh)));
                let _ = write!(out, "path d=\"{d}\" fill=\"{color}\"");
            }
            Primitive::Glyphs(run) => {
                let mut d = String::new();
                for s in &run.strokes {
                    let _ = write!(
                        d,
                        "M{} {}L{} {}",
                        mm(s.a.x.0),
                        mm(-s.a.y.0),
                        mm(s.b.x.0),
                        mm(-s.b.y.0)
                    );
                }
                let fill = path(&polygons(&mesh_contours(&run.fill)));
                let _ = write!(
                    out,
                    "g fill=\"{color}\" stroke=\"{color}\" stroke-width=\"{}\" stroke-linecap=\"round\"",
                    mm(run.stroke_width.0)
                );
                let _ = write!(out, " data-object=\"{}\">", item.object);
                if !d.is_empty() {
                    let _ = write!(out, "<path d=\"{d}\" fill=\"none\"/>");
                }
                if !fill.is_empty() {
                    let _ = write!(out, "<path d=\"{fill}\" stroke=\"none\"/>");
                }
                out.push_str("</g>\n");
                continue;
            }
        }
        let _ = writeln!(out, " data-object=\"{}\"/>", item.object);
    }
    out.push_str("</svg>\n");
    out
}

fn hex(Rgba([r, g, b, a]): Rgba) -> String {
    if a == 255 {
        format!("#{r:02x}{g:02x}{b:02x}")
    } else {
        format!("#{r:02x}{g:02x}{b:02x}{a:02x}")
    }
}

/// SVG path data, with y flipped.
fn path(segs: &[Seg]) -> String {
    let p = |p: &Point| format!("{} {}", mm(p.x.0), mm(-p.y.0));
    let mut d = String::new();
    for s in segs {
        match s {
            Seg::Move(a) => d.push_str(&format!("M{}", p(a))),
            Seg::Line(a) => d.push_str(&format!("L{}", p(a))),
            Seg::Cubic(a, b, c) => d.push_str(&format!("C{} {} {}", p(a), p(b), p(c))),
            Seg::Close => d.push('Z'),
        }
    }
    d
}
