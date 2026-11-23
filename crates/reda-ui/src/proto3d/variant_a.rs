//! PROTOTYPE (issue #433) · variant A, "Same canvas". Throwaway.
//!
//! 3D is the 2D canvas with depth. `3` tilts the very picture the 2D canvas showed into
//! 3D, and `2` tilts it back. The mouse is the 2D mouse: wheel zooms at the pointer, a
//! right or middle drag pans, and the same drag with Shift turns the board. What shows is
//! what the Layers panel shows; `H` hides the Bodies. Nothing is added to the canvas but
//! one line of keys.

use egui::PointerButton;

use super::{Action, Cx, State, distance, mm, span, wheel};

pub fn defaults(s: &mut State) {
    s.look = super::Look {
        realistic: true,
        mask: true,
    };
    s.cam.ortho = true;
}

pub fn navigate(ui: &egui::Ui, s: &mut State, cx: &Cx<'_>) {
    let r = cx.response;
    if let Some(pos) = r.hover_pos() {
        let k = wheel(ui);
        if k != 1.0 {
            s.cam.zoom(cx.rect, pos, k);
        }
    }
    if r.dragged_by(PointerButton::Secondary) || r.dragged_by(PointerButton::Middle) {
        let d = r.drag_delta();
        if ui.input(|i| i.modifiers.shift) {
            s.cam.orbit(-d.x * 0.008, d.y * 0.008, None);
        } else {
            s.cam.pan(cx.rect, d);
        }
    }
}

pub fn chrome(ui: &egui::Ui, s: &mut State, cx: &Cx<'_>) -> Vec<Action> {
    let painter = ui.painter_at(cx.rect);
    let weak = ui.visuals().weak_text_color();
    let hint = if s.measure.on {
        "Measure · click two points · Esc ends"
    } else {
        "3D · 2 back to 2D · Shift+drag turns · T B F K L R I views · M measure · H Bodies"
    };
    painter.text(
        cx.rect.left_bottom() + egui::vec2(12.0, -10.0),
        egui::Align2::LEFT_BOTTOM,
        hint,
        egui::FontId::proportional(11.0),
        weak,
    );
    measure(ui, s, cx);
    Vec::new()
}

/// The measurement, drawn as the 2D ruler draws its: one line, one number.
pub(super) fn measure(ui: &egui::Ui, s: &State, cx: &Cx<'_>) {
    let painter = ui.painter_at(cx.rect);
    let ink = ui.visuals().text_color();
    if let Some((a, b, _)) = span(s)
        && let (Some(pa), Some(pb)) = (s.cam.project(cx.rect, a), s.cam.project(cx.rect, b))
    {
        painter.line_segment([pa, pb], egui::Stroke::new(1.5, ink));
        for p in [pa, pb] {
            painter.circle_filled(p, 3.0, ink);
        }
        let mid = pa + (pb - pa) / 2.0;
        let text = mm(distance(a, b));
        let font = egui::FontId::proportional(13.0);
        let galley = painter.layout_no_wrap(text, font, ink);
        let back = egui::Rect::from_center_size(mid, galley.size() + egui::vec2(10.0, 6.0));
        painter.rect_filled(back, 4.0, ui.visuals().window_fill);
        painter.galley(back.min + egui::vec2(5.0, 3.0), galley, ink);
    } else if let Some(p) = s.measure.hover.and_then(|h| s.cam.project(cx.rect, h)) {
        painter.circle_stroke(p, 4.0, egui::Stroke::new(1.5, ink));
    }
}
