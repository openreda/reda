//! The theme: `Tokens` built from light/dark, Density, text size and families, read through
//! `ui.tokens()`; egui's `Style` generated from them; and following the OS by default.
//! The contrast rules and the spec's value tables are tested beside the tokens, in
//! `src/theme/`.

use egui::{Color32, FontFamily, TextStyle, Theme, ThemePreference};
use reda_ui::engine::preferences;
use reda_ui::theme::{self, Density, Families, Settings, Tokens, TokensExt as _};

fn tokens(theme: Theme, density: Density, text_size: u8) -> Tokens {
    let settings = Settings {
        density,
        text_size,
        ..Settings::default()
    };
    Tokens::new(theme, &settings)
}

#[test]
fn colours_and_shadows_follow_light_and_dark_and_nothing_else() {
    let dark = tokens(Theme::Dark, Density::Comfortable, 13);
    let light = tokens(Theme::Light, Density::Comfortable, 13);
    assert_eq!(dark.color.surface.base, Color32::from_rgb(0x1f, 0x1e, 0x1d));
    assert_eq!(
        light.color.surface.base,
        Color32::from_rgb(0xf0, 0xee, 0xe6)
    );
    assert_ne!(dark.elevation, light.elevation);
    // Space, type, radii and motion are the same in both.
    assert_eq!(dark.space, light.space);
    assert_eq!(dark.ty, light.ty);
    assert_eq!(dark.radius, light.radius);
    assert_eq!(dark.motion, light.motion);
    // Density and text size move no colour.
    let other = tokens(Theme::Dark, Density::Compact, 16);
    assert_eq!(other.color, dark.color);
    assert_eq!(other.elevation, dark.elevation);
}

#[test]
fn density_changes_the_thirteen_paddings_and_through_them_the_heights() {
    let a = tokens(Theme::Dark, Density::Comfortable, 13);
    let b = tokens(Theme::Dark, Density::Compact, 13);
    let values = |t: &Tokens| {
        let s = &t.space;
        [
            s.control_x,
            s.control_y,
            s.row_x,
            s.row_y,
            s.tab_x,
            s.tab_y,
            s.gap,
            s.stack,
            s.section,
            s.panel,
            s.dialog,
            s.menu,
            s.bar_y,
        ]
    };
    assert_eq!(
        values(&a),
        [
            12.0, 5.0, 8.0, 4.0, 12.0, 7.0, 8.0, 8.0, 16.0, 12.0, 20.0, 6.0, 5.0
        ]
    );
    assert_eq!(
        values(&b),
        [
            10.0, 3.0, 6.0, 2.0, 10.0, 5.0, 6.0, 4.0, 12.0, 8.0, 16.0, 4.0, 5.0
        ]
    );
    // Heights are line height plus padding, not stored.
    let heights = |t: &Tokens| {
        [
            t.control_height(),
            t.row_height(),
            t.tab_height(),
            t.title_bar_height(),
        ]
    };
    assert_eq!(heights(&a), [30.0, 28.0, 34.0, 40.0]);
    assert_eq!(heights(&b), [26.0, 24.0, 30.0, 36.0]);
    // The status bar and a floating window's bar ignore Density.
    for t in [&a, &b] {
        assert_eq!(t.status_bar_height(), 24.0);
        assert_eq!(t.floating_bar_height(), 32.0);
    }
    // Nothing else moves.
    assert_eq!(a.ty, b.ty);
    assert_eq!(a.radius, b.radius);
    assert_eq!(a.metric, b.metric);
    assert_eq!(a.color, b.color);
}

#[test]
fn radii_strokes_icons_and_the_disabled_opacity_are_fixed() {
    let t = tokens(Theme::Light, Density::Compact, 16);
    let r = t.radius;
    assert_eq!(
        [r.mark, r.row, r.control, r.card, r.popup, r.window],
        [4.0, 6.0, 8.0, 12.0, 12.0, 16.0]
    );
    // The row radius is the popup radius minus the popup's padding, so corners nest.
    let comfortable = tokens(Theme::Light, Density::Comfortable, 13);
    assert_eq!(r.row, r.popup - comfortable.space.menu);
    assert_eq!(
        [t.stroke.hairline, t.stroke.accent_line, t.stroke.focus_ring],
        [1.0, 2.0, 2.0]
    );
    assert_eq!([t.icon.small, t.icon.bar], [16.0, 20.0]);
    assert_eq!(t.opacity.disabled, 0.45);
    assert_eq!([t.motion.fast, t.motion.base], [0.09, 0.14]);
}

