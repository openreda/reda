//! PROTOTYPE (issue #433) · variant B, "Viewer". Throwaway.
//!
//! 3D is a studio you step into. A 2D | 3D switch sits on the canvas in both modes. The
//! view opens in perspective, three-quarter on, against a neutral backdrop. Nothing is
//! edited here, so the left button turns the board; right or middle pans. A view cube
//! picks the faces, a Show card fades each kind of thing, a toolbar holds the rest, and a
//! selected Component carries a tag.

use egui::PointerButton;

use super::camera::{V3, cross, dot};
use super::mesh::{BOARD, BODY, COPPER, HOLE, INNER, SILK};
use super::{Action, Cx, Preset, State, card, distance, lifted, mm, span, wheel};

pub fn defaults(s: &mut State) {
    s.look = super::Look {
        realistic: true,
        mask: true,
    };
    s.cam.ortho = false;
}

pub fn navigate(ui: &egui::Ui, s: &mut State, cx: &Cx<'_>) {
    let r = cx.response;
    if let Some(pos) = r.hover_pos() {
        let k = wheel(ui);
        if k != 1.0 {
            s.cam.zoom(cx.rect, pos, k);
        }
    }
    if r.dragged_by(PointerButton::Primary) {
        let d = r.drag_delta();
        s.cam.orbit(-d.x * 0.008, d.y * 0.008, None);
    }
    if r.dragged_by(PointerButton::Secondary) || r.dragged_by(PointerButton::Middle) {
        s.cam.pan(cx.rect, r.drag_delta());
    }
}

/// The 2D | 3D switch at the top of the canvas. Returns whether the other mode was
/// clicked.
pub fn mode_switch(ui: &egui::Ui, rect: egui::Rect, in_3d: bool) -> bool {
    let at = rect.center_top() + egui::vec2(0.0, 12.0);
    card(ui, "mode", egui::Align2::CENTER_TOP, at, |ui| {
        ui.horizontal(|ui| {
            let to_2d = ui.selectable_label(!in_3d, "2D").clicked();
            let to_3d = ui.selectable_label(in_3d, "3D").clicked();
            if in_3d { to_2d } else { to_3d }
        })
        .inner
    })
}

pub fn chrome(ui: &egui::Ui, s: &mut State, cx: &Cx<'_>) -> Vec<Action> {
    let mut actions = Vec::new();
    let rect = cx.rect;
    if mode_switch(ui, rect, true) {
        actions.push(Action::Leave);
    }
    if let Some(preset) = view_cube(ui, s, cx, egui::vec2(-74.0, 76.0)) {
        actions.push(Action::View(preset));
    }

    // What shows, and how strongly.
    let at = rect.right_top() + egui::vec2(-12.0, 150.0);
    card(ui, "show", egui::Align2::RIGHT_TOP, at, |ui| {
        ui.set_width(170.0);
        ui.strong("Show");
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
                ui.add_sized([62.0, 16.0], egui::Label::new(name));
                ui.spacing_mut().slider_width = 60.0;
                ui.add(egui::Slider::new(alpha, 0.0..=1.0).show_value(false));
            });
        }
        // Inner copper and hole walls follow the board they are in.
        s.alpha[INNER as usize] = if s.alpha[BOARD as usize] < 0.999 {
            1.0
        } else {
            0.0
        };
        s.alpha[HOLE as usize] = s.alpha[BOARD as usize];
        ui.separator();
        ui.checkbox(&mut s.look.mask, "Solder mask");
    });

    let at = rect.center_bottom() - egui::vec2(0.0, 84.0);
    card(ui, "tools", egui::Align2::CENTER_BOTTOM, at, |ui| {
        ui.horizontal(|ui| {
            if ui.button("Fit").on_hover_text("Home").clicked() {
                actions.push(Action::Fit);
            }
            let label = if s.cam.ortho {
                "Orthographic"
            } else {
                "Perspective"
            };
            if ui.button(label).on_hover_text("P").clicked() {
                actions.push(Action::Projection);
            }
            ui.separator();
            if ui
                .selectable_label(s.measure.on, "Measure")
                .on_hover_text("M")
                .clicked()
            {
                actions.push(Action::Measure);
            }
            ui.separator();
            ui.weak("Drag turns · right-drag pans · wheel zooms");
        });
    });

    // A selected Component's tag.
    let painter = ui.painter_at(rect);
    for body in cx
        .model
        .bodies
        .iter()
        .filter(|b| cx.selected.contains(&b.obj))
    {
        let Some(p) = s.cam.project(rect, lifted(body.top, body.lift, s.explode)) else {
            continue;
        };
        let text = match body.height {
            Some(h) => format!("{} · {h:.2} mm tall", body.designator),
            None => format!("{} · no height stored (stand-in)", body.designator),
        };
        let ink = ui.visuals().strong_text_color();
        let galley = painter.layout_no_wrap(text, egui::FontId::proportional(12.0), ink);
        let anchor = p - egui::vec2(0.0, 34.0);
        let back = egui::Rect::from_center_size(anchor, galley.size() + egui::vec2(14.0, 8.0));
        painter.line_segment([p, back.center_bottom()], egui::Stroke::new(1.0, ink));
        painter.rect_filled(back, 6.0, ui.visuals().window_fill);
        painter.rect_stroke(
            back,
            6.0,
            ui.visuals().window_stroke,
            egui::StrokeKind::Inside,
        );
        painter.galley(back.min + egui::vec2(7.0, 4.0), galley, ink);
    }

    // The measurement: the span on the board, the numbers in a card.
    if s.measure.on {
        let accent = ui.visuals().selection.bg_fill;
        if let Some((a, b, placed)) = span(s) {
            if let (Some(pa), Some(pb)) = (s.cam.project(rect, a), s.cam.project(rect, b)) {
                painter.line_segment([pa, pb], egui::Stroke::new(2.0, accent));
                painter.circle_filled(pa, 4.0, accent);
                painter.circle_filled(pb, 4.0, accent);
            }
            let at = rect.left_top() + egui::vec2(12.0, 12.0);
            card(ui, "measure", egui::Align2::LEFT_TOP, at, |ui| {
                ui.strong(if placed { "Measured" } else { "Measuring…" });
                egui::Grid::new("proto-3d-measure").show(ui, |ui| {
                    let rows = [
                        ("Distance", distance(a, b)),
                        ("ΔX", (b[0] - a[0]).abs()),
                        ("ΔY", (b[1] - a[1]).abs()),
                        ("ΔZ", (b[2] - a[2]).abs()),
                    ];
                    for (name, value) in rows {
                        ui.label(name);
                        ui.monospace(mm(value));
                        ui.end_row();
                    }
                });
            });
        } else if let Some(p) = s.measure.hover.and_then(|h| s.cam.project(rect, h)) {
            painter.circle_stroke(p, 5.0, egui::Stroke::new(2.0, accent));
        }
    }
    actions
}

