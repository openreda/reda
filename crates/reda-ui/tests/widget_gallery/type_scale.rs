//! The type page: the type scale with each style's size and line height, the two bundled
//! typefaces in every weight the scale uses, and the scale at both ends of the text-size
//! range beside the default.
//!
//! Everything is read from the tokens: the styles from `Type` (destructured exhaustively,
//! so a new style must be added here), the faces from `theme::fonts`, the sizes from
//! `Settings::TEXT_SIZES`.

use egui::{FontFamily, Sense, Ui, Vec2, vec2};
use reda_ui::theme::fonts::{self, Weight};
use reda_ui::theme::{Settings, Text, Tokens, TokensExt as _, Type};

use crate::kit;

/// The size the page is snapshotted at.
pub const SIZE: Vec2 = vec2(1200.0, 840.0);
/// The width of the caption column before a sample.
const CAPTION: f32 = 168.0;
/// The width of one text size's column.
const SIZE_COLUMN: f32 = 360.0;

/// The type scale in the order of the spec's table, with a sample for each.
fn styles(ty: &Type) -> [(&'static str, &Text, &'static str); 8] {
    let Type {
        small,
        body,
        body_strong,
        title,
        heading,
        mono,
        numeric,
        numeric_small,
    } = ty;
    [
        ("type.heading", heading, "Board Setup"),
        ("type.title", title, "Design Rules"),
        ("type.body_strong", body_strong, "usb_c_mcu.pcb"),
        ("type.body", body, "Clearance between copper"),
        ("type.small", small, "Top Copper · 0.25 mm"),
        ("type.numeric", numeric, "X 121.920  Y 48.260"),
        ("type.numeric_small", numeric_small, "12 of 140"),
        ("type.mono", mono, "pcb.zone.fill {\"all\": true}"),
    ]
}

pub fn page(ui: &mut Ui) {
    let t = ui.tokens();
    ui.spacing_mut().item_spacing = vec2(t.space.section, t.space.section);
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| kit::section(ui, "The type scale", |ui| scale(ui, &t, true)));
        ui.vertical(|ui| kit::section(ui, "Typefaces and weights", faces));
    });
    kit::section(ui, "Text sizes", sizes);
}

/// Each style of `tokens` with its size and line height, the sample drawn on a tint the
/// height of the line box. `named` puts the token's name in the caption.
fn scale(ui: &mut Ui, tokens: &Tokens, named: bool) {
    let t = ui.tokens();
    ui.spacing_mut().item_spacing.y = t.space.stack;
    for (name, style, sample) in styles(&tokens.ty) {
        ui.horizontal(|ui| {
            let caption = if named {
                format!("{name}  {}/{}", style.size, style.line)
            } else {
                format!("{}/{}", style.size, style.line)
            };
            let width = if named { CAPTION } else { CAPTION / 3.0 };
            let (rect, _) = ui.allocate_exact_size(vec2(width, style.line), Sense::hover());
            let small = ui
                .painter()
                .layout_job(t.ty.small.job(&caption, t.color.text.muted));
            let at = rect.left_center() - vec2(0.0, small.size().y / 2.0);
            ui.painter().galley(at, small, t.color.text.muted);
            line_box(ui, style, sample);
        });
    }
}

/// `sample` in `style`, on a tint the size of its line box.
fn line_box(ui: &mut Ui, style: &Text, sample: &str) {
    let t = ui.tokens();
    let galley = ui
        .painter()
        .layout_job(style.job(sample, t.color.text.primary));
    let (rect, _) = ui.allocate_exact_size(galley.size(), Sense::hover());
    ui.painter().rect_filled(rect, 0.0, t.color.fill.hover);
    ui.painter().galley(rect.min, galley, t.color.text.primary);
}

/// The bundled UI face in its three weights and with tabular digits, and the bundled
/// monospace face, at the body size.
fn faces(ui: &mut Ui) {
    let t = ui.tokens();
    ui.spacing_mut().item_spacing.y = t.space.stack;
    const SAMPLE: &str = "Hamburgefonstiv 0123456789";
    let named = |family: &str| FontFamily::Name(family.into());
    let mut rows: Vec<(String, FontFamily, &str)> = Weight::ALL
        .into_iter()
        .map(|w| {
            let name = format!("{} {w:?} {}", fonts::BUNDLED_UI, w.class());
            (name, named(w.family()), SAMPLE)
        })
        .collect();
    let digits = format!("{} tabular digits", fonts::BUNDLED_UI);
    rows.push((digits, named(fonts::NUMERIC), "111.111  121.920  888.888"));
    let mono = format!("{} Regular 400", fonts::BUNDLED_MONO);
    rows.push((mono, named(fonts::MONO), "0O 1lI| {}[]() => != <="));
    for (name, family, sample) in rows {
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(vec2(CAPTION, t.ty.body.line), Sense::hover());
            let small = ui
                .painter()
                .layout_job(t.ty.small.job(&name, t.color.text.muted));
            let at = rect.left_center() - vec2(0.0, small.size().y / 2.0);
            ui.painter().galley(at, small, t.color.text.muted);
            let style = Text {
                family,
                ..t.ty.body.clone()
            };
            line_box(ui, &style, sample);
        });
    }
}

/// The scale at the smallest, the default and the largest text size, in this page's
/// theme and Density, with the control and row heights each gives.
fn sizes(ui: &mut Ui) {
    let t = ui.tokens();
    let (smallest, largest) = Settings::TEXT_SIZES.into_inner();
    ui.horizontal_top(|ui| {
        for size in [smallest, Settings::DEFAULT_TEXT_SIZE, largest] {
            let settings = Settings {
                density: t.density,
                text_size: size,
                ..Settings::default()
            };
            let at = Tokens::new(t.theme, &settings);
            ui.vertical(|ui| {
                ui.set_width(SIZE_COLUMN);
                let heading = format!(
                    "Body {size} px: control {}, row {}, tab {}",
                    at.control_height(),
                    at.row_height(),
                    at.tab_height()
                );
                kit::caption(ui, &heading);
                scale(ui, &at, false);
            });
        }
    });
}
