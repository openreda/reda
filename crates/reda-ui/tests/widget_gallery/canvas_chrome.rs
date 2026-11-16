//! The canvas chrome page: Layer chips in every state, the Layer swatch on its floating
//! surface, and the Layer strip along a canvas's bottom.

use egui::{Rect, Sense, Ui, Vec2, vec2};
use reda_ui::theme::TokensExt as _;
use reda_ui::widgets::{self, State};

use crate::kit;

/// The size the page is snapshotted at.
pub const SIZE: Vec2 = vec2(1180.0, 480.0);
/// The width of a state column.
const COLUMN: f32 = 168.0;
/// The size of the stand-in canvas the strip runs along.
const CANVAS: Vec2 = vec2(600.0, 100.0);
/// The Layer strip's height (`bars::draw::LAYER_STRIP_HEIGHT`).
const STRIP: f32 = 24.0;

/// Layers' data colours: the same in both themes.
const RED: (u8, u8, u8) = (0xc8, 0x34, 0x34);
const BLUE: (u8, u8, u8) = (0x3d, 0x8f, 0xd6);
const YELLOW: (u8, u8, u8) = (0xd4, 0xc0, 0x3c);

fn layer(text: &str, (r, g, b): (u8, u8, u8)) -> widgets::Chip {
    widgets::chip(text).swatch_rgb(r, g, b)
}

pub fn page(ui: &mut Ui) {
    let t = ui.tokens();
    ui.spacing_mut().item_spacing = vec2(t.space.section, t.space.section);
    kit::section(ui, "Layer chips", chips);
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            kit::section(ui, "Layer swatch (top right of the canvas)", swatch);
        });
        ui.vertical(|ui| {
            kit::section(ui, "Layer strip (along the canvas bottom)", strip);
        });
    });
}

/// The chip in the five states: a Layer, the active Layer, the swatch alone, and the
/// Layer sets chip that opens a menu.
pub fn chips(ui: &mut Ui) {
    kit::states(
        ui,
        "chips",
        COLUMN,
        &mut [
            ("Layer", &mut |ui, s| {
                ui.add(layer("Bottom Copper", BLUE).forced(s));
            }),
            ("Active Layer", &mut |ui, s| {
                ui.add(layer("Top Copper", RED).selected(true).forced(s));
            }),
            ("Swatch only", &mut |ui, s| {
                ui.add(
                    layer("", RED)
                        .selected(true)
                        .name("Active Layer: Top Copper")
                        .forced(s),
                );
            }),
            ("Opens a menu", &mut |ui, s| {
                ui.add(widgets::chip("Layer sets").chevron().forced(s));
            }),
        ],
    );
}

/// The swatch: the active Layer's chip on the floating surface.
fn swatch(ui: &mut Ui) {
    let t = ui.tokens();
    widgets::floating_frame(&t).show(ui, |ui| {
        ui.add(layer("Top Copper", RED).selected(true));
    });
}

/// The strip at the bottom of a stand-in canvas card.
fn strip(ui: &mut Ui) {
    let t = ui.tokens();
    let (canvas, _) = ui.allocate_exact_size(CANVAS, Sense::hover());
    widgets::paint_card(ui, &t, canvas);
    let rect = Rect::from_min_max(
        canvas.left_bottom() - vec2(0.0, STRIP),
        canvas.right_bottom(),
    );
    widgets::chip_strip(ui, rect, |ui| {
        ui.add(widgets::chip("Layer sets").chevron().forced(State::Rest));
        widgets::chip_rule(ui);
        ui.add(layer("Top Copper", RED).selected(true));
        ui.add(layer("Bottom Copper", BLUE));
        ui.add(layer("Top Silkscreen", YELLOW).forced(State::Hover));
    });
}
