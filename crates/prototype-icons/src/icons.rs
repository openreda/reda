// PROTOTYPE — throwaway, lives only on the `prototype/icon-style` branch.
//
// The two candidate icon styles, as SVG files on a 24 px grid (`icons/<style>/<key>.svg`),
// rendered by egui_extras' resvg loader. State is never a tint of the icon colour: active and
// hover are button backgrounds, and disabled is a greyscale copy of the SVG drawn faded.

use eframe::egui::{self, Color32, CornerRadius, FontId, Align2, Rect, Sense, pos2, vec2};
use std::sync::OnceLock;

pub const KEYS: [&str; 12] =
    ["wire", "net_label", "power_port", "track", "via", "zone", "save", "undo", "zoom_fit", "layers", "select", "move"];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Style {
    Line,
    Depth,
}

impl Style {
    pub const ALL: [Style; 2] = [Style::Line, Style::Depth];
    pub fn label(self) -> &'static str {
        match self {
            Style::Line => "1 · Line, colour by meaning",
            Style::Depth => "2 · Filled with soft depth",
        }
    }
    fn dir(self) -> &'static str {
        match self {
            Style::Line => "line",
            Style::Depth => "depth",
        }
    }
    fn idx(self) -> usize {
        self as usize
    }
}

macro_rules! set {
    ($dir:literal) => {
        [
            include_str!(concat!("../icons/", $dir, "/wire.svg")),
            include_str!(concat!("../icons/", $dir, "/net_label.svg")),
            include_str!(concat!("../icons/", $dir, "/power_port.svg")),
            include_str!(concat!("../icons/", $dir, "/track.svg")),
            include_str!(concat!("../icons/", $dir, "/via.svg")),
            include_str!(concat!("../icons/", $dir, "/zone.svg")),
            include_str!(concat!("../icons/", $dir, "/save.svg")),
            include_str!(concat!("../icons/", $dir, "/undo.svg")),
            include_str!(concat!("../icons/", $dir, "/zoom_fit.svg")),
            include_str!(concat!("../icons/", $dir, "/layers.svg")),
            include_str!(concat!("../icons/", $dir, "/select.svg")),
            include_str!(concat!("../icons/", $dir, "/move.svg")),
        ]
    };
}

static SRC: [[&str; 12]; 2] = [set!("line"), set!("depth")];

/// Greyscale copies: every `#rrggbb` becomes its luminance, pulled toward mid grey.
fn grey_src() -> &'static [[&'static [u8]; 12]; 2] {
    static G: OnceLock<[[&'static [u8]; 12]; 2]> = OnceLock::new();
    G.get_or_init(|| {
        SRC.map(|set| {
            set.map(|svg| {
                let mut out = String::with_capacity(svg.len());
                let b = svg.as_bytes();
                let mut i = 0;
                while i < b.len() {
                    if b[i] == b'#' && i + 7 <= b.len() && b[i + 1..i + 7].iter().all(u8::is_ascii_hexdigit) {
                        let h = |j: usize| u8::from_str_radix(&svg[i + j..i + j + 2], 16).unwrap() as f32;
                        let l = 0.299 * h(1) + 0.587 * h(3) + 0.114 * h(5);
                        let g = (l * 0.6 + 128.0 * 0.4) as u8;
                        out.push_str(&format!("#{g:02x}{g:02x}{g:02x}"));
                        i += 7;
                    } else {
                        out.push(b[i] as char);
                        i += 1;
                    }
                }
                &*Box::leak(out.into_bytes().into_boxed_slice())
            })
        })
    })
}

pub fn image(style: Style, key: &str, px: f32, greyed: bool) -> egui::Image<'static> {
    let i = KEYS.iter().position(|k| *k == key).unwrap_or_else(|| panic!("no icon {key}"));
    let (uri, bytes): (String, &'static [u8]) = if greyed {
        (format!("bytes://{}/{key}.grey.svg", style.dir()), grey_src()[style.idx()][i])
    } else {
        (format!("bytes://{}/{key}.svg", style.dir()), SRC[style.idx()][i].as_bytes())
    };
    egui::Image::from_bytes(uri, bytes).fit_to_exact_size(vec2(px, px))
}

pub fn key_for(cmd: &str) -> Option<&'static str> {
    Some(match cmd {
        "schematic.wire.place" => "wire",
        "schematic.net_label.place" => "net_label",
        "schematic.power_port.place" => "power_port",
        "pcb.track.place" | "pcb.route.interactive" => "track",
        "pcb.via.place" => "via",
        "pcb.zone.place" => "zone",
        "file.document.save" => "save",
        "edit.undo" => "undo",
        "view.fit" => "zoom_fit",
        "view.layers" => "layers",
        "edit.select.rect" => "select",
        "edit.selection.move" => "move",
        _ => return None,
    })
}

