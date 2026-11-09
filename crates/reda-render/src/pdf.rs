//! Writing Scenes as a vector PDF, one page per Scene.
//!
//! A minimal, uncompressed PDF 1.4: a catalog, a page tree, an info dictionary and one
//! content stream per page. Glyphs are filled outline paths and no font is embedded, so
//! the text is not searchable. The file holds no date, so the same Scenes give the same
//! bytes.

use std::fmt::Write;

use reda_model::Uuid;
use reda_model::geom::Point;

use crate::color::{Palette, Rgba};
use crate::outline::{Seg, circle, mesh_contours, mm, polygons, rect};
use crate::scene::{Primitive, Scene, SceneSource};
use crate::viewport::ink;

/// The document metadata and outline.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PdfInfo {
    pub title: String,
    /// The bookmarks, top level first; each nests its children. Empty for no outline.
    pub outline: Vec<Bookmark>,
}

/// One entry of the PDF's outline.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Bookmark {
    pub title: String,
    /// The page it opens, from 0.
    pub page: usize,
    /// Where on the page, in the Scene's coordinates; `None` fits the whole page.
    pub at: Option<Point>,
    pub children: Vec<Bookmark>,
}

/// Points per mm (72 / 25.4), the scale every page's content stream starts with.
const PT_PER_MM: &str = "2.834646";

/// `scenes` as a PDF, one page each, in order. Each page is the Scene's bounds (a sheet's
/// paper), drawn in `palette`. A Scene's [`links`](Scene::links) become link annotations
/// to the page of the sheet they lead to, when that sheet is in `scenes`.
pub fn to_pdf(scenes: &[Scene], palette: &Palette, info: &PdfInfo) -> Vec<u8> {
    let mut objects: Vec<Vec<u8>> = Vec::new();
    // 1 catalog, 2 page tree, 3 info, a page and its content per Scene, then the link
    // annotations and the outline.
    let page_ids: Vec<usize> = (0..scenes.len()).map(|i| 4 + 2 * i).collect();
    let origins: Vec<Point> = scenes
        .iter()
        .map(|s| s.bounds().unwrap_or_default().0)
        .collect();
    let page_of = |sheet: Uuid| {
        scenes
            .iter()
            .position(|s| s.source == SceneSource::Schematic(sheet))
    };
    let dest = |page: usize, at: Option<Point>| match at {
        None => format!("[{} 0 R /Fit]", page_ids[page]),
        Some(at) => {
            let o = origins[page];
            format!(
                "[{} 0 R /XYZ {} {} null]",
                page_ids[page],
                points((at.x - o.x).0),
                points((at.y - o.y).0)
            )
        }
    };
    let mut next_id = 4 + 2 * scenes.len();
    let mut annots: Vec<Vec<usize>> = Vec::new();
    let mut annot_bodies: Vec<Vec<u8>> = Vec::new();
    for (scene, origin) in scenes.iter().zip(&origins) {
        let mut ids = Vec::new();
        for link in &scene.links {
            let Some(page) = page_of(link.sheet) else {
                continue;
            };
            let (lo, hi) = link.area;
            annot_bodies.push(
                format!(
                    "<< /Type /Annot /Subtype /Link /Rect [{} {} {} {}] /Border [0 0 0] \
                     /Dest {} >>",
                    points((lo.x - origin.x).0),
                    points((lo.y - origin.y).0),
                    points((hi.x - origin.x).0),
                    points((hi.y - origin.y).0),
                    dest(page, Some(link.at))
                )
                .into_bytes(),
            );
            ids.push(next_id);
            next_id += 1;
        }
        annots.push(ids);
    }
    let outline_root = (!info.outline.is_empty()).then_some(next_id);
    let outline = outline_root
        .map(|root| outline_objects(root, &info.outline, &dest))
        .unwrap_or_default();

    objects.push(match outline_root {
        None => b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        Some(root) => {
            format!("<< /Type /Catalog /Pages 2 0 R /Outlines {root} 0 R /PageMode /UseOutlines >>")
                .into_bytes()
        }
    });
    let kids: Vec<String> = page_ids.iter().map(|id| format!("{id} 0 R")).collect();
    objects.push(
        format!(
            "<< /Type /Pages /Kids [{}] /Count {} >>",
            kids.join(" "),
            scenes.len()
        )
        .into_bytes(),
    );
    objects
        .push(format!("<< /Title {} /Producer (REDA) >>", text_string(&info.title),).into_bytes());
    for ((scene, id), (origin, links)) in scenes
        .iter()
        .zip(&page_ids)
        .zip(origins.iter().zip(&annots))
    {
        let (min, max) = scene.bounds().unwrap_or_default();
        let annots = match links.is_empty() {
            true => String::new(),
            false => {
                let refs: Vec<String> = links.iter().map(|id| format!("{id} 0 R")).collect();
                format!(" /Annots [{}]", refs.join(" "))
            }
        };
        objects.push(
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {} {}] /Contents {} 0 R{annots} >>",
                points((max.x - min.x).0),
                points((max.y - min.y).0),
                id + 1
            )
            .into_bytes(),
        );
        let content = page_content(scene, palette, *origin);
        let mut stream = format!("<< /Length {} >>\nstream\n", content.len()).into_bytes();
        stream.extend_from_slice(content.as_bytes());
        stream.extend_from_slice(b"\nendstream");
        objects.push(stream);
    }
    objects.extend(annot_bodies);
    objects.extend(outline);

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

