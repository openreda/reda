//! PROTOTYPE (issue #276): component recipes for the Paper direction. Throwaway.
//!
//! One question: which recipe does each chrome component lock, in light and dark and both
//! Densities? This module holds a prototype of the `Tokens` value from ADR-0015 (names and
//! values proposed for the spec), REDA widgets that read nothing else (`widgets`), and a
//! gallery of every component in every state (`pages`).
//!
//! Run: `REDA_PROTO_GALLERY=1 cargo run -p reda-app`, or F10 in the running app.
//! F9 next page, Shift+F9 light/dark, Ctrl+F9 Density, F7 / Shift+F7 text size, F6 typeface.

pub mod pages;
pub mod widgets;

use std::sync::atomic::{AtomicU8, Ordering};

use egui::epaint::Shadow;
use egui::text::{LayoutJob, TextFormat};
use egui::{Color32, FontFamily, FontId, Theme};

// ---------------------------------------------------------------------------------------
// Prototype state (statics: the gallery is the only reader).
// ---------------------------------------------------------------------------------------

static GALLERY: AtomicU8 = AtomicU8::new(u8::MAX);
static PAGE: AtomicU8 = AtomicU8::new(0);
static TEXT_SIZE: AtomicU8 = AtomicU8::new(13);
static FACE: AtomicU8 = AtomicU8::new(0);
static COMPACT: AtomicU8 = AtomicU8::new(0);

/// The typefaces the gallery can switch between, to check the recipes against a face that
/// is not Inter: taller, wider, and with no medium or semibold.
pub const FACES: [&str; 3] = [
    "Inter (bundled)",
    "Inter, regular only",
    "DejaVu Sans, regular only",
];

pub fn gallery_on() -> bool {
    let mut v = GALLERY.load(Ordering::Relaxed);
    if v == u8::MAX {
        v = u8::from(std::env::var_os("REDA_PROTO_GALLERY").is_some());
        GALLERY.store(v, Ordering::Relaxed);
    }
    v == 1
}

pub fn set_gallery(on: bool) {
    GALLERY.store(u8::from(on), Ordering::Relaxed);
}

pub fn page() -> usize {
    PAGE.load(Ordering::Relaxed) as usize % pages::PAGES.len()
}

pub fn set_page(i: usize) {
    PAGE.store((i % pages::PAGES.len()) as u8, Ordering::Relaxed);
}

pub fn text_size() -> f32 {
    f32::from(TEXT_SIZE.load(Ordering::Relaxed))
}

pub fn set_text_size(px: u8) {
    TEXT_SIZE.store(px.clamp(11, 16), Ordering::Relaxed);
}

pub fn face() -> usize {
    FACE.load(Ordering::Relaxed) as usize % FACES.len()
}

pub fn compact() -> bool {
    COMPACT.load(Ordering::Relaxed) == 1
}

pub fn set_compact(on: bool) {
    COMPACT.store(u8::from(on), Ordering::Relaxed);
}

/// Sets the typeface and reinstalls the fonts.
pub fn set_face(ctx: &egui::Context, i: usize) {
    FACE.store((i % FACES.len()) as u8, Ordering::Relaxed);
    install_fonts(ctx);
}

// ---------------------------------------------------------------------------------------
// Fonts. Each weight is its own named family (egui has no weight on a text style).
// ---------------------------------------------------------------------------------------

const REGULAR: &str = "reda-ui-regular";
const MEDIUM: &str = "reda-ui-medium";
const SEMIBOLD: &str = "reda-ui-semibold";
/// The face numeric recipes use: the bundled Inter with tabular digits as its default.
const NUMERIC: &str = "reda-ui-numeric";
const MONO: &str = "reda-ui-mono";
/// Other monospace candidates, shown side by side on the Type page.
pub const MONO_CANDIDATES: [(&str, &str); 2] = [
    ("Hack (egui's built-in)", "Hack"),
    ("DejaVu Sans Mono (system)", "proto-dejavu-mono"),
];

fn family(name: &str) -> FontFamily {
    FontFamily::Name(name.into())
}

