//! `Tokens`: every colour, space, radius, text style, shadow and duration of the chrome,
//! named by its job (docs/spec/v0.1/design-language.md#tokens).
//!
//! Each family follows one setting: colours and shadows follow light/dark, `space` follows
//! Density, text styles follow the text size and the families, and the rest is fixed.
//! Heights are not stored: they are line height plus padding, computed by the methods at
//! the end.

use egui::epaint::Shadow;
use egui::text::{LayoutJob, TextFormat};
use egui::{Color32, FontFamily, FontId, Theme};

use super::fonts;
use super::palette::{self, Hue, Palette, Soft, Tint};

/// How much room the chrome gives its controls and rows. It changes paddings and gaps and
/// nothing else.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Density {
    #[default]
    Comfortable,
    Compact,
}

/// The font families the text styles are built from. egui has no weight on a text style, so
/// each weight is a family of its own.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Families {
    pub regular: FontFamily,
    pub medium: FontFamily,
    pub semibold: FontFamily,
    /// The UI face with tabular digits.
    pub numeric: FontFamily,
    pub mono: FontFamily,
}

impl Families {
    /// REDA's named families. [`fonts`] binds them to the bundled or the chosen faces;
    /// the names are the same whatever is chosen.
    pub fn named() -> Families {
        let name = |n: &str| FontFamily::Name(n.into());
        Families {
            regular: name(fonts::REGULAR),
            medium: name(fonts::MEDIUM),
            semibold: name(fonts::SEMIBOLD),
            numeric: name(fonts::NUMERIC),
            mono: name(fonts::MONO),
        }
    }

    /// egui's own proportional and monospace families for every weight: all that a
    /// context has before REDA's fonts are installed on it.
    pub fn egui() -> Families {
        Families {
            regular: FontFamily::Proportional,
            medium: FontFamily::Proportional,
            semibold: FontFamily::Proportional,
            numeric: FontFamily::Proportional,
            mono: FontFamily::Monospace,
        }
    }
}

/// What `Tokens` is built from besides light/dark.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Settings {
    pub density: Density,
    /// The body text size in whole pixels, [`Settings::TEXT_SIZES`]; clamped into it.
    pub text_size: u8,
    /// The chosen UI and monospace families, by name. [`set`](super::set) resolves them
    /// against the installed fonts; the bundled faces stand in for what is missing.
    pub fonts: fonts::Choice,
    /// Whether the chrome animates (`general.animate`). `None` follows the OS
    /// reduced-motion setting as read at launch ([`set`](super::set) resolves it); off,
    /// every transition happens at once.
    pub animate: Option<bool>,
}

impl Settings {
    /// The body text sizes a user can pick.
    pub const TEXT_SIZES: std::ops::RangeInclusive<u8> = 11..=16;
    pub const DEFAULT_TEXT_SIZE: u8 = 13;
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            density: Density::default(),
            text_size: Settings::DEFAULT_TEXT_SIZE,
            fonts: fonts::Choice::default(),
            animate: None,
        }
    }
}

/// A text style: family (which carries the weight), size and line height together. The
/// line height is the token's, not the face's: glyphs are centred in it, so a control is
/// the same height with any face.
#[derive(Clone, PartialEq, Debug)]
pub struct Text {
    pub family: FontFamily,
    pub size: f32,
    pub line: f32,
}

impl Text {
    pub fn font_id(&self) -> FontId {
        FontId::new(self.size, self.family.clone())
    }

    pub fn format(&self, color: Color32) -> TextFormat {
        TextFormat {
            font_id: self.font_id(),
            line_height: Some(self.line),
            color,
            ..Default::default()
        }
    }

    pub fn job(&self, text: &str, color: Color32) -> LayoutJob {
        LayoutJob::single_section(text.to_owned(), self.format(color))
    }

    /// For a stock egui widget: a `RichText` that carries the token's line height.
    pub fn rich(&self, text: impl Into<String>) -> egui::RichText {
        egui::RichText::new(text)
            .font(self.font_id())
            .line_height(Some(self.line))
    }
}

/// The line height that goes with a text size: about 1.5 em on a 2 px grid.
fn line_height(size: f32) -> f32 {
    2.0 * (size * 0.75 + 0.5).floor()
}

