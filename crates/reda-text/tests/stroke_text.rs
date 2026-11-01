//! Hershey Roman Simplex stroke text through `reda_text::layout`.

use reda_geom::{Nm, Point};
use reda_text::{Font, JhfError, OutlineFont, Segment, StrokeFont, TextGeometry, layout};

fn pt(x: i64, y: i64) -> Point {
    Point::new(Nm(x), Nm(y))
}

/// The bounds of the filled (outline) part only.
fn fill_bounds(text: &TextGeometry) -> (Point, Point) {
    TextGeometry {
        fill: text.fill.clone(),
        ..TextGeometry::default()
    }
    .ink_bounds()
    .expect("some fill")
}

/// Roman Simplex capitals are 21 Hershey units tall, so a 2.1 mm cap height makes one unit 0.1 mm.
const CAP_2_1_MM: Nm = Nm(2_100_000);

#[test]
fn capital_a_is_three_strokes_on_the_baseline() {
    let hershey = StrokeFont::hershey_roman_simplex();
    let noto = OutlineFont::noto_sans();

    let text = layout("A", Font::Stroke(&hershey), CAP_2_1_MM, &noto);

    // Glyph 501 is `I[RFJ[ RRFZ[ RMTWT`: left bearing -9, right +9, cap top at -12, baseline at +9.
    // Output is y-up with the origin at the left end of the baseline.
    assert_eq!(
        text.strokes,
        vec![
            Segment::new(pt(900_000, 2_100_000), pt(100_000, 0)),
            Segment::new(pt(900_000, 2_100_000), pt(1_700_000, 0)),
            Segment::new(pt(400_000, 700_000), pt(1_400_000, 700_000)),
        ]
    );
    assert_eq!(text.advance, Nm(1_800_000));
    assert!(text.fill.is_empty());
}

#[test]
fn advances_add_up_along_the_string() {
    let hershey = StrokeFont::hershey_roman_simplex();
    let noto = OutlineFont::noto_sans();

    // A is 18 units wide, space 16 (`JZ`), 1 is 20 (`H\`).
    let text = layout("A 1", Font::Stroke(&hershey), CAP_2_1_MM, &noto);

    assert_eq!(text.advance, Nm(5_400_000));
    // The 1 starts 34 units in with left bearing -10; its stroke `NJPISFS[` starts at (-4, -8).
    assert_eq!(text.strokes[3].a, pt(4_000_000, 1_700_000));
}

#[test]
fn height_scales_the_geometry() {
    let hershey = StrokeFont::hershey_roman_simplex();
    let noto = OutlineFont::noto_sans();

    // 1 mm cap height: one Hershey unit is 1/21 mm, rounded to the nearest nm.
    let text = layout("A", Font::Stroke(&hershey), Nm::from_mm(1), &noto);

    assert_eq!(text.advance, Nm(857_143)); // 18/21 mm
    assert_eq!(
        text.ink_bounds(),
        Some((pt(47_619, 0), pt(809_524, 1_000_000)))
    );
}

#[test]
fn a_character_hershey_lacks_falls_back_to_noto_sans() {
    let hershey = StrokeFont::hershey_roman_simplex();
    let noto = OutlineFont::noto_sans();
    let omega_alone = layout("Ω", Font::Outline(&noto), CAP_2_1_MM, &noto);

    let text = layout("AΩA", Font::Stroke(&hershey), CAP_2_1_MM, &noto);

    // Both As stay strokes; Ω is Noto Sans at the same cap height, placed after the first A.
    assert_eq!(text.strokes.len(), 6);
    assert_eq!(text.fill.double_area(), omega_alone.fill.double_area());
    let (omega_lo, omega_hi) = fill_bounds(&text);
    let (alone_lo, alone_hi) = fill_bounds(&omega_alone);
    assert_eq!(omega_lo, alone_lo + pt(1_800_000, 0));
    assert_eq!(omega_hi, alone_hi + pt(1_800_000, 0));
    let second_a = text.strokes[3].a;
    assert_eq!(
        second_a.x,
        Nm(1_800_000) + omega_alone.advance + Nm(900_000)
    );
    assert_eq!(
        text.advance,
        Nm(1_800_000) + omega_alone.advance + Nm(1_800_000)
    );
}

#[test]
fn a_run_of_missing_characters_is_shaped_together() {
    let hershey = StrokeFont::hershey_roman_simplex();
    let noto = OutlineFont::noto_sans();
    // Greek capitals with Noto Sans kerning between them.
    let run = layout("ΤΥ", Font::Outline(&noto), CAP_2_1_MM, &noto);

    let text = layout("ΤΥ", Font::Stroke(&hershey), CAP_2_1_MM, &noto);

    assert_eq!(text, run);
}

#[test]
fn every_printable_ascii_character_is_a_stroke_glyph() {
    let hershey = StrokeFont::hershey_roman_simplex();

    let missing: String = (' '..='~')
        .filter(|&c| hershey.glyph(c).is_none())
        .collect();

    assert_eq!(missing, "");
    assert!(hershey.glyph('Ω').is_none());
}

#[test]
fn jhf_records_may_wrap_onto_the_next_line() {
    // One record of 5 vertices split after the third: bearings, a stroke, a pen-up, a point.
    let source = "    1  5JZRFRR\n RRX\n    2  1KY\n";

    let font = StrokeFont::parse_jhf(source, 'a', -12, 9).unwrap();

    let a = font.glyph('a').unwrap();
    assert_eq!((a.left, a.right), (-8, 8));
    assert_eq!(a.strokes, vec![vec![(0, -12), (0, 0)], vec![(0, 6)]]);
    assert_eq!(font.glyph('b').unwrap().advance(), 14);
}

#[test]
fn truncated_jhf_is_an_error() {
    let err = StrokeFont::parse_jhf("    1  9JZRFRR\n", 'a', -12, 9).unwrap_err();

    assert_eq!(err, JhfError::Truncated { record: 0 });
}
