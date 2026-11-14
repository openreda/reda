//! PROTOTYPE (issue #276): Panel sections, property rows, lists, trees and tables.

use egui::{Align2, Color32, Rect, Sense, Stroke, Ui, pos2, vec2};

use super::super::Tokens;
use super::super::widgets::{self as w, Check, Field, Glyph, Sel, St};
use super::{column, note, s, section};

/// A Panel section header: chevron, title, an optional count at the right. Collapsible.
pub fn section_header(ui: &mut Ui, t: &Tokens, st: St, open: bool, title: &str, trailing: &str, width: f32) {
    let (rect, _) = w::row(ui, t, st, Sel::None, width, t.row_height());
    let c = &t.color;
    let g = if open { Glyph::ChevronDown } else { Glyph::ChevronRight };
    w::glyph(ui, g, pos2(rect.left() + 10.0, rect.center().y), 9.0, c.text.muted);
    let inner = Rect::from_min_max(pos2(rect.left() + 22.0, rect.top()), rect.max - vec2(t.space.row_x, 0.0));
    w::text_in(ui, inner, Align2::LEFT_CENTER, &t.ty.title, title, c.text.primary);
    w::text_in(ui, inner, Align2::RIGHT_CENTER, &t.ty.numeric_small, trailing, c.text.muted);
}

/// A property row: the name in muted text at the left, the value's control at the right.
fn property(ui: &mut Ui, t: &Tokens, width: f32, name: &str, add: impl FnOnce(&mut Ui, f32)) {
    ui.horizontal(|ui| {
        let name_w = (width * 0.38).round();
        let (rect, _) = ui.allocate_exact_size(vec2(name_w, t.control_height()), Sense::hover());
        w::text_in(ui, rect.with_min_x(rect.left() + 22.0), Align2::LEFT_CENTER, &t.ty.body, name, t.color.text.muted);
        add(ui, width - name_w - t.space.gap);
    });
}

fn properties(ui: &mut Ui, t: &Tokens, width: f32) {
    section_header(ui, t, St::Rest, true, "Track", "3 selected", width);
    property(ui, t, width, "Net", |ui, w_| {
        w::combo(ui, t, St::Rest, "GND", w_);
    });
    property(ui, t, width, "Width", |ui, w_| {
        let f = Field { numeric: true, unit: Some("mm"), ..Default::default() };
        w::text_field(ui, t, St::Focus, &mut s("0.250"), w_, f);
    });
    property(ui, t, width, "Layer", |ui, w_| {
        w::combo(ui, t, St::Rest, "Top Copper", w_);
    });
    property(ui, t, width, "Length", |ui, w_| {
        let (rect, _) = ui.allocate_exact_size(vec2(w_, t.control_height()), Sense::hover());
        w::text_in(ui, rect.shrink2(vec2(t.space.control_x - 2.0, 0.0)), Align2::LEFT_CENTER, &t.ty.numeric, "12.446 mm", t.color.text.primary);
    });
    property(ui, t, width, "Locked", |ui, _| {
        w::checkbox(ui, t, St::Rest, Check::Mixed, "");
    });
    ui.add_space(t.space.section - t.space.stack);
    section_header(ui, t, St::Rest, true, "Position", "", width);
    property(ui, t, width, "Start", |ui, w_| {
        let half = (w_ - t.space.gap) / 2.0;
        let f = Field { numeric: true, ..Default::default() };
        w::text_field(ui, t, St::Rest, &mut s("121.920"), half, f);
        w::text_field(ui, t, St::Rest, &mut s("48.260"), half, f);
    });
    property(ui, t, width, "End", |ui, w_| {
        let half = (w_ - t.space.gap) / 2.0;
        let f = Field { numeric: true, ..Default::default() };
        w::text_field(ui, t, St::Rest, &mut s("124.460"), half, f);
        w::text_field(ui, t, St::Disabled, &mut s("48.260"), half, f);
    });
    ui.add_space(t.space.section - t.space.stack);
    section_header(ui, t, St::Hover, false, "Net Class", "Power", width);
    section_header(ui, t, St::Rest, false, "Rules that apply", "4", width);
}