/// The outline's objects, numbered from `root` (the Outlines dictionary), then each
/// bookmark depth first. Every entry is open, so each `/Count` is all its descendants.
fn outline_objects(
    root: usize,
    top: &[Bookmark],
    dest: &dyn Fn(usize, Option<Point>) -> String,
) -> Vec<Vec<u8>> {
    fn count(marks: &[Bookmark]) -> usize {
        marks.iter().map(|m| 1 + count(&m.children)).sum()
    }
    /// Writes `marks` (children of `parent`) into `out`, where `out[0]` is object `root`.
    fn level(
        marks: &[Bookmark],
        parent: usize,
        root: usize,
        out: &mut Vec<Vec<u8>>,
        dest: &dyn Fn(usize, Option<Point>) -> String,
    ) -> Vec<usize> {
        let mut ids = Vec::new();
        let mut id = root + out.len();
        for m in marks {
            ids.push(id);
            id += 1 + count(&m.children);
        }
        for (i, m) in marks.iter().enumerate() {
            let me = ids[i];
            out.push(Vec::new());
            let kids = level(&m.children, me, root, out, dest);
            let mut d = format!(
                "<< /Title {} /Parent {parent} 0 R /Dest {}",
                text_string(&m.title),
                dest(m.page, m.at)
            );
            if i > 0 {
                let _ = write!(d, " /Prev {} 0 R", ids[i - 1]);
            }
            if let Some(next) = ids.get(i + 1) {
                let _ = write!(d, " /Next {next} 0 R");
            }
            if let (Some(first), Some(last)) = (kids.first(), kids.last()) {
                let _ = write!(
                    d,
                    " /First {first} 0 R /Last {last} 0 R /Count {}",
                    count(&m.children)
                );
            }
            d.push_str(" >>");
            out[me - root] = d.into_bytes();
        }
        ids
    }
    let mut out = vec![Vec::new()];
    let ids = level(top, root, root, &mut out, dest);
    out[0] = format!(
        "<< /Type /Outlines /First {} 0 R /Last {} 0 R /Count {} >>",
        ids[0],
        ids[ids.len() - 1],
        count(top)
    )
    .into_bytes();
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
            Primitive::Hairline { a, b } => {
                // Width 0 is the thinnest line the device can draw.
                let _ = write!(c, "{color} RG\n0 w\n1 j\n{} m\n{} l\nS\n", p(*a), p(*b));
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

#[cfg(test)]
mod tests {
    use super::*;

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
