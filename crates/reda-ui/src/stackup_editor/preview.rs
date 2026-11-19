//! Physical materials in their own colours, shared with the Layers table.
mod drawing;
use super::{Edits, Field, View};
use crate::settings::strong;
use crate::theme::TokensExt as _;
use crate::{tr, widgets};
pub(super) use drawing::Scale;
use drawing::{Band, Barrel};
use egui::{Align2, Color32, Rect, Role, Sense, Stroke, StrokeKind, Ui, WidgetInfo, pos2, vec2};
use reda_engine::model::{DielectricKind, PcbDocument, StackupEntry, StackupInfo, ViaType};

pub(super) fn color(entry: &StackupEntry) -> Color32 {
    match entry {
        StackupEntry::Copper(_) => widgets::data_color(0xc8, 0x75, 0x33),
        StackupEntry::Dielectric(d) if d.kind == DielectricKind::Core => {
            widgets::data_color(0xb5, 0xa6, 0x5a)
        }
        StackupEntry::Dielectric(_) => widgets::data_color(0xd3, 0xc7, 0x8a),
    }
}

fn mask_color(name: Option<&str>) -> Color32 {
    match name.unwrap_or("").to_lowercase().as_str() {
        n if n.contains("red") => widgets::data_color(0xb7, 0x1c, 0x1c),
        n if n.contains("blue") => widgets::data_color(0x15, 0x65, 0xc0),
        n if n.contains("black") => widgets::data_color(0x21, 0x21, 0x21),
        n if n.contains("white") => widgets::data_color(0xf5, 0xf5, 0xf5),
        n if n.contains("yellow") => widgets::data_color(0xf9, 0xa8, 0x25),
        n if n.contains("purple") => widgets::data_color(0x6a, 0x1b, 0x9a),
        _ => widgets::data_color(0x2e, 0x7d, 0x32),
    }
}

/// A resolved thickness in mm, including a fractional share of a nanometre.
pub(super) fn mm(nm: f64) -> String {
    format!("{:.9}", nm / 1_000_000.0)
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_owned()
}

