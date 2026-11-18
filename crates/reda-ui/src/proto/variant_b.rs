//! PROTOTYPE (issue #330), variant B · Ledger: the rows are the bands.
//!
//! One grid. Each entry is a row, and the row's first cell is its band of the
//! cross-section, so the drawing and the table are the same thing and never drift apart.
//! Every number is a cell. Via types are columns of the same grid: a barrel runs down the
//! copper rows it spans, a click on a copper row moves the nearer end there, and the
//! drill and pad sit in the rows under the stack. Impedance is a column, the total is the
//! last row. A thin strip on the left shows the same stack to scale.

use egui::{Align2, Rect, Sense, Stroke, Ui, pos2, vec2};

use super::View;
use super::kit;
use super::model::{Kind, Make, Role, Sel, Stack, mm};
use crate::theme::TokensExt as _;
use crate::widgets;

/// The grid's columns before the Via types: name and width at the default text size.
const COLUMNS: [(&str, f32); 9] = [
    ("", 64.0),
    ("#", 30.0),
    ("Name", 130.0),
    ("Kind", 112.0),
    ("Material", 112.0),
    ("Thickness", 104.0),
    ("Dk", 60.0),
    ("Df", 64.0),
    ("Impedance", 190.0),
];
const VIA: f32 = 84.0;
const GAP: f32 = 10.0;
/// The to-scale strip left of the grid.
const SCALE: f32 = 18.0;