#[test]
fn the_type_scale_moves_with_the_text_size_and_line_height_is_a_token() {
    let size_line = |t: &theme::Text| (t.size, t.line);
    let t = tokens(Theme::Dark, Density::Comfortable, 13).ty;
    assert_eq!(size_line(&t.small), (11.0, 16.0));
    assert_eq!(size_line(&t.body), (13.0, 20.0));
    assert_eq!(size_line(&t.body_strong), (13.0, 20.0));
    assert_eq!(size_line(&t.title), (13.0, 20.0));
    assert_eq!(size_line(&t.heading), (16.0, 24.0));
    // Monospace sits on the body's line.
    assert_eq!(size_line(&t.mono), (12.0, 20.0));
    assert_eq!(size_line(&t.numeric), (13.0, 20.0));
    assert_eq!(size_line(&t.numeric_small), (11.0, 16.0));
    // No style goes below 10 px.
    let t = tokens(Theme::Dark, Density::Comfortable, 11).ty;
    assert_eq!(size_line(&t.small), (10.0, 16.0));
    assert_eq!(size_line(&t.body), (11.0, 16.0));
    assert_eq!(size_line(&t.heading), (14.0, 22.0));
    let t = tokens(Theme::Dark, Density::Comfortable, 16).ty;
    assert_eq!(size_line(&t.small), (14.0, 22.0));
    assert_eq!(size_line(&t.heading), (19.0, 28.0));
    // The text size is 11 to 16: anything else is clamped into it.
    assert_eq!(
        tokens(Theme::Dark, Density::Comfortable, 4).ty,
        tokens(Theme::Dark, Density::Comfortable, 11).ty
    );
    assert_eq!(
        tokens(Theme::Dark, Density::Comfortable, 40).ty,
        tokens(Theme::Dark, Density::Comfortable, 16).ty
    );
    // A text style carries its line height into the text it formats.
    let body = &tokens(Theme::Dark, Density::Comfortable, 13).ty.body;
    let format = body.format(Color32::WHITE);
    assert_eq!(format.line_height, Some(20.0));
    assert_eq!(format.font_id, body.font_id());
}

#[test]
fn each_weight_is_a_family_of_its_own() {
    let name = |n: &str| FontFamily::Name(n.into());
    let settings = Settings {
        families: Families {
            regular: name("ui"),
            medium: name("ui-medium"),
            semibold: name("ui-semibold"),
            numeric: name("ui-numeric"),
            mono: name("mono"),
        },
        ..Settings::default()
    };
    let t = Tokens::new(Theme::Dark, &settings).ty;
    assert_eq!(t.small.family, name("ui"));
    assert_eq!(t.body.family, name("ui"));
    assert_eq!(t.body_strong.family, name("ui-medium"));
    assert_eq!(t.title.family, name("ui-semibold"));
    assert_eq!(t.heading.family, name("ui-semibold"));
    assert_eq!(t.mono.family, name("mono"));
    assert_eq!(t.numeric.family, name("ui-numeric"));
    assert_eq!(t.numeric_small.family, name("ui-numeric"));
    // Until fonts are bundled, every weight is egui's own family.
    let t = Tokens::new(Theme::Dark, &Settings::default()).ty;
    assert_eq!(t.title.family, FontFamily::Proportional);
    assert_eq!(t.mono.family, FontFamily::Monospace);
}

#[test]
fn hover_and_press_fills_go_over_anything_with_one_function() {
    let t = tokens(Theme::Light, Density::Comfortable, 13);
    let c = &t.color;
    // Opaque over anything is itself.
    assert_eq!(theme::over(c.accent.solid, c.surface.base), c.accent.solid);
    // A translucent fill over an opaque ground is opaque, and nearer the ink.
    let hovered = theme::over(c.fill.hover, c.surface.card);
    assert!(hovered.is_opaque());
    assert!(hovered.r() < c.surface.card.r());
    // Black at 50 % over white is mid grey.
    let half = Color32::from_black_alpha(128);
    let grey = theme::over(half, Color32::WHITE);
    assert_eq!(grey, Color32::from_gray(127));
    // Two translucent fills stack into one that is stronger than either.
    let both = theme::over(c.fill.hover, c.fill.control);
    assert!(both.a() > c.fill.control.a() && both.a() < 255);
    assert_eq!(
        theme::over(both, c.surface.base),
        theme::over(c.fill.hover, theme::over(c.fill.control, c.surface.base))
    );
}

