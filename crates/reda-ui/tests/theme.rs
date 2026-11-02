//! REDA's own dark and light themes: WCAG AA text contrast, and following the OS by default.

use egui::{Color32, Theme, ThemePreference, Visuals};
use reda_ui::engine::preferences;
use reda_ui::theme::{self, contrast_ratio};

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

/// Every (text, background) pair the egui widgets draw text with.
fn text_pairs(v: &Visuals) -> Vec<(&'static str, Color32, Color32)> {
    let w = &v.widgets;
    vec![
        ("text on panel", v.text_color(), v.panel_fill),
        ("text on window", v.text_color(), v.window_fill),
        ("text on faint rows", v.text_color(), v.faint_bg_color),
        (
            "text in text fields",
            v.text_color(),
            v.text_edit_bg_color(),
        ),
        ("weak text on panel", v.weak_text_color(), v.panel_fill),
        ("weak text on window", v.weak_text_color(), v.window_fill),
        ("link on panel", v.hyperlink_color, v.panel_fill),
        ("label", w.noninteractive.fg_stroke.color, v.panel_fill),
        (
            "button",
            w.inactive.fg_stroke.color,
            w.inactive.weak_bg_fill,
        ),
        (
            "hovered button",
            w.hovered.fg_stroke.color,
            w.hovered.weak_bg_fill,
        ),
        (
            "pressed button",
            w.active.fg_stroke.color,
            w.active.weak_bg_fill,
        ),
        (
            "open menu button",
            w.open.fg_stroke.color,
            w.open.weak_bg_fill,
        ),
        (
            "selected text",
            v.selection.stroke.color,
            v.selection.bg_fill,
        ),
        ("warning", v.warn_fg_color, v.panel_fill),
        ("error", v.error_fg_color, v.panel_fill),
    ]
}

#[test]
fn both_themes_meet_wcag_aa_text_contrast() {
    let mut failures = Vec::new();
    for theme in [Theme::Dark, Theme::Light] {
        let visuals = theme::visuals(theme);
        assert_eq!(visuals.dark_mode, theme == Theme::Dark);
        for (what, fg, bg) in text_pairs(&visuals) {
            let ratio = contrast_ratio(fg, bg);
            if ratio < 4.5 {
                failures.push(format!("{theme:?} {what}: {ratio:.2}:1"));
            }
        }
    }
    assert!(failures.is_empty(), "below 4.5:1:\n{}", failures.join("\n"));
}

#[test]
fn the_themes_are_not_stock_egui() {
    assert_ne!(theme::visuals(Theme::Dark), Visuals::dark());
    assert_ne!(theme::visuals(Theme::Light), Visuals::light());
    // No shadows.
    for theme in [Theme::Dark, Theme::Light] {
        let v = theme::visuals(theme);
        assert_eq!(v.window_shadow, egui::Shadow::NONE);
        assert_eq!(v.popup_shadow, egui::Shadow::NONE);
    }
}

#[test]
fn the_theme_follows_the_os_by_default() {
    let prefs = preferences::Preferences::default();
    assert_eq!(theme::preference(&prefs), ThemePreference::System);

    let mut prefs = prefs;
    prefs.general.theme = preferences::Theme::Light;
    assert_eq!(theme::preference(&prefs), ThemePreference::Light);
    prefs.general.theme = preferences::Theme::Dark;
    assert_eq!(theme::preference(&prefs), ThemePreference::Dark);
}

#[test]
fn installing_the_theme_sets_both_variants_and_the_preference() {
    let ctx = egui::Context::default();
    theme::install(&ctx, ThemePreference::System);
    assert_eq!(ctx.options(|o| o.theme_preference), ThemePreference::System);
    ctx.set_theme(Theme::Light);
    assert_eq!(ctx.global_style().visuals, theme::visuals(Theme::Light));
    ctx.set_theme(Theme::Dark);
    assert_eq!(ctx.global_style().visuals, theme::visuals(Theme::Dark));
}
