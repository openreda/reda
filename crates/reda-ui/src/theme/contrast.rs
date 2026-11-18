//! The contrast rules: which colour token must stand off which ground, and by how much
//! (docs/spec/v0.1/design-language.md#contrast). One test walks the table for light and
//! dark, and a second fails when a colour token appears in no rule.

use std::collections::{BTreeMap, BTreeSet};

use egui::{Color32, Theme};

use super::{Settings, Tokens, over};

/// The white Glyph on the close button: the one foreground that is not a token.
const WHITE: &str = "white";

/// One rule: `fg` reaches `ratio` on the ground made by laying `layers` over each other,
/// the first on top and the last an opaque surface.
struct Rule {
    fg: &'static str,
    layers: Vec<&'static str>,
    ratio: f64,
}

const SURFACES: [&str; 3] = ["surface.base", "surface.card", "surface.floating"];
const TEXTS: [&str; 2] = ["text.primary", "text.muted"];
/// Each tone's text, subtle, solid and on_solid.
const TONES: [[&str; 4]; 4] = [
    [
        "accent.text",
        "accent.subtle",
        "accent.solid",
        "accent.on_solid",
    ],
    [
        "error.text",
        "error.subtle",
        "error.solid",
        "error.on_solid",
    ],
    [
        "warning.text",
        "warning.subtle",
        "warning.solid",
        "warning.on_solid",
    ],
    [
        "success.text",
        "success.subtle",
        "success.solid",
        "success.on_solid",
    ],
];
const ICONS: [&str; 6] = [
    "icon.context",
    "icon.schematic",
    "icon.copper",
    "icon.general",
    "icon.picked",
    "icon.edge",
];

/// Text and anything read like text.
const TEXT: f64 = 4.5;
/// Lines, rings and icons that must be found.
const GRAPHIC: f64 = 3.0;
/// A field's outline: quiet, but clearly there.
const OUTLINE: f64 = 1.5;
/// A dialog over the dimmed window behind it.
const LIFT: f64 = 1.25;
/// A hairline: the quietest thing that still shows.
const FAINT: f64 = 1.15;

fn rules() -> Vec<Rule> {
    let mut rules = Vec::new();
    let mut rule = |fg: &'static str, layers: &[&'static str], ratio: f64| {
        rules.push(Rule {
            fg,
            layers: layers.to_vec(),
            ratio,
        });
    };
    for surface in SURFACES {
        // Text on a surface, plain and under the hover fill.
        for ground in [vec![surface], vec!["fill.hover", surface]] {
            for text in TEXTS {
                rule(text, &ground, TEXT);
            }
            for [text, ..] in TONES {
                rule(text, &ground, TEXT);
            }
        }
        // A filled control's label, hovered and pressed.
        for state in ["fill.hover", "fill.press"] {
            rule("text.primary", &[state, "fill.control", surface], TEXT);
        }
    }
    for text in TEXTS {
        rule(text, &["surface.well"], TEXT);
    }
    for [tone_text, subtle, solid, on_solid] in TONES {
        rule(on_solid, &[solid], TEXT);
        rule(on_solid, &["fill.hover", solid], TEXT);
        rule(on_solid, &["fill.press", solid], TEXT);
        for surface in SURFACES {
            for ground in [vec![subtle, surface], vec!["fill.hover", subtle, surface]] {
                for text in TEXTS {
                    rule(text, &ground, TEXT);
                }
            }
            rule(tone_text, &[subtle, surface], TEXT);
        }
    }
    for surface in SURFACES {
        for text in TEXTS {
            rule(text, &["fill.selected_unfocused", surface], TEXT);
        }
        // The accent line and the focus ring.
        rule("accent.text", &[surface], GRAPHIC);
        rule("stroke.focus_ring", &[surface], GRAPHIC);
    }
    rule(WHITE, &["close_hover"], TEXT);
    // Each icon role colour at rest, hovered and toggled on. The momentary pressed state
    // is exempt.
    for icon in ICONS {
        for surface in SURFACES {
            rule(icon, &[surface], GRAPHIC);
            rule(icon, &["fill.hover", surface], GRAPHIC);
            rule(icon, &["accent.subtle", surface], GRAPHIC);
        }
    }
    // What the rows above leave out, so that every colour token has a rule.
    for surface in SURFACES {
        rule("stroke.hairline", &[surface], FAINT);
        rule("stroke.strong", &[surface], OUTLINE);
        // The touch-selection rectangle is a line, like the accent line.
        rule("touch", &[surface], GRAPHIC);
    }
    rule("stroke.strong", &["surface.well"], OUTLINE);
    for behind in ["surface.base", "surface.card"] {
        rule("surface.floating", &["surface.scrim", behind], LIFT);
    }
    rules
}