#[test]
fn the_egui_style_is_generated_from_the_tokens() {
    for theme in [Theme::Dark, Theme::Light] {
        for density in [Density::Comfortable, Density::Compact] {
            let t = tokens(theme, density, 14);
            let style = theme::style(&t);
            let v = &style.visuals;
            assert_eq!(v.dark_mode, theme == Theme::Dark);
            assert_eq!(v.panel_fill, t.color.surface.base);
            assert_eq!(v.window_fill, t.color.surface.floating);
            assert_eq!(v.text_edit_bg_color(), t.color.surface.well);
            assert_eq!(v.text_color(), t.color.text.primary);
            assert_eq!(v.weak_text_color(), t.color.text.muted);
            assert_eq!(v.hyperlink_color, t.color.accent.text);
            assert_eq!(v.warn_fg_color, t.color.warning.text);
            assert_eq!(v.error_fg_color, t.color.error.text);
            assert_eq!(v.selection.bg_fill, t.color.accent.subtle);
            assert_eq!(v.window_shadow, t.elevation.window);
            assert_eq!(v.popup_shadow, t.elevation.popup);
            assert_eq!(v.window_corner_radius, egui::CornerRadius::same(16));
            assert_eq!(v.menu_corner_radius, egui::CornerRadius::same(12));
            assert_eq!(v.disabled_alpha, t.opacity.disabled);
            let w = &v.widgets;
            for state in [&w.inactive, &w.hovered, &w.active, &w.open] {
                assert_eq!(state.corner_radius, egui::CornerRadius::same(8));
                assert_eq!(state.fg_stroke.color, t.color.text.primary);
            }
            // A button is filled at rest, and hover and press go over that fill.
            assert_eq!(w.inactive.weak_bg_fill, t.color.fill.control);
            assert_eq!(
                w.hovered.weak_bg_fill,
                theme::over(t.color.fill.hover, t.color.fill.control)
            );
            assert_eq!(
                w.active.weak_bg_fill,
                theme::over(t.color.fill.press, t.color.fill.control)
            );
            // Keyboard focus, which egui draws with the pressed visuals, is the focus ring.
            assert_eq!(w.active.bg_stroke.color, t.color.stroke.focus_ring);
            assert_eq!(w.active.bg_stroke.width, t.stroke.focus_ring);

            let s = &style.spacing;
            assert_eq!(s.interact_size.y, t.control_height());
            assert_eq!(
                s.button_padding,
                egui::vec2(t.space.control_x, t.space.control_y)
            );
            assert_eq!(s.item_spacing.x, t.space.gap);
            assert_eq!(s.menu_margin, egui::Margin::same(t.space.menu as i8));
            assert_eq!(s.window_margin, egui::Margin::same(t.space.dialog as i8));
            assert_eq!(TextStyle::Body.resolve(&style), t.ty.body.font_id());
            assert_eq!(TextStyle::Button.resolve(&style), t.ty.body.font_id());
            assert_eq!(TextStyle::Small.resolve(&style), t.ty.small.font_id());
            assert_eq!(TextStyle::Heading.resolve(&style), t.ty.heading.font_id());
            assert_eq!(TextStyle::Monospace.resolve(&style), t.ty.mono.font_id());
            assert_eq!(style.animation_time, t.motion.base);
        }
    }
}

/// Two styles draw alike. `Style` itself never compares equal: it holds a number
/// formatter, which is a closure.
fn assert_same_style(a: &egui::Style, b: &egui::Style) {
    assert_eq!(a.visuals, b.visuals);
    assert_eq!(a.spacing, b.spacing);
    assert_eq!(a.text_styles, b.text_styles);
    assert_eq!(a.interaction, b.interaction);
    assert_eq!(a.animation_time, b.animation_time);
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
    for theme in [Theme::Light, Theme::Dark] {
        ctx.set_theme(theme);
        // The tokens follow the theme being drawn, and the style is theirs.
        let t = ctx.tokens();
        assert_eq!(*t, Tokens::new(theme, &Settings::default()));
        assert_same_style(&ctx.global_style(), &theme::style(&t));
    }
}

#[test]
fn the_tokens_are_the_defaults_before_the_theme_is_installed() {
    let ctx = egui::Context::default();
    ctx.set_theme(Theme::Light);
    assert_eq!(
        *ctx.tokens(),
        Tokens::new(Theme::Light, &Settings::default())
    );
}

#[test]
fn changing_density_or_text_size_rebuilds_the_tokens_and_both_styles() {
    let ctx = egui::Context::default();
    theme::install(&ctx, ThemePreference::Dark);
    assert_eq!(ctx.tokens().density, Density::Comfortable);
    let settings = Settings {
        density: Density::Compact,
        text_size: 15,
        ..Settings::default()
    };
    theme::set(&ctx, &settings);
    for theme in [Theme::Dark, Theme::Light] {
        ctx.set_theme(theme);
        let t = ctx.tokens();
        assert_eq!(*t, Tokens::new(theme, &settings));
        assert_eq!(t.control_height(), 28.0);
        assert_same_style(&ctx.global_style(), &theme::style(&t));
    }
}

#[test]
fn a_ui_reads_the_tokens_of_its_context() {
    let ctx = egui::Context::default();
    theme::install(&ctx, ThemePreference::Light);
    let mut seen = None;
    let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
        seen = Some(ui.tokens());
    });
    output.textures_delta.clear();
    assert_eq!(*seen.unwrap(), *ctx.tokens());
    assert_eq!(ctx.tokens().theme, Theme::Light);
}
