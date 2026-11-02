//! Colour schemes: one per theme, keyed by schematic object kind.

use reda_render::{ColorKey, Palette, Rgba, SchematicColor, Theme};

/// Relative luminance (WCAG), 0..1.
fn luminance(Rgba([r, g, b, _]): Rgba) -> f64 {
    let lin = |c: u8| {
        let c = f64::from(c) / 255.0;
        if c <= 0.039_28 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b)
}

fn contrast(a: Rgba, b: Rgba) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

fn color(theme: Theme, kind: SchematicColor) -> Rgba {
    Palette::shipped(theme).color(ColorKey::Schematic(kind))
}

#[test]
fn the_sheet_follows_the_theme() {
    assert!(luminance(color(Theme::Dark, SchematicColor::Paper)) < 0.05);
    assert!(luminance(color(Theme::Light, SchematicColor::Paper)) > 0.9);
}

#[test]
fn every_ink_kind_stands_out_from_the_paper_in_both_themes() {
    for theme in [Theme::Dark, Theme::Light] {
        let paper = color(theme, SchematicColor::Paper);
        for &kind in SchematicColor::ALL {
            if matches!(kind, SchematicColor::Paper | SchematicColor::BodyFill) {
                continue;
            }
            let c = contrast(color(theme, kind), paper);
            assert!(c >= 3.0, "{kind:?} on paper in {theme:?}: contrast {c:.2}");
        }
    }
}

#[test]
fn a_user_override_replaces_one_kind_only() {
    let mut palette = Palette::shipped(Theme::Dark);
    let pink = Rgba::rgb(255, 0, 255);
    palette.set(ColorKey::Schematic(SchematicColor::Wire), pink);
    assert_eq!(
        palette.color(ColorKey::Schematic(SchematicColor::Wire)),
        pink
    );
    assert_eq!(
        palette.color(ColorKey::Schematic(SchematicColor::Junction)),
        color(Theme::Dark, SchematicColor::Junction)
    );
}
