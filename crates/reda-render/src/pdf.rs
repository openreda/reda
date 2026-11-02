//! Writing Scenes as a vector PDF, one page per Scene.
//!
//! A minimal, uncompressed PDF 1.4: a catalog, a page tree, an info dictionary and one
//! content stream per page. Glyphs are filled outline paths and no font is embedded, so
//! the text is not searchable. The only time in the file comes from [`PdfInfo::created`],
//! which callers take from the injected Clock, so the bytes are stable for goldens.

use std::fmt::Write;

use reda_model::geom::Point;

use crate::color::{Palette, Rgba};
use crate::outline::{Seg, circle, mesh_contours, mm, polygons, rect};
use crate::scene::{Primitive, Scene};
use crate::viewport::ink;

/// The document metadata.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PdfInfo {
    pub title: String,
    /// When the file was made: ms since the Unix epoch, UTC.
    pub created: i64,
}

/// Points per mm (72 / 25.4), the scale every page's content stream starts with.
const PT_PER_MM: &str = "2.834646";

/// `scenes` as a PDF, one page each, in order. Each page is the Scene's bounds (a sheet's
/// paper), drawn in `palette`.
pub fn to_pdf(scenes: &[Scene], palette: &Palette, info: &PdfInfo) -> Vec<u8> {
    let mut objects: Vec<Vec<u8>> = Vec::new();
    // 1 catalog, 2 page tree, 3 info, then a page and its content per Scene.
    let page_ids: Vec<usize> = (0..scenes.len()).map(|i| 4 + 2 * i).collect();
    objects.push(b"<< /Type /Catalog /Pages 2 0 R >>".to_vec());
    let kids: Vec<String> = page_ids.iter().map(|id| format!("{id} 0 R")).collect();
    objects.push(
        format!(
            "<< /Type /Pages /Kids [{}] /Count {} >>",
            kids.join(" "),
            scenes.len()
        )
        .into_bytes(),
    );
    objects.push(
        format!(
            "<< /Title {} /Producer (REDA) /CreationDate (D:{}Z) >>",
            text_string(&info.title),
            date(info.created)
        )
        .into_bytes(),
    );
    for (scene, id) in scenes.iter().zip(&page_ids) {
        let (min, max) = scene.bounds().unwrap_or_default();
        objects.push(
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {} {}] /Contents {} 0 R >>",
                points((max.x - min.x).0),
                points((max.y - min.y).0),
                id + 1
            )
            .into_bytes(),
        );
        let content = page_content(scene, palette, min);
        let mut stream = format!("<< /Length {} >>\nstream\n", content.len()).into_bytes();
        stream.extend_from_slice(content.as_bytes());
        stream.extend_from_slice(b"\nendstream");
        objects.push(stream);
    }

    let mut out = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::with_capacity(objects.len());
    for (i, body) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
        out.extend_from_slice(body);
        out.extend_from_slice(b"\nendobj\n");
    }
    let xref = out.len();
    let mut tail = format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1);
    for offset in offsets {
        let _ = writeln!(tail, "{offset:010} 00000 n ");
    }
    let _ = write!(
        tail,
        "trailer\n<< /Size {} /Root 1 0 R /Info 3 0 R >>\nstartxref\n{xref}\n%%EOF\n",
        objects.len() + 1
    );
    out.extend_from_slice(tail.as_bytes());
    out
}

