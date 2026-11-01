//! TTF/OTF outline text through `reda_text::layout`, with the bundled Noto Sans.

use reda_geom::{Nm, Point};
use reda_text::{Font, FontError, Mesh, OutlineFont, TextGeometry, layout};

/// Noto Sans capitals are 714 of 1000 font units, so this height makes one unit 1 µm.
const CAP_714_UM: Nm = Nm(714_000);

fn noto(text: &str, height: Nm) -> TextGeometry {
    let font = OutlineFont::noto_sans();
    layout(text, Font::Outline(&font), height, &font)
}

fn covers(mesh: &Mesh, p: Point) -> bool {
    let cross = |a: Point, b: Point| {
        i128::from((b.x - a.x).0) * i128::from((p.y - a.y).0)
            - i128::from((b.y - a.y).0) * i128::from((p.x - a.x).0)
    };
    mesh.indices.chunks_exact(3).any(|t| {
        let [a, b, c] = [0, 1, 2].map(|i| mesh.points[t[i] as usize]);
        let s = [cross(a, b), cross(b, c), cross(c, a)];
        s.iter().all(|&v| v >= 0) || s.iter().all(|&v| v <= 0)
    })
}

#[test]
fn bundled_noto_sans_reports_its_metrics() {
    let font = OutlineFont::noto_sans();

    assert_eq!(font.units_per_em(), 1000);
    assert_eq!(font.cap_height(), 714);
}

#[test]
fn bytes_that_are_not_a_font_are_refused() {
    let err = OutlineFont::from_bytes(b"definitely not a font".to_vec()).unwrap_err();

    assert_eq!(err, FontError::Unreadable);
}

#[test]
fn fonts_load_from_any_bytes_handed_in() {
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/fonts/noto-sans/NotoSans-Regular.ttf"
    ))
    .unwrap();

    let font = OutlineFont::from_bytes(bytes).unwrap();

    assert_eq!(font.cap_height(), 714);
}

#[test]
fn a_capital_sits_on_the_baseline_and_reaches_the_cap_height() {
    let text = noto("H", CAP_714_UM);

    let (lo, hi) = text.ink_bounds().unwrap();
    assert_eq!((lo.y, hi.y), (Nm(0), CAP_714_UM));
    assert!(text.strokes.is_empty());
    assert!(text.advance > hi.x);
}

#[test]
fn a_rectangular_glyph_triangulates_to_exactly_its_rectangle() {
    // The hyphen is a plain bar.
    let text = noto("-", CAP_714_UM);

    let (lo, hi) = text.ink_bounds().unwrap();
    let rect = i128::from((hi.x - lo.x).0) * i128::from((hi.y - lo.y).0);
    assert_eq!(text.fill.double_area(), 2 * rect);
    assert_eq!(text.fill.triangle_count(), 2);
}

#[test]
fn counters_stay_open() {
    let text = noto("O", CAP_714_UM);

    let (lo, hi) = text.ink_bounds().unwrap();
    let centre = Point::new(Nm((lo.x.0 + hi.x.0) / 2), Nm((lo.y.0 + hi.y.0) / 2));
    let left_of_bowl = Point::new(Nm(lo.x.0 + 20_000), centre.y);
    assert!(!covers(&text.fill, centre));
    assert!(covers(&text.fill, left_of_bowl));
}

#[test]
fn shaping_applies_kerning() {
    let pair = noto("AV", CAP_714_UM).advance;
    let apart = noto("A", CAP_714_UM).advance + noto("V", CAP_714_UM).advance;

    assert!(
        pair < apart,
        "AV {pair:?} should kern tighter than {apart:?}"
    );
}

#[test]
fn glyphs_follow_one_another() {
    let one = noto("I", CAP_714_UM);
    let two = noto("II", CAP_714_UM);

    assert_eq!(two.advance, one.advance * 2);
    assert_eq!(two.fill.double_area(), one.fill.double_area() * 2);
    let (lo1, _) = one.ink_bounds().unwrap();
    let (_, hi2) = two.ink_bounds().unwrap();
    let (_, hi1) = one.ink_bounds().unwrap();
    assert_eq!(hi2.x, hi1.x + one.advance);
    assert!(lo1.x > Nm(0));
}

#[test]
fn layout_is_deterministic() {
    assert_eq!(noto("R1 Ωffi", CAP_714_UM), noto("R1 Ωffi", CAP_714_UM));
}

#[test]
fn sample_strings_triangulate_into_valid_meshes() {
    for sample in ["R1 Ωffi", "GND 3V3", "C12 100nF ±5%", "Ünïcødé"] {
        let text = noto(sample, CAP_714_UM);
        let mesh = &text.fill;

        assert!(!mesh.is_empty(), "{sample}");
        assert!(
            mesh.indices
                .iter()
                .all(|&i| (i as usize) < mesh.points.len()),
            "{sample}"
        );
        for t in mesh.indices.chunks_exact(3) {
            let one = Mesh {
                points: mesh.points.clone(),
                indices: t.to_vec(),
            };
            assert!(one.double_area() > 0, "{sample}: degenerate triangle {t:?}");
        }
        let (lo, hi) = text.ink_bounds().unwrap();
        assert!(
            lo.x >= Nm(0) && hi.x <= text.advance,
            "{sample}: ink outside the advance"
        );
    }
}
