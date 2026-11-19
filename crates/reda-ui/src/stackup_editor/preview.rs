//! Physical materials in their own colours, shared with the Layers table.
mod drawing;
mod scene;
pub(super) use super::commands::Scale;
use super::{Edits, Field, ViaField, View};
use crate::settings::strong;
use crate::theme::TokensExt as _;
use crate::{tr, widgets};
use drawing::{Band, Barrel};
use egui::{Color32, Rect, Role, Sense, Stroke, StrokeKind, Ui, WidgetInfo, pos2, vec2};
use reda_engine::model::{
    DielectricKind, PcbDocument, Stackup, StackupEntry, StackupInfo, ViaType,
};
use scene::{Names, Section};
use std::sync::Arc;

const HANDLE_SIZE: f32 = 16.0;

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

/// Glyph runs and material geometry change only with the board, layout or theme.
pub(super) struct Cached {
    stack: Stackup,
    names: Names,
    tokens: Arc<crate::theme::Tokens>,
    body: Rect,
    scale: Scale,
    pitch: f32,
    section: Section,
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
        let mut scale = view.scale;
        ui.add(widgets::segmented(&mut scale, &options));
        if scale != view.scale {
            edits.commands.push((
                None,
                super::commands::SCALE,
                serde_json::json!({"document":pcb.uuid,"scale":scale}),
            ));
        }
    });
    let stack = pcb.stackup();
    let names = Names::new(pcb, info);
    let text_width = |label: &str| {
        ui.painter()
            .layout_no_wrap(label.to_owned(), t.ty.small.font_id(), t.color.text.primary)
            .size()
            .x
    };
    let pitch = names
        .vias
        .iter()
        .map(|s| text_width(s))
        .fold(t.size.of(40.0), f32::max)
        + t.space.panel;
    let margin = t.size.of(84.0);
    let callouts = names
        .callouts
        .iter()
        .map(|s| text_width(s))
        .fold(0.0, f32::max)
        + 2.0 * t.space.gap;
    let height = ui
        .available_height()
        .max(t.size.of(140.0) + t.ty.small.line * stack.entries.len() as f32);
    let split_button = widgets::button(tr!("stackup-split-sheet"));
    let split_width = split_button.width(ui);
    // Leave room on the left of the first barrel for an entire split button. A
    // narrow tab scrolls the drawing horizontally, so both targets stay usable.
    let end_space = (pitch * 0.5).max(t.space.gap + split_width + t.size.of(HANDLE_SIZE));
    let min_body_width = 2.0 * end_space + pitch * names.vias.len().saturating_sub(1) as f32;
    let width = ui.available_width().max(margin + callouts + min_body_width);
    egui::ScrollArea::both()
        .id_salt("cross-section-scroll")
        .auto_shrink(false)
        .show(ui, |ui| {
            let (area, image) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
            image.widget_info(|| {
                WidgetInfo::labeled(
                    Role::Image,
                    true,
                    tr!(
                        "board-setup-cross-section-of",
                        bands = names.callouts.join(", ")
                    ),
                )
            });
            let body = Rect::from_min_max(
                pos2(area.left() + margin, area.top() + 2.0 * t.control_height()),
                pos2(area.right() - callouts, area.bottom() - t.control_height()),
            );
            let draw = ui.painter_at(area);
            let cache = view
                .section
                .take()
                .filter(|c| {
                    c.body == body
                        && c.scale == view.scale
                        && c.pitch == pitch
                        && Arc::ptr_eq(&c.tokens, &t)
                        && c.names == names
                        && c.stack == *stack
                })
                .unwrap_or_else(|| {
                    let section = Section::new(pcb, info, &names, body, view.scale, pitch, &t);
                    Arc::new(Cached {
                        stack: stack.clone(),
                        names,
                        tokens: Arc::clone(&t),
                        body,
                        scale: view.scale,
                        pitch,
                        section,
                    })
                });
            view.section = Some(Arc::clone(&cache));
            let section = &cache.section;
            let bands = &section.bands;
            let labels = section.paint(&draw, &t, view.selected);
            // A barrel sits in front of its Dielectric and owns the pointer there.
            let over_barrel = section
                .barrels
                .iter()
                .any(|(_, b)| ui.rect_contains_pointer(b.bounds));
            let mut hovered = None;
            for (i, band) in bands.iter().enumerate() {
                let selected = Some(band.id) == view.selected;
                if selected {
                    draw.rect_stroke(
                        band.rect,
                        0.0,
                        Stroke::new(t.stroke.accent_line, t.color.accent.solid),
                        StrokeKind::Outside,
                    );
                }
                let label = labels[&band.id];
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
                        tr!("stackup-select-band", name = cache.names.bands[i].as_str()),
                    )
                });
                let label_hit = ui.interact(
                    label,
                    ui.scope_id().with(("callout", band.id)),
                    Sense::CLICK,
                );
                if (hit.clicked() || label_hit.clicked()) && !selected {
                    edits.select(band.id);
                }
                if !band.copper
                    && (ui.rect_contains_pointer(band.rect) || ui.rect_contains_pointer(label))
                {
                    hovered = Some(band);
                }
            }
            for (i, barrel) in &section.barrels {
                let kind = &stack.via_types[*i];
                via(
                    ui,
                    barrel,
                    kind,
                    &cache.names.vias[*i],
                    labels[&kind.uuid],
                    body,
                    bands,
                    view,
                    edits,
                    stack.through_type().uuid != kind.uuid,
                );
            }
            let button_bounds = |band: &Band| {
                Rect::from_center_size(
                    pos2(
                        band.rect.left() + t.space.gap + split_width / 2.0,
                        band.rect.center().y,
                    ),
                    vec2(split_width, t.control_height()),
                )
            };
            // On a thin sheet most of the button lies outside the material itself.
            let active = bands.iter().find(|b| {
                Some(b.id) == view.split_sheet && ui.rect_contains_pointer(button_bounds(b))
            });
            let split = active
                .or(hovered)
                .filter(|_| !over_barrel && stack.copper().count() < 32);
            view.split_sheet = split.map(|b| b.id);
            if let Some(band) = split {
                let bounds = button_bounds(band);
                if ui.put(bounds, split_button).clicked() {
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
    label: Rect,
    body: Rect,
    bands: &[Band],
    view: &mut View,
    edits: &mut Edits,
    movable: bool,
) {
    let t = ui.tokens();
    let draw = ui.painter();
    let selected = view.selected == Some(kind.uuid);
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
        Sense::CLICK,
    );
    if (hit.clicked() || label_hit.clicked()) && !selected {
        edits.select(kind.uuid);
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
    let handle_size = t.size.of(HANDLE_SIZE);
    let close_ends = (barrel.from.center().y - barrel.to.center().y).abs() < handle_size;
    for (key, at, other, current, side) in [
        (ViaField::From, barrel.from, kind.to, kind.from, -1.0),
        (ViaField::To, barrel.to, kind.from, kind.to, 1.0),
    ] {
        let end = pos2(barrel.x, at.center().y);
        let mut center = end;
        if close_ends {
            center.x += side * (handle_size * 0.5 + t.stroke.hairline);
            draw.line_segment(
                [end, center],
                Stroke::new(t.stroke.accent_line, t.color.accent.solid),
            );
        }
        let handle = ui.interact(
            Rect::from_center_size(center, vec2(handle_size, handle_size)),
            ui.scope_id().with(("end", kind.uuid, key)),
            Sense::drag(),
        );
        handle.widget_info(|| {
            WidgetInfo::labeled(
                Role::Button,
                true,
                if key == ViaField::From {
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

#[cfg(test)]
mod tests;
