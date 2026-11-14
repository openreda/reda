//! REDA's own dark and light themes: slate greys, one blue accent, small corner radii, no
//! shadows, flat buttons, tighter spacing and slightly smaller text. Both meet WCAG AA text
//! contrast (4.5:1), and the default follows the OS. The values are the ones the main
//! window prototype settled (docs/spec/v0.1/app.md#window-shell).

use egui::{Color32, CornerRadius, FontFamily, FontId, Shadow, Stroke, TextStyle, Theme, Visuals};
use reda_engine::preferences::{self, Preferences};

/// The colours of one theme.
struct Colors {
    /// PROTOTYPE: group bodies (the cards in Paper and Ink); `panel` is the window base.
    surface: Color32,
    /// PROTOTYPE: accent as text or icon on the surfaces, and what goes on an accent fill.
    accent_text: Color32,
    on_accent: Color32,
    /// PROTOTYPE: a button at rest, when the direction fills buttons.
    button: Color32,
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
    surface: rgb(0x1d2024),
    accent_text: rgb(0x4a98dc),
    on_accent: Color32::WHITE,
    button: Color32::TRANSPARENT,
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
    surface: rgb(0xeceef1),
    accent_text: rgb(0x1d68ad),
    on_accent: Color32::WHITE,
    button: Color32::TRANSPARENT,
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


// ---------------------------------------------------------------------------------------
// PROTOTYPE (issue #272): three visual directions for the chrome, switchable at run time.
// Throwaway: everything between these rules, and every `theme::look()` read elsewhere.
// ---------------------------------------------------------------------------------------

use std::sync::atomic::{AtomicU8, Ordering};

/// A visual direction. `Current` is main's theme, kept for comparison.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Direction {
    Current,
    Graphite,
    Paper,
    Ink,
}

pub const DIRECTIONS: [Direction; 4] = [
    Direction::Graphite,
    Direction::Paper,
    Direction::Ink,
    Direction::Current,
];

/// How a tab group draws its tabs.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tabs {
    /// Main's solid blue bar.
    Bar,
    /// A neutral strip; the active tab takes the body's fill.
    Raised,
    /// Tabs sit outside the card; the active one is a card joined to its panel.
    Card,
    /// A capsule track inside the card with a raised capsule for the active tab.
    Capsule,
}

/// What a direction fixes besides colour.
pub struct Look {
    pub name: &'static str,
    pub tabs: Tabs,
    /// Control radius, menu/popup radius, window radius, card (tab group) radius.
    pub control: u8,
    pub menu: u8,
    pub window: u8,
    pub card: u8,
    /// Floating bars (the Active Bar): radius.
    pub bar: u8,
    /// Control height in Comfortable and Compact.
    pub height: [f32; 2],
    /// Gap between tab groups; hairline when 1.
    pub gap: f32,
    pub text: f32,
    pub small: f32,
    pub heading: f32,
    pub mono: f32,
    /// Whether the top row and status bar lose their separator lines.
    pub borderless_bars: bool,
    /// Whether buttons at rest carry a 1px border.
    pub button_border: bool,
}

const CURRENT: Look = Look {
    name: "Current (main)",
    tabs: Tabs::Bar,
    control: 3,
    menu: 3,
    window: 4,
    card: 0,
    bar: 5,
    height: [20.0, 20.0],
    gap: 3.0,
    text: 12.5,
    small: 10.5,
    heading: 16.0,
    mono: 11.5,
    borderless_bars: false,
    button_border: false,
};

const GRAPHITE: Look = Look {
    name: "A · Graphite",
    tabs: Tabs::Raised,
    control: 6,
    menu: 8,
    window: 12,
    card: 0,
    bar: 8,
    height: [28.0, 24.0],
    gap: 1.0,
    text: 13.0,
    small: 11.0,
    heading: 15.0,
    mono: 12.0,
    borderless_bars: false,
    button_border: true,
};

