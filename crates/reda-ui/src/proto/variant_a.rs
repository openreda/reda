//! PROTOTYPE (issue #330), variant A · Drawing: the cross-section is the editor.
//!
//! There is no table. Every entry is a band you click, the numbers sit beside their band
//! and are typed in place, Via types are barrels in the board with ends you drag, and an
//! inspector card holds the rest of what is selected.
//!
//! - Click a band or a barrel: select it. Double-click a band, or click its thickness:
//!   type the thickness.
//! - Drag a copper Layer up or down: reorder it.
//! - Point at a dielectric: `+ Layer` splits it with a new copper Layer.
//! - To scale: drag a dielectric's lower edge to change its thickness.
//! - Drag a selected barrel's end to another copper Layer: change the span.

use egui::{Align, Align2, CursorIcon, Layout, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2};

use super::View;
use super::kit;
use super::model::{Kind, Make, Role, Sel, Stack, mm};
use crate::theme::TokensExt as _;
use crate::widgets;

/// The names beside the board, and the numbers on the other side.
const NAMES: f32 = 170.0;
const NOTES: f32 = 270.0;
/// Room over the board for the Via types' names, and under it for the notes.
const HEADER: f32 = 48.0;
const FOOTER: f32 = 44.0;
const MASK: f32 = 7.0;
const INSPECTOR: f32 = 290.0;

enum Act {
    Select(Sel),
    Edit(u32),
    Split(usize),
    Place(usize, usize),
    Resize(usize, f64),
    Span(usize, bool, usize),
    AddVia,
}

pub fn show(ui: &mut Ui, s: &mut Stack, v: &mut View) {
    let t = ui.tokens();
    let pad = t.space.panel;
    let full = ui.available_rect_before_wrap().shrink(pad);
    let strip = Rect::from_min_size(full.min, vec2(full.width(), t.control_height()));
    kit::cell(ui, strip, |ui| {
        ui.spacing_mut().item_spacing.x = t.space.gap * 2.0;
        kit::presets(ui, s);
        let views = [(false, "Schematic"), (true, "To scale")];
        ui.add(widgets::segmented(&mut v.to_scale, &views));
        kit::muted(ui, "Impedance at Track width");
        kit::number(ui, &mut s.track, 84.0, "mm");
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            kit::total(ui, s);
            kit::symmetry(ui, s);
        });
    });
    let body = Rect::from_min_max(pos2(full.left(), strip.bottom() + pad), full.max);
    let inspector_width = t.size.of(INSPECTOR);
    let card = Rect::from_min_max(pos2(body.right() - inspector_width, body.top()), body.max);
    let draw = Rect::from_min_max(body.min, pos2(card.left() - pad, body.bottom()));
    drawing(ui, s, v, draw);
    inspector(ui, s, card);
    ui.allocate_rect(full, Sense::hover());
}

