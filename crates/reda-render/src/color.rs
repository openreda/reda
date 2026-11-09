//! Colour schemes: one fixed scheme per app theme, keyed by schematic object kind (and, for
//! PCB Scenes, by Layer kind and side), never by Layer UUID.

use reda_model::{LayerKind, Side};

/// The app theme, which the schematic sheet follows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Theme {
    #[default]
    Dark,
    Light,
}

/// An sRGB colour with straight (not premultiplied) alpha.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Rgba(pub [u8; 4]);

impl Rgba {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Rgba {
        Rgba([r, g, b, 255])
    }
}

/// What an item's colour is looked up by.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ColorKey {
    Schematic(SchematicColor),
    /// PCB ink: keyed by Layer kind and side (see [`BoardColor::layer`]), never by Layer
    /// UUID.
    Board(BoardColor),
    /// A Net's own colour (`color` on the Net): not in the palette, never themed.
    Net(Rgba),
}

impl ColorKey {
    /// How many colour slots the GPU palette has: every key's [`slot`](Self::slot) is below.
    pub const SLOTS: usize = 128;

    /// The key's index in the GPU palette.
    pub fn slot(self) -> usize {
        match self {
            ColorKey::Schematic(kind) => kind as usize,
            ColorKey::Board(kind) => BOARD_SLOTS + kind as usize,
            // Not in the palette: it has the Ratsnest's slot, and the GPU word carries
            // the colour itself (see `gpu_word`).
            ColorKey::Net(_) => BOARD_SLOTS + BoardColor::Ratsnest as usize,
        }
    }

    /// The colour word an instance carries: the palette slot, or, with the top bit set,
    /// a Net colour's own `0xRRGGBB`.
    pub fn gpu_word(self) -> u32 {
        match self {
            ColorKey::Net(Rgba([r, g, b, _])) => {
                0x8000_0000 | (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b)
            }
            key => key.slot() as u32,
        }
    }

    /// Every key, in slot order.
    pub fn all() -> impl Iterator<Item = ColorKey> {
        let schematic = SchematicColor::ALL.iter().map(|k| ColorKey::Schematic(*k));
        schematic.chain(BoardColor::ALL.iter().map(|k| ColorKey::Board(*k)))
    }

    /// The snapshot name.
    pub fn name(self) -> &'static str {
        match self {
            ColorKey::Schematic(kind) => kind.name(),
            ColorKey::Board(kind) => kind.name(),
            ColorKey::Net(_) => "Net",
        }
    }

    /// The snapshot label: the name, with a Net colour's `#rrggbb`.
    pub fn label(self) -> String {
        match self {
            ColorKey::Net(Rgba([r, g, b, _])) => format!("Net#{r:02x}{g:02x}{b:02x}"),
            key => key.name().to_owned(),
        }
    }
}

/// The first GPU palette slot of the board colours.
const BOARD_SLOTS: usize = 64;