/// One page's drawing operators, in mm from the page's lower-left corner `origin`.
fn page_content(scene: &Scene, palette: &Palette, origin: Point) -> String {
    let mut c = format!("{PT_PER_MM} 0 0 {PT_PER_MM} 0 0 cm\n1 J\n");
    let p = |p: Point| format!("{} {}", mm((p.x - origin.x).0), mm((p.y - origin.y).0));
    let path = |segs: &[Seg]| {
        let mut d = String::new();
        for s in segs {
            match s {
                Seg::Move(a) => d.push_str(&format!("{} m\n", p(*a))),
                Seg::Line(a) => d.push_str(&format!("{} l\n", p(*a))),
                Seg::Cubic(a, b, e) => {
                    d.push_str(&format!("{} {} {} c\n", p(*a), p(*b), p(*e)));
                }
                Seg::Close => d.push_str("h\n"),
            }
        }
        d
    };
    for item in &scene.items {
        let color = rgb(ink(item, palette, &[]));
        match &item.primitive {
            Primitive::Capsule { a, b, width } => {
                let _ = write!(
                    c,
                    "{color} RG\n{} w\n1 j\n{} m\n{} l\nS\n",
                    mm(width.0),
                    p(*a),
                    p(*b)
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
                        let _ = write!(c, "{color} rg\n{d}f\n");
                    }
                    Some(width) => {
                        let _ = write!(c, "{color} RG\n{} w\n0 j\n{d}S\n", mm(width.0));
                    }
                }
            }
            Primitive::Ring {
                center,
                outer,
                inner,
            } => {
                if inner.0 <= 0 {
                    let _ = write!(c, "{color} rg\n{}f\n", path(&circle(*center, *outer)));
                } else {
                    let mid = reda_model::geom::Nm((outer.0 + inner.0) / 2);
                    let _ = write!(
                        c,
                        "{color} RG\n{} w\n{}S\n",
                        mm(outer.0 - inner.0),
                        path(&circle(*center, mid))
                    );
                }
            }
            Primitive::Triangles(mesh) => {
                let d = path(&polygons(&mesh_contours(mesh)));
                if !d.is_empty() {
                    let _ = write!(c, "{color} rg\n{d}f\n");
                }
            }
            Primitive::Glyphs(run) => {
                if !run.strokes.is_empty() {
                    let _ = write!(c, "{color} RG\n{} w\n1 j\n", mm(run.stroke_width.0));
                    for s in &run.strokes {
                        let _ = write!(c, "{} m\n{} l\n", p(s.a), p(s.b));
                    }
                    c.push_str("S\n");
                }
                let d = path(&polygons(&mesh_contours(&run.fill)));
                if !d.is_empty() {
                    let _ = write!(c, "{color} rg\n{d}f\n");
                }
            }
        }
    }
    c
}

/// A colour as three PDF components, 0 to 1 to four places.
fn rgb(Rgba([r, g, b, _]): Rgba) -> String {
    let f = |v: u8| {
        let n = (u32::from(v) * 10_000 + 127) / 255;
        match n {
            0 => "0".to_owned(),
            10_000 => "1".to_owned(),
            n => format!("0.{}", format!("{n:04}").trim_end_matches('0')),
        }
    };
    format!("{} {} {}", f(r), f(g), f(b))
}

/// A length in nm as PDF points, to three places.
fn points(nm: i64) -> String {
    let thousandths = (i128::from(nm) * 72_000 + 12_700_000) / 25_400_000;
    let (whole, frac) = (thousandths / 1000, thousandths % 1000);
    if frac == 0 {
        format!("{whole}")
    } else {
        format!("{whole}.{}", format!("{frac:03}").trim_end_matches('0'))
    }
}

/// A PDF text string: a literal when it is printable ASCII, else UTF-16BE in hex.
fn text_string(s: &str) -> String {
    if s.bytes().all(|b| (0x20..0x7f).contains(&b)) {
        let mut out = String::from("(");
        for ch in s.chars() {
            if matches!(ch, '(' | ')' | '\\') {
                out.push('\\');
            }
            out.push(ch);
        }
        out.push(')');
        out
    } else {
        let mut out = String::from("<FEFF");
        for unit in s.encode_utf16() {
            let _ = write!(out, "{unit:04X}");
        }
        out.push('>');
        out
    }
}

/// `ms` since the Unix epoch as a PDF date's `YYYYMMDDHHmmSS`, UTC.
fn date(ms: i64) -> String {
    let secs = ms.div_euclid(1000);
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // Howard Hinnant's days-to-civil algorithm.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}{month:02}{day:02}{:02}{:02}{:02}",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_are_utc_calendar_dates() {
        assert_eq!(date(0), "19700101000000");
        assert_eq!(date(951_782_400_000 + 3_723_000), "20000229010203");
        assert_eq!(date(-1000), "19691231235959");
    }

    #[test]
    fn non_ascii_titles_are_utf16() {
        assert_eq!(text_string("Ω"), "<FEFF03A9>");
        assert_eq!(text_string("a\\b"), "(a\\\\b)");
    }

    #[test]
    fn colours_and_points_are_exact_decimals() {
        assert_eq!(rgb(Rgba::rgb(255, 0, 128)), "1 0 0.502");
        assert_eq!(points(25_400_000), "72");
        assert_eq!(points(210_000_000), "595.276");
    }
}
