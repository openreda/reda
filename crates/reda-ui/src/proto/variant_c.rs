//! PROTOTYPE (issue #330), variant C · Workbench: tables to edit, a drawing to check.
//!
//! Three panes. On the left the Layers table and the Via types table, built from REDA's
//! own table widget: every edit is a cell or a button. In the middle a faithful preview
//! of the board, to scale by default, where a click selects and nothing else. On the
//! right the readouts and everything about the board as ordered: the impedance of every
//! signal Layer, the thickness ordered, the finish and the colours. In this variant the
//! whole Stackup page of Board Setup moves here.

use egui::{Align, Align2, Layout, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2};

use super::View;
use super::kit;
use super::model::{Kind, Make, Role, Sel, Stack, mm};
use crate::theme::TokensExt as _;
use crate::widgets::{self, Column, RowLook, Selection};

const LEFT: f32 = 470.0;
const RIGHT: f32 = 310.0;
const MASK: f32 = 6.0;

pub fn show(ui: &mut Ui, s: &mut Stack, v: &mut View) {
    let t = ui.tokens();
    let pad = t.space.panel;
    let full = ui.available_rect_before_wrap().shrink(pad);
    let (left, right) = (t.size.of(LEFT), t.size.of(RIGHT));
    let a = Rect::from_min_size(full.min, vec2(left, full.height()));
    let c = Rect::from_min_max(pos2(full.right() - right, full.top()), full.max);
    let b = Rect::from_min_max(pos2(a.right() + pad, full.top()), pos2(c.left() - pad, full.bottom()));
    pane(ui, a, |ui| tables(ui, s));
    pane(ui, b, |ui| preview(ui, s, v));
    pane(ui, c, |ui| readouts(ui, s));
    ui.allocate_rect(full, Sense::hover());
}

/// One pane: a card filling `rect`.
fn pane(ui: &mut Ui, rect: Rect, add: impl FnOnce(&mut Ui)) {
    let t = ui.tokens();
    let layout = Layout::top_down(Align::Min);
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect).layout(layout));
    widgets::card_frame(&t).show(&mut child, |ui| {
        ui.set_width(rect.width() - 2.0 * t.space.panel - 2.0);
        ui.set_min_height(rect.height() - 2.0 * t.space.panel - 2.0);
        ui.spacing_mut().item_spacing.y = t.space.stack;
        add(ui);
    });
}

fn title(ui: &mut Ui, text: &str) {
    let t = ui.tokens();
    kit::text(ui, &t.ty.title, text, t.color.text.primary);
}

