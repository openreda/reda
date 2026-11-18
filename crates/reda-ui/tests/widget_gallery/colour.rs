//! The colour page: every colour token as a labelled swatch, one card per family
//! (surface, text, stroke, fill, the accent and the three semantic tones, the icon role
//! colours, and the two that stand alone).
//!
//! The swatches come from `Colors::named`, whose destructuring is exhaustive, so a new
//! colour token shows here without touching the page.

use egui::{Color32, Sense, Stroke, StrokeKind, Ui, Vec2, vec2};
use reda_ui::theme::TokensExt as _;

use crate::kit;

/// The size the page is snapshotted at.
pub const SIZE: Vec2 = vec2(1420.0, 600.0);
/// The width of what a family's card holds.
const CARD: f32 = 188.0;
/// A swatch's size.
const SWATCH: Vec2 = vec2(44.0, 28.0);

/// The card of the tokens whose names have no family (`touch`, `close_hover`).
const ALONE: &str = "standalone";
/// The strengths of a tone (the accent and the semantic colours), the ends of its tokens'
/// names.
const STRENGTHS: [&str; 4] = [".text", ".subtle", ".solid", ".on_solid"];

/// The neutral families in one row, the tones in strengths below them.
pub fn page(ui: &mut Ui) {
    let t = ui.tokens();
    ui.spacing_mut().item_spacing = vec2(t.space.section, t.space.section);
    let (tones, neutral): (Vec<Family>, Vec<Family>) = families(&t.color.named())
        .into_iter()
        .partition(|(_, tokens)| {
            (tokens.iter()).all(|(name, _)| STRENGTHS.iter().any(|s| name.ends_with(s)))
        });
    for row in [neutral, tones] {
        ui.horizontal_top(|ui| {
            for (family, tokens) in row {
                ui.vertical(|ui| {
                    kit::section(ui, family, |ui| {
                        ui.set_width(CARD);
                        swatches(ui, &tokens);
                    });
                });
            }
        });
    }
}

type Family = (&'static str, Vec<(&'static str, Color32)>);

/// The tokens grouped by the part of their name before the dot, in the order the
/// families first appear.
fn families(named: &[(&'static str, Color32)]) -> Vec<Family> {
    let mut families: Vec<Family> = Vec::new();
    for &(name, colour) in named {
        let family = name.split_once('.').map_or(ALONE, |(family, _)| family);
        match families.iter_mut().find(|(f, _)| *f == family) {
            Some((_, tokens)) => tokens.push((name, colour)),
            None => families.push((family, vec![(name, colour)])),
        }
    }
    families
}

/// One swatch per token, on the card: a translucent token shows what it does over it.
fn swatches(ui: &mut Ui, tokens: &[(&str, Color32)]) {
    let t = ui.tokens();
    ui.spacing_mut().item_spacing.y = t.space.stack;
    for &(name, colour) in tokens {
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(SWATCH, Sense::hover());
            let painter = ui.painter();
            painter.rect_filled(rect, t.radius.row, colour);
            let outline = Stroke::new(t.stroke.hairline, t.color.stroke.hairline);
            painter.rect_stroke(rect, t.radius.row, outline, StrokeKind::Inside);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                ui.label(t.ty.small.rich(name).color(t.color.text.primary));
                ui.label(
                    t.ty.numeric_small
                        .rich(hex(colour))
                        .color(t.color.text.muted),
                );
            });
        });
    }
}

/// `#rrggbb`, or a translucent colour's opacity: egui keeps colours premultiplied in
/// 8 bits, so a faint tint's own hue cannot be read back exactly (the spec's tables name
/// it).
fn hex(c: Color32) -> String {
    let [r, g, b, a] = c.to_srgba_unmultiplied();
    if a == 255 {
        format!("#{r:02x}{g:02x}{b:02x}")
    } else {
        format!("{:.0} % opacity", f32::from(a) / 2.55)
    }
}