/// The view cube, top right: it turns with the board, and a click on a face looks at
/// that face.
pub(super) fn view_cube(
    ui: &egui::Ui,
    s: &State,
    cx: &Cx<'_>,
    offset: egui::Vec2,
) -> Option<Preset> {
    let center = cx.rect.right_top() + offset;
    let size = 34.0;
    let area = egui::Rect::from_center_size(center, egui::vec2(124.0, 124.0));
    let response = ui.interact(
        area,
        egui::Id::unique("proto-3d-cube"),
        egui::Sense::click(),
    );
    let painter = ui.painter_at(cx.rect);
    let (right, up, fwd) = s.cam.basis();
    let to_screen = |q: V3| center + egui::vec2(dot(q, right), -dot(q, up)) * size;
    let faces: [(V3, &str, Preset); 6] = [
        ([0.0, 0.0, 1.0], "Top", Preset::Top),
        ([0.0, 0.0, -1.0], "Bottom", Preset::Bottom),
        ([0.0, -1.0, 0.0], "Front", Preset::Front),
        ([0.0, 1.0, 0.0], "Back", Preset::Back),
        ([-1.0, 0.0, 0.0], "Left", Preset::Left),
        ([1.0, 0.0, 0.0], "Right", Preset::Right),
    ];
    let pointer = response.hover_pos();
    let mut clicked = None;
    let visuals = ui.visuals();
    for (n, label, preset) in faces {
        let facing = -dot(n, fwd);
        if facing < 0.05 {
            continue;
        }
        let t = if n[2].abs() > 0.5 {
            [1.0, 0.0, 0.0]
        } else {
            [0.0, 0.0, 1.0]
        };
        let (u, v) = (cross(n, t), t);
        let corner = |a: f32, b: f32| {
            to_screen([
                n[0] + u[0] * a + v[0] * b,
                n[1] + u[1] * a + v[1] * b,
                n[2] + u[2] * a + v[2] * b,
            ])
        };
        let quad = vec![
            corner(-1.0, -1.0),
            corner(1.0, -1.0),
            corner(1.0, 1.0),
            corner(-1.0, 1.0),
        ];
        let inside = pointer.is_some_and(|p| {
            let side = |i: usize| {
                let (a, b) = (quad[i], quad[(i + 1) % 4]);
                (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x)
            };
            (0..4).all(|i| side(i) >= 0.0) || (0..4).all(|i| side(i) <= 0.0)
        });
        let fill = if inside {
            visuals.selection.bg_fill
        } else {
            visuals.window_fill.gamma_multiply(0.55 + 0.45 * facing)
        };
        painter.add(egui::Shape::convex_polygon(
            quad,
            fill,
            visuals.window_stroke,
        ));
        if facing > 0.35 {
            painter.text(
                to_screen(n),
                egui::Align2::CENTER_CENTER,
                label,
                egui::FontId::proportional(10.0),
                visuals.text_color(),
            );
        }
        if inside && response.clicked() {
            clicked = Some(preset);
        }
    }
    clicked
}
