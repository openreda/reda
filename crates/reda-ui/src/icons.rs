//! Icons: one SVG per semantic icon key (docs/spec/v0.1/app.md#icons).
//!
//! Commands, Presets, Panels and Document types name an icon by key (`"save"`,
//! `"panel_layers"`); [`ICONS`](table::ICONS) maps each key to its SVG in `icons/`. Each
//! SVG is drawn with the dark-theme colours. On load, [`recolour`] swaps them for the light
//! theme's, and makes the greyscale copy disabled buttons show (the button fades it, once);
//! state otherwise shows on the button, never on the icon. Every icon is REDA's own art,
//! listed with its provenance in `icons/reda.txt`; `cargo xtask icons` regenerates the
//! table from it (see `icons/README.md`).

mod table;

use std::collections::BTreeMap;
use std::sync::OnceLock;

use egui::load::Bytes;
use egui::{Color32, Image, ImageSource, Theme, Vec2};
use reda_engine::model::DocumentKind;

use crate::layout::PanelId;

/// Icon size on toolbars and the Active Bar, in points: the 24 px drawing grid.
pub const TOOLBAR: f32 = 24.0;
/// Icon size in menus.
pub const MENU: f32 = 20.0;
/// Icon size on tabs and Dock strips.
pub const TAB: f32 = 16.0;

/// The Start page tab's icon.
pub const START_PAGE: &str = "home";

/// A Panel's icon key: the one its `panel.show` Preset carries.
pub fn panel_icon(panel: PanelId) -> &'static str {
    match panel {
        PanelId::Projects => "panel_projects",
        PanelId::Properties => "panel_properties",
        PanelId::Messages => "panel_messages",
        PanelId::Library => "panel_library",
        PanelId::Layers => "panel_layers",
        PanelId::Journal => "panel_journal",
        PanelId::Navigator => "panel_navigator",
    }
}

/// The icon key of a Document type, for its tabs.
pub fn document_icon(kind: DocumentKind) -> &'static str {
    match kind {
        DocumentKind::Schematic => "document_schematic",
        DocumentKind::Pcb => "document_pcb",
    }
}

/// Every icon key with art, sorted.
pub fn keys() -> impl Iterator<Item = &'static str> {
    table::ICONS.iter().map(|(key, _)| *key)
}

/// Whether `key` has art.
pub fn has(key: &str) -> bool {
    source(key).is_some()
}

fn source(key: &str) -> Option<&'static [u8]> {
    let i = table::ICONS.binary_search_by(|(k, _)| (*k).cmp(key)).ok()?;
    match &table::ICONS[i].1 {
        ImageSource::Bytes {
            bytes: Bytes::Static(bytes),
            ..
        } => Some(bytes),
        _ => None,
    }
}

/// The style guide's role colours (issue #32): each one's dark-theme value, which the SVGs
/// are drawn with, and its light-theme value.
const ROLES: [(u32, u32); 6] = [
    (0x8a93a0, 0x727984), // existing / context
    (0x2fb36b, 0x228a51), // created, schematic
    (0xe08a2e, 0xac6921), // created, PCB copper
    (0x3d8fd6, 0x347dbc), // created, general
    (0xec4f7a, 0xd2456c), // picked point
    (0xd4c03c, 0x8a7a12), // created, board edge and keepout (not copper)
];

/// The role colours as (dark theme, light theme) pairs: the only colours the icons may use.
pub fn role_colours() -> impl Iterator<Item = (Color32, Color32)> {
    let colour = |c: u32| Color32::from_rgb((c >> 16) as u8, (c >> 8) as u8, c as u8);
    ROLES
        .iter()
        .map(move |(dark, light)| (colour(*dark), colour(*light)))
}

/// How far a disabled icon's greys are pulled toward mid grey.
const DISABLED_PULL: f32 = 0.4;

/// `svg` (drawn with dark-theme colours) for `theme`: on the light theme each listed colour
/// is swapped; when not `enabled` every colour then becomes its luminance pulled toward mid
/// grey. The copy is not faded: the disabled button that shows it fades it, once, by the
/// theme's [`DISABLED_ALPHA`](crate::theme::DISABLED_ALPHA).
pub fn recolour(svg: &str, theme: Theme, enabled: bool) -> String {
    if theme == Theme::Dark && enabled {
        return svg.to_owned();
    }
    let mut out = String::with_capacity(svg.len() + 16);
    let mut rest = svg;
    while let Some(at) = rest.find('#') {
        out.push_str(&rest[..at]);
        let tail = &rest[at + 1..];
        let digits = tail.bytes().take_while(u8::is_ascii_hexdigit).count();
        match parse_hex(&tail[..digits]) {
            Some(colour) => {
                let colour = map_colour(colour, theme, enabled);
                out.push_str(&format!("#{colour:06x}"));
            }
            None => {
                out.push('#');
                out.push_str(&tail[..digits]);
            }
        }
        rest = &tail[digits..];
    }
    out.push_str(rest);
    out
}