fn drawing(ui: &mut Ui, s: &mut Stack, v: &mut View, area: Rect) {
    let t = ui.tokens();
    let c = &t.color;
    let painter = ui.painter_at(area);
    let (names, notes) = (t.size.of(NAMES), t.size.of(NOTES));
    let width = (area.width() - names - notes).clamp(160.0, 620.0);
    let left = area.left() + names + ((area.width() - names - notes - width) / 2.0).max(0.0);
    let height = (area.height() - HEADER - FOOTER - 2.0 * MASK).max(120.0);
    let heights = kit::band_heights(s, height, v.to_scale);
    let top = area.top() + HEADER + MASK;
    let rects = kit::band_rects(pos2(left, top), width, &heights);
    let board = Rect::from_min_max(rects[0].min, rects[rects.len() - 1].max);
    let mut acts = Vec::new();
    let pointer = ui.input(|i| i.pointer.interact_pos());

    // The solder mask over and under the stack: drawn, not an entry.
    let mask = kit::mask_color(&s.mask);
    for (y, align) in [(board.top() - MASK, Align2::RIGHT_BOTTOM), (board.bottom(), Align2::RIGHT_TOP)] {
        let r = Rect::from_min_size(pos2(left, y), vec2(width, MASK));
        kit::paint_band(&painter, &t, r, mask);
        let at = pos2(left - 12.0, if y < board.top() { r.bottom() } else { r.top() });
        let label = format!("Solder mask · {}", s.mask);
        kit::paint_text(&painter, at, align, &t.ty.small, label, c.text.muted);
    }

    // The copper Layer a pointer at height `y` is nearest to, by copper number.
    let centres: Vec<f32> = (0..s.entries.len())
        .filter(|i| s.entries[*i].is_copper())
        .map(|i| rects[i].center().y)
        .collect();
    let nearest = |y: f32| -> usize {
        let mut best = (0, f32::MAX);
        for (n, c) in centres.iter().enumerate() {
            if (c - y).abs() < best.1 {
                best = (n, (c - y).abs());
            }
        }
        best.0
    };

    for i in 0..s.entries.len() {
        let rect = rects[i];
        let cy = rect.center().y;
        let copper = s.copper_number(i);
        let id = s.entries[i].id;
        let sense = if copper.is_some() {
            Sense::click_and_drag()
        } else {
            Sense::click()
        };
        let band = kit::hit(ui, rect, ("a-band", id), sense);
        kit::paint_band(&painter, &t, rect, kit::entry_color(&s.entries[i]));
        let selected = s.sel == Sel::Entry(id);
        if selected {
            let stroke = Stroke::new(t.stroke.accent_line, c.accent.solid);
            painter.rect_stroke(rect.expand(1.0), 0.0, stroke, StrokeKind::Outside);
        } else if band.hovered() && v.dragging.is_none() {
            let stroke = Stroke::new(t.stroke.hairline, c.text.primary);
            painter.rect_stroke(rect, 0.0, stroke, StrokeKind::Outside);
        }
        if band.clicked() {
            acts.push(Act::Select(Sel::Entry(id)));
        }
        if band.double_clicked() {
            acts.push(Act::Edit(id));
        }

        // The name on the left.
        let e = &s.entries[i];
        let ink = if selected { c.accent.text } else { c.text.primary };
        match copper {
            Some(n) => {
                let at = pos2(left - 12.0, cy);
                let name = kit::paint_text(&painter, at, Align2::RIGHT_CENTER, &t.ty.body_strong, &e.name, ink);
                let at = pos2(name.left() - 8.0, cy);
                let number = format!("L{}", n + 1);
                kit::paint_text(&painter, at, Align2::RIGHT_CENTER, &t.ty.numeric_small, number, c.text.muted);
            }
            None => {
                let at = pos2(left - 12.0, cy);
                let label = format!("{} {}", e.kind_name(), e.material);
                kit::paint_text(&painter, at, Align2::RIGHT_CENTER, &t.ty.small, label, c.text.muted);
            }
        }

        // The numbers on the right: the thickness, typed in place, then the readout.
        let field = Rect::from_min_size(pos2(board.right() + 12.0, cy - t.control_height() / 2.0), vec2(t.size.of(96.0), t.control_height()));
        if v.editing == Some(id) {
            let r = kit::cell(ui, field, |ui| kit::thickness(ui, &mut s.entries[i], 96.0));
            if r.lost_focus() {
                v.editing = None;
            } else if !r.has_focus() {
                r.request_focus();
            }
        } else {
            let label = kit::hit(ui, field, ("a-thickness", id), Sense::click());
            let ink = if label.hovered() { c.accent.text } else { c.text.primary };
            let at = pos2(field.left(), cy);
            let text = format!("{} mm", s.entries[i].text);
            let drawn = kit::paint_text(&painter, at, Align2::LEFT_CENTER, &t.ty.numeric, text, ink);
            if label.hovered() {
                ui.ctx().set_cursor_icon(CursorIcon::Text);
                painter.hline(drawn.x_range(), drawn.bottom(), Stroke::new(t.stroke.hairline, ink));
            }
            if label.clicked() {
                acts.push(Act::Edit(id));
            }
        }
        let e = &s.entries[i];
        let note = match (&e.kind, s.impedance(i)) {
            (Kind::Copper { role }, Some(z)) => {
                format!("{} · {} {} over {}", role.name(), z.text(), z.model.to_lowercase(), z.over)
            }
            (Kind::Copper { role: Role::Plane }, None) => "Plane".to_owned(),
            (Kind::Copper { role }, None) => format!("{} · no reference plane", role.name()),
            (Kind::Dielectric { .. }, _) => format!("Dk {}", e.dk),
        };
        let at = pos2(field.right() + 8.0, cy);
        kit::paint_text(&painter, at, Align2::LEFT_CENTER, &t.ty.small, note, c.text.muted);

        // Reordering: drag a copper Layer to another copper Layer's place.
        if let Some(n) = copper {
            if band.drag_started() {
                v.dragging = Some(n);
                acts.push(Act::Select(Sel::Entry(id)));
            }
            if band.dragged() && v.dragging == Some(n) {
                ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
                if let Some(p) = pointer {
                    let target = rects[s.copper_entry(nearest(p.y))];
                    let stroke = Stroke::new(3.0, c.accent.solid);
                    painter.hline(left - 8.0..=board.right() + 8.0, target.center().y, stroke);
                    let ghost = Rect::from_center_size(pos2(board.center().x, p.y), rect.size());
                    painter.rect_filled(ghost, 0.0, kit::COPPER.gamma_multiply(0.6));
                }
            }
            if band.drag_stopped() && v.dragging == Some(n) {
                v.dragging = None;
                if let Some(p) = pointer {
                    acts.push(Act::Place(n, nearest(p.y)));
                }
            }
        }
    }

    // Via types: barrels in the right half of the board, their names over it.
    let count = s.vias.len();
    let pitch = ((width * 0.55) / count as f32).clamp(26.0, 64.0);
    let first = board.right() - 34.0 - pitch * (count as f32 - 1.0);
    for j in 0..count {
        let x = first + pitch * j as f32;
        let barrel = kit::barrel(s, j, x, &rects, 7.0);
        let hit = kit::hit(ui, barrel.rect(), ("a-via", j), Sense::click());
        barrel.paint(&painter, &t);
        let selected = s.sel == Sel::Via(j);
        // Names step down in two rows so neighbours do not run into each other.
        let y = board.top() - MASK - 6.0 - if j % 2 == 1 { 15.0 } else { 0.0 };
        let ink = if selected { c.accent.text } else { c.text.primary };
        let name = kit::paint_text(&painter, pos2(x, y), Align2::CENTER_BOTTOM, &t.ty.small, &s.vias[j].name, ink);
        painter.vline(x, name.bottom() + 1.0..=board.top() - MASK, Stroke::new(t.stroke.hairline, c.stroke.strong));
        let head = kit::hit(ui, name.expand(3.0), ("a-via-name", j), Sense::click());
        if hit.clicked() || head.clicked() {
            acts.push(Act::Select(Sel::Via(j)));
        }
        if selected {
            for (start, band) in [(true, barrel.from), (false, barrel.to)] {
                let at = pos2(x, band.center().y);
                let grip = Rect::from_center_size(at, vec2(18.0, 18.0));
                let r = kit::hit(ui, grip, ("a-via-end", j, start), Sense::drag());
                painter.circle(at, 5.5, c.accent.solid, Stroke::new(1.5, c.surface.card));
                if r.hovered() || r.dragged() {
                    ui.ctx().set_cursor_icon(CursorIcon::ResizeVertical);
                }
                if let (true, Some(p)) = (r.dragged(), pointer) {
                    acts.push(Act::Span(j, start, nearest(p.y)));
                }
            }
        }
    }
    let add = Rect::from_min_size(pos2(board.right() + 12.0, area.top()), vec2(t.size.of(120.0), t.control_height()));
    if kit::cell(ui, add, |ui| ui.add(widgets::button("+ Via type").flat())).clicked() {
        acts.push(Act::AddVia);
    }

    // Over the bands: the `+ Layer` button of the dielectric pointed at, and the lower
    // edge that resizes it when the drawing is to scale.
    if v.dragging.is_none() {
        for i in 0..s.entries.len() {
            if s.entries[i].is_copper() {
                continue;
            }
            let rect = rects[i];
            let id = s.entries[i].id;
            if v.to_scale {
                let edge = Rect::from_center_size(rect.left_bottom() + vec2(width * 0.2, 0.0), vec2(width * 0.4, 8.0));
                let r = kit::hit(ui, edge, ("a-edge", id), Sense::drag());
                if r.hovered() || r.dragged() {
                    ui.ctx().set_cursor_icon(CursorIcon::ResizeVertical);
                    let stroke = Stroke::new(t.stroke.accent_line, c.accent.solid);
                    painter.hline(rect.x_range(), rect.bottom(), stroke);
                }
                if r.dragged() {
                    let per_px = s.entries[i].um / f64::from(rect.height());
                    let um = s.entries[i].um + f64::from(r.drag_delta().y) * per_px;
                    acts.push(Act::Resize(i, um.round()));
                }
            }
            let fits = rect.height() >= t.control_height();
            if fits && ui.rect_contains_pointer(rect) {
                let at = Rect::from_min_size(
                    pos2(rect.left() + 8.0, rect.center().y - t.control_height() / 2.0),
                    vec2(t.size.of(90.0), t.control_height()),
                );
                if kit::cell(ui, at, |ui| ui.add(widgets::button("+ Layer"))).clicked() {
                    acts.push(Act::Split(i));
                }
            }
        }
    }

    let at = pos2(left, board.bottom() + MASK + 12.0);
    let scale = if v.to_scale {
        "Drawn to scale. Drag a dielectric's lower edge to change its thickness."
    } else {
        "Not to scale: bands are sized by kind."
    };
    let line = kit::paint_text(&painter, at, Align2::LEFT_TOP, &t.ty.small, scale, c.text.muted);
    let hint = "Drag a copper Layer to reorder it. Click a thickness to type it.";
    kit::paint_text(&painter, line.left_bottom() + vec2(0.0, 2.0), Align2::LEFT_TOP, &t.ty.small, hint, c.text.muted);

    for act in acts {
        match act {
            Act::Select(sel) => s.sel = sel,
            Act::Edit(id) => {
                s.sel = Sel::Entry(id);
                v.editing = Some(id);
            }
            Act::Split(i) => s.split_with_copper(i),
            Act::Place(from, to) => s.place_copper(from, to),
            Act::Resize(i, um) => s.entries[i].set_um(um.max(20.0)),
            Act::Span(j, start, n) => {
                let via = &mut s.vias[j];
                let (end, other) = if start {
                    (&mut via.from, via.to)
                } else {
                    (&mut via.to, via.from)
                };
                if n != other {
                    *end = n;
                }
            }
            Act::AddVia => s.add_via(),
        }
    }
}

