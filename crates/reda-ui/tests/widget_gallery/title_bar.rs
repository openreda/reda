//! The Title bar page: the window controls and the command field in every state, and the
//! slim bar of a floating window, active, in the background, maximised and with room for
//! macOS's traffic lights.

use egui::{Frame, Stroke, Ui, Vec2, vec2};
use reda_ui::theme::TokensExt as _;
use reda_ui::widgets::{self, State, WindowAction};

use crate::kit;

/// The size the page is snapshotted at.
pub const SIZE: Vec2 = vec2(1400.0, 1040.0);
/// The width of a state column.
const COLUMN: f32 = 180.0;
/// The width of a floating window drawn whole.
const FLOATING: f32 = 360.0;

pub fn page(ui: &mut Ui) {
    let t = ui.tokens();
    ui.spacing_mut().item_spacing = vec2(t.space.section, t.space.section);
    kit::section(ui, "Window controls", controls);
    kit::section(ui, "Command field", field);
    kit::section(ui, "Floating window's bar", floating);
}

fn controls(ui: &mut Ui) {
    let control = |action: WindowAction, focused: bool| {
        move |ui: &mut Ui, s: State| {
            ui.add(
                widgets::window_control(action)
                    .window_focused(focused)
                    .forced(s),
            );
        }
    };
    let (mut min, mut max, mut restore, mut close, mut behind) = (
        control(WindowAction::Minimise, true),
        control(WindowAction::Maximise, true),
        control(WindowAction::Restore, true),
        control(WindowAction::Close, true),
        control(WindowAction::Close, false),
    );
    kit::states(
        ui,
        "window-controls",
        COLUMN,
        &mut [
            ("Minimise", &mut min),
            ("Maximise", &mut max),
            ("Restore", &mut restore),
            ("Close", &mut close),
            ("Close, window in the background", &mut behind),
        ],
    );
}

fn field(ui: &mut Ui) {
    let width = COLUMN;
    kit::states(
        ui,
        "command-field",
        COLUMN,
        &mut [
            ("Project and Document", &mut |ui, s| {
                ui.add(
                    widgets::command_field("usb_c_mcu — Main board")
                        .width(width)
                        .window_focused(true)
                        .forced(s),
                );
            }),
            ("No Project open", &mut |ui, s| {
                ui.add(
                    widgets::command_field("Search commands")
                        .hint(true)
                        .width(width)
                        .window_focused(true)
                        .forced(s),
                );
            }),
            ("Window in the background", &mut |ui, s| {
                ui.add(
                    widgets::command_field("usb_c_mcu — Main board")
                        .width(width)
                        .window_focused(false)
                        .forced(s),
                );
            }),
        ],
    );
    let t = ui.tokens();
    ui.add_space(t.space.stack);
    kit::row(ui, "Widest, with its key", COLUMN, |ui| {
        ui.add(
            widgets::command_field("usb_c_mcu — Main board")
                .shortcut("Ctrl+P")
                .window_focused(true),
        );
    });
    kit::row(ui, "Narrowest, a long title", COLUMN, |ui| {
        ui.add(
            widgets::command_field("power_supply_rev_c — Regulator and protection")
                .shortcut("Ctrl+P")
                .width(widgets::COMMAND_FIELD_MIN)
                .window_focused(true),
        );
    });
}

/// A floating window's top: its bar on the base, over the edge of a card.
fn window(ui: &mut Ui, bar: widgets::FloatingBar) {
    let t = ui.tokens();
    Frame::new()
        .fill(t.color.surface.base)
        .stroke(Stroke::new(t.stroke.hairline, t.color.stroke.hairline))
        .corner_radius(t.radius.window)
        .show(ui, |ui| {
            ui.set_width(FLOATING);
            bar.show(ui);
            Frame::new()
                .outer_margin(egui::Margin {
                    left: widgets::CARD_GAP as i8,
                    right: widgets::CARD_GAP as i8,
                    top: 0,
                    bottom: widgets::CARD_GAP as i8,
                })
                .show(ui, |ui| {
                    let (rect, _) = ui.allocate_exact_size(
                        vec2(ui.available_width(), t.control_height() * 2.0),
                        egui::Sense::hover(),
                    );
                    widgets::paint_card(ui, &t, rect);
                });
        });
}

fn floating(ui: &mut Ui) {
    let title = "Layers — usb_c_mcu";
    let hot = [State::Rest, State::Rest, State::Hover];
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            kit::caption(ui, "Active, close hovered");
            window(
                ui,
                widgets::floating_bar(title)
                    .window_focused(true)
                    .forced(hot),
            );
        });
        ui.vertical(|ui| {
            kit::caption(ui, "In the background");
            window(ui, widgets::floating_bar(title).window_focused(false));
        });
        ui.vertical(|ui| {
            kit::caption(ui, "Maximised, maximise hovered: the restore Glyph");
            let hot = [State::Rest, State::Hover, State::Rest];
            let bar = widgets::floating_bar(title)
                .maximised(true)
                .window_focused(true);
            window(ui, bar.forced(hot));
        });
    });
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            kit::caption(
                ui,
                "macOS: the native title-bar height, room for the traffic lights, no REDA controls",
            );
            let bar = widgets::floating_bar(title)
                .height(reda_ui::title_bar::macos::NATIVE_TITLE_BAR_HEIGHT)
                .controls(false)
                .leading_inset(reda_ui::title_bar::macos::TRAFFIC_LIGHTS_INSET)
                .window_focused(true);
            window(ui, bar);
        });
    });
}