pub fn install_fonts(ctx: &egui::Context) {
    use egui::{FontData, FontDefinitions};
    let mut fonts = FontDefinitions::default();
    let fallback = fonts.families[&FontFamily::Proportional].clone();
    let mut add = |fonts: &mut FontDefinitions, key: &str, bytes: Vec<u8>| {
        fonts
            .font_data
            .insert(key.into(), FontData::from_owned(bytes).into());
    };
    let inter = [
        include_bytes!("../assets/prototype-fonts/Inter-Regular.ttf").to_vec(),
        include_bytes!("../assets/prototype-fonts/Inter-Medium.ttf").to_vec(),
        include_bytes!("../assets/prototype-fonts/Inter-SemiBold.ttf").to_vec(),
    ];
    let dejavu = std::fs::read("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf").ok();
    let faces: [Vec<u8>; 3] = match (face(), dejavu) {
        (1, _) => [inter[0].clone(), inter[0].clone(), inter[0].clone()],
        (2, Some(d)) => [d.clone(), d.clone(), d],
        _ => inter.clone(),
    };
    for (key, bytes) in [REGULAR, MEDIUM, SEMIBOLD].into_iter().zip(faces) {
        add(&mut fonts, key, bytes);
    }
    // Numeric recipes: the bundled Inter with tabular digits baked in (see `tabular`),
    // whatever the chosen face, unless that face's digits already measure equal.
    add(&mut fonts, NUMERIC, tabular::bake(&inter[0]));
    add(&mut fonts, "proto-inter-proportional", inter[0].clone());
    if let Ok(mono) = std::fs::read("/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf") {
        add(&mut fonts, "proto-dejavu-mono", mono);
    }
    let with = |first: &str| {
        let mut list = vec![first.to_owned()];
        list.extend(fallback.iter().cloned());
        list
    };
    fonts
        .families
        .insert(FontFamily::Proportional, with(REGULAR));
    for key in [REGULAR, MEDIUM, SEMIBOLD, "proto-inter-proportional"] {
        fonts.families.insert(family(key), with(key));
    }
    // The families the direction prototype's code still asks for.
    fonts.families.insert(family("ui-medium"), with(MEDIUM));
    fonts.families.insert(family("ui-semibold"), with(SEMIBOLD));
    // DejaVu Sans beside Inter, for the line-height check on the Type page.
    if let Ok(d) = std::fs::read("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf") {
        add(&mut fonts, "proto-dejavu", d);
        fonts.families.insert(family("proto-dejavu"), with("proto-dejavu"));
    } else {
        fonts.families.insert(family("proto-dejavu"), with(REGULAR));
    }
    // DejaVu's digits are tabular already, so its numeric recipes keep the chosen face.
    let numeric = if face() == 2 { REGULAR } else { NUMERIC };
    fonts.families.insert(family(NUMERIC), with(numeric));
    // The bundled monospace: Hack for now (egui ships it); the pick is still open.
    let mono = fonts.families[&FontFamily::Monospace].clone();
    fonts.families.insert(family(MONO), mono.clone());
    for (_, key) in MONO_CANDIDATES {
        let mut list = vec![key.to_owned()];
        list.extend(mono.iter().cloned());
        if fonts.font_data.contains_key(key) {
            fonts.families.insert(family(key), list);
        } else {
            fonts.families.insert(family(key), mono.clone());
        }
    }
    ctx.set_fonts(fonts);
}

pub mod tabular;

// ---------------------------------------------------------------------------------------
// Tokens.
// ---------------------------------------------------------------------------------------

/// A text style: family (which carries the weight), size and line height together. The
/// line height is the token's, not the face's: glyphs are centred in it.
#[derive(Clone)]
pub struct Text {
    pub family: FontFamily,
    pub size: f32,
    pub line: f32,
}

impl Text {
    pub fn format(&self, color: Color32) -> TextFormat {
        TextFormat {
            font_id: FontId::new(self.size, self.family.clone()),
            line_height: Some(self.line),
            color,
            ..Default::default()
        }
    }

    pub fn job(&self, text: &str, color: Color32) -> LayoutJob {
        LayoutJob::single_section(text.to_owned(), self.format(color))
    }

