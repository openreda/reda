//! REDA's own dark and light themes: neutral greys, one blue accent, small corner radii, no
//! shadows, tighter spacing and slightly smaller text. Both meet WCAG AA text contrast
//! (4.5:1), and the default follows the OS.

use egui::{Color32, CornerRadius, FontFamily, FontId, Shadow, Stroke, TextStyle, Theme, Visuals};
use reda_engine::preferences::{self, Preferences};

/// The colours of one theme.
struct Colors {
    panel: Color32,
    window: Color32,
    faint: Color32,
    field: Color32,
    separator: Color32,
    text: Color32,
    weak_text: Color32,
    /// Buttons at rest, hovered, pressed and open: (background, text).
    button: (Color32, Color32),
    button_hovered: (Color32, Color32),
    button_pressed: (Color32, Color32),
    button_open: (Color32, Color32),
    /// The one accent: selections, focus and links.
    accent: Color32,
    selection: Color32,
    link: Color32,
    warn: Color32,
    error: Color32,
}

const fn grey(v: u8) -> Color32 {
    Color32::from_rgb(v, v, v.saturating_add(4))
}

const DARK: Colors = Colors {
    panel: Color32::from_rgb(0x1e, 0x1f, 0x22),
    window: Color32::from_rgb(0x25, 0x27, 0x2b),
    faint: Color32::from_rgb(0x26, 0x28, 0x2c),
    field: Color32::from_rgb(0x15, 0x16, 0x18),
    separator: Color32::from_rgb(0x3a, 0x3d, 0x42),
    text: Color32::from_rgb(0xd4, 0xd6, 0xda),
    weak_text: Color32::from_rgb(0x9a, 0x9e, 0xa6),
    button: (
        Color32::from_rgb(0x2f, 0x31, 0x36),
        Color32::from_rgb(0xd4, 0xd6, 0xda),
    ),
    button_hovered: (
        Color32::from_rgb(0x3a, 0x3d, 0x43),
        Color32::from_rgb(0xf2, 0xf3, 0xf5),
    ),
    button_pressed: (Color32::from_rgb(0x45, 0x49, 0x4f), Color32::WHITE),
    button_open: (
        Color32::from_rgb(0x34, 0x37, 0x3c),
        Color32::from_rgb(0xe0, 0xe2, 0xe6),
    ),
    accent: Color32::from_rgb(0x4c, 0x8d, 0xf6),
    selection: Color32::from_rgb(0x1f, 0x5f, 0xbf),
    link: Color32::from_rgb(0x6a, 0xa9, 0xff),
    warn: Color32::from_rgb(0xe8, 0xa3, 0x3d),
    error: Color32::from_rgb(0xff, 0x6b, 0x6b),
};

const LIGHT: Colors = Colors {
    panel: Color32::from_rgb(0xf3, 0xf4, 0xf6),
    window: Color32::WHITE,
    faint: Color32::from_rgb(0xec, 0xee, 0xf1),
    field: Color32::WHITE,
    separator: grey(0xcc),
    text: Color32::from_rgb(0x1f, 0x23, 0x29),
    weak_text: Color32::from_rgb(0x5c, 0x63, 0x6e),
    button: (
        Color32::from_rgb(0xe3, 0xe6, 0xea),
        Color32::from_rgb(0x1f, 0x23, 0x29),
    ),
    button_hovered: (
        Color32::from_rgb(0xd6, 0xda, 0xe0),
        Color32::from_rgb(0x0b, 0x0d, 0x10),
    ),
    button_pressed: (Color32::from_rgb(0xc8, 0xcd, 0xd4), Color32::BLACK),
    button_open: (
        Color32::from_rgb(0xdd, 0xe1, 0xe6),
        Color32::from_rgb(0x1f, 0x23, 0x29),
    ),
    accent: Color32::from_rgb(0x1f, 0x6f, 0xeb),
    selection: Color32::from_rgb(0x1f, 0x5f, 0xbf),
    link: Color32::from_rgb(0x1a, 0x5f, 0xc8),
    warn: Color32::from_rgb(0x8a, 0x5a, 0x00),
    error: Color32::from_rgb(0xc6, 0x28, 0x28),
};

/// The theme's one accent colour: selections, focus and links.
pub fn accent(theme: Theme) -> Color32 {
    match theme {
        Theme::Dark => DARK.accent,
        Theme::Light => LIGHT.accent,
    }
}