/// The Layers list: visibility check, data swatch, name, the active one selected.
pub fn layers(ui: &mut Ui, t: &Tokens, width: f32, count: usize) {
    ui.spacing_mut().item_spacing.y = 0.0;
    let rows = [
        ("Top Copper", 0xc83434u32, true),
        ("Inner 1", 0xd4c03c, true),
        ("Inner 2", 0x2fb36b, false),
        ("Bottom Copper", 0x3d8fd6, true),
        ("Top Silkscreen", 0xf2f2f2, true),
        ("Board Edge", 0xb85cd6, true),
    ];
    for (i, (name, hex, visible)) in rows.into_iter().take(count).enumerate() {
        let sel = if i == 0 { Sel::Live } else { Sel::None };
        let st = if i == 1 { St::Hover } else { St::Rest };
        let (rect, _) = w::row(ui, t, st, sel, width, t.row_height());
        let c = &t.color;
        let mark = Rect::from_center_size(pos2(rect.left() + t.space.row_x + 8.0, rect.center().y), vec2(16.0, 16.0));
        if visible {
            ui.painter().rect_filled(mark, t.radius.mark, c.accent.solid);
            w::glyph(ui, Glyph::Tick, mark.center(), 10.0, c.accent.on_solid);
        } else {
            ui.painter().rect_filled(mark, t.radius.mark, c.surface.well);
            ui.painter().rect_stroke(mark, t.radius.mark, Stroke::new(1.0, c.stroke.strong), egui::StrokeKind::Inside);
        }
        let sw = Rect::from_center_size(pos2(mark.right() + 8.0 + 7.0, rect.center().y), vec2(14.0, 14.0));
        ui.painter().rect_filled(sw, t.radius.mark, Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8));
        ui.painter().rect_stroke(sw, t.radius.mark, Stroke::new(1.0, c.stroke.strong), egui::StrokeKind::Inside);
        let ink = if visible { c.text.primary } else { c.text.muted };
        w::text_in(ui, rect.with_min_x(sw.right() + 8.0), Align2::LEFT_CENTER, &t.ty.body, name, ink);
    }
}

/// A tree: 16px indent per level, a chevron on what opens, an icon, the name.
pub fn tree(ui: &mut Ui, t: &Tokens, width: f32, selected: Sel) {
    ui.spacing_mut().item_spacing.y = 0.0;
    let rows: [(usize, Option<bool>, &str, &str, St, bool); 8] = [
        (0, Some(true), "dock", "usb_c_mcu", St::Rest, false),
        (1, Some(true), "document_schematic", "Schematic", St::Rest, false),
        (2, None, "document_schematic", "Sheet 1", St::Rest, false),
        (2, None, "document_schematic", "Power", St::Rest, true),
        (2, None, "document_schematic", "USB", St::Hover, false),
        (1, None, "document_pcb", "usb_c_mcu.pcb", St::Rest, false),
        (1, Some(false), "render", "Output Jobs", St::Rest, false),
        (0, Some(false), "panel_library", "Libraries", St::Disabled, false),
    ];
    for (depth, open, icon, name, st, sel) in rows {
        w::stated(ui, t, st, |ui| {
            let sel = if sel { selected } else { Sel::None };
            let (rect, _) = w::row(ui, t, st, sel, width, t.row_height());
            let mut x = rect.left() + t.space.row_x + 16.0 * depth as f32;
            if let Some(open) = open {
                let g = if open { Glyph::ChevronDown } else { Glyph::ChevronRight };
                w::glyph(ui, g, pos2(x + 5.0, rect.center().y), 9.0, t.color.text.muted);
            }
            x += 16.0;
            w::icon(ui, icon, pos2(x + t.icon / 2.0, rect.center().y), t.icon, st != St::Disabled);
            x += t.icon + 6.0;
            w::text_in(ui, rect.with_min_x(x), Align2::LEFT_CENTER, &t.ty.body, name, t.color.text.primary);
        });
    }
}