    /// For stock egui widgets: a `RichText` that carries the token's line height.
    pub fn rich(&self, text: &str) -> egui::RichText {
        egui::RichText::new(text)
            .font(FontId::new(self.size, self.family.clone()))
            .line_height(Some(self.line))
    }
}

/// The line height that goes with a text size: about 1.5 em on a 2px grid.
/// 10→16, 11→16, 12→18, 13→20, 14→22, 15→22, 16→24, 19→28.
pub fn line_height(size: f32) -> f32 {
    2.0 * (size * 0.75 + 0.5).floor()
}

/// One semantic colour (or the accent) in its three strengths.
#[derive(Clone, Copy)]
pub struct Tone {
    /// Text, icons and lines.
    pub text: Color32,
    /// A tint behind a row, a cell or a count (translucent: laid over the surface).
    pub subtle: Color32,
    /// A fill with its own on-colour.
    pub solid: Color32,
    pub on_solid: Color32,
}

pub struct Surface {
    /// The window: behind cards, under the Title bar and status bar.
    pub base: Color32,
    /// Tab groups, Docks, the active tab.
    pub card: Color32,
    /// Menus, popups, dialogs, tooltips, floating bars.
    pub floating: Color32,
    /// Text fields and other wells.
    pub well: Color32,
    /// Behind a modal.
    pub scrim: Color32,
}

pub struct TextColor {
    pub primary: Color32,
    pub muted: Color32,
}

pub struct StrokeColor {
    /// Card rims, separators, table rules.
    pub hairline: Color32,
    /// Field and checkbox outlines.
    pub strong: Color32,
    pub focus_ring: Color32,
}

/// Neutral state fills. All translucent, laid over whatever is underneath.
pub struct Fill {
    /// A filled button or combo box at rest.
    pub control: Color32,
    /// Hover on anything flat (rows, menu items, flat buttons, tabs), and added to a
    /// filled control on hover.
    pub hover: Color32,
    pub press: Color32,
    /// The selected row in a Panel without keyboard focus, or in a background window.
    pub selected_unfocused: Color32,
}

pub struct Colors {
    pub surface: Surface,
    pub text: TextColor,
    pub stroke: StrokeColor,
    pub fill: Fill,
    pub accent: Tone,
    pub error: Tone,
    pub warning: Tone,
    pub success: Tone,
    /// The touch-selection rectangle on the canvas.
    pub touch: Color32,
    /// The close button of the window controls on hover.
    pub close_hover: Color32,
}

pub struct Elevation {
    /// The 1px soft shadow under a card.
    pub card: Shadow,
    /// Menus, popups, tooltips, floating bars.
    pub popup: Shadow,
    /// Dialogs and the Command Palette.
    pub window: Shadow,
}

/// Paddings and gaps: the only tokens that follow Density.
pub struct Space {
    /// Inside buttons, fields, combo boxes: with the body line height this gives the
    /// control height (30 / 26 at 13px text).
    pub control_x: f32,
    pub control_y: f32,
    /// Rows of lists, trees, tables and menus (28 / 24).
    pub row_x: f32,
    pub row_y: f32,
    /// A tab in a tab strip (34 / 30).
    pub tab_x: f32,
    pub tab_y: f32,
    /// Between controls side by side.
    pub gap: f32,
    /// Between stacked rows of a form.
    pub stack: f32,
    /// Between sections.
    pub section: f32,
    /// Inside a Panel's card.
    pub panel: f32,
    /// Inside a dialog.
    pub dialog: f32,
    /// Inside a menu or popup, around its items.
    pub menu: f32,
    /// Around the controls in the Title bar (bar height 40 / 36).
    pub bar_y: f32,
}

pub struct Type {
    pub small: Text,
    pub body: Text,
    /// Emphasis in running text and the active tab. Medium; a regular-only face shows it
    /// as regular, so nothing relies on it alone.
    pub body_strong: Text,
    /// Dialog titles, Panel section headers. Semibold at the body size.
    pub title: Text,
    /// Page headings (Start page, Preferences pages). Semibold, body + 3.
    pub heading: Text,
    pub mono: Text,
    /// Numeric recipes (number fields, coordinates, number columns): tabular digits.
    pub numeric: Text,
    pub numeric_small: Text,
}

