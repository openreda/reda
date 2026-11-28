//! A board's Guides over its canvas (docs/spec/v0.3/grids-guides.md#drawing): a thin
//! dashed line across the whole canvas for a line Guide, a small cross for a point Guide,
//! in the Guide colour; a selected one is drawn solid and wider. Guides are editor aids,
//! so they are painted here, over the Scene, and never reach an output.

use egui::{Color32, Pos2, Rect, Shape, Stroke, Ui, Vec2};
use reda_engine::model::Guide;
use reda_engine::render::{BoardColor, ColorKey, Palette};

use super::widget::CanvasInputs;
use crate::shell::DocumentTab;

/// The dash and gap of a line Guide, in points.
const DASH: (f32, f32) = (6.0, 4.0);
/// Half the arm of a point Guide's cross, in points.
const CROSS: f32 = 5.0;

/// Draws `tab`'s shown Guides, unless Show Grids & Guides is off.
pub(super) fn draw(ui: &Ui, input: &CanvasInputs, tab: &DocumentTab, palette: &Palette) {
    if !tab.aids_shown || tab.scene.guides.is_empty() {
        return;
    }
    let painter = ui.painter_at(input.rect);
    let [r, g, b, _] = palette.color(ColorKey::Board(BoardColor::Guide)).0;
    let color = Color32::from_rgb(r, g, b);
    for guide in tab.scene.guides.iter().filter(|g| g.shown) {
        let selected = tab.selection.objects.contains(&guide.uuid);
        let width = if selected { 2.0 } else { 1.0 };
        let stroke = Stroke::new(width, color);
        let at = input.screen_point((guide.at.x.0 as f64, guide.at.y.0 as f64));
        match direction(input, guide) {
            Some(d) => {
                let Some((a, b)) = across(input.rect, at, d) else {
                    continue;
                };
                if selected {
                    painter.line_segment([a, b], stroke);
                } else {
                    painter.extend(Shape::dashed_line(&[a, b], stroke, DASH.0, DASH.1));
                }
            }
            None => {
                let arm = if selected { CROSS + 2.0 } else { CROSS };
                for v in [Vec2::new(arm, arm), Vec2::new(arm, -arm)] {
                    painter.line_segment([at - v, at + v], stroke);
                }
            }
        }
    }
}

/// A line Guide's direction on screen, as a unit vector; `None` for a point Guide.
fn direction(input: &CanvasInputs, guide: &Guide) -> Option<Vec2> {
    let t = (guide.angle?.0 as f64 / 1000.0).to_radians();
    // A step along the line of a screen's width or so, in nm, so the rounding is fine.
    let step = input.camera.nm_per_point * 100.0;
    let (x, y) = (guide.at.x.0 as f64, guide.at.y.0 as f64);
    let a = input.screen_point((x, y));
    let b = input.screen_point((x + t.cos() * step, y + t.sin() * step));
    let d = b - a;
    (d.length() > 0.0).then(|| d.normalized())
}

/// The part of the line through `at` along `d` that crosses `rect`.
fn across(rect: Rect, at: Pos2, d: Vec2) -> Option<(Pos2, Pos2)> {
    // From the foot of the rectangle's centre on the line, half a diagonal each way covers
    // the rectangle; past that the painter clips.
    let half = rect.size().length() / 2.0;
    let foot = at + d * (rect.center() - at).dot(d);
    let (a, b) = (foot - d * half, foot + d * half);
    rect.expand(1.0)
        .intersects(Rect::from_two_pos(a, b))
        .then_some((a, b))
}
