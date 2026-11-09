//! The SVG and PDF writers, checked as byte goldens of small hand-built Scenes.

use std::path::{Path, PathBuf};

use reda_model::Uuid;
use reda_model::geom::{Millideg, Nm, Point};
use reda_render::{
    Align, ColorKey, GlyphRun, Item, Palette, PdfInfo, Primitive, Scene, SceneLayer, SceneSource,
    SchematicColor, Theme, Viewport, to_pdf, to_svg,
};
use reda_testkit::assert_golden;
use reda_text::{Mesh, Segment};

fn golden(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(name)
}

fn mm(v: i64) -> Nm {
    Nm::from_mm(v)
}

fn pt(x: i64, y: i64) -> Point {
    Point::new(mm(x), mm(y))
}

fn uuid(n: u128) -> Uuid {
    Uuid::v4_from_bits(n)
}

fn item(object: u128, color: SchematicColor, primitive: Primitive) -> Item {
    Item {
        child: None,
        object: uuid(object),
        layer: SceneLayer::Schematic,
        color: ColorKey::Schematic(color),
        primitive,
        look: Default::default(),
    }
}

/// Two triangles making the unit square at `origin`, sharing their diagonal.
fn square(origin: Point, side: Nm) -> Mesh {
    let (x, y) = (origin.x, origin.y);
    Mesh {
        points: vec![
            Point::new(x, y),
            Point::new(x + side, y),
            Point::new(x + side, y + side),
            Point::new(x, y + side),
        ],
        indices: vec![0, 1, 2, 0, 2, 3],
    }
}

/// One of every primitive on a 40 x 30 mm paper.
fn every_primitive() -> Scene {
    use SchematicColor as C;
    let run = GlyphRun {
        text: "A".into(),
        at: pt(30, 5),
        height: mm(2),
        rotation: Millideg(0),
        align: Align::default(),
        view_label: false,
        strokes: vec![Segment::new(pt(30, 5), pt(31, 7))],
        stroke_width: Nm(200_000),
        fill: square(pt(32, 5), mm(2)),
    };
    Scene {
        source: SceneSource::None,
        items: vec![
            item(
                1,
                C::Paper,
                Primitive::Rect {
                    center: pt(20, 15),
                    size: (mm(40), mm(30)),
                    rotation: Millideg(0),
                    corner_radius: Nm(0),
                    stroke: None,
                },
            ),
            item(
                2,
                C::Wire,
                Primitive::Capsule {
                    a: pt(2, 2),
                    b: pt(12, 2),
                    width: Nm(152_400),
                },
            ),
            item(
                3,
                C::Body,
                Primitive::Rect {
                    center: pt(10, 20),
                    size: (mm(6), mm(4)),
                    rotation: Millideg(30_000),
                    corner_radius: mm(1),
                    stroke: Some(Nm(254_000)),
                },
            ),
            item(
                4,
                C::Junction,
                Primitive::Ring {
                    center: pt(20, 20),
                    outer: mm(1),
                    inner: Nm(0),
                },
            ),
            item(
                5,
                C::Port,
                Primitive::Ring {
                    center: pt(25, 20),
                    outer: mm(2),
                    inner: mm(1),
                },
            ),
            item(
                6,
                C::BodyFill,
                Primitive::Triangles(square(pt(2, 8), mm(4))),
            ),
            item(7, C::Text, Primitive::Glyphs(run)),
        ],
    }
}

#[test]
fn svg_draws_every_primitive_with_y_up() {
    let scene = every_primitive();
    let (min, max) = scene.bounds().unwrap();
    let svg = to_svg(
        &scene,
        &Palette::shipped(Theme::Light),
        &Viewport::fit(min, max, 400),
        &[],
    );
    assert_golden(golden("every_primitive.svg"), svg.as_bytes());
}

#[test]
fn fitting_keeps_the_aspect_ratio_on_the_long_side() {
    let v = Viewport::fit(pt(0, 0), pt(40, 30), 400);
    assert_eq!((v.width, v.height), (400, 300));
    let v = Viewport::fit(pt(0, 0), pt(30, 40), 1024);
    assert_eq!((v.width, v.height), (768, 1024));
}

#[test]
fn highlighting_dims_everything_else_but_the_paper() {
    let scene = every_primitive();
    let (min, max) = scene.bounds().unwrap();
    let palette = Palette::shipped(Theme::Light);
    let plain = to_svg(&scene, &palette, &Viewport::fit(min, max, 400), &[]);
    let lit = to_svg(&scene, &palette, &Viewport::fit(min, max, 400), &[uuid(2)]);
    let line = |svg: &str, object: u128| {
        let tag = format!("data-object=\"{}\"", uuid(object));
        svg.lines().find(|l| l.contains(&tag)).unwrap().to_owned()
    };
    assert_eq!(line(&plain, 1), line(&lit, 1), "the paper is never dimmed");
    assert_eq!(
        line(&plain, 2),
        line(&lit, 2),
        "the highlighted Wire keeps its colour"
    );
    assert_ne!(line(&plain, 4), line(&lit, 4), "the Junction is dimmed");
}

/// 2026-01-01T00:00:00Z, the test Clock's time, in ms.
const EPOCH_MS: i64 = 1_767_225_600_000;

fn info() -> PdfInfo {
    PdfInfo {
        title: "Demo (rev B)".into(),
        created: EPOCH_MS,
    }
}

#[test]
fn pdf_writes_one_vector_page_per_scene() {
    let first = every_primitive();
    let mut second = every_primitive();
    second.items.truncate(2);
    let pdf = to_pdf(&[first, second], &Palette::shipped(Theme::Light), &info());
    assert_golden(golden("every_primitive.pdf"), &pdf);
}

#[test]
fn pdf_embeds_no_font_and_dates_itself_from_the_clock() {
    let pdf = to_pdf(
        &[every_primitive()],
        &Palette::shipped(Theme::Light),
        &info(),
    );
    let text = String::from_utf8_lossy(&pdf);
    assert!(text.starts_with("%PDF-1.4\n"));
    assert!(!text.contains("/Font"), "glyphs are paths, not text");
    assert!(text.contains("/CreationDate (D:20260101000000Z)"));
    assert!(text.contains("/Title (Demo \\(rev B\\))"));
    assert!(
        text.contains("/MediaBox [0 0 113.386 85.039]"),
        "40 x 30 mm in points"
    );
    assert!(text.trim_end().ends_with("%%EOF"));
}

#[test]
fn pdf_cross_reference_offsets_point_at_their_objects() {
    let pdf = to_pdf(
        &[every_primitive()],
        &Palette::shipped(Theme::Light),
        &info(),
    );
    let text = String::from_utf8_lossy(&pdf);
    let xref = text.rfind("\nxref\n").unwrap() + 1;
    let startxref: usize = text
        .rsplit("startxref\n")
        .next()
        .unwrap()
        .lines()
        .next()
        .unwrap()
        .parse()
        .unwrap();
    assert_eq!(startxref, xref);
    let entries: Vec<&str> = text[xref..]
        .lines()
        .skip(3)
        .take_while(|l| l.ends_with(" n "))
        .collect();
    assert!(!entries.is_empty());
    for (i, e) in entries.iter().enumerate() {
        let offset: usize = e[..10].parse().unwrap();
        assert!(
            text[offset..].starts_with(&format!("{} 0 obj", i + 1)),
            "object {} at {offset}",
            i + 1
        );
    }
}
