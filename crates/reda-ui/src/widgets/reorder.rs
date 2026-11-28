//! A list whose rows a drag reorders: each row starts with a grip, and dropping it on
//! another row's top or bottom half puts it before or after that row. The list draws a
//! line where the row would land.

use egui::{Id, Rect, Role, Sense, Stroke, Ui, WidgetInfo};

use crate::theme::TokensExt as _;

/// The grip's width.
const GRIP_WIDTH: f32 = 10.0;

/// The payload a grip carries: which list, and which row of it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RowDrag {
    list: Id,
    index: usize,
}

/// Draws `count` rows, each a grip labelled `grip_label(i)` and then `row(ui, i)`. When a
/// grip is dropped on a row, returns the order the drop asks for: the row indices, first
/// to last; `None` when nothing was dropped or the order is unchanged.
pub fn reorder_rows(
    ui: &mut Ui,
    list: &str,
    count: usize,
    grip_label: impl Fn(usize) -> String,
    mut row: impl FnMut(&mut Ui, usize),
) -> Option<Vec<usize>> {
    let list = Id::NULL.with(list);
    let mut dropped = None;
    for i in 0..count {
        let response = ui
            .push_id((list, i), |ui| {
                ui.horizontal_wrapped(|ui| {
                    let payload = RowDrag { list, index: i };
                    let grip = ui
                        .dnd_drag_source(list.with(("grip", i)), payload, |ui| {
                            let size = egui::vec2(GRIP_WIDTH, super::glyph::BOX);
                            let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
                            grip_lines(ui, rect);
                        })
                        .response;
                    let label = grip_label(i);
                    grip.widget_info(|| WidgetInfo::labeled(Role::Button, true, &label));
                    row(ui, i);
                })
                .response
            })
            .inner;
        if let Some(drag) = response.dnd_hover_payload::<RowDrag>()
            && drag.list == list
        {
            let t = ui.tokens();
            let y = if below_middle(&response) {
                response.rect.bottom()
            } else {
                response.rect.top()
            };
            let stroke = Stroke::new(t.stroke.accent_line, t.color.accent.text);
            (response.ctx.layer_painter(response.layer_id)).hline(
                response.rect.x_range(),
                y,
                stroke,
            );
        }
        if let Some(drag) = response.dnd_release_payload::<RowDrag>()
            && drag.list == list
        {
            dropped = Some((drag.index, i, below_middle(&response)));
        }
    }
    let (moved, target, after) = dropped?;
    let order: Vec<usize> = (0..count).collect();
    let new = reordered(&order, moved, target, after);
    (new != order).then_some(new)
}

/// `order` with `moved` put just before `target`, or just after it with `after`.
pub fn reordered<T: Copy + PartialEq>(order: &[T], moved: T, target: T, after: bool) -> Vec<T> {
    if moved == target || !order.contains(&moved) || !order.contains(&target) {
        return order.to_vec();
    }
    let mut out: Vec<T> = order.iter().copied().filter(|x| *x != moved).collect();
    let at = out.iter().position(|x| *x == target).unwrap_or(out.len());
    out.insert(if after { at + 1 } else { at }, moved);
    out
}

/// Whether the pointer is in the lower half of `row`.
fn below_middle(row: &egui::Response) -> bool {
    let rect: Rect = row.rect;
    row.ctx
        .input(|i| i.pointer.interact_pos())
        .is_some_and(|p| p.y > rect.center().y)
}

/// The drag handle: three short lines.
fn grip_lines(ui: &Ui, rect: Rect) {
    let t = ui.tokens();
    let stroke = Stroke::new(t.stroke.hairline, t.color.text.muted);
    for dy in [-3.0, 0.0, 3.0] {
        let y = rect.center().y + dy;
        ui.painter()
            .hline(rect.left() + 1.0..=rect.right() - 1.0, y, stroke);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_drop_puts_the_row_before_or_after_its_target() {
        assert_eq!(reordered(&[0, 1, 2], 2, 0, false), [2, 0, 1]);
        assert_eq!(reordered(&[0, 1, 2], 0, 1, true), [1, 0, 2]);
        assert_eq!(reordered(&[0, 1, 2], 1, 1, true), [0, 1, 2]);
    }
}