const PAPER: Look = Look {
    name: "B · Paper",
    tabs: Tabs::Card,
    control: 8,
    menu: 12,
    window: 16,
    card: 12,
    bar: 12,
    height: [30.0, 26.0],
    gap: 6.0,
    text: 13.0,
    small: 11.0,
    heading: 16.0,
    mono: 12.0,
    borderless_bars: true,
    button_border: false,
};

const INK: Look = Look {
    name: "C · Ink",
    tabs: Tabs::Capsule,
    control: 255,
    menu: 12,
    window: 18,
    card: 14,
    bar: 255,
    height: [32.0, 28.0],
    gap: 6.0,
    text: 13.0,
    small: 11.0,
    heading: 16.0,
    mono: 12.0,
    borderless_bars: true,
    button_border: true,
};

static DIRECTION: AtomicU8 = AtomicU8::new(u8::MAX);
static COMPACT: AtomicU8 = AtomicU8::new(u8::MAX);

/// The direction in force: `REDA_PROTO_DIRECTION` (graphite, paper, ink, current) at
/// first, Graphite without it, then whatever the switcher set.
pub fn direction() -> Direction {
    let mut v = DIRECTION.load(Ordering::Relaxed);
    if v == u8::MAX {
        v = match std::env::var("REDA_PROTO_DIRECTION").as_deref() {
            Ok("paper") => 1,
            Ok("ink") => 2,
            Ok("current") => 3,
            _ => 0,
        };
        DIRECTION.store(v, Ordering::Relaxed);
    }
    DIRECTIONS[v as usize]
}

/// Whether the Density is Compact: `REDA_PROTO_DENSITY=compact` at first.
pub fn compact() -> bool {
    let mut v = COMPACT.load(Ordering::Relaxed);
    if v == u8::MAX {
        v = u8::from(std::env::var("REDA_PROTO_DENSITY").as_deref() == Ok("compact"));
        COMPACT.store(v, Ordering::Relaxed);
    }
    v == 1
}

/// Switches direction and Density and reinstalls styles and fonts.
pub fn set_look(ctx: &egui::Context, direction: Direction, compact: bool) {
    let i = DIRECTIONS.iter().position(|d| *d == direction).unwrap();
    DIRECTION.store(i as u8, Ordering::Relaxed);
    COMPACT.store(u8::from(compact), Ordering::Relaxed);
    ctx.set_style_of(Theme::Dark, style(Theme::Dark));
    ctx.set_style_of(Theme::Light, style(Theme::Light));
    install_fonts(ctx);
}

pub fn look() -> &'static Look {
    match direction() {
        Direction::Current => &CURRENT,
        Direction::Graphite => &GRAPHITE,
        Direction::Paper => &PAPER,
        Direction::Ink => &INK,
    }
}

/// The control height for the Density in force.
pub fn control_height() -> f32 {
    look().height[usize::from(compact())]
}

/// The height of a tab strip.
pub fn tab_strip() -> f32 {
    let c = if compact() { -4.0 } else { 0.0 };
    match look().tabs {
        Tabs::Bar => 24.0,
        Tabs::Raised => 32.0 + c,
        Tabs::Card => 34.0 + c,
        Tabs::Capsule => 40.0 + c,
    }
}

/// The fill of a tab group's body.
pub fn surface(theme: Theme) -> Color32 {
    colors(theme).surface
}

/// The accent where it is text or a line, and what is drawn on an accent fill.
pub fn accent_text(theme: Theme) -> Color32 {
    colors(theme).accent_text
}

pub fn on_accent(theme: Theme) -> Color32 {
    colors(theme).on_accent
}

pub fn hairline(theme: Theme) -> Color32 {
    colors(theme).stroke
}

/// The named font families the bundled typeface's weights are installed as.
pub fn medium() -> FontFamily {
    FontFamily::Name("ui-medium".into())
}

pub fn semibold() -> FontFamily {
    FontFamily::Name("ui-semibold".into())
}