/// A colour written as 3 or 6 hex digits.
fn parse_hex(digits: &str) -> Option<u32> {
    match digits.len() {
        6 => u32::from_str_radix(digits, 16).ok(),
        3 => {
            let short = u32::from_str_radix(digits, 16).ok()?;
            let (r, g, b) = ((short >> 8) & 0xf, (short >> 4) & 0xf, short & 0xf);
            Some(((r * 0x11) << 16) | ((g * 0x11) << 8) | (b * 0x11))
        }
        _ => None,
    }
}

fn map_colour(colour: u32, theme: Theme, enabled: bool) -> u32 {
    let colour = match theme {
        Theme::Dark => colour,
        Theme::Light => ROLES
            .iter()
            .find(|(dark, _)| *dark == colour)
            .map_or(colour, |(_, light)| *light),
    };
    if enabled {
        return colour;
    }
    let [r, g, b] = [(colour >> 16) & 0xff, (colour >> 8) & 0xff, colour & 0xff].map(|c| c as f32);
    let luminance = 0.2126 * r + 0.7152 * g + 0.0722 * b;
    let grey = (luminance + DISABLED_PULL * (128.0 - luminance)).round() as u32;
    (grey << 16) | (grey << 8) | grey
}

/// Loaded SVGs by (key, light theme, enabled).
type Loaded = BTreeMap<(&'static str, bool, bool), Vec<u8>>;

/// The four loaded variants of every icon, made once.
fn loaded() -> &'static Loaded {
    static LOADED: OnceLock<Loaded> = OnceLock::new();
    LOADED.get_or_init(|| {
        let mut map = BTreeMap::new();
        for key in keys() {
            let svg = String::from_utf8_lossy(source(key).expect("the table holds bytes"));
            for (theme, light) in [(Theme::Dark, false), (Theme::Light, true)] {
                for enabled in [true, false] {
                    let bytes = recolour(&svg, theme, enabled).into_bytes();
                    map.insert((key, light, enabled), bytes);
                }
            }
        }
        map
    })
}

/// The SVG for `key` on `theme`, the greyscale copy when not `enabled`; `None` without art.
pub fn svg(key: &str, theme: Theme, enabled: bool) -> Option<&'static [u8]> {
    let key = keys().find(|k| *k == key)?;
    loaded()
        .get(&(key, theme == Theme::Light, enabled))
        .map(Vec::as_slice)
}

/// `svg` with every colour made white: a silhouette for [`Image::tint`] to colour, for
/// icons drawn on a coloured ground (the tabs' blue bars) rather than in their role colours.
pub fn silhouette(svg: &str) -> String {
    let mut out = String::with_capacity(svg.len());
    let mut rest = svg;
    while let Some(at) = rest.find('#') {
        out.push_str(&rest[..at]);
        let tail = &rest[at + 1..];
        let digits = tail.bytes().take_while(u8::is_ascii_hexdigit).count();
        out.push('#');
        match parse_hex(&tail[..digits]) {
            Some(_) => out.push_str("ffffff"),
            None => out.push_str(&tail[..digits]),
        }
        rest = &tail[digits..];
    }
    out.push_str(rest);
    out
}

/// The white silhouette of `key` at `size` points, to [`Image::tint`] to one colour;
/// `None` when the key has no art. Drawing it needs [`install_loaders`].
pub fn silhouette_image(key: &str, size: f32) -> Option<Image<'static>> {
    static LOADED: OnceLock<BTreeMap<&'static str, Vec<u8>>> = OnceLock::new();
    let key = keys().find(|k| *k == key)?;
    let loaded = LOADED.get_or_init(|| {
        keys()
            .map(|key| {
                let svg = String::from_utf8_lossy(source(key).expect("the table holds bytes"));
                (key, silhouette(&svg).into_bytes())
            })
            .collect()
    });
    let source = ImageSource::Bytes {
        uri: format!("bytes://reda-icons/silhouette/{key}.svg").into(),
        bytes: Bytes::Static(loaded.get(key)?.as_slice()),
    };
    Some(Image::new(source).fit_to_exact_size(Vec2::splat(size)))
}

/// Lets `ctx` draw icons: installs `egui_extras`' image loaders, which render SVG with
/// resvg. The window does this on its first frame.
pub fn install_loaders(ctx: &egui::Context) {
    egui_extras::install_image_loaders(ctx);
}

/// The image for `key` at `size` points on `theme`, or `None` when the key has no art.
/// Drawing it needs [`install_loaders`].
pub fn image(key: &str, size: f32, theme: Theme, enabled: bool) -> Option<Image<'static>> {
    let bytes = svg(key, theme, enabled)?;
    let variant = match (theme, enabled) {
        (Theme::Dark, true) => "dark",
        (Theme::Dark, false) => "dark-disabled",
        (Theme::Light, true) => "light",
        (Theme::Light, false) => "light-disabled",
    };
    let source = ImageSource::Bytes {
        uri: format!("bytes://reda-icons/{variant}/{key}.svg").into(),
        bytes: Bytes::Static(bytes),
    };
    Some(Image::new(source).fit_to_exact_size(Vec2::splat(size)))
}