/// The accent or a semantic colour in its three strengths.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Tone {
    /// Text, Glyphs and lines.
    pub text: Color32,
    /// A translucent tint behind a row, a cell or a count.
    pub subtle: Color32,
    /// A fill with its own on-colour.
    pub solid: Color32,
    pub on_solid: Color32,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Surface {
    /// The window: behind cards, and under the Title bar and status bar.
    pub base: Color32,
    /// Tab groups, Docks and the front tab.
    pub card: Color32,
    /// Menus, popups, tooltips, floating bars, dialogs and the Command Palette.
    pub floating: Color32,
    /// Text fields and other wells, and the unchecked check box and radio.
    pub well: Color32,
    /// Laid over the window behind a modal.
    pub scrim: Color32,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct TextColor {
    pub primary: Color32,
    /// Secondary text: property names, shortcuts, captions, back tabs, table headers.
    pub muted: Color32,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct StrokeColor {
    /// Card rims, separators, the rule under a table header.
    pub hairline: Color32,
    /// Field, check box and radio outlines.
    pub strong: Color32,
    pub focus_ring: Color32,
}

/// The neutral state fills: the primary text colour at a low opacity, laid over whatever is
/// underneath with [`over`](super::over).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Fill {
    /// A filled button or combo box at rest.
    pub control: Color32,
    /// Hover on anything flat, and added on top of a filled control when hovered.
    pub hover: Color32,
    /// Replaces the hover fill while pressed.
    pub press: Color32,
    /// The selected row where the selection is not live.
    pub selected_unfocused: Color32,
}

/// The six icon role colours (docs/spec/v0.1/design-language.md#icons-and-glyphs).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct IconColor {
    /// What already exists: pins, pads, frames, the thing acted on.
    pub context: Color32,
    pub schematic: Color32,
    pub copper: Color32,
    pub general: Color32,
    pub picked: Color32,
    /// Board edge and Keepout.
    pub edge: Color32,
}

impl IconColor {
    /// The six, in the order of the keys the SVG files are drawn with.
    pub fn roles(&self) -> [Color32; 6] {
        let IconColor {
            context,
            schematic,
            copper,
            general,
            picked,
            edge,
        } = *self;
        [context, schematic, copper, general, picked, edge]
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
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
    /// The close button of the window controls when hovered.
    pub close_hover: Color32,
    pub icon: IconColor,
}

impl Colors {
    /// Every colour token with its name, as the contrast rules and the spec's tables name
    /// it. The destructuring is exhaustive, so a new token cannot be left out.
    pub fn named(&self) -> Vec<(&'static str, Color32)> {
        let Colors {
            surface:
                Surface {
                    base,
                    card,
                    floating,
                    well,
                    scrim,
                },
            text: TextColor { primary, muted },
            stroke:
                StrokeColor {
                    hairline,
                    strong,
                    focus_ring,
                },
            fill:
                Fill {
                    control,
                    hover,
                    press,
                    selected_unfocused,
                },
            accent,
            error,
            warning,
            success,
            touch,
            close_hover,
            icon:
                IconColor {
                    context,
                    schematic,
                    copper,
                    general,
                    picked,
                    edge,
                },
        } = *self;
        let mut named = vec![
            ("surface.base", base),
            ("surface.card", card),
            ("surface.floating", floating),
            ("surface.well", well),
            ("surface.scrim", scrim),
            ("text.primary", primary),
            ("text.muted", muted),
            ("stroke.hairline", hairline),
            ("stroke.strong", strong),
            ("stroke.focus_ring", focus_ring),
            ("fill.control", control),
            ("fill.hover", hover),
            ("fill.press", press),
            ("fill.selected_unfocused", selected_unfocused),
            ("touch", touch),
            ("close_hover", close_hover),
            ("icon.context", context),
            ("icon.schematic", schematic),
            ("icon.copper", copper),
            ("icon.general", general),
            ("icon.picked", picked),
            ("icon.edge", edge),
        ];
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
        for (names, tone) in TONES.into_iter().zip([accent, error, warning, success]) {
            let Tone {
                text,
                subtle,
                solid,
                on_solid,
            } = tone;
            named.extend(names.into_iter().zip([text, subtle, solid, on_solid]));
        }
        named
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Elevation {
    /// The 1 px soft shadow under a card.
    pub card: Shadow,
    /// Menus, popups, tooltips and floating bars.
    pub popup: Shadow,
    /// Dialogs, the Command Palette and windows.
    pub window: Shadow,
}

/// Paddings and gaps: the only tokens that follow Density.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Space {
    /// Inside buttons, fields and combo boxes, sideways.
    pub control_x: f32,
    /// The same, vertically.
    pub control_y: f32,
    /// Inside rows of lists, trees, tables and menus, sideways.
    pub row_x: f32,
    pub row_y: f32,
    /// Inside a tab, sideways.
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
    /// Above and below the Title bar's controls.
    pub bar_y: f32,
}

#[derive(Clone, PartialEq, Debug)]
pub struct Type {
    /// Captions, table headers, the status bar, key chips, badges.
    pub small: Text,
    pub body: Text,
    /// Emphasis in running text, and the active Document's tab. Medium.
    pub body_strong: Text,
    /// Dialog titles and Panel section headers. Semibold at the body size.
    pub title: Text,
    /// Page headings. Semibold.
    pub heading: Text,
    /// The Journal, file names and patterns, command and key text.
    pub mono: Text,
    /// Numbers that must line up: tabular digits.
    pub numeric: Text,
    pub numeric_small: Text,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Radius {
    /// Check boxes, swatches, key chips.
    pub mark: f32,
    /// Rows and menu items: the popup radius minus the popup's padding, so corners nest.
    pub row: f32,
    pub control: f32,
    pub card: f32,
    pub popup: f32,
    pub window: f32,
}

/// Stroke widths.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct StrokeWidth {
    pub hairline: f32,
    /// The line along the top of the active Document's tab.
    pub accent_line: f32,
    pub focus_ring: f32,
}

/// Widths that follow the text size, and never Density: fields, dialog columns and the
/// Docks' opening widths. Each is given at the default text size and grows with the body
/// text above it, so text that fits at 13 px fits at 16. Below the default they keep their
/// 13 px width: icons, marks and paddings do not shrink with the text, so a narrower
/// column would cut what fits at 13 px.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Size {
    /// The body size over the default one, and never below 1: what a width given at
    /// 13 px is multiplied by.
    pub scale: f32,
    /// The width a side Dock opens at before the user drags it.
    pub dock: f32,
    /// A text field's width when it is given none.
    pub field: f32,
}

impl Size {
    /// The widths for text whose body is `body` px.
    fn at(body: f32) -> Size {
        let scale = (body / f32::from(Settings::DEFAULT_TEXT_SIZE)).max(1.0);
        let of = |width: f32| (width * scale).round();
        Size {
            scale,
            dock: of(SIDE_DOCK),
            field: of(FIELD),
        }
    }