/// The Messages table: severity mark, message, where, and a numeric column.
pub fn messages(ui: &mut Ui, t: &Tokens, width: f32, count: usize, focused: bool) {
    ui.spacing_mut().item_spacing.y = 0.0;
    let c = &t.color;
    let cols = [width - 330.0, 150.0, 90.0, 90.0];
    // The header: small muted text, a hairline under it. Numeric columns align right.
    let (head, _) = ui.allocate_exact_size(vec2(width, t.ty.small.line + 2.0 * t.space.row_y), Sense::hover());
    let mut x = head.left() + t.space.row_x;
    for (i, name) in ["Message", "Where", "Actual", "Required"].into_iter().enumerate() {
        let cell = Rect::from_min_max(pos2(x, head.top()), pos2(x + cols[i] - 12.0, head.bottom()));
        let align = if i >= 2 { Align2::RIGHT_CENTER } else { Align2::LEFT_CENTER };
        w::text_in(ui, cell, align, &t.ty.small, name, c.text.muted);
        x += cols[i];
    }
    ui.painter().hline(head.x_range(), head.bottom(), Stroke::new(1.0, c.stroke.hairline));
    let rows = [
        (Some(false), "Clearance between Track and Pad", "U1 pin 4 · Top Copper", "0.127", "0.150", St::Rest, true, false),
        (Some(false), "Track narrower than the Net Class allows", "GND · Top Copper", "0.150", "0.250", St::Hover, false, false),
        (Some(true), "Silkscreen over a Pad", "C3 · Top Silkscreen", "", "", St::Rest, false, false),
        (None, "Clearance between Zone and Via (waived)", "GND · Inner 1", "0.140", "0.150", St::Rest, false, true),
        (Some(true), "Unconnected pin", "J2 pin 7 · Sheet 1", "", "", St::Rest, false, false),
    ];
    for (severity, text, place, actual, required, st, sel, waived) in rows.into_iter().take(count) {
        let sel = match (sel, focused) {
            (true, true) => Sel::Live,
            (true, false) => Sel::Unfocused,
            _ => Sel::None,
        };
        let (rect, _) = w::row(ui, t, st, sel, width, t.row_height());
        let ink = if waived { c.text.muted } else { c.text.primary };
        let mark = pos2(rect.left() + t.space.row_x + 7.0, rect.center().y);
        match severity {
            Some(false) => w::glyph(ui, Glyph::Cross, mark, 9.0, c.error.text),
            Some(true) => w::glyph(ui, Glyph::Triangle, mark, 11.0, c.warning.text),
            None => w::glyph(ui, Glyph::Minus, mark, 9.0, c.text.muted),
        }
        let mut x = rect.left() + t.space.row_x;
        for (i, cell_text) in [text, place, actual, required].into_iter().enumerate() {
            let lead = if i == 0 { 20.0 } else { 0.0 };
            let cell = Rect::from_min_max(pos2(x + lead, rect.top()), pos2(x + cols[i] - 12.0, rect.bottom()));
            if i >= 2 {
                w::text_in(ui, cell, Align2::RIGHT_CENTER, &t.ty.numeric, cell_text, ink);
            } else {
                let col = if i == 1 { c.text.muted } else { ink };
                w::text_clipped(ui, cell, &t.ty.body, cell_text, col);
            }
            x += cols[i];
        }
    }
}