pub fn show(ui: &mut Ui, s: &mut Stack, _v: &mut View) {
    let t = ui.tokens();
    let c = &t.color;
    let pad = t.space.panel;
    let full = ui.available_rect_before_wrap().shrink(pad);
    let strip = Rect::from_min_size(full.min, vec2(full.width(), t.control_height()));
    toolbar(ui, s, strip);

    let row = t.control_height() + 8.0;
    let origin = pos2(full.left() + SCALE + 14.0, strip.bottom() + pad);
    // Column edges: the fixed columns, then one per Via type.
    let mut x = origin.x;
    let mut edges = Vec::new();
    for (_, w) in COLUMNS {
        let w = t.size.of(w);
        edges.push((x, w));
        x += w + GAP;
    }
    let via_x: Vec<f32> = (0..s.vias.len())
        .map(|j| x + (t.size.of(VIA) + GAP) * j as f32)
        .collect();
    let via_w = t.size.of(VIA);
    let right = via_x.last().map_or(x, |x| x + via_w);
    let col = |n: usize, y: f32| Rect::from_min_size(pos2(edges[n].0, y), vec2(edges[n].1, row));
    let painter = ui.painter_at(full);
    let rule = Stroke::new(t.stroke.hairline, c.stroke.hairline);

    // Header.
    let mut y = origin.y;
    for (n, (name, _)) in COLUMNS.iter().enumerate() {
        let r = col(n, y);
        kit::paint_text(&painter, r.left_center(), Align2::LEFT_CENTER, &t.ty.small, name, c.text.muted);
    }
    let mut select = None;
    for (j, x) in via_x.iter().enumerate() {
        let r = Rect::from_min_size(pos2(*x, y), vec2(via_w, row));
        let head = kit::hit(ui, r, ("b-via-head", j), Sense::click());
        let on = s.sel == Sel::Via(j);
        let ink = if on { c.accent.text } else { c.text.primary };
        let name = &s.vias[j].name;
        let galley = painter.layout_no_wrap(name.clone(), t.ty.small.font_id(), ink);
        let clip = painter.with_clip_rect(r);
        let at = pos2(r.center().x - galley.size().x.min(via_w) / 2.0, r.center().y - galley.size().y / 2.0);
        clip.galley(at, galley, ink);
        if head.clicked() {
            select = Some(Sel::Via(j));
        }
    }
    let add = Rect::from_min_size(pos2(right + GAP, y), vec2(t.size.of(110.0), row));
    if kit::cell(ui, add, |ui| ui.add(widgets::button("+ Via type").flat())).clicked() {
        s.add_via();
    }
    y += row;
    painter.hline(origin.x..=right, y, rule);
    let top = y;

    // The selected Via type's column is tinted from the header to the pad row.
    let rows_height = row * (s.entries.len() + 2) as f32;
    if let Sel::Via(j) = s.sel
        && let Some(x) = via_x.get(j)
    {
        let r = Rect::from_min_size(pos2(*x, origin.y), vec2(via_w, row + rows_height + 3.0 * row));
        painter.rect_filled(r, t.radius.row, c.accent.subtle);
    }

    // The rows: mask, the entries, mask.
    let mask = kit::mask_color(&s.mask);
    let mask_row = |y: f32| {
        let band = col(0, y);
        let thin = Rect::from_center_size(band.center(), vec2(band.width(), 8.0));
        kit::paint_band(&painter, &t, thin, mask);
        let label = format!("Solder mask · {}", s.mask);
        kit::paint_text(&painter, col(2, y).left_center(), Align2::LEFT_CENTER, &t.ty.body, label, c.text.muted);
    };
    mask_row(y);
    y += row;
    let first_row = y;
    let mut band_rows = Vec::new();
    for i in 0..s.entries.len() {
        let line = Rect::from_min_max(pos2(origin.x, y), pos2(right, y + row));
        band_rows.push(line);
        let id = s.entries[i].id;
        let hit = kit::hit(ui, line, ("b-row", id), Sense::click());
        let on = s.sel == Sel::Entry(id);
        if on {
            painter.rect_filled(line.expand2(vec2(4.0, 0.0)), t.radius.row, c.accent.subtle);
        } else if hit.hovered() {
            painter.rect_filled(line.expand2(vec2(4.0, 0.0)), t.radius.row, c.fill.hover);
        }
        if hit.clicked() {
            select = Some(Sel::Entry(id));
        }
        // The band: a dielectric fills its row; copper is a strip between its neighbours.
        let band = col(0, y);
        let copper = s.copper_number(i);
        if copper.is_some() {
            let above = i.checked_sub(1).map(|k| kit::entry_color(&s.entries[k]));
            let below = s.entries.get(i + 1).map(kit::entry_color);
            let (upper, lower) = band.split_top_bottom_at_fraction(0.5);
            painter.rect_filled(upper, 0.0, above.unwrap_or(mask));
            painter.rect_filled(lower, 0.0, below.unwrap_or(mask));
            let strip = Rect::from_center_size(band.center(), vec2(band.width(), 10.0));
            kit::paint_band(&painter, &t, strip, kit::COPPER);
        } else {
            painter.rect_filled(band, 0.0, kit::entry_color(&s.entries[i]));
        }
        painter.vline(band.left(), band.y_range(), Stroke::new(t.stroke.hairline, c.stroke.strong));
        painter.vline(band.right(), band.y_range(), Stroke::new(t.stroke.hairline, c.stroke.strong));

        if let Some(n) = copper {
            let at = col(1, y).left_center();
            kit::paint_text(&painter, at, Align2::LEFT_CENTER, &t.ty.numeric, format!("L{}", n + 1), c.text.muted);
        }
        let z = s.impedance(i);
        let e = &mut s.entries[i];
        kit::cell(ui, col(2, y), |ui| ui.add(widgets::text_field(&mut e.name).width(edges[2].1)));
        kit::cell(ui, col(3, y), |ui| {
            let shown = e.kind_name();
            widgets::dropdown(("b-kind", id), shown)
                .width(edges[3].1)
                .max_width(edges[3].1)
                .show(ui, |ui| match &mut e.kind {
                    Kind::Copper { role } => {
                        for r in Role::ALL {
                            widgets::choice_value(ui, role, r, r.name());
                        }
                    }
                    Kind::Dielectric { make } => {
                        for m in [Make::Core, Make::Prepreg] {
                            widgets::choice_value(ui, make, m, m.name());
                        }
                    }
                });
        });
        kit::cell(ui, col(4, y), |ui| ui.add(widgets::text_field(&mut e.material).width(edges[4].1)));
        kit::cell(ui, col(5, y), |ui| kit::thickness(ui, e, COLUMNS[5].1));
        if e.is_copper() {
            let (text, ink) = match (z, e.role()) {
                (Some(z), _) => (format!("{} · {}", z.text(), z.model), c.text.primary),
                (None, Some(Role::Plane)) => ("Plane".to_owned(), c.text.muted),
                _ => ("No reference plane".to_owned(), c.text.muted),
            };
            kit::paint_text(&painter, col(8, y).left_center(), Align2::LEFT_CENTER, &t.ty.body, text, ink);
        } else {
            kit::cell(ui, col(6, y), |ui| kit::number(ui, &mut e.dk, COLUMNS[6].1, ""));
            kit::cell(ui, col(7, y), |ui| kit::number(ui, &mut e.df, COLUMNS[7].1, ""));
        }
        y += row;
    }
    mask_row(y);
    y += row;
    painter.hline(origin.x..=right, y, rule);

    // Via types: a barrel down each column; a click on a copper row moves the nearer end.
    for (j, x) in via_x.iter().enumerate() {
        let center = x + via_w / 2.0;
        // The barrel runs between the copper strips, which are 10 px tall mid-row.
        let strips: Vec<Rect> = band_rows
            .iter()
            .map(|r| Rect::from_center_size(pos2(center, r.center().y), vec2(via_w, 10.0)))
            .collect();
        for (i, line) in band_rows.iter().enumerate() {
            let Some(n) = s.copper_number(i) else {
                continue;
            };
            let cell = Rect::from_min_size(pos2(*x, line.top()), vec2(via_w, row));
            let hit = kit::hit(ui, cell, ("b-via-cell", j, i), Sense::click());
            let via = &s.vias[j];
            let (a, b) = (via.from.min(via.to), via.from.max(via.to));
            if hit.hovered() && !(a..=b).contains(&n) || hit.hovered() && n != via.from && n != via.to {
                painter.circle_stroke(cell.center(), 5.0, Stroke::new(1.5, c.accent.solid));
            }
            if hit.clicked() {
                let via = &mut s.vias[j];
                let nearer_from = n.abs_diff(via.from) <= n.abs_diff(via.to);
                if nearer_from && n != via.to {
                    via.from = n;
                } else if n != via.from {
                    via.to = n;
                }
                select = Some(Sel::Via(j));
            }
        }
        kit::barrel(s, j, center, &strips, 7.0).paint(&painter, &t);
    }

    // Under the stack: each Via type's kind, drill and pad, then the totals.
    let class: Vec<&str> = s.vias.iter().map(|v| v.class(s.coppers())).collect();
    for (label, k) in [("Via kind", 0), ("Drill", 1), ("Pad", 2)] {
        let at = col(8, y).right_center();
        kit::paint_text(&painter, at, Align2::RIGHT_CENTER, &t.ty.small, label, c.text.muted);
        for (j, x) in via_x.iter().enumerate() {
            let r = Rect::from_min_size(pos2(*x, y), vec2(via_w, row));
            let via = &mut s.vias[j];
            match k {
                0 => {
                    kit::paint_text(&painter, r.center(), Align2::CENTER_CENTER, &t.ty.body, class[j], c.text.primary);
                }
                1 => {
                    kit::cell(ui, r, |ui| kit::number(ui, &mut via.drill, VIA, "mm"));
                }
                _ => {
                    kit::cell(ui, r, |ui| kit::number(ui, &mut via.pad, VIA, "mm"));
                }
            }
        }
        // The totals share these rows, under the Thickness column.
        let (copper, dielectric, total) = s.totals();
        let (name, value) = [("Copper", copper), ("Dielectric", dielectric), ("Total", total)][k];
        let style = if k == 2 { &t.ty.body_strong } else { &t.ty.body };
        kit::paint_text(&painter, col(2, y).left_center(), Align2::LEFT_CENTER, style, name, c.text.primary);
        kit::paint_text(&painter, col(5, y).left_center() + vec2(t.space.control_x, 0.0), Align2::LEFT_CENTER, &t.ty.numeric, format!("{} mm", mm(value)), c.text.primary);
        y += row;
    }
    let ordered = Rect::from_min_size(pos2(edges[2].0, y), vec2(edges[2].1, row));
    kit::paint_text(&painter, ordered.left_center(), Align2::LEFT_CENTER, &t.ty.body, "Ordered", c.text.primary);
    kit::cell(ui, col(5, y), |ui| kit::number(ui, &mut s.target, COLUMNS[5].1, "mm"));
    let (line, ok) = s.total_line();
    if !ok {
        let at = col(6, y).left_center();
        let text = format!("The stack adds up to {line}.");
        kit::paint_text(&painter, at, Align2::LEFT_CENTER, &t.ty.body, text, c.warning.text);
    }
    y += row;

    // The same stack to scale, in a strip left of the grid, tied to the rows.
    let scale = Rect::from_min_max(pos2(full.left(), first_row), pos2(full.left() + SCALE, first_row + row * s.entries.len() as f32));
    let heights = kit::band_heights(s, scale.height(), true);
    let bands = kit::band_rects(scale.min, SCALE, &heights);
    for (i, band) in bands.iter().enumerate() {
        let e = &s.entries[i];
        painter.rect_filled(*band, 0.0, kit::entry_color(e));
        let on = s.sel == Sel::Entry(e.id);
        let stroke = if on {
            Stroke::new(1.5, c.accent.solid)
        } else {
            Stroke::new(t.stroke.hairline, c.stroke.hairline)
        };
        let to = pos2(origin.x - 2.0, band_rows[i].center().y);
        painter.line_segment([pos2(scale.right(), band.center().y), to], stroke);
    }
    painter.rect_stroke(scale, 0.0, Stroke::new(t.stroke.hairline, c.stroke.strong), egui::StrokeKind::Outside);
    kit::paint_text(&painter, scale.left_bottom() + vec2(0.0, row + 4.0), Align2::LEFT_TOP, &t.ty.small, "To scale", c.text.muted);

    let note = "Finish, mask and silkscreen colours and Fab Notes stay in Board Setup.";
    kit::paint_text(&painter, pos2(origin.x, y + 8.0), Align2::LEFT_TOP, &t.ty.small, note, c.text.muted);
    let _ = top;
    if let Some(sel) = select {
        s.sel = sel;
    }
    ui.allocate_rect(full, Sense::hover());
}