pub struct Radius {
    /// Checkboxes, swatches, key chips.
    pub mark: f32,
    /// Rows and menu items: a popup's 12 minus its 6 padding, so corners nest.
    pub row: f32,
    pub control: f32,
    pub card: f32,
    pub popup: f32,
    pub window: f32,
}

pub struct Motion {
    /// Hover and press fills.
    pub fast: f32,
    /// Toggles, checks, tab changes.
    pub base: f32,
    /// Popups, flyouts, Dock collapse.
    pub slow: f32,
}

pub struct Tokens {
    pub dark: bool,
    pub compact: bool,
    pub color: Colors,
    pub elevation: Elevation,
    pub space: Space,
    pub ty: Type,
    pub radius: Radius,
    pub motion: Motion,
    /// Applied once to a whole disabled widget.
    pub opacity_disabled: f32,
    pub hairline: f32,
    pub accent_line: f32,
    pub focus_ring: f32,
    /// Icons on tabs, rows, buttons and menus; and in the Title bar's quick strip.
    pub icon: f32,
    pub icon_bar: f32,
}

const fn rgb(hex: u32) -> Color32 {
    Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

fn alpha(c: Color32, a: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), (a * 255.0).round() as u8)
}

fn shadow(y: i8, blur: u8, a: u8) -> Shadow {
    Shadow {
        offset: [0, y],
        blur,
        spread: 0,
        color: Color32::from_black_alpha(a),
    }
}

impl Tokens {
    pub fn of(ui: &egui::Ui) -> Tokens {
        Tokens::new(ui.ctx().theme(), compact(), text_size())
    }