    /// A width given at the default text size, at this one, on a whole pixel.
    pub fn of(&self, at_default: f32) -> f32 {
        (at_default * self.scale).round()
    }
}

/// A side Dock's opening width at the default text size: the Library Panel's three
/// buttons, the Properties values and the navigator's columns need about 250 px inside it.
const SIDE_DOCK: f32 = 288.0;
/// A text field's default width at the default text size.
const FIELD: f32 = 280.0;

/// Icon sizes. Only UI scale changes them.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct IconSize {
    /// Tabs, rows, buttons with text, menus.
    pub small: f32,
    /// The Title bar's quick strip, Tool bars, the Active Bar, collapsed Dock strips.
    pub bar: f32,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Opacity {
    /// Applied once to a whole disabled widget, icon included.
    pub disabled: f32,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Easing {
    /// Ease-out, cubic.
    Out,
    Linear,
}

/// How one kind of change is animated. Times are in seconds, and zero is at once.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Transition {
    pub enter: f32,
    /// `None` where nothing leaves (a progress bar only catches up).
    pub leave: Option<f32>,
    pub easing: Easing,
}

/// The Crisp motion profile (docs/spec/v0.1/design-language.md#motion). With Animate off
/// every transition is at once, the travel is zero and nothing scales; the tooltip wait
/// stays, because it is a wait and not a movement.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Motion {
    /// Whether the chrome animates: the Animate Preference, or the OS setting while it
    /// is unset.
    pub animate: bool,
    pub fast: f32,
    pub base: f32,
    /// A hover fill, and a scrollbar widening.
    pub hover: Transition,
    pub press: Transition,
    /// A check box, radio or toggle turning on.
    pub toggle: Transition,
    pub switch: Transition,
    /// The accent line arriving on the new active tab.
    pub tab_line: Transition,
    /// A menu, a context menu and a combo box's list.
    pub menu: Transition,
    pub tooltip: Transition,
    pub flyout: Transition,
    pub dock_collapse: Transition,
    pub section: Transition,
    /// A dialog and its scrim.
    pub dialog: Transition,
    pub palette: Transition,
    pub selected_row: Transition,
    /// A Tool's bar arriving on the canvas, and an on state there.
    pub canvas_bar: Transition,
    pub progress: Transition,
    /// How far what arrives travels, in pixels.
    pub travel: f32,
    pub flyout_travel: f32,
    /// A dialog's scale when it starts to arrive.
    pub dialog_scale: f32,
    pub tooltip_wait: f32,
    /// A tooltip shows without the wait when another was showing within this time.
    pub tooltip_chain: f32,
}