pub fn page(ui: &mut Ui, t: &Tokens) {
    let c = &t.color;
    ui.horizontal_top(|ui| {
        column(ui, 340.0, |ui| {
            section(ui, t, "Panel sections and property rows", |ui| {
                let inner = 340.0 - 2.0 * t.space.panel;
                ui.set_width(inner);
                properties(ui, t, inner);
            });
            note(ui, t, "A section header is a row: chevron, semibold title, an optional muted count. Property names are muted so the values read first; values line up in one column. A read-only value is plain text on the same baseline.");
        });
        column(ui, 300.0, |ui| {
            section(ui, t, "List (Layers)", |ui| {
                let inner = 300.0 - 2.0 * t.space.panel;
                ui.set_width(inner);
                layers(ui, t, inner, 6);
            });
            section(ui, t, "Row states", |ui| {
                let inner = 300.0 - 2.0 * t.space.panel;
                ui.set_width(inner);
                ui.spacing_mut().item_spacing.y = 2.0;
                for (name, st, sel) in [
                    ("Rest", St::Rest, Sel::None),
                    ("Hover", St::Hover, Sel::None),
                    ("Press", St::Press, Sel::None),
                    ("Selected, Panel focused", St::Rest, Sel::Live),
                    ("Selected and hovered", St::Hover, Sel::Live),
                    ("Selected, focus elsewhere", St::Rest, Sel::Unfocused),
                    ("Keyboard focus", St::Focus, Sel::None),
                    ("Disabled", St::Disabled, Sel::None),
                ] {
                    w::stated(ui, t, st, |ui| {
                        let (rect, _) = w::row(ui, t, st, sel, inner, t.row_height());
                        w::text_in(ui, rect.shrink2(vec2(t.space.row_x, 0.0)), Align2::LEFT_CENTER, &t.ty.body, name, c.text.primary);
                    });
                }
            });
        });
        column(ui, 300.0, |ui| {
            section(ui, t, "Tree (Projects), Panel focused", |ui| {
                let inner = 300.0 - 2.0 * t.space.panel;
                ui.set_width(inner);
                tree(ui, t, inner, Sel::Live);
            });
            note(ui, t, "Rows are rounded (6px) and inset by the card's padding; nothing runs edge to edge. No zebra stripes. Trees indent 16px per level. Row height is the body line plus the Density's row padding: 28 Comfortable, 24 Compact at 13px.");
        });
        column(ui, 590.0, |ui| {
            section(ui, t, "Table (Messages)", |ui| {
                let inner = 590.0 - 2.0 * t.space.panel;
                ui.set_width(inner);
                ui.horizontal(|ui| {
                    w::badge(ui, t, Some(&c.error), false, Some(Glyph::Cross), "2 Errors");
                    w::badge(ui, t, Some(&c.warning), false, Some(Glyph::Triangle), "2 Warnings");
                    w::badge(ui, t, None, false, None, "1 Waived");
                });
                ui.add_space(4.0);
                messages(ui, t, inner, 5, true);
            });
            section(ui, t, "Table with number columns (Net lengths)", |ui| {
                let inner = 590.0 - 2.0 * t.space.panel;
                ui.set_width(inner);
                ui.spacing_mut().item_spacing.y = 0.0;
                let cols = [inner - 300.0, 100.0, 100.0, 100.0];
                let (head, _) = ui.allocate_exact_size(vec2(inner, t.ty.small.line + 2.0 * t.space.row_y), Sense::hover());
                let mut x = head.left() + t.space.row_x;
                for (i, name) in ["Net", "Length", "Vias", "Δ to target"].into_iter().enumerate() {
                    let cell = Rect::from_min_max(pos2(x, head.top()), pos2(x + cols[i] - 12.0, head.bottom()));
                    let align = if i >= 1 { Align2::RIGHT_CENTER } else { Align2::LEFT_CENTER };
                    w::text_in(ui, cell, align, &t.ty.small, name, c.text.muted);
                    x += cols[i];
                }
                ui.painter().hline(head.x_range(), head.bottom(), Stroke::new(1.0, c.stroke.hairline));
                for (hex, net, len, vias, delta, bad) in [
                    (0x3d8fd6u32, "USB_D+", "41.118", "2", "+0.012", false),
                    (0x3d8fd6, "USB_D−", "41.106", "2", "0.000", false),
                    (0xc83434, "VBUS", "111.110", "11", "", false),
                    (0x2fb36b, "SWDIO", "8.890", "0", "−1.270", true),
                ] {
                    let (rect, _) = w::row(ui, t, St::Rest, Sel::None, inner, t.row_height());
                    let sw = Rect::from_center_size(pos2(rect.left() + t.space.row_x + 7.0, rect.center().y), vec2(14.0, 14.0));
                    ui.painter().rect_filled(sw, t.radius.mark, Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8));
                    ui.painter().rect_stroke(sw, t.radius.mark, Stroke::new(1.0, c.stroke.strong), egui::StrokeKind::Inside);
                    let mut x = rect.left() + t.space.row_x;
                    for (i, text) in [net, len, vias, delta].into_iter().enumerate() {
                        let lead = if i == 0 { 22.0 } else { 0.0 };
                        let cell = Rect::from_min_max(pos2(x + lead, rect.top()), pos2(x + cols[i] - 12.0, rect.bottom()));
                        if i == 0 {
                            w::text_in(ui, cell, Align2::LEFT_CENTER, &t.ty.body, text, c.text.primary);
                        } else {
                            let ink = if i == 3 && bad { c.error.text } else { c.text.primary };
                            w::text_in(ui, cell, Align2::RIGHT_CENTER, &t.ty.numeric, text, ink);
                        }
                        x += cols[i];
                    }
                }
            });
            note(ui, t, "Table headers are small muted text over one hairline; there are no rules between rows or columns. Number columns are right-aligned in tabular digits. Severity is a shape and a colour (cross, triangle); a waived row is muted text with a neutral mark. The Errors and Warnings counts are tinted only above zero.");
        });
    });
}