    pub fn new(theme: Theme, compact: bool, body: f32) -> Tokens {
        let dark = theme == Theme::Dark;
        // The palette: private raw values. Tokens below name jobs.
        let color = if dark {
            let ink = rgb(0xf3f1ea);
            Colors {
                surface: Surface {
                    base: rgb(0x1f1e1d),
                    card: rgb(0x262624),
                    floating: rgb(0x2c2b29),
                    well: rgb(0x1a1918),
                    scrim: Color32::from_black_alpha(140),
                },
                text: TextColor {
                    primary: ink,
                    muted: rgb(0xc2c0b7),
                },
                stroke: StrokeColor {
                    hairline: rgb(0x3b3a37),
                    strong: rgb(0x55534e),
                    focus_ring: rgb(0x8fbaf8),
                },
                fill: Fill {
                    control: alpha(ink, 0.08),
                    hover: alpha(ink, 0.07),
                    press: alpha(ink, 0.13),
                    selected_unfocused: alpha(ink, 0.10),
                },
                accent: Tone {
                    text: rgb(0x8fbaf8),
                    subtle: alpha(rgb(0x4f8fe8), 0.22),
                    solid: rgb(0x2458b0),
                    on_solid: Color32::WHITE,
                },
                error: Tone {
                    text: rgb(0xf5998c),
                    subtle: alpha(rgb(0xe5534a), 0.20),
                    solid: rgb(0xad2f27),
                    on_solid: Color32::WHITE,
                },
                warning: Tone {
                    text: rgb(0xe5b04a),
                    subtle: alpha(rgb(0xe5a530), 0.16),
                    solid: rgb(0xe5b04a),
                    on_solid: rgb(0x231a05),
                },
                success: Tone {
                    text: rgb(0x7cc792),
                    subtle: alpha(rgb(0x4faf6e), 0.18),
                    solid: rgb(0x25683f),
                    on_solid: Color32::WHITE,
                },
                touch: rgb(0x6fbf8a),
                close_hover: rgb(0xc4392f),
            }
        } else {
            let ink = rgb(0x1f1e1c);
            Colors {
                surface: Surface {
                    base: rgb(0xf0eee6),
                    card: rgb(0xfaf9f5),
                    floating: rgb(0xfdfcfa),
                    well: rgb(0xffffff),
                    scrim: Color32::from_black_alpha(90),
                },
                text: TextColor {
                    primary: ink,
                    muted: rgb(0x59574f),
                },
                stroke: StrokeColor {
                    hairline: rgb(0xdedbd0),
                    strong: rgb(0xc4c0b3),
                    focus_ring: rgb(0x1f5fbf),
                },
                fill: Fill {
                    control: alpha(ink, 0.07),
                    hover: alpha(ink, 0.055),
                    press: alpha(ink, 0.11),
                    selected_unfocused: alpha(ink, 0.08),
                },
                accent: Tone {
                    text: rgb(0x1b55ad),
                    subtle: alpha(rgb(0x1f5fbf), 0.14),
                    solid: rgb(0x1f5fbf),
                    on_solid: Color32::WHITE,
                },
                error: Tone {
                    text: rgb(0xa8231b),
                    subtle: alpha(rgb(0xc8372d), 0.13),
                    solid: rgb(0xb3261e),
                    on_solid: Color32::WHITE,
                },
                warning: Tone {
                    text: rgb(0x7a5200),
                    subtle: alpha(rgb(0xd99a00), 0.20),
                    solid: rgb(0xf0b429),
                    on_solid: rgb(0x2a1f00),
                },
                success: Tone {
                    text: rgb(0x1f6139),
                    subtle: alpha(rgb(0x2e8b57), 0.15),
                    solid: rgb(0x26734a),
                    on_solid: Color32::WHITE,
                },
                touch: rgb(0x2f7d4f),
                close_hover: rgb(0xc4392f),
            }
        };
        let elevation = if dark {
            Elevation {
                card: shadow(1, 4, 70),
                popup: shadow(6, 24, 100),
                window: shadow(12, 40, 120),
            }
        } else {
            Elevation {
                card: shadow(1, 4, 22),
                popup: shadow(6, 24, 26),
                window: shadow(12, 40, 30),
            }
        };
        // Comfortable / Compact, hand-picked on a 2px grid.
        let d = |comfortable: f32, compact_value: f32| {
            if compact { compact_value } else { comfortable }
        };
        let space = Space {
            control_x: d(12.0, 10.0),
            control_y: d(5.0, 3.0),
            row_x: d(8.0, 6.0),
            row_y: d(4.0, 2.0),
            tab_x: d(12.0, 10.0),
            tab_y: d(7.0, 5.0),
            gap: d(8.0, 6.0),
            stack: d(8.0, 4.0),
            section: d(16.0, 12.0),
            panel: d(12.0, 8.0),
            dialog: d(20.0, 16.0),
            menu: d(6.0, 4.0),
            bar_y: d(5.0, 5.0),
        };
        // The type scale moves with the body size by the same pixel offset, floor 10.
        let at = |offset: f32, name: &str| {
            let size = (body + offset).max(10.0);
            Text {
                family: family(name),
                size,
                line: line_height(size),
            }
        };
        let ty = Type {
            small: at(-2.0, REGULAR),
            body: at(0.0, REGULAR),
            body_strong: at(0.0, MEDIUM),
            title: at(0.0, SEMIBOLD),
            heading: at(3.0, SEMIBOLD),
            mono: Text {
                line: line_height(body),
                ..at(-1.0, MONO)
            },
            numeric: at(0.0, NUMERIC),
            numeric_small: at(-2.0, NUMERIC),
        };
        Tokens {
            dark,
            compact,
            color,
            elevation,
            space,
            ty,
            radius: Radius {
                mark: 4.0,
                row: 6.0,
                control: 8.0,
                card: 12.0,
                popup: 12.0,
                window: 16.0,
            },
            motion: Motion {
                fast: 0.09,
                base: 0.14,
                slow: 0.20,
            },
            opacity_disabled: 0.45,
            hairline: 1.0,
            accent_line: 2.0,
            focus_ring: 2.0,
            icon: 16.0,
            icon_bar: 20.0,
        }
    }

    /// Heights are derived, never stored: line height plus twice the vertical padding.
    pub fn control_height(&self) -> f32 {
        self.ty.body.line + 2.0 * self.space.control_y
    }

    pub fn row_height(&self) -> f32 {
        self.ty.body.line + 2.0 * self.space.row_y
    }

    pub fn tab_height(&self) -> f32 {
        self.ty.body.line + 2.0 * self.space.tab_y
    }