/// Installs the direction's typeface: Inter for Graphite and Paper, Geist for Ink, egui's
/// default for Current. Weights are separate named families (a text style carries none).
pub fn install_fonts(ctx: &egui::Context) {
    use egui::{FontData, FontDefinitions};
    let mut fonts = FontDefinitions::default();
    let faces: Option<[(&str, &'static [u8]); 3]> = match direction() {
        Direction::Current => None,
        Direction::Ink => Some([
            (
                "ui-regular",
                include_bytes!("../assets/prototype-fonts/Geist-Regular.ttf"),
            ),
            (
                "ui-medium",
                include_bytes!("../assets/prototype-fonts/Geist-Medium.ttf"),
            ),
            (
                "ui-semibold",
                include_bytes!("../assets/prototype-fonts/Geist-SemiBold.ttf"),
            ),
        ]),
        _ => Some([
            (
                "ui-regular",
                include_bytes!("../assets/prototype-fonts/Inter-Regular.ttf"),
            ),
            (
                "ui-medium",
                include_bytes!("../assets/prototype-fonts/Inter-Medium.ttf"),
            ),
            (
                "ui-semibold",
                include_bytes!("../assets/prototype-fonts/Inter-SemiBold.ttf"),
            ),
        ]),
    };
    let fallback = fonts.families[&FontFamily::Proportional].clone();
    for name in ["ui-medium", "ui-semibold"] {
        fonts
            .families
            .insert(FontFamily::Name(name.into()), fallback.clone());
    }
    if let Some(faces) = faces {
        for (name, bytes) in faces {
            fonts
                .font_data
                .insert(name.into(), FontData::from_static(bytes).into());
        }
        fonts
            .families
            .get_mut(&FontFamily::Proportional)
            .unwrap()
            .insert(0, "ui-regular".into());
        for name in ["ui-medium", "ui-semibold"] {
            fonts
                .families
                .get_mut(&FontFamily::Name(name.into()))
                .unwrap()
                .insert(0, name.into());
        }
    }
    ctx.set_fonts(fonts);
}

const fn rgba(hex: u32, alpha: u8) -> Color32 {
    // Premultiplied by hand: const fns cannot call `from_rgba_unmultiplied`.
    let a = alpha as u32;
    Color32::from_rgba_premultiplied(
        (((hex >> 16) & 0xff) * a / 255) as u8,
        (((hex >> 8) & 0xff) * a / 255) as u8,
        ((hex & 0xff) * a / 255) as u8,
        alpha,
    )
}

// A · Graphite: dead neutral, flat, hairlines (Codex / T3 Code).
const GRAPHITE_LIGHT: Colors = Colors {
    surface: rgb(0xffffff),
    accent_text: rgb(0x0a64d6),
    on_accent: Color32::WHITE,
    button: rgb(0xffffff),
    panel: rgb(0xf7f7f8),
    window: rgb(0xffffff),
    field: rgb(0xffffff),
    faint: rgb(0xf4f4f5),
    widget: rgb(0xe4e4e7),
    hovered: rgb(0xededf0),
    pressed: rgb(0xe2e2e6),
    stroke: rgb(0xe2e2e6),
    text: rgb(0x18181b),
    strong: rgb(0x09090b),
    weak_text: rgb(0x65656d),
    accent: rgb(0x0a64d6),
    selection_alpha: 0.14,
    gap: rgb(0xe2e2e6),
    link: rgb(0x0a64d6),
    touch: rgb(0x1a7f45),
    warn: rgb(0x8a5a00),
    error: rgb(0xc62828),
};

const GRAPHITE_DARK: Colors = Colors {
    surface: rgb(0x212121),
    accent_text: rgb(0x4da3ff),
    on_accent: Color32::WHITE,
    button: rgb(0x282828),
    panel: rgb(0x181818),
    window: rgb(0x262626),
    field: rgb(0x1a1a1a),
    faint: rgb(0x262626),
    widget: rgb(0x3a3a3a),
    hovered: rgb(0x303030),
    pressed: rgb(0x3a3a3a),
    stroke: rgb(0x333333),
    text: rgb(0xededed),
    strong: rgb(0xffffff),
    weak_text: rgb(0xa1a1a6),
    accent: rgb(0x1f6fe0),
    selection_alpha: 0.30,
    gap: rgb(0x0e0e0e),
    link: rgb(0x4da3ff),
    touch: rgb(0x4cb87a),
    warn: rgb(0xe8a33d),
    error: rgb(0xff6b6b),
};

// B · Paper: warm, soft, low contrast (Claude).
const PAPER_LIGHT: Colors = Colors {
    surface: rgb(0xfaf9f5),
    accent_text: rgb(0xa8452b),
    on_accent: Color32::WHITE,
    button: rgb(0xece9df),
    panel: rgb(0xf0eee6),
    window: rgb(0xfdfcfa),
    field: rgb(0xffffff),
    faint: rgb(0xf4f2ec),
    widget: rgb(0xdedbd0),
    hovered: rgb(0xe8e5db),
    pressed: rgb(0xdedace),
    stroke: rgb(0xdedbd0),
    text: rgb(0x1f1e1c),
    strong: rgb(0x141413),
    weak_text: rgb(0x65635b),
    accent: rgb(0xa8452b),
    selection_alpha: 0.16,
    gap: rgb(0xf0eee6),
    link: rgb(0xa8452b),
    touch: rgb(0x2f7d4f),
    warn: rgb(0x855a00),
    error: rgb(0xb3261e),
};

const PAPER_DARK: Colors = Colors {
    surface: rgb(0x262624),
    accent_text: rgb(0xe08a6d),
    on_accent: rgb(0x141413),
    button: rgb(0x32312e),
    panel: rgb(0x1f1e1d),
    window: rgb(0x2c2b29),
    field: rgb(0x1a1918),
    faint: rgb(0x2b2a28),
    widget: rgb(0x45433f),
    hovered: rgb(0x383734),
    pressed: rgb(0x42413d),
    stroke: rgb(0x3b3a37),
    text: rgb(0xf3f1ea),
    strong: rgb(0xffffff),
    weak_text: rgb(0xb0aea5),
    accent: rgb(0xd97757),
    selection_alpha: 0.28,
    gap: rgb(0x1f1e1d),
    link: rgb(0xe08a6d),
    touch: rgb(0x6fbf8a),
    warn: rgb(0xe8a33d),
    error: rgb(0xff7b6b),
};

// C · Ink: cool near-black, layered, capsule controls (BridgeMind / T3 dark).
const INK_LIGHT: Colors = Colors {
    surface: rgb(0xffffff),
    accent_text: rgb(0x1a5fdf),
    on_accent: Color32::WHITE,
    button: rgb(0xf1f3f7),
    panel: rgb(0xeef0f5),
    window: rgb(0xffffff),
    field: rgb(0xf4f5f9),
    faint: rgb(0xf4f5f9),
    widget: rgb(0xd9dde8),
    hovered: rgb(0xe6e9f0),
    pressed: rgb(0xd9dde8),
    stroke: rgb(0xdde1ea),
    text: rgb(0x10121a),
    strong: rgb(0x05060a),
    weak_text: rgb(0x5a6072),
    accent: rgb(0x1a5fdf),
    selection_alpha: 0.14,
    gap: rgb(0xeef0f5),
    link: rgb(0x1a5fdf),
    touch: rgb(0x1a7f45),
    warn: rgb(0x8a5a00),
    error: rgb(0xc62828),
};

const INK_DARK: Colors = Colors {
    surface: rgb(0x121318),
    accent_text: rgb(0x6ea4ff),
    on_accent: Color32::WHITE,
    button: rgb(0x1c1e25),
    panel: rgb(0x07080c),
    window: rgb(0x16181e),
    field: rgb(0x0b0c11),
    faint: rgb(0x181a20),
    widget: rgb(0x33363f),
    hovered: rgb(0x23252d),
    pressed: rgb(0x2d3039),
    stroke: rgb(0x262830),
    text: rgb(0xf7f8fc),
    strong: rgb(0xffffff),
    weak_text: rgb(0xaaaebc),
    accent: rgb(0x1a5fdf),
    selection_alpha: 0.38,
    gap: rgb(0x07080c),
    link: rgb(0x6ea4ff),
    touch: rgb(0x4cb87a),
    warn: rgb(0xe8a33d),
    error: rgb(0xff6b6b),
};

/// The shadow under popups and menus, and under windows and floating bars.
fn shadows(theme: Theme) -> (Shadow, Shadow) {
    let dark = theme == Theme::Dark;
    let shadow = |y: i8, blur: u8, light: u8, darker: u8| Shadow {
        offset: [0, y],
        blur,
        spread: 0,
        color: rgba(0x000000, if dark { darker } else { light }),
    };
    match direction() {
        Direction::Current => (Shadow::NONE, Shadow::NONE),
        Direction::Graphite => (shadow(6, 20, 28, 110), shadow(10, 32, 34, 130)),
        Direction::Paper => (shadow(6, 24, 26, 100), shadow(12, 40, 30, 120)),
        Direction::Ink => (shadow(12, 36, 44, 170), shadow(20, 60, 56, 200)),
    }
}

/// The soft shadow Paper puts under each card, and the rim Ink draws around it.
pub fn card_shadow(theme: Theme) -> Option<Shadow> {
    (direction() == Direction::Paper).then_some(Shadow {
        offset: [0, 1],
        blur: 4,
        spread: 0,
        color: rgba(0x000000, if theme == Theme::Dark { 70 } else { 22 }),
    })
}

pub fn card_rim(theme: Theme) -> Option<Color32> {
    match (direction(), theme) {
        (Direction::Ink, Theme::Dark) => Some(rgba(0xffffff, 22)),
        (Direction::Ink, Theme::Light) => Some(rgb(0xdde1ea)),
        (Direction::Paper, Theme::Light) => Some(rgb(0xe6e3d9)),
        (Direction::Paper, Theme::Dark) => Some(rgb(0x33322f)),
        _ => None,
    }
}

/// A checkbox with a small radius whatever the direction's control radius: stock egui has
/// one radius for every widget kind, so capsule buttons would make checkboxes circles.
pub fn checkbox(
    ui: &mut egui::Ui,
    checked: &mut bool,
    text: impl Into<egui::WidgetText>,
) -> egui::Response {
    ui.scope(|ui| {
        if look().control > 6 {
            let w = &mut ui.visuals_mut().widgets;
            for s in [&mut w.inactive, &mut w.hovered, &mut w.active, &mut w.open] {
                s.corner_radius = CornerRadius::same(4);
            }
        }
        ui.checkbox(checked, text)
    })
    .inner
}

// ---------------------------------------------------------------------------------------

fn colors(theme: Theme) -> &'static Colors {
    match (direction(), theme) {
        (Direction::Current, Theme::Dark) => &DARK,
        (Direction::Current, Theme::Light) => &LIGHT,
        (Direction::Graphite, Theme::Dark) => &GRAPHITE_DARK,
        (Direction::Graphite, Theme::Light) => &GRAPHITE_LIGHT,
        (Direction::Paper, Theme::Dark) => &PAPER_DARK,
        (Direction::Paper, Theme::Light) => &PAPER_LIGHT,
        (Direction::Ink, Theme::Dark) => &INK_DARK,
        (Direction::Ink, Theme::Light) => &INK_LIGHT,
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

/// What lies behind the active tab on its [`tab_bar`]: white at 15 % (premultiplied).
pub const ACTIVE_TAB_FILL: Color32 = Color32::from_rgba_premultiplied(38, 38, 38, 38);

/// The height of the white underline across the active tab.
pub const ACTIVE_TAB_UNDERLINE: f32 = 2.0;

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
    let look = look();
    let proto = direction() != Direction::Current;
    let small = CornerRadius::same(look.control);
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
    v.window_corner_radius = CornerRadius::same(look.window);
    let (popup, window) = shadows(theme);
    v.window_shadow = window;
    v.popup_shadow = popup;
    v.menu_corner_radius = CornerRadius::same(look.menu);
    v.panel_fill = c.panel;
    v.selection.bg_fill = c.accent.gamma_multiply(c.selection_alpha);
    v.selection.stroke = Stroke::new(1.0, c.strong);
    if proto {
        v.striped = false;
        v.slider_trailing_fill = true;
        v.handle_shape = egui::style::HandleShape::Circle;
    }
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
    if proto {
        // Buttons at rest are filled (and bordered where the direction says so); checkbox
        // and slider wells take the stronger `widget` grey so they stand off any surface.
        w.inactive.weak_bg_fill = c.button;
        w.inactive.bg_fill = c.widget;
        if look.button_border {
            w.inactive.bg_stroke = Stroke::new(1.0, c.stroke);
        }
        w.hovered.bg_stroke = if look.button_border {
            Stroke::new(1.0, c.stroke)
        } else {
            Stroke::NONE
        };
        w.active.bg_stroke = Stroke::new(1.5, c.accent_text);
        v.selection.stroke = Stroke::new(1.0, c.text);
        v.hyperlink_color = c.accent_text;
        v.text_cursor.stroke = Stroke::new(1.5, c.accent_text);
    }
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
    let look = look();
    if direction() != Direction::Current {
        let compact = compact();
        style.text_styles = [
            (
                TextStyle::Small,
                FontId::new(look.small, FontFamily::Proportional),
            ),
            (
                TextStyle::Body,
                FontId::new(look.text, FontFamily::Proportional),
            ),
            (
                TextStyle::Button,
                FontId::new(look.text, FontFamily::Proportional),
            ),
            (TextStyle::Heading, FontId::new(look.heading, semibold())),
            (
                TextStyle::Monospace,
                FontId::new(look.mono, FontFamily::Monospace),
            ),
        ]
        .into();
        let h = control_height();
        let wide = if look.control == 255 { 4.0 } else { 0.0 };
        let spacing = &mut style.spacing;
        spacing.interact_size = egui::vec2(40.0, h);
        // Heights derive from the line height plus padding; the padding is the Density's.
        spacing.button_padding = if compact {
            egui::vec2(8.0 + wide, 3.0)
        } else {
            egui::vec2(10.0 + wide, 5.0)
        };
        spacing.item_spacing = if compact {
            egui::vec2(6.0, 2.0)
        } else {
            egui::vec2(8.0, 4.0)
        };
        spacing.menu_margin = egui::Margin::same(if compact { 4 } else { 6 });
        spacing.window_margin = egui::Margin::same(if compact { 10 } else { 14 });
        spacing.icon_width = 16.0;
        spacing.icon_width_inner = 10.0;
        spacing.icon_spacing = 6.0;
        spacing.indent = 16.0;
        spacing.scroll = egui::style::ScrollStyle::thin();
        spacing.combo_height = 320.0;
        style.animation_time = 0.14;
        return style;
    }
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
    install_fonts(ctx);
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

/// PROTOTYPE: the contrast of each direction's text pairs, for the write-up.
pub fn contrast_report() -> String {
    let mut out = String::new();
    for d in [Direction::Graphite, Direction::Paper, Direction::Ink] {
        let i = DIRECTIONS.iter().position(|x| *x == d).unwrap();
        DIRECTION.store(i as u8, Ordering::Relaxed);
        for theme in [Theme::Light, Theme::Dark] {
            let c = colors(theme);
            let sel = over(c.accent.gamma_multiply(c.selection_alpha), c.surface);
            let pairs = [
                ("text/base", c.text, c.panel),
                ("text/surface", c.text, c.surface),
                ("text/hover", c.text, c.hovered),
                ("text/selection", c.text, sel),
                ("muted/base", c.weak_text, c.panel),
                ("muted/surface", c.weak_text, c.surface),
                ("muted/hover", c.weak_text, c.hovered),
                ("accent text/base", c.accent_text, c.panel),
                ("accent text/surface", c.accent_text, c.surface),
                ("accent text/hover", c.accent_text, c.hovered),
                ("on accent/accent", c.on_accent, c.accent),
                ("warn/surface", c.warn, c.surface),
                ("error/surface", c.error, c.surface),
            ];
            for (name, a, b) in pairs {
                let r = contrast_ratio(a, b);
                let flag = if r < 4.5 { "  FAIL" } else { "" };
                out += &format!("{d:?} {theme:?} {name}: {r:.2}{flag}\n");
            }
        }
    }
    out
}
