//! REDA's own dark and light themes: slate greys, one blue accent, small corner radii, no
//! shadows, flat buttons, tighter spacing and slightly smaller text. Both meet WCAG AA text
//! contrast (4.5:1), and the default follows the OS. The values are the ones the main
//! window prototype settled (docs/spec/v0.1/app.md#window-shell).

use egui::{Color32, CornerRadius, FontFamily, FontId, Shadow, Stroke, TextStyle, Theme, Visuals};
use reda_engine::preferences::{self, Preferences};

/// The colours of one theme.
struct Colors {
    panel: Color32,
    window: Color32,
    /// Text fields.
    field: Color32,
    /// Tab strips, Dock headers and icon strips, striped rows.
    faint: Color32,
    /// Checkboxes and other widgets at rest.
    widget: Color32,
    hovered: Color32,
    pressed: Color32,
    /// Window outlines and separators.
    stroke: Color32,
    text: Color32,
    /// Text on hovered and pressed buttons and on selections.
    strong: Color32,
    weak_text: Color32,
    /// The one accent: selections, focus, the active Document's tab.
    accent: Color32,
    /// How strongly a selection shows the accent over the panel.
    selection_alpha: f32,
    /// What separates tab groups and Docks.
    gap: Color32,
    link: Color32,
    /// A selection rectangle dragged right to left, which picks what it touches; one
    /// dragged left to right (enclosing) uses the accent.
    touch: Color32,
    warn: Color32,
    error: Color32,
}

const fn rgb(hex: u32) -> Color32 {
    Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

const DARK: Colors = Colors {
    panel: rgb(0x1d2024),
    window: rgb(0x23272c),
    field: rgb(0x141619),
    faint: rgb(0x24282d),
    widget: rgb(0x2b3036),
    hovered: rgb(0x343a41),
    pressed: rgb(0x3d444c),
    stroke: rgb(0x383e45),
    text: rgb(0xc4cad3),
    strong: rgb(0xeef1f4),
    weak_text: rgb(0x949ba5),
    accent: rgb(0x3d8fd6),
    selection_alpha: 0.55,
    gap: rgb(0x141619),
    link: rgb(0x4a98dc),
    touch: rgb(0x4cb87a),
    warn: rgb(0xe8a33d),
    error: rgb(0xff6b6b),
};

const LIGHT: Colors = Colors {
    panel: rgb(0xeceef1),
    window: rgb(0xf7f8fa),
    field: rgb(0xffffff),
    faint: rgb(0xe4e7eb),
    widget: rgb(0xdfe3e8),
    hovered: rgb(0xd2d7de),
    pressed: rgb(0xc5ccd5),
    stroke: rgb(0xc3c9d1),
    text: rgb(0x2a2f36),
    strong: rgb(0x0d1014),
    weak_text: rgb(0x596069),
    accent: rgb(0x1f6fb8),
    selection_alpha: 0.35,
    gap: rgb(0xc3c9d1),
    link: rgb(0x1d68ad),
    touch: rgb(0x23864f),
    warn: rgb(0x8a5a00),
    error: rgb(0xc62828),
};

fn colors(theme: Theme) -> &'static Colors {
    match theme {
        Theme::Dark => &DARK,
        Theme::Light => &LIGHT,
    }
}

/// The theme's one accent colour: selections, focus and the active Document's tab.
pub fn accent(theme: Theme) -> Color32 {
    colors(theme).accent
}

/// A selection rectangle's colour: the accent when it encloses (dragged left to right),
/// green when it touches (right to left), so the two directions read apart at a glance.
pub fn selection_box(theme: Theme, touches: bool) -> Color32 {
    let c = colors(theme);
    if touches { c.touch } else { c.accent }
}

/// A tab group's tab strip: a solid blue bar, the deep accent on the active Document's
/// group (the one the menus follow) and a muted blue on the others. Both themes share it,
/// so the tabs' light titles keep 4.5:1 on either.
pub fn tab_bar(theme: Theme, active_group: bool) -> Color32 {
    match (theme, active_group) {
        (_, true) => rgb(0x1f5fbf),
        (Theme::Dark, false) => rgb(0x2b4f7a),
        (Theme::Light, false) => rgb(0x3f6593),
    }
}