    /// The Title bar: a control row plus its own padding.
    pub fn title_bar_height(&self) -> f32 {
        self.control_height() + 2.0 * self.space.bar_y
    }

    /// Fixed-height surfaces ignore Density: the status bar and a floating window's bar.
    pub fn status_bar_height(&self) -> f32 {
        self.ty.small.line + 8.0
    }

    pub fn floating_bar_height(&self) -> f32 {
        self.ty.body.line + 12.0
    }
}

/// `top` (possibly translucent) over the opaque `under`: the one blend the theme allows.
pub fn over(top: Color32, under: Color32) -> Color32 {
    crate::theme::over(top, under)
}

/// The contrast rules, as (name, foreground, background, required ratio), for one theme.
pub fn contrast_rules(t: &Tokens) -> Vec<(String, Color32, Color32, f64)> {
    let c = &t.color;
    let mut rules = Vec::new();
    let surfaces = [
        ("base", c.surface.base),
        ("card", c.surface.card),
        ("floating", c.surface.floating),
    ];
    let tones = [
        ("accent", &c.accent),
        ("error", &c.error),
        ("warning", &c.warning),
        ("success", &c.success),
    ];
    for (ground, bg) in surfaces {
        // Text on a surface and on its hover fill.
        for (state, bg) in [("", bg), ("+hover", over(c.fill.hover, bg))] {
            let ground = format!("{ground}{state}");
            rules.push((format!("text.primary on {ground}"), c.text.primary, bg, 4.5));
            rules.push((format!("text.muted on {ground}"), c.text.muted, bg, 4.5));
            for (name, tone) in tones {
                rules.push((format!("{name}.text on {ground}"), tone.text, bg, 4.5));
            }
        }
        // A filled control's label, hovered and pressed.
        let control = over(c.fill.control, bg);
        for (state, top) in [("hover", c.fill.hover), ("press", c.fill.press)] {
            rules.push((
                format!("text.primary on {ground}+control+{state}"),
                c.text.primary,
                over(top, control),
                4.5,
            ));
        }
    }
    rules.push(("text.primary on well".into(), c.text.primary, c.surface.well, 4.5));
    rules.push(("text.muted on well".into(), c.text.muted, c.surface.well, 4.5));
    for (name, tone) in tones {
        for (state, fill) in [
            ("", tone.solid),
            ("+hover", over(c.fill.hover, tone.solid)),
            ("+press", over(c.fill.press, tone.solid)),
        ] {
            rules.push((
                format!("{name}.on_solid on {name}.solid{state}"),
                tone.on_solid,
                fill,
                4.5,
            ));
        }
        for (sname, s) in surfaces {
            let tint = over(tone.subtle, s);
            for (state, tint) in [("", tint), ("+hover", over(c.fill.hover, tint))] {
                rules.push((
                    format!("text.primary on {name}.subtle{state}/{sname}"),
                    c.text.primary,
                    tint,
                    4.5,
                ));
                rules.push((
                    format!("text.muted on {name}.subtle{state}/{sname}"),
                    c.text.muted,
                    tint,
                    4.5,
                ));
            }
            rules.push((
                format!("{name}.text on {name}.subtle/{sname}"),
                tone.text,
                tint,
                4.5,
            ));
        }
    }
    for (sname, s) in surfaces {
        let grey = over(c.fill.selected_unfocused, s);
        rules.push((
            format!("text.primary on selected_unfocused/{sname}"),
            c.text.primary,
            grey,
            4.5,
        ));
        rules.push((
            format!("text.muted on selected_unfocused/{sname}"),
            c.text.muted,
            grey,
            4.5,
        ));
        rules.push((
            format!("accent line on {sname}"),
            c.accent.text,
            s,
            3.0,
        ));
        rules.push((
            format!("focus ring on {sname}"),
            c.stroke.focus_ring,
            s,
            3.0,
        ));
    }
    rules.push((
        "white glyph on close_hover".into(),
        Color32::WHITE,
        c.close_hover,
        4.5,
    ));
    rules
}