/// The WCAG 2 contrast ratio between two opaque colours, from 1 (none) to 21.
fn contrast_ratio(a: Color32, b: Color32) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    let (hi, lo) = if la > lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

/// WCAG relative luminance of an sRGB colour.
fn luminance(c: Color32) -> f64 {
    let channel = |v: u8| {
        let v = f64::from(v) / 255.0;
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(c.r()) + 0.7152 * channel(c.g()) + 0.0722 * channel(c.b())
}

#[test]
fn contrast_ratio_matches_the_wcag_worked_values() {
    assert!((contrast_ratio(Color32::BLACK, Color32::WHITE) - 21.0).abs() < 1e-6);
    assert!((contrast_ratio(Color32::WHITE, Color32::WHITE) - 1.0).abs() < 1e-6);
    // #777777 on white is the classic "just fails AA" grey: 4.48:1.
    let grey = Color32::from_rgb(0x77, 0x77, 0x77);
    assert!((contrast_ratio(grey, Color32::WHITE) - 4.48).abs() < 0.01);
    // The ratio does not depend on which colour is the text.
    assert_eq!(
        contrast_ratio(grey, Color32::WHITE),
        contrast_ratio(Color32::WHITE, grey)
    );
}

#[test]
fn every_contrast_rule_holds_in_light_and_dark() {
    let rules = rules();
    // 129 rules of text, lines and the close button, 54 for the icons, and 12 for the
    // tokens the others leave out.
    assert_eq!(rules.len(), 129 + 54 + 12);
    let mut failures = Vec::new();
    for theme in [Theme::Light, Theme::Dark] {
        let tokens = Tokens::new(theme, &Settings::default());
        let mut colours: BTreeMap<&str, Color32> = tokens.color.named().into_iter().collect();
        colours.insert(WHITE, Color32::WHITE);
        for rule in &rules {
            let mut layers = rule.layers.iter().rev().map(|name| colours[name]);
            let surface = layers.next().expect("a rule names its ground");
            assert!(
                surface.is_opaque(),
                "{:?} does not end on a surface",
                rule.layers
            );
            let ground = layers.fold(surface, |under, top| over(top, under));
            let ratio = contrast_ratio(colours[rule.fg], ground);
            if ratio < rule.ratio {
                failures.push(format!(
                    "{theme:?}: {} on {}: {ratio:.2}:1, needs {}:1",
                    rule.fg,
                    rule.layers.join(" over "),
                    rule.ratio
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn every_colour_token_appears_in_a_contrast_rule() {
    let rules = rules();
    let used: BTreeSet<&str> = rules
        .iter()
        .flat_map(|r| r.layers.iter().copied().chain([r.fg]))
        .collect();
    let tokens = Tokens::new(Theme::Dark, &Settings::default());
    let named = tokens.color.named();
    let missing: Vec<&str> = named
        .iter()
        .map(|(name, _)| *name)
        .filter(|name| !used.contains(name))
        .collect();
    assert!(missing.is_empty(), "in no contrast rule: {missing:?}");
    // And no rule names something that is not a token.
    let names: BTreeSet<&str> = named.iter().map(|(name, _)| *name).collect();
    let unknown: Vec<&&str> = used
        .iter()
        .filter(|name| **name != WHITE && !names.contains(**name))
        .collect();
    assert!(unknown.is_empty(), "not a colour token: {unknown:?}");
}
