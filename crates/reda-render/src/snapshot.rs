//! The Scene as text, for snapshot tests that give the same bytes on every platform.
//!
//! One line per item: `<object> <layer> <colour> <primitive…>`. Lengths are exact mm and
//! angles exact degrees. Text runs are summarised (text, anchor, style, glyph counts and
//! ink bounds) rather than listed triangle by triangle.

use std::fmt::Write;

use reda_model::geom::{Millideg, Nm, Point};

use crate::scene::{HAlign, Primitive, Scene, SceneLayer, SceneSource, VAlign};

impl Scene {
    /// The snapshot text of the Scene.
    pub fn to_text(&self) -> String {
        let mut out = String::new();
        match self.source {
            SceneSource::None => out.push_str("scene\n"),
            SceneSource::Schematic(uuid) => {
                let _ = writeln!(out, "scene schematic {uuid}");
            }
            SceneSource::Pcb(uuid) => {
                let _ = writeln!(out, "scene pcb {uuid}");
            }
        }
        for item in &self.items {
            let layer = match item.layer {
                SceneLayer::Schematic => "sch".to_owned(),
                SceneLayer::Board(uuid) => format!("layer:{uuid}"),
                SceneLayer::Drill => "drill".to_owned(),
                SceneLayer::Ratsnest => "ratsnest".to_owned(),
            };
            let color = item.color.name();
            let _ = write!(out, "{} {layer} {color} ", item.object);
            primitive(&mut out, &item.primitive);
            out.push('\n');
        }
        out
    }
}

fn primitive(out: &mut String, p: &Primitive) {
    let _ = match p {
        Primitive::Capsule { a, b, width } => {
            write!(out, "capsule {} {} w={}", pt(*a), pt(*b), mm(*width))
        }
        Primitive::Rect {
            center,
            size,
            rotation,
            corner_radius,
            stroke,
        } => {
            let _ = write!(
                out,
                "rect c={} size={}x{}",
                pt(*center),
                mm(size.0),
                mm(size.1)
            );
            if rotation.0 != 0 {
                let _ = write!(out, " rot={}", deg(*rotation));
            }
            if corner_radius.0 != 0 {
                let _ = write!(out, " r={}", mm(*corner_radius));
            }
            match stroke {
                Some(w) => write!(out, " stroke={}", mm(*w)),
                None => write!(out, " fill"),
            }
        }
        Primitive::Ring {
            center,
            outer,
            inner,
        } => write!(
            out,
            "ring c={} outer={} inner={}",
            pt(*center),
            mm(*outer),
            mm(*inner)
        ),
        Primitive::Triangles(mesh) => {
            let _ = write!(out, "triangles n={}", mesh.triangle_count());
            bounds(out, p);
            Ok(())
        }
        Primitive::Glyphs(run) => {
            let h = match run.align.h {
                HAlign::Left => "left",
                HAlign::Center => "center",
                HAlign::Right => "right",
            };
            let v = match run.align.v {
                VAlign::Baseline => "baseline",
                VAlign::Middle => "middle",
                VAlign::Top => "top",
            };
            let _ = write!(
                out,
                "text {:?} at={} h={} rot={} align={h}/{v}",
                run.text,
                pt(run.at),
                mm(run.height),
                deg(run.rotation)
            );
            if run.view_label {
                out.push_str(" label");
            }
            if !run.strokes.is_empty() {
                let _ = write!(
                    out,
                    " strokes={} w={}",
                    run.strokes.len(),
                    mm(run.stroke_width)
                );
            }
            if !run.fill.is_empty() {
                let _ = write!(out, " tris={}", run.fill.triangle_count());
            }
            bounds(out, p);
            Ok(())
        }
    };
}

fn bounds(out: &mut String, p: &Primitive) {
    if let Some((lo, hi)) = p.bounds() {
        let _ = write!(out, " bounds={}-{}", pt(lo), pt(hi));
    }
}

fn pt(p: Point) -> String {
    format!("({},{})", mm(p.x), mm(p.y))
}

/// nm as exact mm, without trailing zeros: `12.7`, `-0.000001`, `0`.
pub(crate) fn mm(v: Nm) -> String {
    exact(v.0, 1_000_000)
}

fn deg(a: Millideg) -> String {
    exact(a.0, 1000)
}

fn exact(v: i64, unit: i64) -> String {
    let sign = if v < 0 { "-" } else { "" };
    let v = v.unsigned_abs();
    let unit = unit.unsigned_abs();
    let (whole, frac) = (v / unit, v % unit);
    if frac == 0 {
        return format!("{sign}{whole}");
    }
    let digits = unit.to_string().len() - 1;
    let frac = format!("{frac:0digits$}");
    format!("{sign}{whole}.{}", frac.trim_end_matches('0'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lengths_print_as_exact_millimetres() {
        assert_eq!(mm(Nm(12_700_000)), "12.7");
        assert_eq!(mm(Nm(-1)), "-0.000001");
        assert_eq!(mm(Nm(0)), "0");
        assert_eq!(mm(Nm(-2_540_000)), "-2.54");
        assert_eq!(deg(Millideg(22_500)), "22.5");
    }
}