/// The strip over the grid: the sample, and what can be done to the selected row or
/// column.
fn toolbar(ui: &mut Ui, s: &mut Stack, strip: Rect) {
    let t = ui.tokens();
    kit::cell(ui, strip, |ui| {
        ui.spacing_mut().item_spacing.x = t.space.gap;
        kit::presets(ui, s);
        widgets::bar_separator(ui);
        let entry = s.selected_entry();
        let copper = entry.and_then(|i| s.copper_number(i));
        let inner = copper.is_some_and(|n| n > 0 && n + 1 < s.coppers());
        let add = ui.add_enabled(entry.is_some(), widgets::button("Add Layer"));
        let remove = ui.add_enabled(inner, widgets::button("Remove Layer"));
        let up = ui.add_enabled(copper.is_some_and(|n| n > 0), widgets::button("Move up"));
        let down = ui.add_enabled(
            copper.is_some_and(|n| n + 1 < s.coppers()),
            widgets::button("Move down"),
        );
        if let Some(i) = entry {
            if add.clicked() {
                if copper.is_some() {
                    s.add_copper_near(i);
                } else {
                    s.split_with_copper(i);
                }
            } else if remove.clicked() {
                s.remove_copper(i);
            } else if up.clicked() || down.clicked() {
                let down = down.clicked();
                s.move_copper(i, down);
                let n = copper.unwrap_or(0);
                let n = if down { n + 1 } else { n.saturating_sub(1) };
                let at = s.copper_entry(n);
                s.sel = Sel::Entry(s.entries[at].id);
            }
        }
        widgets::bar_separator(ui);
        let via = match s.sel {
            Sel::Via(j) => Some(j),
            _ => None,
        };
        let can = via.is_some() && s.vias.len() > 1;
        if ui.add_enabled(can, widgets::button("Remove Via type")).clicked()
            && let Some(j) = via
        {
            s.remove_via(j);
        }
        widgets::bar_separator(ui);
        kit::muted(ui, "Impedance at Track width");
        kit::number(ui, &mut s.track, 84.0, "mm");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            kit::symmetry(ui, s);
        });
    });
}