/// The single styling source of the chrome. Read it through
/// [`TokensExt::tokens`](super::TokensExt::tokens).
#[derive(Clone, PartialEq, Debug)]
pub struct Tokens {
    /// What this value was built for.
    pub theme: Theme,
    pub density: Density,
    pub color: Colors,
    pub elevation: Elevation,
    pub space: Space,
    pub ty: Type,
    pub radius: Radius,
    pub stroke: StrokeWidth,
    pub icon: IconSize,
    pub size: Size,
    pub opacity: Opacity,
    pub motion: Motion,
}

pub(super) const fn rgb(hex: u32) -> Color32 {
    Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

pub(super) fn tint(t: Tint) -> Color32 {
    let c = rgb(t.hex);
    let alpha = (t.percent / 100.0 * 255.0).round() as u8;
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), alpha)
}

fn tone(hue: Hue) -> Tone {
    Tone {
        text: rgb(hue.text),
        subtle: tint(hue.tint),
        solid: rgb(hue.solid),
        on_solid: rgb(hue.on_solid),
    }
}

fn shadow(soft: Soft) -> Shadow {
    Shadow {
        offset: [0, soft.y],
        blur: soft.blur,
        spread: 0,
        color: Color32::from_black_alpha(soft.alpha),
    }
}

pub(super) fn palette(theme: Theme) -> &'static Palette {
    match theme {
        Theme::Dark => &palette::DARK,
        Theme::Light => &palette::LIGHT,
    }
}

fn colors(p: &Palette) -> Colors {
    let [base, card, floating] = p.paper.map(rgb);
    let fill = p.ink_fills.map(|percent| {
        tint(Tint {
            hex: p.ink,
            percent,
        })
    });
    let icon = p.icon_roles.map(rgb);
    Colors {
        surface: Surface {
            base,
            card,
            floating,
            well: rgb(p.well),
            scrim: Color32::from_black_alpha(p.scrim_alpha),
        },
        text: TextColor {
            primary: rgb(p.ink),
            muted: rgb(p.ink_soft),
        },
        stroke: StrokeColor {
            hairline: rgb(p.line),
            strong: rgb(p.line_strong),
            focus_ring: rgb(p.ring),
        },
        fill: Fill {
            control: fill[0],
            hover: fill[1],
            press: fill[2],
            selected_unfocused: fill[3],
        },
        accent: tone(p.blue),
        error: tone(p.red),
        warning: tone(p.amber),
        success: tone(p.green),
        touch: rgb(p.touch_green),
        close_hover: rgb(p.close_red),
        icon: IconColor {
            context: icon[0],
            schematic: icon[1],
            copper: icon[2],
            general: icon[3],
            picked: icon[4],
            edge: icon[5],
        },
    }
}

/// Comfortable and Compact, each hand-picked on a 2 px grid: there is no multiplier.
fn space(density: Density) -> Space {
    let d = |comfortable: f32, compact: f32| match density {
        Density::Comfortable => comfortable,
        Density::Compact => compact,
    };
    Space {
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
    }
}

/// The type scale: every style moves with the body size by the same pixel offset, and none
/// goes below 10 px.
fn type_scale(body: f32, f: &Families) -> Type {
    let at = |offset: f32, family: &FontFamily| {
        let size = (body + offset).max(10.0);
        Text {
            family: family.clone(),
            size,
            line: line_height(size),
        }
    };
    Type {
        small: at(-2.0, &f.regular),
        body: at(0.0, &f.regular),
        body_strong: at(0.0, &f.medium),
        title: at(0.0, &f.semibold),
        heading: at(3.0, &f.semibold),
        // On the body's line height, so a monospace run sits in a body row.
        mono: Text {
            line: line_height(body),
            ..at(-1.0, &f.mono)
        },
        numeric: at(0.0, &f.numeric),
        numeric_small: at(-2.0, &f.numeric),
    }
}