/// An icon-only button: accent background when active, hover background on hover, a faded
/// greyscale icon when disabled.
pub fn button(ui: &mut egui::Ui, style: Style, key: &str, px: f32, active: bool, enabled: bool, tip: &str) -> egui::Response {
    let pad = if px >= 24.0 { 4.0 } else { 3.0 };
    let (rect, resp) = ui.allocate_exact_size(vec2(px + 2.0 * pad, px + 2.0 * pad), if enabled { Sense::click() } else { Sense::hover() });
    let v = ui.visuals();
    let fill = if active {
        v.selection.bg_fill
    } else if enabled && resp.hovered() {
        v.widgets.hovered.bg_fill
    } else {
        Color32::TRANSPARENT
    };
    ui.painter().rect_filled(rect, CornerRadius::same(3), fill);
    let mut img = image(style, key, px, !enabled);
    if !enabled {
        img = img.tint(Color32::from_white_alpha(120));
    }
    img.paint_at(ui, Rect::from_center_size(rect.center(), vec2(px, px)));
    resp.on_hover_text(tip)
}

/// Every icon in every state, size and theme, painted side by side on theme backgrounds.
pub fn sheet(ui: &mut egui::Ui, style: Style) {
    const SIZES: [f32; 3] = [16.0, 20.0, 24.0];
    const STATES: [&str; 4] = ["norm", "hover", "active", "off"];
    let cell = 34.0;
    let block_w = SIZES.len() as f32 * STATES.len() as f32 * cell + 12.0;
    let head = 36.0;
    let w = 2.0 * block_w + 90.0;
    let h = head + KEYS.len() as f32 * cell + 8.0;
    let (r, _) = ui.allocate_exact_size(vec2(w, h), Sense::hover());
    let p = ui.painter();
    let rgb = |x: u32| Color32::from_rgb((x >> 16) as u8, (x >> 8) as u8, x as u8);
    let themes = [
        ("REDA dark", rgb(0x1d2024), rgb(0x343a41), rgb(0x3d8fd6).gamma_multiply(0.55), rgb(0xc4cad3)),
        ("REDA light", rgb(0xeceef1), rgb(0xd2d7de), rgb(0x1f6fb8).gamma_multiply(0.35), rgb(0x2a2f36)),
    ];
    let text = ui.visuals().text_color();
    for (row, key) in KEYS.iter().enumerate() {
        p.text(pos2(r.left(), r.top() + head + row as f32 * cell + cell / 2.0), Align2::LEFT_CENTER, *key, FontId::proportional(11.0), text);
    }
    for (t, (name, bg, hover, active, fg)) in themes.iter().enumerate() {
        let x0 = r.left() + 90.0 + t as f32 * block_w;
        let block = Rect::from_min_size(pos2(x0, r.top()), vec2(block_w - 8.0, h));
        p.rect_filled(block, CornerRadius::same(4), *bg);
        p.text(pos2(x0 + 6.0, r.top() + 4.0), Align2::LEFT_TOP, *name, FontId::proportional(11.0), *fg);
        for (si, px) in SIZES.iter().enumerate() {
            for (st, sname) in STATES.iter().enumerate() {
                let cx = x0 + 6.0 + (si * STATES.len() + st) as f32 * cell + cell / 2.0;
                let label = if st == 0 { format!("{px} {sname}") } else { sname.to_string() };
                p.text(pos2(cx, r.top() + 20.0), Align2::CENTER_TOP, label, FontId::proportional(9.0), *fg);
                for (row, key) in KEYS.iter().enumerate() {
                    let c = pos2(cx, r.top() + head + row as f32 * cell + cell / 2.0);
                    let btn = Rect::from_center_size(c, vec2(px + 6.0, px + 6.0));
                    match st {
                        1 => { p.rect_filled(btn, CornerRadius::same(3), *hover); }
                        2 => { p.rect_filled(btn, CornerRadius::same(3), *active); }
                        _ => {}
                    }
                    let mut img = image(style, key, *px, st == 3);
                    if st == 3 {
                        img = img.tint(Color32::from_white_alpha(120));
                    }
                    img.paint_at(ui, Rect::from_center_size(c, vec2(*px, *px)));
                }
            }
        }
    }
}
