//! PROTOTYPE (throwaway, #398): the live 2D top view of a template's frame, drawn with
//! egui's painter rather than the reda-render canvas.

use std::collections::HashSet;

use egui::epaint::{Mesh, PathShape, PathStroke};
use egui::{
    Align2, Color32, FontId, Pos2, Rect, Sense, Shape as EShape, Stroke, Ui, Vec2, pos2, vec2,
};

use super::catalogue::{Frame, ItemKind, Shape, trim};
use crate::theme::TokensExt as _;

/// The colours, kept close to the PCB canvas: outline yellow, copper orange.
pub struct Look {
    pub board: Color32,
    pub outline: Color32,
    pub top: Color32,
    pub bottom: Color32,
    pub hole: Color32,
    pub keepout: Color32,
    pub dim: Color32,
    pub bg: Color32,
}

pub fn look(ui: &Ui) -> Look {
    let t = ui.tokens();
    let dark = ui.visuals().dark_mode;
    Look {
        board: if dark {
            Color32::from_rgb(28, 52, 40)
        } else {
            Color32::from_rgb(214, 232, 214)
        },
        outline: Color32::from_rgb(230, 200, 60),
        top: Color32::from_rgb(214, 120, 50),
        bottom: Color32::from_rgb(80, 140, 230),
        hole: if dark {
            Color32::from_gray(20)
        } else {
            Color32::WHITE
        },
        keepout: Color32::from_rgba_unmultiplied(220, 60, 60, 60),
        dim: t.color.text.muted,
        bg: t.color.surface.well,
    }
}

/// Draws `frame` fitted into a `size` box. `detail` adds dimensions and labels; a
/// thumbnail leaves them out. Items in `unticked` are drawn faint.
pub fn show(
    ui: &mut Ui,
    frame: Option<&Frame>,
    size: Vec2,
    detail: bool,
    unticked: &HashSet<String>,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    let l = look(ui);
    let painter = ui.painter_at(rect);
    let t = ui.tokens();
    painter.rect_filled(rect, t.radius.control, l.bg);
    let Some(frame) = frame.filter(|f| !f.outline.is_empty()) else {
        painter.text(
            rect.center(),
            Align2::CENTER_CENTER,
            if frame.is_some() {
                "Draw the outline later"
            } else {
                ""
            },
            FontId::proportional(13.0),
            l.dim,
        );
        return response;
    };
    let b = frame.bounds();
    let margin = if detail { 44.0 } else { 8.0 };
    let avail = rect.shrink(margin);
    let scale = (avail.width() / b.width()).min(avail.height() / b.height());
    let off = avail.center() - vec2(b.center().x * scale, -b.center().y * scale);
    let to = |p: Pos2| pos2(off.x + p.x * scale, off.y - p.y * scale);
    let pts = |v: &[Pos2]| v.iter().map(|p| to(*p)).collect::<Vec<_>>();

    // Board.
    let outline = pts(&frame.outline);
    painter.add(fill(&outline, l.board));
    for cut in &frame.cutouts {
        match cut {
            Shape::Poly(p) => {
                let p = pts(p);
                painter.add(fill(&p, l.bg));
                painter.add(PathShape::closed_line(p, Stroke::new(1.5, l.outline)));
            }
            Shape::Circle(c, r) => {
                painter.circle(to(*c), r * scale, l.bg, Stroke::new(1.5, l.outline));
            }
        }
    }
    painter.add(PathShape::closed_line(outline, Stroke::new(1.5, l.outline)));

    // Keepouts, then items.
    for k in &frame.keepouts {
        if k.protects.as_ref().is_some_and(|id| unticked.contains(id)) {
            continue;
        }
        match &k.shape {
            Shape::Poly(p) => {
                let p = pts(p);
                painter.add(fill(&p, l.keepout));
                painter.add(EShape::dashed_line(
                    &closed(&p),
                    Stroke::new(1.0, l.keepout.to_opaque()),
                    4.0,
                    3.0,
                ));
            }
            Shape::Circle(c, r) => {
                painter.circle_filled(to(*c), r * scale, l.keepout);
            }
        }
    }
    for item in &frame.items {
        let faint = unticked.contains(&item.id);
        let alpha = |c: Color32| if faint { c.gamma_multiply(0.18) } else { c };
        match &item.kind {
            ItemKind::Hole { at, diameter } => {
                painter.circle(
                    to(*at),
                    diameter / 2.0 * scale,
                    alpha(l.hole),
                    Stroke::new(1.0, alpha(l.dim)),
                );
            }
            ItemKind::Connector { body, pads, bottom } => {
                let color = alpha(if *bottom { l.bottom } else { l.top });
                for (c, w, h) in pads {
                    let r = Rect::from_center_size(
                        to(*c),
                        vec2(w * scale, h * scale).max(vec2(1.0, 1.0)),
                    );
                    painter.rect_filled(r, 0.0, color.gamma_multiply(0.85));
                }
                let body = closed(&pts(body));
                if *bottom {
                    painter.add(EShape::dashed_line(
                        &body,
                        Stroke::new(1.0, color),
                        4.0,
                        3.0,
                    ));
                } else {
                    painter.add(EShape::line(body.clone(), Stroke::new(1.0, color)));
                }
                if detail && !faint {
                    let tag = item.label.split(' ').next().unwrap_or_default();
                    let r = Rect::from_points(&body);
                    painter.text(
                        r.center_top() - vec2(0.0, 2.0),
                        Align2::CENTER_BOTTOM,
                        tag,
                        FontId::proportional(11.0),
                        color,
                    );
                }
            }
        }
    }

    // Dimensions.
    if detail {
        for d in &frame.dims {
            let (a, b) = (to(d.a), to(d.b));
            let horizontal = (a.y - b.y).abs() < 0.5;
            let gap = 18.0 * if d.offset < 0.0 { -1.0 } else { 1.0 };
            let n = if horizontal {
                vec2(0.0, -gap)
            } else {
                vec2(gap, 0.0)
            };
            let (a2, b2) = (a + n, b + n);
            let s = Stroke::new(1.0, l.dim);
            painter.line_segment([a + n * 0.2, a2 + n * 0.25], s);
            painter.line_segment([b + n * 0.2, b2 + n * 0.25], s);
            painter.add(EShape::line(vec![a2, b2], PathStroke::new(1.0, l.dim)));
            arrow(&painter, a2, b2, s);
            arrow(&painter, b2, a2, s);
            let mm = (d.b - d.a).length();
            let mid = a2 + (b2 - a2) / 2.0;
            let text = format!("{} mm", trim(mm));
            let font = FontId::proportional(11.0);
            if horizontal {
                painter.text(
                    mid - vec2(0.0, 2.0),
                    Align2::CENTER_BOTTOM,
                    text,
                    font,
                    l.dim,
                );
            } else {
                let g = painter.layout_no_wrap(text, font, l.dim);
                let pos = mid + vec2(4.0, g.size().x / 2.0);
                painter.add(
                    egui::epaint::TextShape::new(pos, g, l.dim)
                        .with_angle(-std::f32::consts::FRAC_PI_2),
                );
            }
        }
    }
    response
}

