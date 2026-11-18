//! The palette: the raw values of the Paper look, private to `theme`. Nothing outside this
//! module's parent names a value; [`Tokens`](super::Tokens) gives each one its job.
//!
//! A value changes here, and `cargo xtask tokens` then rewrites the tables in
//! `docs/spec/v0.1/design-language.md`.

/// A colour laid over what is underneath: a hex at an opacity in percent.
#[derive(Clone, Copy)]
pub(super) struct Tint {
    pub hex: u32,
    pub percent: f32,
}

/// One hue (the blue accent or a semantic colour) in its strengths.
#[derive(Clone, Copy)]
pub(super) struct Hue {
    pub text: u32,
    pub tint: Tint,
    pub solid: u32,
    pub on_solid: u32,
}

/// A shadow: its vertical offset, its blur and the alpha of its black, of 255.
#[derive(Clone, Copy)]
pub(super) struct Soft {
    pub y: i8,
    pub blur: u8,
    pub alpha: u8,
}

/// The raw values of one theme.
pub(super) struct Palette {
    /// The neutral papers, from the window's base up to what floats, and the well.
    pub paper: [u32; 3],
    pub well: u32,
    /// The alpha of the black laid over the window behind a modal, of 255.
    pub scrim_alpha: u8,
    pub ink: u32,
    pub ink_soft: u32,
    pub line: u32,
    pub line_strong: u32,
    pub ring: u32,
    /// The ink's opacity, in percent, as a control's fill, on hover, on press and on a
    /// selected row whose selection is not live.
    pub ink_fills: [f32; 4],
    pub blue: Hue,
    pub red: Hue,
    pub amber: Hue,
    pub green: Hue,
    pub touch_green: u32,
    pub close_red: u32,
    /// The six icon role colours, in the order of [`ICON_KEYS`].
    pub icon_roles: [u32; 6],
    /// The shadows under a card, a popup and a window.
    pub shadows: [Soft; 3],
}

/// The six icon role colours as the SVG files in `icons/` write them: fixed keys in the
/// art, not theme values. `icons::recolour` swaps each for the theme's `icon_roles`
/// entry in the same place, so the two tables sit side by side here.
pub(super) const ICON_KEYS: [u32; 6] = [
    0x8a93a0, // existing / context
    0x2fb36b, // created, schematic
    0xe08a2e, // created, PCB copper
    0x3d8fd6, // created, general
    0xec4f7a, // picked point
    0xd4c03c, // created, board edge and keepout (not copper)
];

const WHITE: u32 = 0xffffff;

const fn tint(hex: u32, percent: f32) -> Tint {
    Tint { hex, percent }
}

const fn soft(y: i8, blur: u8, alpha: u8) -> Soft {
    Soft { y, blur, alpha }
}

pub(super) const LIGHT: Palette = Palette {
    paper: [0xf0eee6, 0xfaf9f5, 0xfdfcfa],
    well: 0xffffff,
    scrim_alpha: 90,
    ink: 0x1f1e1c,
    ink_soft: 0x59574f,
    line: 0xdedbd0,
    line_strong: 0xc4c0b3,
    ring: 0x1f5fbf,
    ink_fills: [7.0, 5.5, 11.0, 8.0],
    blue: Hue {
        text: 0x1b55ad,
        tint: tint(0x1f5fbf, 14.0),
        solid: 0x1f5fbf,
        on_solid: WHITE,
    },
    red: Hue {
        text: 0xa8231b,
        tint: tint(0xc8372d, 13.0),
        solid: 0xb3261e,
        on_solid: WHITE,
    },
    amber: Hue {
        text: 0x7a5200,
        tint: tint(0xd99a00, 20.0),
        solid: 0xf0b429,
        on_solid: 0x2a1f00,
    },
    green: Hue {
        text: 0x1f6139,
        tint: tint(0x2e8b57, 15.0),
        solid: 0x26734a,
        on_solid: WHITE,
    },
    touch_green: 0x2f7d4f,
    close_red: 0xc4392f,
    icon_roles: [0x79766e, 0x228a51, 0xac6921, 0x347dbc, 0xd2456c, 0x8a7a12],
    shadows: [soft(1, 4, 22), soft(6, 24, 26), soft(12, 40, 30)],
};

pub(super) const DARK: Palette = Palette {
    paper: [0x1f1e1d, 0x262624, 0x2c2b29],
    well: 0x1a1918,
    scrim_alpha: 140,
    ink: 0xf3f1ea,
    ink_soft: 0xc2c0b7,
    line: 0x3b3a37,
    line_strong: 0x55534e,
    ring: 0x8fbaf8,
    ink_fills: [8.0, 7.0, 13.0, 10.0],
    blue: Hue {
        text: 0x8fbaf8,
        tint: tint(0x4f8fe8, 22.0),
        solid: 0x2458b0,
        on_solid: WHITE,
    },
    red: Hue {
        text: 0xf5998c,
        tint: tint(0xe5534a, 20.0),
        solid: 0xad2f27,
        on_solid: WHITE,
    },
    amber: Hue {
        text: 0xe5b04a,
        tint: tint(0xe5a530, 16.0),
        solid: 0xe5b04a,
        on_solid: 0x231a05,
    },
    green: Hue {
        text: 0x7cc792,
        tint: tint(0x4faf6e, 18.0),
        solid: 0x25683f,
        on_solid: WHITE,
    },
    touch_green: 0x6fbf8a,
    close_red: 0xc4392f,
    icon_roles: [0x96918a, 0x2fb36b, 0xe08a2e, 0x3d8fd6, 0xef5a82, 0xd4c03c],
    shadows: [soft(1, 4, 70), soft(6, 24, 100), soft(12, 40, 120)],
};