/// The contrast table, one line per rule, failures flagged.
pub fn contrast_report() -> (String, usize) {
    let mut out = String::new();
    let mut failures = 0;
    for theme in [Theme::Light, Theme::Dark] {
        let t = Tokens::new(theme, false, 13.0);
        for (name, fg, bg, need) in contrast_rules(&t) {
            let ratio = crate::theme::contrast_ratio(fg, bg);
            let fail = ratio < need;
            failures += usize::from(fail);
            out += &format!(
                "{theme:?} {name}: {ratio:.2} (needs {need}){}\n",
                if fail { "  FAIL" } else { "" }
            );
        }
    }
    (out, failures)
}

/// The floating bar and keys that drive the gallery. Never part of the design under
/// review; hidden with `REDA_PROTO_SWITCHER=0`.
pub fn switcher(ctx: &egui::Context) {
    use egui::{Key, Modifiers};
    let n = pages::PAGES.len();
    let (mut step, mut flip_theme, mut flip_density, mut size, mut next_face) =
        (0isize, false, false, 0i8, false);
    ctx.input_mut(|i| {
        if i.consume_key(Modifiers::SHIFT, Key::F9) {
            flip_theme = true;
        }
        if i.consume_key(Modifiers::CTRL, Key::F9) {
            flip_density = true;
        }
        if i.consume_key(Modifiers::NONE, Key::F9) {
            step = 1;
        }
        if i.consume_key(Modifiers::SHIFT, Key::F7) {
            size = -1;
        }
        if i.consume_key(Modifiers::NONE, Key::F7) {
            size = 1;
        }
        if i.consume_key(Modifiers::NONE, Key::F6) {
            next_face = true;
        }
    });
    if std::env::var("REDA_PROTO_SWITCHER").as_deref() != Ok("0") {
        let screen = ctx.content_rect();
        egui::Area::new(egui::Id::new("proto-gallery-switcher"))
            .order(egui::Order::Tooltip)
            .pivot(egui::Align2::CENTER_BOTTOM)
            .fixed_pos(screen.center_bottom() - egui::vec2(0.0, 10.0))
            .show(ctx, |ui| {
                let ink = Color32::from_rgb(0xff, 0xe0, 0x4a);
                egui::Frame::new()
                    .fill(Color32::from_rgb(0x11, 0x11, 0x11))
                    .stroke(egui::Stroke::new(1.0, ink))
                    .corner_radius(255)
                    .inner_margin(egui::Margin::symmetric(12, 4))
                    .show(ui, |ui| {
                        ui.visuals_mut().override_text_color = Some(ink);
                        ui.horizontal(|ui| {
                            let flat = |t: &str| egui::Button::new(t).frame(false);
                            if ui.add(flat("◀")).clicked() {
                                step = -1;
                            }
                            ui.label(format!("PROTOTYPE  {}", pages::PAGES[page()].0));
                            if ui.add(flat("▶")).clicked() {
                                step = 1;
                            }
                            ui.label("·");
                            let dark = ui.ctx().theme() == Theme::Dark;
                            flip_theme |= ui.add(flat(if dark { "Dark" } else { "Light" })).clicked();
                            ui.label("·");
                            let d = if compact() { "Compact" } else { "Comfortable" };
                            flip_density |= ui.add(flat(d)).clicked();
                            ui.label("·");
                            if ui.add(flat("−")).clicked() {
                                size = -1;
                            }
                            ui.label(format!("text {}", text_size()));
                            if ui.add(flat("+")).clicked() {
                                size = 1;
                            }
                            ui.label("·");
                            next_face |= ui.add(flat(FACES[face()])).clicked();
                            ui.label("·  F9 page, Shift+F9 theme, Ctrl+F9 Density, F7 size, F6 face, F10 app");
                        });
                    });
            });
    }
    if step != 0 {
        set_page((page() as isize + step).rem_euclid(n as isize) as usize);
    }
    if flip_theme {
        ctx.set_theme(match ctx.theme() {
            Theme::Dark => egui::ThemePreference::Light,
            Theme::Light => egui::ThemePreference::Dark,
        });
    }
    if flip_density {
        set_compact(!compact());
    }
    if size != 0 {
        set_text_size((text_size() as i8 + size) as u8);
    }
    if next_face {
        set_face(ctx, face() + 1);
    }
}