fn arrow(p: &egui::Painter, from: Pos2, to: Pos2, s: Stroke) {
    let d = (to - from).normalized() * 6.0;
    let n = vec2(-d.y, d.x) * 0.5;
    p.line_segment([from, from + d + n], s);
    p.line_segment([from, from + d - n], s);
}

fn closed(p: &[Pos2]) -> Vec<Pos2> {
    let mut v = p.to_vec();
    if let Some(f) = p.first() {
        v.push(*f);
    }
    v
}

/// A filled simple polygon, convex or not, by ear clipping.
fn fill(points: &[Pos2], color: Color32) -> EShape {
    let mut mesh = Mesh::default();
    for p in points {
        mesh.colored_vertex(*p, color);
    }
    let area: f32 = points
        .iter()
        .zip(points.iter().cycle().skip(1))
        .map(|(a, b)| a.x * b.y - b.x * a.y)
        .sum();
    let ccw = area > 0.0;
    let mut idx: Vec<usize> = (0..points.len()).collect();
    let mut guard = 0;
    while idx.len() > 3 && guard < 10_000 {
        guard += 1;
        let n = idx.len();
        let mut clipped = false;
        for i in 0..n {
            let (ia, ib, ic) = (idx[(i + n - 1) % n], idx[i], idx[(i + 1) % n]);
            let (a, b, c) = (points[ia], points[ib], points[ic]);
            let cross = (b - a).x * (c - b).y - (b - a).y * (c - b).x;
            if (cross > 0.0) != ccw || cross.abs() < 1e-6 {
                continue;
            }
            let inside = idx
                .iter()
                .any(|&j| j != ia && j != ib && j != ic && in_tri(points[j], a, b, c));
            if inside {
                continue;
            }
            mesh.add_triangle(ia as u32, ib as u32, ic as u32);
            idx.remove(i);
            clipped = true;
            break;
        }
        if !clipped {
            break;
        }
    }
    if idx.len() == 3 {
        mesh.add_triangle(idx[0] as u32, idx[1] as u32, idx[2] as u32);
    }
    EShape::mesh(mesh)
}

fn in_tri(p: Pos2, a: Pos2, b: Pos2, c: Pos2) -> bool {
    let s = |p1: Pos2, p2: Pos2, p3: Pos2| {
        (p1.x - p3.x) * (p2.y - p3.y) - (p2.x - p3.x) * (p1.y - p3.y)
    };
    let (d1, d2, d3) = (s(p, a, b), s(p, b, c), s(p, c, a));
    let neg = d1 < 0.0 || d2 < 0.0 || d3 < 0.0;
    let pos = d1 > 0.0 || d2 > 0.0 || d3 > 0.0;
    !(neg && pos)
}

/// The legend under a detailed preview.
pub fn legend(ui: &mut Ui) {
    let l = look(ui);
    ui.horizontal_wrapped(|ui| {
        for (color, text) in [
            (l.outline, "Outline and cutouts"),
            (l.top, "Connector, top"),
            (l.bottom, "Connector, bottom"),
            (l.keepout.to_opaque(), "Keepout"),
            (l.dim, "Hole"),
        ] {
            let (r, _) = ui.allocate_exact_size(vec2(10.0, 10.0), Sense::hover());
            ui.painter().rect_filled(r, 2.0, color);
            crate::settings::layout::muted(ui, text);
            ui.add_space(6.0);
        }
    });
}