/// The motion tokens; with `animate` off, every transition is at once.
pub(super) fn motion(animate: bool) -> Motion {
    let crisp = crisp();
    if animate {
        return crisp;
    }
    let still = Transition {
        enter: 0.0,
        leave: Some(0.0),
        easing: Easing::Out,
    };
    Motion {
        animate: false,
        hover: still,
        press: still,
        toggle: still,
        switch: still,
        tab_line: still,
        menu: still,
        tooltip: still,
        flyout: still,
        dock_collapse: still,
        section: still,
        dialog: still,
        palette: still,
        selected_row: still,
        canvas_bar: still,
        progress: Transition {
            leave: None,
            ..still
        },
        travel: 0.0,
        flyout_travel: 0.0,
        dialog_scale: 1.0,
        ..crisp
    }
}

fn crisp() -> Motion {
    let (fast, base) = (0.09, 0.14);
    let t = |enter: f32, leave: f32| Transition {
        enter,
        leave: Some(leave),
        easing: Easing::Out,
    };
    Motion {
        animate: true,
        fast,
        base,
        hover: Transition {
            easing: Easing::Linear,
            ..t(0.0, fast)
        },
        press: t(0.0, fast),
        toggle: t(fast, fast),
        switch: t(base, base),
        tab_line: t(fast, 0.0),
        menu: t(fast, 0.0),
        tooltip: t(fast, 0.0),
        flyout: t(base, fast),
        dock_collapse: t(base, base),
        section: t(base, fast),
        dialog: t(base, fast),
        palette: t(base, fast),
        selected_row: t(0.0, 0.0),
        canvas_bar: t(fast, fast),
        progress: Transition {
            leave: None,
            ..t(base, 0.0)
        },
        travel: 4.0,
        flyout_travel: 8.0,
        dialog_scale: 0.97,
        tooltip_wait: 0.5,
        tooltip_chain: 0.4,
    }
}

/// Added to the small line for the status bar's height, and to the body line for a
/// floating window's bar: neither follows Density.
const STATUS_BAR_PAD: f32 = 8.0;
const FLOATING_BAR_PAD: f32 = 12.0;

impl Tokens {
    /// The tokens for `theme` and `settings`, with text in REDA's named families. Which
    /// faces those are is settled when the fonts are installed ([`set`](super::set)).
    pub fn new(theme: Theme, settings: &Settings) -> Tokens {
        Tokens::with_families(theme, settings, &Families::named())
    }

    /// The same tokens with text in `families`. A context that has no REDA fonts yet
    /// (a bare `egui::Context` in a test) takes [`Families::egui`].
    pub fn with_families(theme: Theme, settings: &Settings, families: &Families) -> Tokens {
        let p = palette(theme);
        let (lo, hi) = Settings::TEXT_SIZES.into_inner();
        let body = f32::from(settings.text_size.clamp(lo, hi));
        let [card, popup, window] = p.shadows.map(shadow);
        Tokens {
            theme,
            density: settings.density,
            color: colors(p),
            elevation: Elevation {
                card,
                popup,
                window,
            },
            space: space(settings.density),
            ty: type_scale(body, families),
            radius: Radius {
                mark: 4.0,
                row: 6.0,
                control: 8.0,
                card: 12.0,
                popup: 12.0,
                window: 16.0,
            },
            stroke: StrokeWidth {
                hairline: 1.0,
                accent_line: 2.0,
                focus_ring: 2.0,
            },
            icon: IconSize {
                small: 16.0,
                bar: 20.0,
            },
            size: Size::at(body),
            opacity: Opacity { disabled: 0.45 },
            motion: motion(settings.animate.unwrap_or(true)),
        }
    }

    /// A button, field or combo box: the body line plus its padding.
    pub fn control_height(&self) -> f32 {
        self.ty.body.line + 2.0 * self.space.control_y
    }

    /// A row of a list, tree, table or menu.
    pub fn row_height(&self) -> f32 {
        self.ty.body.line + 2.0 * self.space.row_y
    }

    pub fn tab_height(&self) -> f32 {
        self.ty.body.line + 2.0 * self.space.tab_y
    }

    /// The Title bar: a row of controls plus its own padding.
    pub fn title_bar_height(&self) -> f32 {
        self.control_height() + 2.0 * self.space.bar_y
    }

    /// The status bar, which ignores Density.
    pub fn status_bar_height(&self) -> f32 {
        self.ty.small.line + STATUS_BAR_PAD
    }

    /// A floating window's bar, which ignores Density.
    pub fn floating_bar_height(&self) -> f32 {
        self.ty.body.line + FLOATING_BAR_PAD
    }
}
