//! PROTOTYPE (issue #433) · variant D, "Same canvas plus". Throwaway.
//!
//! Added after the first reaction: A's tilt in and out, 2D mouse and material colours,
//! with three things from B moved to where they do not cover the Active Bar. The 2D | 3D
//! switch sits at the canvas's top left in both modes. A "3D" chip takes the place the
//! active Layer's chip has in 2D, and opens a menu holding what B had in its Show card
//! and its toolbar: a switch and an opacity for each kind of thing, the solder mask, the
//! projection, the views and Measure. Nothing is at the bottom.
//!
//! After the second reaction: an axis gizmo, as 3D tools have, in place of B's cube; a
//! left drag turns the board; and the measurement adds its X, Y and Z parts.

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

/// The 2D mouse (wheel zooms at the pointer, a right or middle drag pans, Shift turns
/// that drag), and since nothing is edited here a left drag turns the board too.
pub fn navigate(ui: &egui::Ui, s: &mut State, cx: &Cx<'_>) {
    super::variant_a::navigate(ui, s, cx);
    if cx.response.dragged_by(egui::PointerButton::Primary) {
        let d = cx.response.drag_delta();
        s.cam.orbit(-d.x * 0.008, d.y * 0.008, None);
    }
}

/// The orientation gizmo, under the chip: the board's three axes as the view sees them,
/// as 3D tools draw theirs. A click on an end looks along that axis; a drag on the gizmo
/// turns the board.
fn gizmo(ui: &egui::Ui, s: &mut State, cx: &Cx<'_>) -> Option<Preset> {
    use super::camera::dot;
    let center = cx.rect.right_top() + vec2(-58.0, 104.0);
    let reach = 34.0;
    let area = egui::Rect::from_center_size(center, vec2(100.0, 100.0));
    let id = egui::Id::unique("proto-3d-gizmo");
    let response = ui.interact(area, id, egui::Sense::click_and_drag());
    if response.dragged() {
        let d = response.drag_delta();
        s.cam.orbit(-d.x * 0.02, d.y * 0.02, None);
    }
    let painter = ui.painter_at(cx.rect);
    let visuals = ui.visuals();
    if response.hovered() || response.dragged() {
        painter.circle_filled(center, 50.0, visuals.window_fill.gamma_multiply(0.6));
    }
    let (right, up, fwd) = s.cam.basis();
    // theme-exempt: the axis colours 3D tools share.
    let (red, green, blue) = (
        egui::Color32::from_rgb(0xe2, 0x4b, 0x4a),
        egui::Color32::from_rgb(0x5f, 0xb0, 0x3c),
        egui::Color32::from_rgb(0x3d, 0x85, 0xe0),
    );
    let mut ends = [
        ([1.0, 0.0, 0.0], "X", red, true, Preset::Right),
        ([-1.0, 0.0, 0.0], "X", red, false, Preset::Left),
        ([0.0, 1.0, 0.0], "Y", green, true, Preset::Back),
        ([0.0, -1.0, 0.0], "Y", green, false, Preset::Front),
        ([0.0, 0.0, 1.0], "Z", blue, true, Preset::Top),
        ([0.0, 0.0, -1.0], "Z", blue, false, Preset::Bottom),
    ];
    // Far ends first, so near ones draw over them.
    ends.sort_by(|a, b| dot(b.0, fwd).total_cmp(&dot(a.0, fwd)));
    let pointer = response.hover_pos();
    let mut clicked = None;
    for (axis, letter, color, positive, preset) in ends {
        let at = center + vec2(dot(axis, right), -dot(axis, up)) * reach;
        let near = 1.0 - 0.35 * (dot(axis, fwd) + 1.0) / 2.0;
        let color = color.gamma_multiply(near);
        let radius = if positive { 9.0 } else { 6.5 };
        let hot = pointer.is_some_and(|p| p.distance(at) <= radius + 2.0);
        if positive {
            painter.line_segment([center, at], egui::Stroke::new(2.0, color));
            painter.circle_filled(at, radius, color);
        } else {
            painter.circle_filled(at, radius, visuals.window_fill);
            painter.circle_stroke(at, radius, egui::Stroke::new(1.5, color));
        }
        if hot {
            painter.circle_stroke(
                at,
                radius + 2.0,
                egui::Stroke::new(1.5, visuals.strong_text_color()),
            );
        }
        if positive || hot {
            let ink = if positive {
                egui::Color32::BLACK
            } else {
                visuals.text_color()
            };
            let text = if positive {
                letter.to_owned()
            } else {
                format!("-{letter}")
            };
            let font = egui::FontId::proportional(if positive { 11.0 } else { 9.0 });
            painter.text(at, Align2::CENTER_CENTER, text, font, ink);
        }
        if hot && response.clicked() {
            clicked = Some(preset);
        }
    }
    clicked
}

/// The measurement: the span on the board, its length, and under it how far apart the
/// ends are along the board's X and Y and in height.
fn measure(ui: &egui::Ui, s: &State, cx: &Cx<'_>) {
    let painter = ui.painter_at(cx.rect);
    let ink = ui.visuals().strong_text_color();
    let Some((a, b, _)) = super::span(s) else {
        if let Some(p) = s.measure.hover.and_then(|h| s.cam.project(cx.rect, h)) {
            painter.circle_stroke(p, 4.0, egui::Stroke::new(1.5, ink));
        }
        return;
    };
    let (Some(pa), Some(pb)) = (s.cam.project(cx.rect, a), s.cam.project(cx.rect, b)) else {
        return;
    };
    painter.line_segment([pa, pb], egui::Stroke::new(1.5, ink));
    for p in [pa, pb] {
        painter.circle_filled(p, 3.0, ink);
    }
    let big = painter.layout_no_wrap(
        super::mm(super::distance(a, b)),
        egui::FontId::proportional(14.0),
        ink,
    );
    let d = [0, 1, 2].map(|k| (b[k] - a[k]).abs());
    let parts = format!("X {:.3}   Y {:.3}   Z {:.3}", d[0], d[1], d[2]);
    let small = painter.layout_no_wrap(
        parts,
        egui::FontId::monospace(10.5),
        ui.visuals().text_color(),
    );
    let size = vec2(
        big.size().x.max(small.size().x),
        big.size().y + small.size().y + 2.0,
    );
    let back = egui::Rect::from_center_size(pa + (pb - pa) / 2.0, size + vec2(14.0, 10.0));
    painter.rect_filled(back, 5.0, ui.visuals().window_fill);
    painter.rect_stroke(
        back,
        5.0,
        ui.visuals().window_stroke,
        egui::StrokeKind::Inside,
    );
    let top = back.min + vec2(7.0, 5.0);
    let below = top + vec2(0.0, big.size().y + 2.0);
    painter.galley(top, big, ink);
    painter.galley(below, small, ink);
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

    if let Some(preset) = gizmo(ui, s, cx) {
        actions.push(Action::View(preset));
    }
    measure(ui, s, cx);
    actions
}
