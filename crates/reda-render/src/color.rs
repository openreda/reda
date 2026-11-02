//! Colour schemes: one fixed scheme per app theme, keyed by schematic object kind (and, for
//! PCB Scenes, by Layer kind and side), never by Layer UUID.

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
}

impl ColorKey {
    /// How many colour slots the GPU palette has: every key's [`slot`](Self::slot) is below.
    pub const SLOTS: usize = 128;

    /// The key's index in the GPU palette.
    pub fn slot(self) -> usize {
        match self {
            ColorKey::Schematic(kind) => kind as usize,
        }
    }

    /// Every key, in slot order.
    pub fn all() -> impl Iterator<Item = ColorKey> {
        SchematicColor::ALL.iter().map(|k| ColorKey::Schematic(*k))
    }
}

macro_rules! schematic_colors {
    ($($(#[$doc:meta])* $name:ident: $dark:expr, $light:expr;)*) => {
        /// The kinds of schematic ink, each with its own colour.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub enum SchematicColor {
            $($(#[$doc])* $name,)*
        }

        impl SchematicColor {
            pub const ALL: &[SchematicColor] = &[$(SchematicColor::$name,)*];

            /// The snapshot name.
            pub const fn name(self) -> &'static str {
                match self {
                    $(SchematicColor::$name => stringify!($name),)*
                }
            }

            const fn shipped(self, theme: Theme) -> Rgba {
                match (self, theme) {
                    $(
                        (SchematicColor::$name, Theme::Dark) => $dark,
                        (SchematicColor::$name, Theme::Light) => $light,
                    )*
                }
            }
        }
    };
}

schematic_colors! {
    /// The sheet's paper.
    Paper: Rgba::rgb(0x1c, 0x1d, 0x20), Rgba::rgb(0xfb, 0xfb, 0xf8);
    /// The frame border and title block lines.
    Frame: Rgba::rgb(0x6e, 0x72, 0x7a), Rgba::rgb(0x8a, 0x5a, 0x44);
    /// Title block text.
    FrameText: Rgba::rgb(0xb4, 0xb8, 0xc0), Rgba::rgb(0x5a, 0x3a, 0x2c);
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

    /// The canvas grid's dots.
    pub fn grid(&self) -> Rgba {
        match self.theme {
            Theme::Dark => Rgba([0xb4, 0xb8, 0xc0, 0x80]),
            Theme::Light => Rgba([0x40, 0x44, 0x4c, 0x80]),
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
        }
    }
}

const _: () = assert!(SchematicColor::ALL.len() <= ColorKey::SLOTS);