/// The card on the right: what is selected, in full.
fn inspector(ui: &mut Ui, s: &mut Stack, rect: Rect) {
    let t = ui.tokens();
    let layout = Layout::top_down(Align::Min);
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect).layout(layout));
    widgets::card_frame(&t).show(&mut child, |ui| {
        ui.set_width(rect.width() - 2.0 * t.space.panel - 2.0);
        ui.set_min_height(rect.height() - 2.0 * t.space.panel - 2.0);
        ui.spacing_mut().item_spacing.y = t.space.stack;
        let field = 150.0;
        match s.sel {
            Sel::Entry(id) if s.index_of(id).is_some() => {
                let i = s.index_of(id).unwrap_or(0);
                let copper = s.copper_number(i);
                let z = s.impedance(i);
                let track = s.track.clone();
                let title = match copper {
                    Some(n) => format!("Copper Layer L{}", n + 1),
                    None => "Dielectric".to_owned(),
                };
                kit::text(ui, &t.ty.title, title, t.color.text.primary);
                let e = &mut s.entries[i];
                let mut weight = None;
                widgets::form("a-entry").show(ui, |ui| {
                    widgets::form_row(ui, "Name", |ui| {
                        ui.add(widgets::text_field(&mut e.name).width(t.size.of(field)))
                    });
                    match &mut e.kind {
                        Kind::Copper { role } => {
                            widgets::form_row(ui, "Role", |ui| {
                                let roles = Role::ALL.map(|r| (r, r.name()));
                                ui.add(widgets::segmented(role, &roles))
                            });
                            widgets::form_row(ui, "Copper", |ui| {
                                let shown = weight_name(e.um);
                                widgets::dropdown("a-weight", shown)
                                    .width(t.size.of(field))
                                    .show(ui, |ui| {
                                        for (name, um) in WEIGHTS {
                                            if ui.add(widgets::choice(shown == name, name)).clicked() {
                                                weight = Some(um);
                                            }
                                        }
                                    });
                            });
                        }
                        Kind::Dielectric { make } => {
                            widgets::form_row(ui, "Made as", |ui| {
                                let makes = [(Make::Core, "Core"), (Make::Prepreg, "Prepreg")];
                                ui.add(widgets::segmented(make, &makes))
                            });
                            widgets::form_row(ui, "Material", |ui| {
                                ui.add(widgets::text_field(&mut e.material).width(t.size.of(field)))
                            });
                        }
                    }
                    widgets::form_row(ui, "Thickness", |ui| kit::thickness(ui, e, field));
                    if !e.is_copper() {
                        widgets::form_row(ui, "Dk", |ui| kit::number(ui, &mut e.dk, field, ""));
                        widgets::form_row(ui, "Df", |ui| kit::number(ui, &mut e.df, field, ""));
                    }
                });
                if let Some(um) = weight.filter(|um| *um > 0.0) {
                    e.set_um(um);
                }
                if copper.is_some() {
                    ui.add_space(t.space.stack);
                    kit::strong(ui, "Impedance");
                    match z {
                        Some(z) => {
                            kit::text(ui, &t.ty.heading, z.text(), t.color.text.primary);
                            kit::muted(ui, format!("{} over {}, at a {track} mm Track. A readout: it sets no rule.", z.model, z.over));
                        }
                        None => kit::muted(ui, "No readout: a plane Layer, or no plane Layer to refer to."),
                    }
                    ui.add_space(t.space.section);
                    let (mut up, mut down, mut remove, mut add) = (false, false, false, false);
                    ui.horizontal(|ui| {
                        up = ui.add(widgets::button("Move up")).clicked();
                        down = ui.add(widgets::button("Move down")).clicked();
                    });
                    ui.horizontal(|ui| {
                        add = ui.add(widgets::button("Add Layer below")).clicked();
                        remove = ui.add(widgets::button("Remove").danger()).clicked();
                    });
                    if up || down {
                        s.move_copper(i, down);
                        // The selection follows the Layer that moved.
                        let n = copper.unwrap_or(0);
                        let n = if down { n + 1 } else { n.saturating_sub(1) };
                        let at = s.copper_entry(n.min(s.coppers() - 1));
                        s.sel = Sel::Entry(s.entries[at].id);
                    } else if add {
                        s.add_copper_near(i);
                    } else if remove && !s.remove_copper(i) {
                        // The outer two Layers stay.
                    }
                } else {
                    ui.add_space(t.space.section);
                    if ui.add(widgets::button("Add a copper Layer inside")).clicked() {
                        s.split_with_copper(i);
                    }
                }
            }
            Sel::Via(j) if j < s.vias.len() => {
                kit::text(ui, &t.ty.title, "Via type", t.color.text.primary);
                let names: Vec<String> = (0..s.coppers())
                    .map(|n| format!("L{} {}", n + 1, s.entries[s.copper_entry(n)].name))
                    .collect();
                let class = s.vias[j].class(s.coppers());
                let via = &mut s.vias[j];
                widgets::form("a-via").show(ui, |ui| {
                    widgets::form_row(ui, "Name", |ui| {
                        ui.add(widgets::text_field(&mut via.name).width(t.size.of(field)))
                    });
                    for (label, end) in [("From", &mut via.from), ("To", &mut via.to)] {
                        widgets::form_row(ui, label, |ui| {
                            widgets::dropdown(("a-via-end", label), names[*end].clone())
                                .width(t.size.of(field))
                                .show(ui, |ui| {
                                    for (n, name) in names.iter().enumerate() {
                                        widgets::choice_value(ui, end, n, name);
                                    }
                                });
                        });
                    }
                    widgets::form_row(ui, "Drill", |ui| kit::number(ui, &mut via.drill, field, "mm"));
                    widgets::form_row(ui, "Pad", |ui| kit::number(ui, &mut via.pad, field, "mm"));
                    widgets::form_row(ui, "Kind", |ui| kit::text(ui, &t.ty.body, class, t.color.text.primary));
                });
                kit::muted(ui, "Through, blind, buried or micro is read off the span.");
                ui.add_space(t.space.section);
                let only = s.vias.len() == 1;
                let remove = ui.add_enabled(!only, widgets::button("Remove Via type").danger());
                if remove.clicked() {
                    s.remove_via(j);
                }
            }
            _ => {
                kit::text(ui, &t.ty.title, "Stackup", t.color.text.primary);
                let (copper, dielectric, total) = s.totals();
                widgets::form("a-board").show(ui, |ui| {
                    widgets::form_row(ui, "Ordered", |ui| kit::number(ui, &mut s.target, field, "mm"));
                    for (name, um) in [("Total", total), ("Copper", copper), ("Dielectric", dielectric)] {
                        widgets::form_row(ui, name, |ui| {
                            kit::text(ui, &t.ty.numeric, format!("{} mm", mm(um)), t.color.text.primary)
                        });
                    }
                    widgets::form_row(ui, "Copper Layers", |ui| {
                        kit::text(ui, &t.ty.numeric, s.coppers().to_string(), t.color.text.primary)
                    });
                });
                ui.add_space(t.space.section);
                kit::muted(ui, "Click a Layer, a dielectric or a Via type to edit it.");
                kit::muted(ui, "Finish, mask and silkscreen colours and Fab Notes stay in Board Setup.");
            }
        }
    });
}

/// The copper weights offered, with their thickness in micrometres.
const WEIGHTS: [(&str, f64); 4] = [("0.5 oz", 17.5), ("1 oz", 35.0), ("2 oz", 70.0), ("Custom", 0.0)];

fn weight_name(um: f64) -> &'static str {
    (WEIGHTS.iter())
        .find(|(_, w)| (w - um).abs() < 0.3)
        .map_or("Custom", |(name, _)| name)
}
