//! The two optional copper shapes of a plated Pad, edited in the Pad's own frame.
use super::{Row, SHAPES, corner_name, shape_name};
use crate::pad_form::{DEFAULT_CHAMFER_RATIO, cornered, format_ratio, parse_ratio};
use crate::{tr, widgets};
use egui::{Id, Ui};
use reda_engine::model::{Corner, Corners, Pad, PadCopperShape, PadKind, PadShape};
use reda_engine::preferences::Units;

pub(crate) fn rows(
    ui: &mut Ui,
    id: Id,
    pad: &Pad,
    units: Units,
) -> Vec<(&'static str, Option<PadCopperShape>)> {
    if pad.kind != PadKind::PlatedThroughHole {
        return vec![];
    }
    let mut edits = Vec::new();
    let top = pad.form().copper();
    for (field, group, shape) in [
        ("middle", tr!("pad-copper-middle"), pad.middle),
        ("bottom", tr!("pad-copper-bottom"), pad.bottom),
    ] {
        let id = id.with(field);
        ui.push_id(id, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(tr!("pad-copper-group", group = group.as_str()));
                if shape.is_none() {
                    ui.label(tr!("pad-copper-inherit"));
                    if ui
                        .add(widgets::button(tr!(
                            "pad-copper-customize",
                            group = group.as_str()
                        )))
                        .clicked()
                    {
                        edits.push((field, Some(top)));
                    }
                } else if ui
                    .add(widgets::button(tr!(
                        "pad-copper-clear",
                        group = group.as_str()
                    )))
                    .clicked()
                {
                    edits.push((field, None));
                }
            });
            if let Some(original) = shape {
                let mut updated = original;
                widgets::form(id.with("shape")).show(ui, |ui| {
                    let mut row = Row { ui, id, units };
                    fields(&mut row, &group, &mut updated);
                });
                if updated != original {
                    edits.push((field, Some(updated)));
                }
            }
        });
    }
    edits
}

fn fields(row: &mut Row<'_>, group: &str, copper: &mut PadCopperShape) {
    if let Some(shape) = row.combo(
        "shape",
        &tr!("pad-copper-shape", group = group),
        copper.shape,
        &SHAPES,
        shape_name,
    ) {
        copper.shape = shape;
        if shape == PadShape::ChamferedRect && copper.chamfer_ratio.is_none() {
            copper.chamfer_ratio = Some(DEFAULT_CHAMFER_RATIO);
        }
    }
    if let Some(width) = row.length(
        "width",
        &tr!("pad-copper-width", group = group),
        copper.width,
    ) {
        copper.width = width;
    }
    if let Some(height) = row.length(
        "height",
        &tr!("pad-copper-height", group = group),
        copper.height,
    ) {
        copper.height = height;
    }
    if !cornered(copper.shape) {
        return;
    }
    for (key, label, ratio) in [
        (
            "round",
            tr!("pad-copper-corner", group = group),
            &mut copper.corner_ratio,
        ),
        (
            "chamfer",
            tr!("pad-copper-chamfer", group = group),
            &mut copper.chamfer_ratio,
        ),
    ] {
        if key == "chamfer" && copper.shape != PadShape::ChamferedRect {
            continue;
        }
        let shown = ratio.map(format_ratio).unwrap_or_default();
        if let Some(text) = row.text(key, &label, &shown) {
            if text.trim().is_empty() {
                *ratio = None;
            } else if let Some(value) = parse_ratio(&text, copper.width, copper.height) {
                *ratio = Some(value);
            }
        }
    }
    row.ui
        .add(widgets::form_name(tr!("pad-copper-corners", group = group)));
    let corners = copper.corners.unwrap_or(Corners::ALL);
    let mut updated = Corners::NONE;
    row.ui.horizontal_wrapped(|ui| {
        for c in Corner::ALL {
            let mut on = corners.contains(c);
            ui.add(widgets::checkbox(&mut on, corner_name(c)));
            if on {
                updated = updated.with(c);
            }
        }
    });
    row.ui.end_row();
    if corners != updated {
        copper.corners = (updated != Corners::ALL).then_some(updated);
    }
}
