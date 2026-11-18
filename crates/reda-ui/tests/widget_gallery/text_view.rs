//! The text view page: a long text to read and copy (the About box's third-party
//! notices), in every state, in the monospace and the body style.

use egui::{Ui, Vec2, vec2};
use reda_ui::theme::TokensExt as _;
use reda_ui::widgets::{self, State};

use crate::kit;

/// The size the page is snapshotted at.
pub const SIZE: Vec2 = vec2(1180.0, 380.0);
/// The width of a state column.
const COLUMN: f32 = 168.0;

const NOTICE: &str = "MIT License\nCopyright (c)\nthe egui authors";
const PROSE: &str = "Read-only text,\nselectable to copy.";

pub fn page(ui: &mut Ui) {
    let t = ui.tokens();
    ui.spacing_mut().item_spacing = vec2(t.space.section, t.space.section);
    kit::section(ui, "Text view", views);
}

/// The view in the five states, in the monospace style and the body style.
pub fn views(ui: &mut Ui) {
    let view = |ui: &mut Ui, text: &str, mono: bool, s: State| {
        ui.vertical(|ui| {
            ui.set_width(COLUMN);
            let view = widgets::text_view(text).forced(s);
            ui.add(if mono { view.mono() } else { view });
        });
    };
    // One grid per style: a grid row takes its height from the frame before, and these
    // rows are taller than a control.
    kit::states(
        ui,
        "text-views-mono",
        COLUMN,
        &mut [("Monospace", &mut |ui, s| view(ui, NOTICE, true, s))],
    );
    kit::states(
        ui,
        "text-views-body",
        COLUMN,
        &mut [("Body", &mut |ui, s| view(ui, PROSE, false, s))],
    );
}