pub(super) fn show(
    ui: &mut Ui,
    pcb: &PcbDocument,
    info: &StackupInfo,
    view: &mut View,
    edits: &mut Edits,
) {
    let t = ui.tokens();
    ui.horizontal_wrapped(|ui| {
        strong(ui, tr!("board-setup-cross-section"));
        let options = [
            (Scale::Physical, tr!("stackup-to-scale")),
            (Scale::Schematic, tr!("stackup-schematic")),
        ];
        ui.add(widgets::segmented(&mut view.scale, &options));
    });
    let stack = pcb.stackup();
    let names: Vec<_> = stack
        .entries
        .iter()
        .enumerate()
        .map(|(i, _)| super::entry_name(pcb, i))
        .collect();
    let labels: Vec<_> = names
        .iter()
        .zip(&info.entries)
        .map(|(name, entry)| format!("{name} · {} mm", mm(entry.thickness)))
        .collect();
    let text_width = |label: &str| {
        ui.painter()
            .layout_no_wrap(label.to_owned(), t.ty.small.font_id(), t.color.text.primary)
            .size()
            .x
    };
    let via_names: Vec<_> = stack
        .via_types
        .iter()
        .map(|kind| kind.display_name(pcb))
        .collect();
    let pitch = via_names
        .iter()
        .map(|s| text_width(s))
        .fold(t.size.of(40.0), f32::max)
        + t.space.panel;
    let margin = t.size.of(84.0);
    let callouts = labels.iter().map(|s| text_width(s)).fold(0.0, f32::max) + 2.0 * t.space.gap;
    let height = ui
        .available_height()
        .max(t.size.of(140.0) + t.ty.small.line * stack.entries.len() as f32);
    let width = ui
        .available_width()
        .max(margin + callouts + (pitch * via_names.len() as f32).max(t.size.of(120.0)));
    egui::ScrollArea::both()
        .id_salt("cross-section-scroll")
        .auto_shrink(false)
        .show(ui, |ui| {
            let (area, image) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
            image.widget_info(|| {
                WidgetInfo::labeled(
                    Role::Image,
                    true,
                    tr!("board-setup-cross-section-of", bands = labels.join(", ")),
                )
            });
            let body = Rect::from_min_max(
                pos2(area.left() + margin, area.top() + 2.0 * t.control_height()),
                pos2(area.right() - callouts, area.bottom() - t.control_height()),
            );
            let draw = ui.painter_at(area);
            let bands = drawing::bands(pcb, info, body, view.scale, t.ty.small.line);
            let mask = mask_color(stack.mask_color.as_deref());
            for y in [body.top() - t.stroke.accent_line, body.bottom()] {
                draw.rect_filled(
                    Rect::from_min_size(
                        pos2(body.left(), y),
                        vec2(body.width(), t.stroke.accent_line),
                    ),
                    0.0,
                    mask,
                );
            }
            let mut hovered = None;
            for (i, band) in bands.iter().enumerate() {
                let selected = Some(band.id) == view.selected;
                let ink = if selected {
                    t.color.accent.text
                } else {
                    t.color.text.primary
                };
                draw.rect_filled(band.rect, 0.0, color(&stack.entries[i]));
                if selected {
                    draw.rect_stroke(
                        band.rect,
                        0.0,
                        Stroke::new(t.stroke.accent_line, t.color.accent.solid),
                        StrokeKind::Outside,
                    );
                }
                let leader_end = pos2(body.right() + t.space.gap, band.label_y);
                draw.line_segment(
                    [band.rect.right_center(), leader_end],
                    Stroke::new(t.stroke.hairline, t.color.stroke.strong),
                );
                let label = draw.text(
                    leader_end + vec2(t.space.gap, 0.0),
                    Align2::LEFT_CENTER,
                    &labels[i],
                    t.ty.small.font_id(),
                    ink,
                );
                let hit = ui.interact(
                    band.rect,
                    ui.scope_id().with(("band", band.id)),
                    Sense::click(),
                );
                hit.widget_info(|| {
                    WidgetInfo::selected(
                        Role::Button,
                        true,
                        selected,
                        tr!("stackup-select-band", name = names[i].as_str()),
                    )
                });
                let label_hit = ui.interact(
                    label,
                    ui.scope_id().with(("callout", band.id)),
                    Sense::click(),
                );
                if hit.clicked() || label_hit.clicked() {
                    view.selected = Some(band.id);
                }
                if !band.copper
                    && (ui.rect_contains_pointer(band.rect) || ui.rect_contains_pointer(label))
                {
                    hovered = Some(band);
                }
            }
            let guide = Stroke::new(t.stroke.hairline, t.color.text.primary);
            let x = body.left() - t.space.gap;
            draw.vline(x, body.y_range(), guide);
            for y in [body.top(), body.bottom()] {
                draw.hline(x - t.space.gap..=body.left(), y, guide);
            }
            draw.text(
                pos2(x - t.space.gap, body.center().y),
                Align2::RIGHT_CENTER,
                format!("{} mm", info.totals.total.mm_exact()),
                t.ty.numeric.font_id(),
                t.color.text.primary,
            );
            let first =
                body.center().x - (stack.via_types.len().saturating_sub(1) as f32) * pitch / 2.0;
            for (i, (kind, name)) in stack.via_types.iter().zip(via_names).enumerate() {
                let x = first + pitch * i as f32;
                if let Some(barrel) = drawing::barrel(&bands, kind, x, t.size.of(13.0)) {
                    via(
                        ui,
                        &barrel,
                        kind,
                        &name,
                        body,
                        &bands,
                        view,
                        edits,
                        stack.through_type().uuid != kind.uuid,
                    );
                }
            }
            let button_bounds = |band: &Band| {
                Rect::from_center_size(
                    pos2(
                        band.rect.left() + t.space.gap + t.size.of(30.0),
                        band.rect.center().y,
                    ),
                    vec2(t.size.of(60.0), t.control_height()),
                )
            };
            // On a thin sheet most of the button lies outside the material itself.
            let active = bands.iter().find(|b| {
                Some(b.id) == view.split_sheet && ui.rect_contains_pointer(button_bounds(b))
            });
            let split = active.or(hovered).filter(|_| stack.copper().count() < 32);
            view.split_sheet = split.map(|b| b.id);
            if let Some(band) = split {
                let bounds = button_bounds(band);
                if ui
                    .put(bounds, widgets::button(tr!("stackup-split-sheet")))
                    .clicked()
                {
                    edits.commands.push((
                        None,
                        "pcb.layer.add",
                        serde_json::json!({"document":pcb.uuid,"split":band.id}),
                    ));
                }
            }
        });
}