macro_rules! colors {
    ($(#[$enum_doc:meta])* $enum:ident {
        $($(#[$doc:meta])* $name:ident: $dark:expr, $light:expr;)*
    }) => {
        $(#[$enum_doc])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub enum $enum {
            $($(#[$doc])* $name,)*
        }

        impl $enum {
            pub const ALL: &[$enum] = &[$($enum::$name,)*];

            /// The snapshot name.
            pub const fn name(self) -> &'static str {
                match self {
                    $($enum::$name => stringify!($name),)*
                }
            }

            const fn shipped(self, theme: Theme) -> Rgba {
                match (self, theme) {
                    $(
                        ($enum::$name, Theme::Dark) => $dark,
                        ($enum::$name, Theme::Light) => $light,
                    )*
                }
            }
        }
    };
}

colors! {
    /// The kinds of schematic ink, each with its own colour.
    SchematicColor {
    /// The sheet's paper.
    Paper: Rgba::rgb(0x1c, 0x1d, 0x20), Rgba::rgb(0xfb, 0xfb, 0xf8);
    /// The frame border and title block lines.
    Frame: Rgba::rgb(0x6e, 0x72, 0x7a), Rgba::rgb(0x8a, 0x5a, 0x44);
    /// Title block text.
    FrameText: Rgba::rgb(0xb4, 0xb8, 0xc0), Rgba::rgb(0x5a, 0x3a, 0x2c);
    /// Title block text the sheet takes from the Project (its revision), greyed on the
    /// canvas.
    FrameTextInherited: Rgba::rgb(0x80, 0x84, 0x8c), Rgba::rgb(0x8c, 0x84, 0x7c);
    /// The tint behind an empty title field, on the canvas only.
    EmptyField: Rgba::rgb(0x33, 0x2c, 0x1c), Rgba::rgb(0xfb, 0xee, 0xc8);
    /// The Border Zones' band, dividers and labels.
    Zone: Rgba::rgb(0x6e, 0x72, 0x7a), Rgba::rgb(0x8a, 0x5a, 0x44);
    Wire: Rgba::rgb(0x4c, 0xc2, 0x6a), Rgba::rgb(0x0f, 0x7a, 0x2e);
    Junction: Rgba::rgb(0x4c, 0xc2, 0x6a), Rgba::rgb(0x0f, 0x7a, 0x2e);
    NetLabel: Rgba::rgb(0x6c, 0xb6, 0xff), Rgba::rgb(0x1f, 0x5f, 0xbf);
    NoConnect: Rgba::rgb(0x5a, 0x8c, 0xff), Rgba::rgb(0x2a, 0x4c, 0xc8);
    /// Ports: the outline and the name.
    Port: Rgba::rgb(0xd0, 0x9a, 0x4c), Rgba::rgb(0x9a, 0x5a, 0x00);
    /// Power Ports: the symbol and the name.
    PowerPort: Rgba::rgb(0xe0, 0x5c, 0x8c), Rgba::rgb(0xa0, 0x1a, 0x50);
    /// Symbol body outlines.
    Body: Rgba::rgb(0xe0, 0x6c, 0x5c), Rgba::rgb(0x9e, 0x22, 0x1a);
    /// Filled Symbol body shapes.
    BodyFill: Rgba::rgb(0x4a, 0x2e, 0x2a), Rgba::rgb(0xff, 0xf3, 0xc4);
    Pin: Rgba::rgb(0xe0, 0x6c, 0x5c), Rgba::rgb(0x9e, 0x22, 0x1a);
    /// A Not Fitted Component's body outlines, masked under its overlay.
    MaskedBody: Rgba::rgb(0x4a, 0x40, 0x3e), Rgba::rgb(0xd8, 0xc4, 0xc0);
    /// A Not Fitted Component's filled body shapes.
    MaskedFill: Rgba::rgb(0x26, 0x22, 0x22), Rgba::rgb(0xf4, 0xf0, 0xe6);
    /// A Not Fitted Component's Pins with their names and numbers.
    MaskedPin: Rgba::rgb(0x4a, 0x40, 0x3e), Rgba::rgb(0xd8, 0xc4, 0xc0);
    /// The `cross` Not Fitted overlay.
    NotFittedCross: Rgba::rgb(0xff, 0x40, 0x40), Rgba::rgb(0xd0, 0x10, 0x10);
    /// The `mesh` Not Fitted overlay.
    NotFittedMesh: Rgba::rgb(0x8c, 0x90, 0x96), Rgba::rgb(0x7c, 0x80, 0x86);
    PinName: Rgba::rgb(0xc8, 0xc8, 0xc8), Rgba::rgb(0x30, 0x30, 0x30);
    PinNumber: Rgba::rgb(0x96, 0x96, 0x96), Rgba::rgb(0x60, 0x60, 0x60);
    Designator: Rgba::rgb(0xe8, 0xc4, 0x5a), Rgba::rgb(0x00, 0x6a, 0x8a);
    Value: Rgba::rgb(0xc8, 0xc8, 0xc8), Rgba::rgb(0x30, 0x30, 0x30);
    /// Other visible Component fields.
    Field: Rgba::rgb(0x96, 0x96, 0x96), Rgba::rgb(0x60, 0x60, 0x60);
    /// Free text on the sheet.
    Text: Rgba::rgb(0xdc, 0xdc, 0xdc), Rgba::rgb(0x20, 0x20, 0x20);
    /// Free lines and rectangles on the sheet.
    Graphic: Rgba::rgb(0xa0, 0xa8, 0xb4), Rgba::rgb(0x44, 0x4c, 0x58);
}}

colors! {
    /// The kinds of PCB ink: one per Layer kind and side, plus what is on no Layer.
    BoardColor {
    CopperTop: Rgba::rgb(0xc8, 0x3c, 0x34), Rgba::rgb(0xb0, 0x28, 0x22);
    CopperBottom: Rgba::rgb(0x48, 0x7c, 0xd0), Rgba::rgb(0x24, 0x54, 0xb0);
    /// Copper Layers between the outer ones.
    CopperInner: Rgba::rgb(0xc8, 0xb4, 0x3c), Rgba::rgb(0x8a, 0x78, 0x10);
    /// A stale Zone fill on the top copper: the copper colour, dimmed.
    StaleFillTop: Rgba::rgb(0x5a, 0x2a, 0x28), Rgba::rgb(0xd8, 0xa8, 0xa4);
    StaleFillBottom: Rgba::rgb(0x2a, 0x3c, 0x5c), Rgba::rgb(0xa4, 0xb4, 0xd8);
    StaleFillInner: Rgba::rgb(0x58, 0x52, 0x2a), Rgba::rgb(0xcc, 0xc4, 0x98);
    SilkscreenTop: Rgba::rgb(0xec, 0xec, 0xec), Rgba::rgb(0x30, 0x30, 0x30);
    SilkscreenBottom: Rgba::rgb(0xd8, 0xb8, 0xf0), Rgba::rgb(0x6a, 0x3c, 0x8c);
    SolderMaskTop: Rgba::rgb(0xb4, 0x50, 0xb4), Rgba::rgb(0x9a, 0x30, 0x9a);
    SolderMaskBottom: Rgba::rgb(0x40, 0xb4, 0xa8), Rgba::rgb(0x18, 0x84, 0x7a);
    PasteTop: Rgba::rgb(0xa8, 0xa8, 0xa8), Rgba::rgb(0x70, 0x70, 0x70);
    PasteBottom: Rgba::rgb(0x70, 0x88, 0x94), Rgba::rgb(0x50, 0x64, 0x70);
    CourtyardTop: Rgba::rgb(0xf0, 0x3c, 0xd8), Rgba::rgb(0xc0, 0x10, 0xa8);
    CourtyardBottom: Rgba::rgb(0x3c, 0xd8, 0xf0), Rgba::rgb(0x10, 0x94, 0xb0);
    AssemblyTop: Rgba::rgb(0xc8, 0xb0, 0x78), Rgba::rgb(0x84, 0x6c, 0x34);
    AssemblyBottom: Rgba::rgb(0x8c, 0xa0, 0xc8), Rgba::rgb(0x4c, 0x60, 0x8c);
    BoardOutline: Rgba::rgb(0xe8, 0xd0, 0x40), Rgba::rgb(0x9a, 0x84, 0x00);
    Mechanical: Rgba::rgb(0xc0, 0xa0, 0x70), Rgba::rgb(0x7c, 0x60, 0x34);
    Documentation: Rgba::rgb(0x9a, 0xa0, 0xa8), Rgba::rgb(0x50, 0x56, 0x5e);
    /// Drilled holes, drawn over every Layer.
    Hole: Rgba::rgb(0x0a, 0x0a, 0x0c), Rgba::rgb(0xfc, 0xfc, 0xfa);
    /// The Ratsnest's lines.
    Ratsnest: Rgba::rgb(0xd0, 0xd4, 0xdc), Rgba::rgb(0x38, 0x3c, 0x44);
    /// Pad numbers, drawn on the Pad.
    PadNumber: Rgba::rgb(0xff, 0xff, 0xff), Rgba::rgb(0xff, 0xff, 0xff);
    /// Net names on Pads and Tracks.
    NetName: Rgba::rgb(0xf0, 0xf0, 0xd8), Rgba::rgb(0xff, 0xff, 0xf0);
    /// The derived sheet-name label over a cluster of Footprints off the board.
    SheetLabel: Rgba::rgb(0x9c, 0xa8, 0xb8), Rgba::rgb(0x4a, 0x55, 0x66);
}}

impl BoardColor {
    /// The colour of ink on a Layer of `kind` on `side`. A copper Layer with no side is an
    /// inner one.
    pub const fn layer(kind: LayerKind, side: Option<Side>) -> BoardColor {
        use BoardColor as C;
        let top = !matches!(side, Some(Side::Bottom));
        match kind {
            LayerKind::Copper => match side {
                Some(Side::Top) => C::CopperTop,
                Some(Side::Bottom) => C::CopperBottom,
                None => C::CopperInner,
            },
            LayerKind::Silkscreen if top => C::SilkscreenTop,
            LayerKind::Silkscreen => C::SilkscreenBottom,
            LayerKind::SolderMask if top => C::SolderMaskTop,
            LayerKind::SolderMask => C::SolderMaskBottom,
            LayerKind::Paste if top => C::PasteTop,
            LayerKind::Paste => C::PasteBottom,
            LayerKind::Courtyard if top => C::CourtyardTop,
            LayerKind::Courtyard => C::CourtyardBottom,
            LayerKind::Assembly if top => C::AssemblyTop,
            LayerKind::Assembly => C::AssemblyBottom,
            LayerKind::BoardOutline => C::BoardOutline,
            LayerKind::Mechanical => C::Mechanical,
            LayerKind::Documentation => C::Documentation,
        }
    }

    /// The colour of a stale Zone fill on a copper Layer on `side`.
    pub const fn stale_fill(side: Option<Side>) -> BoardColor {
        match side {
            Some(Side::Top) => BoardColor::StaleFillTop,
            Some(Side::Bottom) => BoardColor::StaleFillBottom,
            None => BoardColor::StaleFillInner,
        }
    }
}

/// The colours one theme draws with: the shipped scheme plus the user's overrides (a
/// Preference).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Palette {
    theme: Theme,
    overrides: Vec<(ColorKey, Rgba)>,
}

impl Palette {
    /// The scheme REDA ships for `theme`.
    pub fn shipped(theme: Theme) -> Palette {
        Palette {
            theme,
            overrides: Vec::new(),
        }
    }

    /// The mono scheme of a schematic PDF: black ink on white paper, every fill white.
    /// Masked colours are plain ink too, so a Not Fitted body is drawn unmasked, under its
    /// cross or mesh in black.
    pub fn mono() -> Palette {
        use SchematicColor as C;
        let (white, black) = (Rgba::rgb(0xff, 0xff, 0xff), Rgba::rgb(0, 0, 0));
        let overrides = SchematicColor::ALL
            .iter()
            .map(|&kind| {
                let paper = matches!(kind, C::Paper | C::BodyFill | C::MaskedFill | C::EmptyField);
                (ColorKey::Schematic(kind), if paper { white } else { black })
            })
            .collect();
        Palette {
            theme: Theme::Light,
            overrides,
        }
    }

    pub fn theme(&self) -> Theme {
        self.theme
    }

    /// The canvas around the Document: what an image shows outside the sheet, and what
    /// dimmed objects fade towards.
    pub fn background(&self) -> Rgba {
        match self.theme {
            Theme::Dark => Rgba::rgb(0x12, 0x13, 0x15),
            Theme::Light => Rgba::rgb(0xe2, 0xe3, 0xe0),
        }
    }

    /// The canvas grid's dots: opaque and quiet, about 1.4:1 on the sheet's paper in either
    /// theme, as in the main window prototype.
    pub fn grid(&self) -> Rgba {
        match self.theme {
            Theme::Dark => Rgba::rgb(0x33, 0x37, 0x3d),
            Theme::Light => Rgba::rgb(0xdc, 0xdc, 0xd7),
        }
    }

    /// Overrides one colour.
    pub fn set(&mut self, key: ColorKey, color: Rgba) {
        self.overrides.retain(|(k, _)| *k != key);
        self.overrides.push((key, color));
    }

    pub fn color(&self, key: ColorKey) -> Rgba {
        if let Some((_, c)) = self.overrides.iter().find(|(k, _)| *k == key) {
            return *c;
        }
        match key {
            ColorKey::Schematic(kind) => kind.shipped(self.theme),
            ColorKey::Board(kind) => kind.shipped(self.theme),
            ColorKey::Net(color) => color,
        }
    }
}

const _: () = assert!(SchematicColor::ALL.len() <= BOARD_SLOTS);

impl SchematicColor {
    /// Whether this is the ink of a Unit's Symbol body (outlines or fills), masked or not:
    /// what picks and boxes a Unit, as opposed to its Pins and fields.
    pub const fn is_unit_body(self) -> bool {
        matches!(
            self,
            SchematicColor::Body
                | SchematicColor::BodyFill
                | SchematicColor::MaskedBody
                | SchematicColor::MaskedFill
        )
    }
}
const _: () = assert!(BOARD_SLOTS + BoardColor::ALL.len() <= ColorKey::SLOTS);