/// The left pane: the Layers and the Via types, as tables.
fn tables(ui: &mut Ui, s: &mut Stack) {
    let t = ui.tokens();
    ui.horizontal(|ui| {
        title(ui, "Layers");
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| kit::presets(ui, s));
    });
    let mut select = None;
    let numbers: Vec<Option<usize>> = (0..s.entries.len()).map(|i| s.copper_number(i)).collect();
    widgets::table("c-layers")
        .columns([
            Column::fixed(t.size.of(24.0)),
            Column::fill(),
            Column::fixed(t.size.of(108.0)),
            Column::fixed(t.size.of(96.0)),
            Column::fixed(t.size.of(100.0)),
        ])
        .show(ui, |table| {
            table.header(|h| {
                h.text("#");
                h.text("Name");
                h.text("Kind");
                h.text("Material");
                h.text("Thickness");
            });
            for (i, e) in s.entries.iter_mut().enumerate() {
                let on = s.sel == Sel::Entry(e.id);
                let look = RowLook::selected(if on {
                    Selection::Live
                } else {
                    Selection::None
                });
                let id = e.id;
                let row = table.row_with(look, |row| {
                    match numbers[i] {
                        Some(n) => row.muted(format!("L{}", n + 1)),
                        None => row.ui(|ui| ui.add(widgets::swatch(kit::entry_color(e)))),
                    };
                    row.text(&e.name);
                    row.ui(|ui| {
                        widgets::dropdown(("c-kind", id), e.kind_name())
                            .width(t.size.of(108.0))
                            .max_width(t.size.of(108.0))
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
                    row.ui(|ui| ui.add(widgets::text_field(&mut e.material).width(t.size.of(96.0))));
                    row.ui(|ui| kit::thickness(ui, e, 100.0));
                });
                if row.clicked() {
                    select = Some(Sel::Entry(id));
                }
            }
        });
    if let Some(sel) = select {
        s.sel = sel;
    }
    let entry = s.selected_entry();
    let copper = entry.and_then(|i| s.copper_number(i));
    let inner = copper.is_some_and(|n| n > 0 && n + 1 < s.coppers());
    ui.horizontal(|ui| {
        let add = ui.add(widgets::button("Add Layer"));
        let remove = ui.add_enabled(inner, widgets::button("Remove"));
        let up = ui.add_enabled(copper.is_some_and(|n| n > 0), widgets::button("Move up"));
        let down = ui.add_enabled(
            copper.is_some_and(|n| n + 1 < s.coppers()),
            widgets::button("Move down"),
        );
        if add.clicked() {
            // With nothing selected the new Layer goes in the middle of the stack.
            let i = entry.unwrap_or(s.entries.len() / 2);
            if s.entries[i].is_copper() {
                s.add_copper_near(i);
            } else {
                s.split_with_copper(i);
            }
        } else if let Some(i) = entry {
            if remove.clicked() {
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
    });
    if let Some(i) = entry.filter(|i| !s.entries[*i].is_copper()) {
        let e = &mut s.entries[i];
        ui.horizontal(|ui| {
            kit::muted(ui, format!("{}: Dk", e.name));
            kit::number(ui, &mut e.dk, 64.0, "");
            kit::muted(ui, "Df");
            kit::number(ui, &mut e.df, 64.0, "");
        });
    }

    ui.add_space(t.space.section);
    title(ui, "Via types");
    let names: Vec<String> = (0..s.coppers())
        .map(|n| format!("L{} {}", n + 1, s.entries[s.copper_entry(n)].name))
        .collect();
    let coppers = s.coppers();
    let mut select = None;
    widgets::table("c-vias")
        .columns([
            Column::fill(),
            Column::fixed(t.size.of(92.0)),
            Column::fixed(t.size.of(92.0)),
            Column::fixed(t.size.of(74.0)),
            Column::fixed(t.size.of(74.0)),
        ])
        .show(ui, |table| {
            table.header(|h| {
                h.text("Name");
                h.text("From");
                h.text("To");
                h.text("Drill");
                h.text("Pad");
            });
            for (j, via) in s.vias.iter_mut().enumerate() {
                let on = s.sel == Sel::Via(j);
                let look = RowLook::selected(if on {
                    Selection::Live
                } else {
                    Selection::None
                });
                let class = via.class(coppers);
                let row = table.row_with(look, |row| {
                    row.ui(|ui| {
                        ui.add(widgets::text_field(&mut via.name).width(t.size.of(96.0)));
                        kit::muted(ui, class);
                    });
                    for (which, end) in [("from", &mut via.from), ("to", &mut via.to)] {
                        row.ui(|ui| {
                            widgets::dropdown(("c-via", j, which), names[*end].clone())
                                .width(t.size.of(92.0))
                                .max_width(t.size.of(92.0))
                                .show(ui, |ui| {
                                    for (n, name) in names.iter().enumerate() {
                                        widgets::choice_value(ui, end, n, name);
                                    }
                                });
                        });
                    }
                    row.ui(|ui| kit::number(ui, &mut via.drill, 74.0, ""));
                    row.ui(|ui| kit::number(ui, &mut via.pad, 74.0, ""));
                });
                if row.clicked() {
                    select = Some(Sel::Via(j));
                }
            }
        });
    if let Some(sel) = select {
        s.sel = sel;
    }
    ui.horizontal(|ui| {
        if ui.add(widgets::button("Add Via type")).clicked() {
            s.add_via();
        }
        let via = match s.sel {
            Sel::Via(j) if s.vias.len() > 1 => Some(j),
            _ => None,
        };
        if ui.add_enabled(via.is_some(), widgets::button("Remove")).clicked()
            && let Some(j) = via
        {
            s.remove_via(j);
        }
    });
    kit::muted(ui, "Drill and pad in mm. Routing offers only these; DRC flags any other Via.");
}

/// The middle pane: the board as it will be built. A click selects; nothing is edited here.
fn preview(ui: &mut Ui, s: &mut Stack, v: &mut View) {
    let t = ui.tokens();
    let c = &t.color;
    ui.horizontal(|ui| {
        title(ui, "Cross-section");
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let views = [(true, "To scale"), (false, "Schematic")];
            ui.add(widgets::segmented(&mut v.to_scale, &views));
        });
    });
    let area = ui.available_rect_before_wrap();
    let painter = ui.painter_at(area);
    let to_scale = v.to_scale;
    let labels = t.size.of(150.0);
    let dim = 40.0;
    let width = (area.width() - 2.0 * labels - dim).max(120.0);
    let left = area.left() + labels + dim;
    let height = (area.height() - 2.0 * MASK - 70.0).max(100.0);
    let heights = kit::band_heights(s, height, to_scale);
    let rects = kit::band_rects(pos2(left, area.top() + 40.0 + MASK), width, &heights);
    let board = Rect::from_min_max(rects[0].min, rects[rects.len() - 1].max);
    let mask = kit::mask_color(&s.mask);
    for y in [board.top() - MASK, board.bottom()] {
        kit::paint_band(&painter, &t, Rect::from_min_size(pos2(left, y), vec2(width, MASK)), mask);
    }
    let mut select = None;
    // Labels are kept from overlapping when bands are thin: each sits at or below the last.
    let mut next_y = f32::MIN;
    for (i, rect) in rects.iter().enumerate() {
        let e = &s.entries[i];
        let hit = kit::hit(ui, *rect, ("c-band", e.id), Sense::click());
        kit::paint_band(&painter, &t, *rect, kit::entry_color(e));
        let on = s.sel == Sel::Entry(e.id);
        if on {
            let stroke = Stroke::new(t.stroke.accent_line, c.accent.solid);
            painter.rect_stroke(rect.expand(1.0), 0.0, stroke, StrokeKind::Outside);
        }
        if hit.clicked() {
            select = Some(Sel::Entry(e.id));
        }
        let y = rect.center().y.max(next_y);
        next_y = y + t.ty.small.line;
        let ink = if on { c.accent.text } else { c.text.primary };
        let leader = Stroke::new(t.stroke.hairline, c.stroke.strong);
        painter.line_segment([pos2(board.right(), rect.center().y), pos2(board.right() + 8.0, y)], leader);
        let name = match s.copper_number(i) {
            Some(n) => format!("L{} {}", n + 1, e.name),
            None => format!("{} {}", e.kind_name(), e.material),
        };
        let text = format!("{name} · {} mm", e.text);
        kit::paint_text(&painter, pos2(board.right() + 12.0, y), Align2::LEFT_CENTER, &t.ty.small, text, ink);
    }
    // The thickness, dimensioned on the left.
    let x = left - dim / 2.0;
    let line = Stroke::new(t.stroke.hairline, c.text.primary);
    painter.vline(x, board.y_range(), line);
    for y in [board.top(), board.bottom()] {
        painter.hline(x - 5.0..=left - 4.0, y, line);
    }
    let total = format!("{} mm", mm(s.totals().2));
    kit::paint_text(&painter, pos2(x - 8.0, board.center().y), Align2::RIGHT_CENTER, &t.ty.numeric, total, c.text.primary);

    let count = s.vias.len();
    let pitch = (width * 0.8 / count as f32).clamp(24.0, 70.0);
    let first = board.center().x - pitch * (count as f32 - 1.0) / 2.0;
    for j in 0..count {
        let x = first + pitch * j as f32;
        let barrel = kit::barrel(s, j, x, &rects, 7.0);
        let hit = kit::hit(ui, barrel.rect(), ("c-via", j), Sense::click());
        barrel.paint(&painter, &t);
        let y = board.top() - MASK - 5.0 - if j % 2 == 1 { 14.0 } else { 0.0 };
        let ink = if s.sel == Sel::Via(j) { c.accent.text } else { c.text.muted };
        kit::paint_text(&painter, pos2(x, y), Align2::CENTER_BOTTOM, &t.ty.small, &s.vias[j].name, ink);
        if hit.clicked() {
            select = Some(Sel::Via(j));
        }
    }
    let note = if to_scale {
        "Drawn to scale."
    } else {
        "Not to scale: bands are sized by kind."
    };
    kit::paint_text(&painter, pos2(left, board.bottom() + MASK + 10.0), Align2::LEFT_TOP, &t.ty.small, note, c.text.muted);
    if let Some(sel) = select {
        s.sel = sel;
    }
    ui.allocate_rect(area, Sense::hover());
}

/// The right pane: the impedance of every signal Layer, and the board as ordered.
fn readouts(ui: &mut Ui, s: &mut Stack) {
    let t = ui.tokens();
    title(ui, "Impedance");
    ui.horizontal(|ui| {
        kit::muted(ui, "At Track width");
        kit::number(ui, &mut s.track, 84.0, "mm");
    });
    widgets::table("c-impedance")
        .columns([Column::auto(), Column::fill(), Column::auto().number()])
        .show(ui, |table| {
            table.header(|h| {
                h.text("Layer");
                h.text("Refers to");
                h.text("Z0");
            });
            for i in 0..s.entries.len() {
                let e = &s.entries[i];
                if !e.is_copper() || e.role() == Some(Role::Plane) {
                    continue;
                }
                let z = s.impedance(i);
                let on = s.sel == Sel::Entry(e.id);
                let look = RowLook::selected(if on {
                    Selection::Live
                } else {
                    Selection::None
                });
                table.row_with(look, |row| {
                    row.text(&e.name);
                    match &z {
                        Some(z) => {
                            row.muted(&z.over);
                            row.text(z.text());
                        }
                        None => {
                            row.muted("No plane Layer");
                            row.muted("–");
                        }
                    }
                });
            }
        });
    kit::muted(ui, "A readout only: it sets no rule.");

    ui.add_space(t.space.section);
    title(ui, "Board");
    let (copper, dielectric, total) = s.totals();
    let field = 150.0;
    widgets::form("c-board").show(ui, |ui| {
        widgets::form_row(ui, "Ordered", |ui| kit::number(ui, &mut s.target, field, "mm"));
        for (name, um) in [("Total", total), ("Copper", copper), ("Dielectric", dielectric)] {
            widgets::form_row(ui, name, |ui| {
                kit::text(ui, &t.ty.numeric, format!("{} mm", mm(um)), t.color.text.primary)
            });
        }
        widgets::form_row(ui, "Finish", |ui| {
            ui.add(widgets::text_field(&mut s.finish).width(t.size.of(field)))
        });
        widgets::form_row(ui, "Solder mask", |ui| {
            ui.add(widgets::text_field(&mut s.mask).width(t.size.of(field)))
        });
        widgets::form_row(ui, "Silkscreen", |ui| {
            ui.add(widgets::text_field(&mut s.silk).width(t.size.of(field)))
        });
    });
    ui.add_space(t.space.stack);
    ui.horizontal(|ui| {
        kit::symmetry(ui, s);
        kit::total(ui, s);
    });
    kit::muted(ui, "Board Setup keeps a summary and a button that opens this tab.");
}