#[allow(clippy::too_many_arguments)]
fn via(
    ui: &mut Ui,
    barrel: &Barrel,
    kind: &ViaType,
    name: &str,
    body: Rect,
    bands: &[Band],
    view: &mut View,
    edits: &mut Edits,
    movable: bool,
) {
    let t = ui.tokens();
    let draw = ui.painter();
    let selected = view.selected == Some(kind.uuid);
    let copper = widgets::data_color(0xc8, 0x75, 0x33);
    let rim = Stroke::new(t.stroke.hairline, t.color.stroke.strong);
    for land in &barrel.pads {
        draw.rect_filled(*land, 0.0, copper);
        draw.rect_stroke(*land, 0.0, rim, StrokeKind::Inside);
    }
    let [a, b, c, d] = barrel.hole;
    draw.add(egui::Shape::convex_polygon(
        vec![a, b, c, d],
        t.color.surface.base,
        Stroke::NONE,
    ));
    let wall = Stroke::new(t.stroke.accent_line, copper);
    for line in [[a, d], [b, c]] {
        draw.line_segment(line, wall);
    }
    if kind.micro {
        draw.line_segment([d, c], wall);
    }
    let ink = if selected {
        t.color.accent.text
    } else {
        t.color.text.primary
    };
    let label = draw.text(
        pos2(barrel.x, body.top() - t.space.gap),
        Align2::CENTER_BOTTOM,
        name,
        t.ty.small.font_id(),
        ink,
    );
    let hit = ui.interact(
        barrel.bounds,
        ui.scope_id().with(("barrel", kind.uuid)),
        Sense::click(),
    );
    hit.widget_info(|| {
        WidgetInfo::selected(
            Role::Button,
            true,
            selected,
            tr!("stackup-select-via", name = name),
        )
    });
    let label_hit = ui.interact(
        label,
        ui.scope_id().with(("barrel-name", kind.uuid)),
        Sense::click(),
    );
    if hit.clicked() || label_hit.clicked() {
        view.selected = Some(kind.uuid);
    }
    if !selected {
        return;
    }
    draw.rect_stroke(
        barrel.bounds.expand(t.stroke.accent_line),
        t.radius.mark,
        Stroke::new(t.stroke.accent_line, t.color.accent.solid),
        StrokeKind::Outside,
    );
    if !movable {
        return;
    }
    for (key, at, other, current) in [
        ("from", barrel.from, kind.to, kind.from),
        ("to", barrel.to, kind.from, kind.to),
    ] {
        let center = pos2(barrel.x, at.center().y);
        let handle = ui.interact(
            Rect::from_center_size(center, vec2(t.size.of(16.0), t.size.of(16.0))),
            ui.scope_id().with(("end", kind.uuid, key)),
            Sense::drag(),
        );
        handle.widget_info(|| {
            WidgetInfo::labeled(
                Role::Button,
                true,
                if key == "from" {
                    tr!("via-from-field", name = name)
                } else {
                    tr!("via-to-field", name = name)
                },
            )
        });
        draw.circle(
            center,
            t.size.of(5.0),
            t.color.accent.solid,
            Stroke::new(t.stroke.hairline, t.color.surface.card),
        );
        if handle.hovered() || handle.dragged() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
        }
        if let Some(position) = handle.interact_pointer_pos()
            && body.expand(t.size.of(8.0)).contains(position)
            && let Some(target) = drawing::nearest_copper(bands, position)
            && target != current
            && target != other
        {
            if handle.dragged()
                && let Some(band) = bands.iter().find(|b| b.id == target)
            {
                draw.hline(
                    body.x_range(),
                    band.rect.center().y,
                    Stroke::new(t.stroke.accent_line, t.color.accent.solid),
                );
            }
            if handle.drag_stopped() {
                edits.set(Field::Via(kind.uuid, key), serde_json::json!(target));
            }
        }
    }
}
