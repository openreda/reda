//! PROTOTYPE (issue #433) · variant C, "X-ray". Throwaway.
//!
//! 3D is the 2D canvas's Layers pulled apart, for looking inside the board. Copper keeps
//! its 2D Layer colours on the 2D background, flat-lit; the board and the Bodies are
//! ghosts. A rail lists the stack top to bottom and switches each Layer; Spread lifts the
//! Layers apart and Cut slices the board open. A right drag turns the view about the
//! point under the pointer, a middle drag pans. Selecting something dims the rest, and a
//! measurement shows its X, Y and Z legs.

use egui::PointerButton;

use super::mesh::{BOARD, BODY, COPPER, HOLE, INNER, SILK};
use super::{Action, Cx, State, card, distance, mm, point_under, span, wheel};

pub fn defaults(s: &mut State) {
    s.look = super::Look {
        realistic: false,
        mask: false,
    };
    s.cam.ortho = true;
    s.alpha[BOARD as usize] = 0.22;
    s.alpha[BODY as usize] = 0.28;
    s.alpha[HOLE as usize] = 0.6;
}

pub fn navigate(ui: &egui::Ui, s: &mut State, cx: &Cx<'_>) {
    let r = cx.response;
    if let Some(pos) = r.hover_pos() {
        let k = wheel(ui);
        if k != 1.0 {
            s.cam.zoom(cx.rect, pos, k);
        }
    }
    if r.drag_started_by(PointerButton::Secondary) {
        s.pivot = r.interact_pointer_pos().and_then(|p| point_under(s, cx, p));
    }
    if r.dragged_by(PointerButton::Secondary) {
        let d = r.drag_delta();
        s.cam.orbit(-d.x * 0.008, d.y * 0.008, s.pivot);
    } else if !r.drag_started_by(PointerButton::Secondary) {
        s.pivot = None;
    }
    if r.dragged_by(PointerButton::Middle) {
        s.cam.pan(cx.rect, r.drag_delta());
    }
}

pub fn chrome(ui: &egui::Ui, s: &mut State, cx: &Cx<'_>) -> Vec<Action> {
    let mut actions = Vec::new();
    let rect = cx.rect;
    let painter = ui.painter_at(rect);

    // The stack, top to bottom: one row a Layer.
    let at = rect.left_top() + egui::vec2(12.0, 12.0);
    card(ui, "rail", egui::Align2::LEFT_TOP, at, |ui| {
        ui.set_width(200.0);
        let row = |ui: &mut egui::Ui, color: egui::Color32, name: &str, on: bool| {
            ui.horizontal(|ui| {
                let (swatch, _) =
                    ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
                let fill = if on {
                    color
                } else {
                    egui::Color32::TRANSPARENT
                };
                ui.painter().rect_filled(swatch, 2.0, fill);
                let stroke = egui::Stroke::new(1.0, color);
                ui.painter()
                    .rect_stroke(swatch, 2.0, stroke, egui::StrokeKind::Inside);
                let text = if on {
                    egui::RichText::new(name)
                } else {
                    egui::RichText::new(name).weak()
                };
                ui.add(egui::Label::new(text).sense(egui::Sense::click()))
                    .clicked()
            })
            .inner
        };
        let grey = ui.visuals().text_color();
        let kind = |ui: &mut egui::Ui, s: &mut State, cat: u32, name: &str, shown: f32| {
            let on = s.alpha[cat as usize] > 0.0;
            if row(ui, grey, name, on) {
                s.alpha[cat as usize] = if on { 0.0 } else { shown };
            }
        };
        kind(ui, s, BODY, "Bodies", 0.28);
        kind(ui, s, SILK, "Silkscreen", 1.0);
        for (i, layer) in cx.model.layers.iter().enumerate() {
            let c = layer.color.map(|v| (v * 255.0) as u8);
            let color = egui::Color32::from_rgb(c[0], c[1], c[2]);
            let on = !s.hidden_layers.contains(&(i as u16));
            if row(ui, color, &layer.name, on) {
                if on {
                    s.hidden_layers.insert(i as u16);
                } else {
                    s.hidden_layers.remove(&(i as u16));
                }
            }
        }
        kind(ui, s, BOARD, "Board", 0.22);
        s.alpha[COPPER as usize] = 1.0;
        s.alpha[INNER as usize] = 1.0;
        ui.separator();
        ui.spacing_mut().slider_width = 90.0;
        ui.add(
            egui::Slider::new(&mut s.explode, 0.0..=1.0)
                .show_value(false)
                .text("Spread"),
        );
        ui.add(
            egui::Slider::new(&mut s.cut, 0.0..=1.0)
                .show_value(false)
                .text("Cut"),
        );
        ui.separator();
        if ui.selectable_label(s.measure.on, "Measure  M").clicked() {
            actions.push(Action::Measure);
        }
        ui.weak("Right-drag turns about the pointer\nMiddle-drag pans · 2 back to 2D");
    });

    // What a drag is turning about.
    if let Some(p) = s.pivot.and_then(|p| s.cam.project(rect, p)) {
        let ink = ui.visuals().strong_text_color();
        painter.circle_stroke(p, 6.0, egui::Stroke::new(1.5, ink));
        painter.circle_filled(p, 1.5, ink);
    }

    // The measurement: its X, Y and Z legs, then the straight span.
    if let Some((a, b, _)) = span(s) {
        let legs = [
            (
                a,
                [b[0], a[1], a[2]],
                egui::Color32::from_rgb(0xe0, 0x5a, 0x4f),
                "X",
            ),
            (
                [b[0], a[1], a[2]],
                [b[0], b[1], a[2]],
                egui::Color32::from_rgb(0x4c, 0xb0, 0x6a),
                "Y",
            ),
            (
                [b[0], b[1], a[2]],
                b,
                egui::Color32::from_rgb(0x4f, 0x8f, 0xe0),
                "Z",
            ),
        ];
        for (from, to, color, axis) in legs {
            let length = distance(from, to);
            if length < 1e-4 {
                continue;
            }
            if let (Some(p), Some(q)) = (s.cam.project(rect, from), s.cam.project(rect, to)) {
                painter.line_segment([p, q], egui::Stroke::new(2.0, color));
                painter.text(
                    p + (q - p) / 2.0 + egui::vec2(6.0, -6.0),
                    egui::Align2::LEFT_BOTTOM,
                    format!("{axis} {}", mm(length)),
                    egui::FontId::monospace(11.0),
                    color,
                );
            }
        }
        if let (Some(p), Some(q)) = (s.cam.project(rect, a), s.cam.project(rect, b)) {
            let ink = ui.visuals().strong_text_color();
            painter.add(egui::Shape::dashed_line(
                &[p, q],
                egui::Stroke::new(1.0, ink),
                5.0,
                4.0,
            ));
            painter.text(
                q + egui::vec2(8.0, 8.0),
                egui::Align2::LEFT_TOP,
                mm(distance(a, b)),
                egui::FontId::monospace(12.0),
                ink,
            );
        }
    } else if let Some(p) = s.measure.hover.and_then(|h| s.cam.project(rect, h)) {
        let ink = ui.visuals().strong_text_color();
        painter.circle_stroke(p, 4.0, egui::Stroke::new(1.5, ink));
    }
    actions
}