/// The egui visuals of REDA's theme.
pub fn visuals(theme: Theme) -> Visuals {
    let (c, mut v) = match theme {
        Theme::Dark => (&DARK, Visuals::dark()),
        Theme::Light => (&LIGHT, Visuals::light()),
    };
    let small = CornerRadius::same(3);
    v.override_text_color = None;
    v.weak_text_color = Some(c.weak_text);
    v.hyperlink_color = c.link;
    v.faint_bg_color = c.faint;
    v.extreme_bg_color = c.field;
    v.text_edit_bg_color = Some(c.field);
    v.code_bg_color = c.faint;
    v.warn_fg_color = c.warn;
    v.error_fg_color = c.error;
    v.window_fill = c.window;
    v.window_stroke = Stroke::new(1.0, c.separator);
    v.window_corner_radius = CornerRadius::same(4);
    v.window_shadow = Shadow::NONE;
    v.popup_shadow = Shadow::NONE;
    v.menu_corner_radius = CornerRadius::same(4);
    v.panel_fill = c.panel;
    v.selection.bg_fill = c.selection;
    v.selection.stroke = Stroke::new(1.0, Color32::WHITE);
    v.text_cursor.stroke = Stroke::new(2.0, c.accent);

    let w = &mut v.widgets;
    w.noninteractive.bg_fill = c.panel;
    w.noninteractive.weak_bg_fill = c.panel;
    w.noninteractive.bg_stroke = Stroke::new(1.0, c.separator);
    w.noninteractive.fg_stroke = Stroke::new(1.0, c.text);
    w.noninteractive.corner_radius = small;
    for (state, (bg, fg), stroke) in [
        (&mut w.inactive, c.button, Stroke::NONE),
        (
            &mut w.hovered,
            c.button_hovered,
            Stroke::new(1.0, c.separator),
        ),
        (&mut w.active, c.button_pressed, Stroke::new(1.0, c.accent)),
        (&mut w.open, c.button_open, Stroke::new(1.0, c.separator)),
    ] {
        state.weak_bg_fill = bg;
        state.bg_fill = bg;
        state.fg_stroke = Stroke::new(1.0, fg);
        state.bg_stroke = stroke;
        state.corner_radius = small;
        state.expansion = 0.0;
    }
    // Checkboxes and slider rails need a fill that stands off the panel.
    w.inactive.bg_fill = c.separator;
    v
}

/// The full style of REDA's theme: the visuals plus tighter spacing and smaller text.
pub fn style(theme: Theme) -> egui::Style {
    let mut style = egui::Style {
        visuals: visuals(theme),
        ..egui::Style::default()
    };
    style.text_styles = [
        (TextStyle::Small, FontId::new(9.5, FontFamily::Proportional)),
        (TextStyle::Body, FontId::new(12.0, FontFamily::Proportional)),
        (
            TextStyle::Button,
            FontId::new(12.0, FontFamily::Proportional),
        ),
        (
            TextStyle::Heading,
            FontId::new(16.0, FontFamily::Proportional),
        ),
        (
            TextStyle::Monospace,
            FontId::new(11.5, FontFamily::Monospace),
        ),
    ]
    .into();
    let spacing = &mut style.spacing;
    spacing.item_spacing = egui::vec2(6.0, 3.0);
    spacing.button_padding = egui::vec2(5.0, 2.0);
    spacing.menu_margin = egui::Margin::same(4);
    spacing.window_margin = egui::Margin::same(6);
    spacing.interact_size.y = 18.0;
    style
}

/// The egui theme preference that the *General → Theme* Preference asks for.
pub fn preference(prefs: &Preferences) -> egui::ThemePreference {
    match prefs.general.theme {
        preferences::Theme::FollowOs => egui::ThemePreference::System,
        preferences::Theme::Dark => egui::ThemePreference::Dark,
        preferences::Theme::Light => egui::ThemePreference::Light,
    }
}

/// Installs both themes on `ctx` and picks one by `preference`.
pub fn install(ctx: &egui::Context, preference: egui::ThemePreference) {
    ctx.set_style_of(Theme::Dark, style(Theme::Dark));
    ctx.set_style_of(Theme::Light, style(Theme::Light));
    ctx.set_theme(preference);
}

/// The canvas colours that go with an egui theme.
pub fn canvas_theme(theme: Theme) -> reda_engine::render::Theme {
    match theme {
        Theme::Dark => reda_engine::render::Theme::Dark,
        Theme::Light => reda_engine::render::Theme::Light,
    }
}

/// The WCAG 2 contrast ratio between two opaque colours, from 1 (none) to 21.
pub fn contrast_ratio(a: Color32, b: Color32) -> f64 {
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