/// A tab's title and icon on its [`tab_bar`]: bright on the active tab, dimmer otherwise.
pub fn tab_text(active_tab: bool) -> Color32 {
    if active_tab {
        Color32::WHITE
    } else {
        rgb(0xd6e2f2)
    }
}

/// The colour of the gaps between tab groups and Docks.
pub fn gap(theme: Theme) -> Color32 {
    colors(theme).gap
}

/// The opaque background of an active or toggled button: the selection's accent over the
/// panel. Blue icons must stay legible on it (the follow-up in issue #32).
pub fn active_background(theme: Theme) -> Color32 {
    let v = visuals(theme);
    over(v.selection.bg_fill, v.panel_fill)
}

/// `top`, which may be translucent (premultiplied, as egui keeps colours), drawn over the
/// opaque `under`.
pub fn over(top: Color32, under: Color32) -> Color32 {
    let a = f32::from(top.a()) / 255.0;
    let mix = |t: u8, u: u8| (f32::from(t) + f32::from(u) * (1.0 - a)).round().min(255.0) as u8;
    Color32::from_rgb(
        mix(top.r(), under.r()),
        mix(top.g(), under.g()),
        mix(top.b(), under.b()),
    )
}

/// How far a disabled widget fades, its icon included: once, to about 47 %.
pub const DISABLED_ALPHA: f32 = 0.47;

/// The egui visuals of REDA's theme.
pub fn visuals(theme: Theme) -> Visuals {
    let c = colors(theme);
    let mut v = match theme {
        Theme::Dark => Visuals::dark(),
        Theme::Light => Visuals::light(),
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
    v.window_stroke = Stroke::new(1.0, c.stroke);
    v.window_corner_radius = CornerRadius::same(4);
    v.window_shadow = Shadow::NONE;
    v.popup_shadow = Shadow::NONE;
    v.menu_corner_radius = small;
    v.panel_fill = c.panel;
    v.selection.bg_fill = c.accent.gamma_multiply(c.selection_alpha);
    v.selection.stroke = Stroke::new(1.0, c.strong);
    v.text_cursor.stroke = Stroke::new(2.0, c.accent);
    v.disabled_alpha = DISABLED_ALPHA;

    let w = &mut v.widgets;
    w.noninteractive.bg_fill = c.panel;
    w.noninteractive.weak_bg_fill = c.panel;
    w.noninteractive.bg_stroke = Stroke::new(1.0, c.stroke);
    w.noninteractive.fg_stroke = Stroke::new(1.0, c.text);
    w.noninteractive.corner_radius = small;
    for (state, bg, fg, stroke) in [
        (&mut w.inactive, c.widget, c.text, Stroke::NONE),
        (
            &mut w.hovered,
            c.hovered,
            c.strong,
            Stroke::new(1.0, c.stroke),
        ),
        (
            &mut w.active,
            c.pressed,
            c.strong,
            Stroke::new(1.0, c.accent),
        ),
        (&mut w.open, c.hovered, c.strong, Stroke::new(1.0, c.stroke)),
    ] {
        state.weak_bg_fill = bg;
        state.bg_fill = bg;
        state.fg_stroke = Stroke::new(1.0, fg);
        state.bg_stroke = stroke;
        state.corner_radius = small;
        state.expansion = 0.0;
    }
    // Buttons are flat until hovered. Checkboxes and slider rails need a fill that stands
    // off the panel.
    w.inactive.weak_bg_fill = Color32::TRANSPARENT;
    w.inactive.bg_fill = c.stroke;
    v
}

/// The full style of REDA's theme: the visuals plus tighter spacing and smaller text.
pub fn style(theme: Theme) -> egui::Style {
    let mut style = egui::Style {
        visuals: visuals(theme),
        ..egui::Style::default()
    };
    style.text_styles = [
        (
            TextStyle::Small,
            FontId::new(10.5, FontFamily::Proportional),
        ),
        (TextStyle::Body, FontId::new(12.5, FontFamily::Proportional)),
        (
            TextStyle::Button,
            FontId::new(12.5, FontFamily::Proportional),
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
    spacing.button_padding = egui::vec2(6.0, 2.0);
    spacing.menu_margin = egui::Margin::same(4);
    spacing.window_margin = egui::Margin::same(6);
    spacing.interact_size.y = 20.0;
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
