//! PROTOTYPE (issue #433) · variant D, "Same canvas plus". Throwaway.
//!
//! Added after the first reaction: A's tilt in and out, 2D mouse and material colours,
//! with three things from B moved to where they do not cover the Active Bar. The 2D | 3D
//! switch sits at the canvas's top left in both modes. A "3D" chip takes the place the
//! active Layer's chip has in 2D, and opens a menu holding what B had in its Show card
//! and its toolbar: a switch and an opacity for each kind of thing, the solder mask, the
//! projection, the views and Measure. B's view cube sits under the chip. Nothing is at
//! the bottom.

use egui::{Align2, Order, vec2};

use super::mesh::{BOARD, BODY, COPPER, HOLE, INNER, SILK};
use super::{Action, Cx, Preset, State};
use crate::bars::draw::ACTIVE_BAR_TOP;
use crate::theme::TokensExt as _;
use crate::widgets;

/// How far the corner controls sit from the canvas's sides, as the Layer chip does.
const INSET: f32 = 8.0;

pub fn defaults(s: &mut State) {
    super::variant_a::defaults(s);
}

/// The 2D | 3D switch at the canvas's top left, clear of the Active Bar. Returns whether
/// the other mode was chosen.
pub fn mode_switch(ui: &egui::Ui, rect: egui::Rect, in_3d: bool) -> bool {
    let t = ui.tokens();
    let mut mode = in_3d;
    egui::Area::new(egui::Id::unique("proto-3d-mode-d"))
        .order(Order::Middle)
        .fixed_pos(rect.left_top() + vec2(INSET, ACTIVE_BAR_TOP))
        .pivot(Align2::LEFT_TOP)
        .show(ui.ctx(), |ui| {
            widgets::floating_frame(&t).show(ui, |ui| {
                ui.add(widgets::segmented(
                    &mut mode,
                    &[(false, "2D"), (true, "3D")],
                ));
            });
        });
    mode != in_3d
}

pub fn chrome(ui: &egui::Ui, s: &mut State, cx: &Cx<'_>) -> Vec<Action> {
    let mut actions = Vec::new();
    let rect = cx.rect;
    if mode_switch(ui, rect, true) {
        actions.push(Action::Leave);
    }
    let t = ui.tokens();

    // The 3D chip, where the active Layer's chip is in 2D, and its menu.
    egui::Area::new(egui::Id::unique("proto-3d-chip-d"))
        .order(Order::Middle)
        .fixed_pos(rect.right_top() + vec2(-INSET, ACTIVE_BAR_TOP))
        .pivot(Align2::RIGHT_TOP)
        .show(ui.ctx(), |ui| {
            widgets::floating_frame(&t).show(ui, |ui| {
                let face = ui.add(widgets::chip("3D view"));
                widgets::menu(&face).width(250.0).stay_open().show(|ui| {
                    let heading = |ui: &mut egui::Ui, text: &str| {
                        ui.label(t.ty.small.rich(text).color(t.color.text.muted));
                    };
                    heading(ui, "Show");
                    for (cat, name) in [
                        (BODY, "Bodies"),
                        (SILK, "Silkscreen"),
                        (COPPER, "Copper"),
                        (BOARD, "Board"),
                    ] {
                        ui.horizontal(|ui| {
                            let alpha = &mut s.alpha[cat as usize];
                            let mut shown = *alpha > 0.0;
                            if ui.checkbox(&mut shown, "").changed() {
                                *alpha = if shown { 1.0 } else { 0.0 };
                            }
                            ui.add_sized(
                                [80.0, 18.0],
                                egui::Label::new(name).halign(egui::Align::Min),
                            );
                            ui.spacing_mut().slider_width = 90.0;
                            ui.add(egui::Slider::new(alpha, 0.0..=1.0).show_value(false));
                        });
                    }
                    // Inner copper and hole walls follow the board they are in.
                    let board = s.alpha[BOARD as usize];
                    s.alpha[INNER as usize] = if board < 0.999 { 1.0 } else { 0.0 };
                    s.alpha[HOLE as usize] = board;
                    ui.checkbox(&mut s.look.mask, "Solder mask");
                    widgets::menu_separator(ui);
                    heading(ui, "Projection");
                    let options = [(true, "Orthographic"), (false, "Perspective")];
                    ui.add(widgets::segmented(&mut s.cam.ortho, &options));
                    widgets::menu_separator(ui);
                    heading(ui, "View");
                    if ui.add(widgets::menu_item("Fit").shortcut("Home")).clicked() {
                        actions.push(Action::Fit);
                    }
                    for (preset, name, key) in Preset::ALL {
                        if ui.add(widgets::menu_item(name).shortcut(key)).clicked() {
                            actions.push(Action::View(preset));
                        }
                    }
                    widgets::menu_separator(ui);
                    let measure = widgets::menu_item("Measure").shortcut("M");
                    if ui.add(measure.checked(s.measure.on)).clicked() {
                        actions.push(Action::Measure);
                    }
                    if ui
                        .add(widgets::menu_item("Back to 2D").shortcut("2"))
                        .clicked()
                    {
                        actions.push(Action::Leave);
                    }
                });
            });
        });

    // The orientation cube, under the chip.
    if let Some(preset) = super::variant_b::view_cube(ui, s, cx, vec2(-66.0, 118.0)) {
        actions.push(Action::View(preset));
    }
    super::variant_a::measure(ui, s, cx);
    actions
}
