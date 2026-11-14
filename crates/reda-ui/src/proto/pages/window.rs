//! PROTOTYPE (issue #276): the window page (Title bar, tab strips, Docks, status bar) and
//! the dialogs page.

use egui::{Align, Align2, Layout, Rect, Sense, Stroke, StrokeKind, Ui, UiBuilder, pos2, vec2};

use super::super::Tokens;
use super::super::widgets::{self as w, Check, Field, Glyph, Kind, Sel, St, Tab, TabKind};
use super::{column, note, s};

#[derive(Clone, Copy)]
struct Win {
    /// REDA's window has focus.
    active: bool,
    /// macOS: native traffic lights at the left, no REDA window controls.
    mac: bool,
    /// The system title bar Preference: no window controls, square corners.
    system: bool,
    maximised: bool,
    /// A window control drawn hovered.
    hot: Option<usize>,
    /// Only the Title bar and the top of the tab strips.
    bar_only: bool,
}

const WIN: Win = Win { active: true, mac: false, system: false, maximised: false, hot: None, bar_only: false };

/// Runs `add` in a left-to-right row laid out in `rect`.
fn at<R>(ui: &mut Ui, rect: Rect, add: impl FnOnce(&mut Ui) -> R) -> R {
    // A child that allocates nothing in `ui`: the mock windows place everything by hand.
    let builder = UiBuilder::new()
        .max_rect(rect)
        .layout(Layout::left_to_right(Align::Center));
    let mut child = ui.new_child(builder);
    add(&mut child)
}

const MENUS: [&str; 8] = ["File", "Edit", "View", "Place", "Route", "Tools", "Window", "Help"];

/// The main window's Title bar in `bar`: quick strip, menus, the command field (centred
/// in the window), the Preferences gear, window controls.
fn title_bar(ui: &mut Ui, t: &Tokens, bar: Rect, o: Win) {
    let c = &t.color;
    let ink = if o.active { c.text.primary } else { c.text.muted };
    let h = t.control_height();
    let mut left = bar.left() + 8.0;
    if o.mac {
        left = bar.left() + w::traffic_lights(ui, bar, o.active);
    }
    let controls = if o.mac || o.system {
        8.0
    } else {
        let corner = if o.maximised { 0.0 } else { t.radius.window };
        w::window_controls(ui, t, bar, o.hot, o.maximised, o.active, corner)
    };
    // The gear, then a drag gap of at least 48px beside the window controls.
    let gear = Rect::from_center_size(
        pos2(bar.right() - controls - 48.0 - h / 2.0, bar.center().y),
        vec2(h, h),
    );
    at(ui, gear, |ui| w::icon_button(ui, t, St::Rest, "rules", false, t.icon_bar));
    let right = gear.left() - 8.0;

    // The quick strip never collapses.
    let quick = ["save", "undo", "redo"];
    let quick_w = quick.len() as f32 * (h + 2.0);
    let strip = Rect::from_min_size(pos2(left, bar.center().y - h / 2.0), vec2(quick_w, h));
    at(ui, strip, |ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        for (i, key) in quick.into_iter().enumerate() {
            let st = if i == 2 { St::Disabled } else { St::Rest };
            w::icon_button(ui, t, st, key, false, t.icon_bar);
        }
    });
    left += quick_w + 6.0;

    let menus_w: f32 = MENUS
        .iter()
        .map(|m| w::width(ui, &t.ty.body, m) + 2.0 * t.space.control_x - 6.0)
        .sum();
    // Narrow windows: the command field shrinks to a minimum, then to an icon button;
    // then the menus collapse into one menu button.
    let room = right - left;
    let (menus, field) = if room >= menus_w + 12.0 + 180.0 {
        (true, (room - menus_w - 12.0).min(420.0))
    } else if room >= menus_w + 12.0 + h {
        (true, 0.0)
    } else {
        (false, if room >= h + 12.0 + 180.0 { (room - h - 12.0).min(420.0) } else { 0.0 })
    };
    let menu_rect = Rect::from_min_max(pos2(left, bar.top()), pos2(right, bar.bottom()));
    let used = at(ui, menu_rect, |ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.spacing_mut().interact_size.y = h;
        if menus {
            for m in MENUS {
                let (rect, response) = ui.allocate_exact_size(
                    vec2(w::width(ui, &t.ty.body, m) + 2.0 * t.space.control_x - 6.0, h),
                    Sense::click(),
                );
                if response.hovered() {
                    ui.painter().rect_filled(rect, t.radius.control, c.fill.hover);
                }
                w::text_in(ui, rect, Align2::CENTER_CENTER, &t.ty.body, m, ink);
            }
        } else {
            let (rect, _) = ui.allocate_exact_size(vec2(h, h), Sense::click());
            for dy in [-4.0, 0.0, 4.0] {
                let y = rect.center().y + dy;
                ui.painter().hline(
                    (rect.center().x - 6.0)..=(rect.center().x + 6.0),
                    y,
                    Stroke::new(1.5, ink),
                );
            }
        }
        ui.min_rect().right()
    });
    let lo = used + 12.0;
    if field > 0.0 {
        // Centred in the window where it fits, else pushed aside by what is beside it.
        let x = (bar.center().x - field / 2.0).clamp(lo, (right - field).max(lo));
        let rect = Rect::from_min_size(pos2(x, bar.center().y - h / 2.0), vec2(field, h));
        ui.painter().rect_filled(rect, t.radius.control, c.fill.control);
        w::glyph(ui, Glyph::Search, rect.left_center() + vec2(14.0, 0.0), 13.0, c.text.muted);
        let inner = rect.shrink2(vec2(30.0, 0.0));
        if field > 300.0 {
            w::text_in(ui, rect.shrink2(vec2(8.0, 0.0)), Align2::RIGHT_CENTER, &t.ty.small, "Ctrl+K", c.text.muted);
        }
        let title = "usb_c_mcu — Main board";
        if w::width(ui, &t.ty.body, title) <= inner.width() {
            w::text_in(ui, inner, Align2::CENTER_CENTER, &t.ty.body, title, ink);
        } else {
            w::text_clipped(ui, inner, &t.ty.body, title, ink);
        }
    } else {
        let rect = Rect::from_min_size(pos2(lo - 6.0, bar.center().y - h / 2.0), vec2(h, h));
        w::glyph(ui, Glyph::Search, rect.center(), 14.0, ink);
    }
}

/// A window's outline: base fill, rim, shadow, rounded unless maximised or system-framed.
fn window_frame(ui: &mut Ui, t: &Tokens, size: egui::Vec2, o: Win) -> Rect {
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let radius = if o.maximised || o.system { 0.0 } else { t.radius.window };
    ui.painter().add(t.elevation.popup.as_shape(rect, radius));
    ui.painter().rect_filled(rect, radius, t.color.surface.base);
    ui.painter().rect_stroke(
        rect,
        radius,
        Stroke::new(1.0, t.color.stroke.hairline),
        StrokeKind::Inside,
    );
    rect
}

/// A tab strip over a card, in `area`. Returns the card's inside.
fn tab_group(ui: &mut Ui, t: &Tokens, area: Rect, tabs: &[Tab<'_>], collapse: bool) -> Rect {
    let strip = area.with_max_y(area.top() + t.tab_height());
    let card = area.with_min_y(strip.bottom());
    w::card(ui, t, card);
    at(ui, strip.with_min_x(strip.left() + 6.0), |ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        for tab in tabs {
            w::tab(ui, t, St::Rest, *tab);
        }
    });
    if collapse {
        // The Dock header's collapse button: a flat icon button at the strip's end.
        let side = t.tab_height() - 8.0;
        let rect = Rect::from_center_size(
            pos2(strip.right() - side / 2.0 - 4.0, strip.center().y),
            vec2(side, side),
        );
        w::glyph(ui, Glyph::Minus, rect.center(), 10.0, t.color.text.muted);
    }
    card.shrink(t.space.panel.min(8.0))
}

fn tab<'a>(kind: TabKind, icon: &'a str, title: &'a str, active: bool) -> Tab<'a> {
    Tab { kind, icon, title, modified: false, out_of_sync: false, window_active: active }
}

fn main_window(ui: &mut Ui, t: &Tokens, size: egui::Vec2, o: Win) {
    let c = &t.color;
    let size = if o.bar_only {
        vec2(size.x, t.title_bar_height() + t.tab_height() - 6.0)
    } else {
        size
    };
    let rect = window_frame(ui, t, size, o);
    // Everything inside is placed by hand and clipped to the window.
    let mut child = ui.new_child(UiBuilder::new().max_rect(rect));
    child.set_clip_rect(rect.intersect(ui.clip_rect()));
    let ui = &mut child;
    let bar = rect.with_max_y(rect.top() + t.title_bar_height());
    title_bar(ui, t, bar, o);
    if o.bar_only {
        let strip = Rect::from_min_max(pos2(rect.left() + 12.0, bar.bottom()), rect.max);
        at(ui, strip.with_max_y(strip.top() + t.tab_height()), |ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            w::tab(ui, t, St::Rest, tab(TabKind::Back, "document_schematic", "Sheet 1", o.active));
            w::tab(ui, t, St::Rest, tab(TabKind::Active, "document_pcb", "usb_c_mcu.pcb", o.active));
        });
        return;
    }
    let status = rect.with_min_y(rect.bottom() - t.status_bar_height());
    let body = Rect::from_min_max(
        pos2(rect.left() + 6.0, bar.bottom()),
        pos2(rect.right() - 6.0, status.top()),
    );
    let gap = 6.0;
    // Left Dock: Projects in front, Navigator behind.
    let left = body.with_max_x(body.left() + (size.x * 0.16).max(240.0));
    let inner = tab_group(
        ui,
        t,
        left,
        &[
            tab(TabKind::Front, "panel_projects", "Projects", o.active),
            tab(TabKind::Back, "panel_navigator", "Navigator", o.active),
        ],
        true,
    );
    at(ui, inner, |ui| {
        ui.shrink_clip_rect(inner);
        ui.vertical(|ui| super::data::tree(ui, t, inner.width(), Sel::Unfocused));
    });
    // Right Dock, collapsed: an icon strip on the base.
    let strip_w = t.control_height() + 8.0;
    let strip = body.with_min_x(body.right() - strip_w);
    for (i, (key, on)) in [("panel_properties", true), ("panel_layers", false), ("panel_library", false), ("panel_find", false)]
        .into_iter()
        .enumerate()
    {
        let r = Rect::from_min_size(
            pos2(strip.left() + 4.0, strip.top() + 4.0 + i as f32 * (t.control_height() + 2.0)),
            vec2(t.control_height(), t.control_height()),
        );
        at(ui, r, |ui| w::icon_button(ui, t, St::Rest, key, on, t.icon_bar));
    }
    // Centre: the active Document's tab group over a second one.
    let centre = Rect::from_min_max(
        pos2(left.right() + gap, body.top()),
        pos2(strip.left() - gap, body.bottom()),
    );
    let shown = if size.y > 400.0 { 3 } else { 2 };
    let table = t.tab_height() + 16.0 + t.ty.small.line + 2.0 * t.space.row_y + shown as f32 * t.row_height() + 4.0;
    let top = centre.with_max_y(centre.bottom() - table - gap);
    let mut pcb = tab(TabKind::Active, "document_pcb", "usb_c_mcu.pcb", o.active);
    pcb.modified = true;
    let mut power = tab(TabKind::Back, "document_schematic", "Power", o.active);
    power.out_of_sync = true;
    let canvas = tab_group(
        ui,
        t,
        top,
        &[tab(TabKind::Back, "document_schematic", "Sheet 1", o.active), pcb, power],
        false,
    );
    let well = canvas.expand(t.space.panel.min(8.0) - 1.0);
    ui.painter().rect_filled(well, t.radius.card - 1.0, c.surface.well);
    w::text_in(ui, well, Align2::CENTER_CENTER, &t.ty.small, "canvas (its colours are a separate effort)", c.text.muted);
    // The Active Bar floats on the canvas.
    let bar_rect = Rect::from_min_size(well.left_top() + vec2(12.0, 12.0), vec2(4.0 * (t.control_height() + 2.0) + 6.0, t.control_height() + 8.0));
    w::popup(ui, t, bar_rect);
    at(ui, bar_rect.shrink(4.0), |ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        for (key, on) in [("select", false), ("track", true), ("via", false), ("zone", false)] {
            w::icon_button(ui, t, St::Rest, key, on, t.icon_bar);
        }
    });
    let bottom = centre.with_min_y(top.bottom() + gap);
    let inner = tab_group(
        ui,
        t,
        bottom,
        &[
            tab(TabKind::Front, "panel_messages", "Messages", o.active),
            tab(TabKind::Back, "panel_journal", "Journal", o.active),
        ],
        true,
    );
    at(ui, inner, |ui| {
        ui.vertical(|ui| super::data::messages(ui, t, inner.width(), shown, o.active));
    });
    // Status bar: on the base, no rule above it, small text, numeric where it counts.
    let line = status.shrink2(vec2(14.0, 0.0));
    let ink = c.text.muted;
    w::text_in(ui, line, Align2::LEFT_CENTER, &t.ty.small, "Route Track: click to start, Backspace steps back", ink);
    let mut x = line.right();
    for (text, numeric) in [
        ("mm", false),
        ("Grid 0.100", true),
        ("ΔX 2.540  ΔY 0.000", true),
        ("X 121.920  Y 48.260", true),
        ("Top Copper", false),
    ] {
        let style = if numeric { &t.ty.numeric_small } else { &t.ty.small };
        let wd = w::width(ui, style, text);
        let r = Rect::from_min_max(pos2(x - wd, line.top()), pos2(x, line.bottom()));
        w::text_in(ui, r, Align2::RIGHT_CENTER, style, text, ink);
        x -= wd + 18.0;
    }
}

/// A floating window: a slim bar with the title at the left and the same controls.
fn floating_window(ui: &mut Ui, t: &Tokens, size: egui::Vec2, o: Win) {
    let rect = window_frame(ui, t, size, o);
    let mut child = ui.new_child(UiBuilder::new().max_rect(rect));
    child.set_clip_rect(rect.intersect(ui.clip_rect()));
    let ui = &mut child;
    let bar = rect.with_max_y(rect.top() + t.floating_bar_height());
    let ink = if o.active { t.color.text.primary } else { t.color.text.muted };
    let left = if o.mac { w::traffic_lights(ui, bar, o.active) } else { 14.0 };
    if !o.mac {
        w::window_controls(ui, t, bar, o.hot, false, o.active, t.radius.window);
    }
    w::text_in(ui, bar.with_min_x(bar.left() + left), Align2::LEFT_CENTER, &t.ty.body, "Layers — usb_c_mcu", ink);
    let body = Rect::from_min_max(pos2(rect.left() + 6.0, bar.bottom()), rect.max - vec2(6.0, 6.0));
    w::card(ui, t, body);
    at(ui, body.shrink(8.0), |ui| {
        ui.vertical(|ui| super::data::layers(ui, t, body.width() - 16.0, 3));
    });
}

pub fn page(ui: &mut Ui, t: &Tokens) {
    let small = &t.ty.small;
    let muted = t.color.text.muted;
    let bar = Win { bar_only: true, ..WIN };
    main_window(ui, t, vec2(1560.0, 396.0), WIN);
    ui.add_space(6.0);
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 16.0;
        column(ui, 780.0, |ui| {
            w::label(ui, small, muted, "Window in the background: bar text and control glyphs go muted; the accent line and the live selection go neutral");
            main_window(ui, t, vec2(780.0, 262.0), Win { active: false, ..WIN });
        });
        column(ui, 764.0, |ui| {
            ui.horizontal_top(|ui| {
                column(ui, 300.0, |ui| {
                    w::label(ui, small, muted, "Floating window");
                    floating_window(ui, t, vec2(300.0, 128.0), WIN);
                });
                column(ui, 300.0, |ui| {
                    w::label(ui, small, muted, "Floating window, macOS");
                    floating_window(ui, t, vec2(300.0, 128.0), Win { mac: true, ..WIN });
                });
            });
            ui.add_space(4.0);
            w::label(ui, small, muted, "With the system title bar (the Preference; sway and Hyprland): no window controls, square corners");
            main_window(ui, t, vec2(764.0, 0.0), Win { system: true, ..bar });
        });
    });
    ui.add_space(4.0);
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 16.0;
        column(ui, 900.0, |ui| {
            w::label(ui, small, muted, "Narrow window: first the command field shrinks to a minimum (close hovered)");
            main_window(ui, t, vec2(900.0, 0.0), Win { hot: Some(2), ..bar });
        });
        column(ui, 644.0, |ui| {
            w::label(ui, small, muted, "macOS: native traffic lights; the quick strip starts after their inset");
            main_window(ui, t, vec2(644.0, 0.0), Win { mac: true, ..bar });
        });
    });
    ui.add_space(4.0);
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 16.0;
        column(ui, 760.0, |ui| {
            w::label(ui, small, muted, "…then the field becomes an icon button (maximise hovered)…");
            main_window(ui, t, vec2(760.0, 0.0), Win { hot: Some(1), ..bar });
        });
        column(ui, 560.0, |ui| {
            w::label(ui, small, muted, "…then the menus collapse into one button. Maximised: restore glyph, square corners");
            main_window(ui, t, vec2(560.0, 0.0), Win { maximised: true, ..bar });
        });
    });
}

// --- 5 · Dialogs ----------------------------------------------------------------------

/// A dialog: floating surface, window radius and shadow, a title row with a close ×.
fn dialog(ui: &mut Ui, t: &Tokens, width: f32, title: &str, add: impl FnOnce(&mut Ui)) {
    egui::Frame::new()
        .fill(t.color.surface.floating)
        .stroke(Stroke::new(t.hairline, t.color.stroke.hairline))
        .corner_radius(t.radius.window as u8)
        .shadow(t.elevation.window)
        .inner_margin(t.space.dialog as i8)
        .show(ui, |ui| {
            ui.vertical(|ui| {
                let inner = width - 2.0 * t.space.dialog;
                ui.set_width(inner);
                let (rect, _) = ui.allocate_exact_size(vec2(inner, t.ty.title.line), Sense::hover());
                w::text_in(ui, rect, Align2::LEFT_CENTER, &t.ty.title, title, t.color.text.primary);
                w::glyph(ui, Glyph::Cross, rect.right_center() - vec2(6.0, 0.0), 9.0, t.color.text.muted);
                ui.add_space(t.space.stack);
                add(ui);
            });
        });
}

/// The button row: right-aligned, the primary button last.
fn buttons(ui: &mut Ui, t: &Tokens, add: impl FnOnce(&mut Ui)) {
    ui.add_space(t.space.section - t.space.stack);
    ui.with_layout(Layout::right_to_left(Align::Center), add);
}

/// A form row: label at the left, control at the right.
fn form(ui: &mut Ui, t: &Tokens, label: &str, add: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(vec2(120.0, t.control_height()), Sense::hover());
        w::text_in(ui, rect, Align2::LEFT_CENTER, &t.ty.body, label, t.color.text.primary);
        add(ui);
    });
}

pub fn dialogs(ui: &mut Ui, t: &Tokens) {
    let c = &t.color;
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 24.0;
        column(ui, 400.0, |ui| {
            w::label(ui, &t.ty.small, c.text.muted, "A form (Command params)");
            dialog(ui, t, 400.0, "Paste Array", |ui| {
                form(ui, t, "Columns", |ui| {
                    w::text_field(ui, t, St::Rest, &mut s("4"), 90.0, Field { numeric: true, ..Default::default() });
                });
                form(ui, t, "Rows", |ui| {
                    w::text_field(ui, t, St::Focus, &mut s("2"), 90.0, Field { numeric: true, ..Default::default() });
                });
                form(ui, t, "Column pitch", |ui| {
                    let f = Field { numeric: true, unit: Some("mm"), ..Default::default() };
                    w::text_field(ui, t, St::Rest, &mut s("2.540"), 120.0, f);
                });
                form(ui, t, "Row pitch", |ui| {
                    let f = Field { numeric: true, unit: Some("mm"), invalid: true, ..Default::default() };
                    w::text_field(ui, t, St::Rest, &mut s("0"), 120.0, f);
                });
                ui.horizontal(|ui| {
                    ui.add_space(120.0 + t.space.gap);
                    w::label(ui, &t.ty.small, c.error.text, "The pitch must be above zero");
                });
                form(ui, t, "Numbering", |ui| {
                    w::combo(ui, t, St::Rest, "Continue from last", 180.0);
                });
                form(ui, t, "", |ui| {
                    w::checkbox(ui, t, St::Rest, Check::On, "Keep Net names");
                });
                buttons(ui, t, |ui| {
                    w::button(ui, t, St::Disabled, Kind::Primary, None, "Paste");
                    w::button(ui, t, St::Rest, Kind::Secondary, None, "Cancel");
                });
            });
            ui.add_space(16.0);
            w::label(ui, &t.ty.small, c.text.muted, "A job run report");
            dialog(ui, t, 400.0, "Output Job: Fab files", |ui| {
                let inner = 400.0 - 2.0 * t.space.dialog;
                w::progress(ui, t, &c.accent, 0.62, inner);
                w::label(ui, &t.ty.small, c.text.muted, "Writing drill files (5 of 8)");
                for (mark, tone, line) in [
                    (Glyph::Tick, &c.success, "gerbers/usb_c_mcu-F_Cu.gbr"),
                    (Glyph::Tick, &c.success, "gerbers/usb_c_mcu-B_Cu.gbr"),
                    (Glyph::Triangle, &c.warning, "bom.csv: 2 Parts have no MPN"),
                ] {
                    ui.horizontal(|ui| {
                        let (r, _) = ui.allocate_exact_size(vec2(14.0, t.ty.mono.line), Sense::hover());
                        w::glyph(ui, mark, r.center(), 9.0, tone.text);
                        w::label(ui, &t.ty.mono, c.text.primary, line);
                    });
                }
                buttons(ui, t, |ui| {
                    w::button(ui, t, St::Rest, Kind::Secondary, None, "Cancel");
                });
            });
        });
        column(ui, 440.0, |ui| {
            w::label(ui, &t.ty.small, c.text.muted, "A modal over the scrim; destructive confirm");
            let (stage, _) = ui.allocate_exact_size(vec2(440.0, 250.0), Sense::hover());
            w::card(ui, t, stage);
            at(ui, stage.shrink(10.0), |ui| {
                ui.vertical(|ui| super::data::layers(ui, t, 200.0, 6));
            });
            ui.painter().rect_filled(stage, t.radius.card, c.surface.scrim);
            let builder = UiBuilder::new().max_rect(Rect::from_center_size(stage.center(), vec2(360.0, 150.0)));
            let mut modal = ui.new_child(builder);
            {
                let ui = &mut modal;
                dialog(ui, t, 360.0, "Delete 14 Tracks?", |ui| {
                    w::paragraph(ui, &t.ty.body, c.text.primary, "The Tracks on Net GND between U1 and J2 are removed. This can be undone.", 360.0 - 2.0 * t.space.dialog);
                    buttons(ui, t, |ui| {
                        w::button(ui, t, St::Rest, Kind::Danger, None, "Delete");
                        w::button(ui, t, St::Focus, Kind::Secondary, None, "Cancel");
                    });
                });
            }
            ui.add_space(16.0);
            w::label(ui, &t.ty.small, c.text.muted, "Three choices (closing with unsaved changes)");
            dialog(ui, t, 440.0, "Save changes to usb_c_mcu?", |ui| {
                w::paragraph(ui, &t.ty.body, c.text.primary, "Two Documents have changes that are not saved.", 440.0 - 2.0 * t.space.dialog);
                buttons(ui, t, |ui| {
                    w::button(ui, t, St::Rest, Kind::Primary, None, "Save All");
                    w::button(ui, t, St::Rest, Kind::Secondary, None, "Cancel");
                    ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                        w::button(ui, t, St::Rest, Kind::Flat, None, "Don't Save");
                    });
                });
            });
            ui.add_space(10.0);
            note(ui, t, "Dialogs sit on the floating surface with 16px corners and the window shadow. One primary button, last on the right; the destructive confirm is the error solid and keyboard focus starts on Cancel. Titles are semibold at the body size.");
        });
        column(ui, 600.0, |ui| {
            w::label(ui, &t.ty.small, c.text.muted, "A settings window (Preferences)");
            egui::Frame::new()
                .fill(c.surface.floating)
                .stroke(Stroke::new(t.hairline, c.stroke.hairline))
                .corner_radius(t.radius.window as u8)
                .shadow(t.elevation.window)
                .show(ui, |ui| {
                    ui.horizontal_top(|ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        // The sidebar: on the base colour, a list with one selection.
                        let side_w = 170.0;
                        let (side, _) = ui.allocate_exact_size(vec2(side_w, 460.0), Sense::hover());
                        let radius = egui::CornerRadius { nw: 16, sw: 16, ne: 0, se: 0 };
                        ui.painter().rect_filled(side.shrink(1.0), radius, c.surface.base);
                        at(ui, side.shrink(8.0), |ui| {
                            ui.vertical(|ui| {
                                ui.spacing_mut().item_spacing.y = 0.0;
                                w::label(ui, &t.ty.small, c.text.muted, "  PREFERENCES");
                                ui.add_space(4.0);
                                for (i, name) in ["General", "Appearance", "Keymap", "Grids", "Libraries", "Agents"].into_iter().enumerate() {
                                    let sel = if i == 1 { Sel::Live } else { Sel::None };
                                    let st = if i == 3 { St::Hover } else { St::Rest };
                                    let (rect, _) = w::row(ui, t, st, sel, side_w - 16.0, t.row_height());
                                    w::text_in(ui, rect.shrink2(vec2(t.space.row_x + 2.0, 0.0)), Align2::LEFT_CENTER, &t.ty.body, name, c.text.primary);
                                }
                            });
                        });
                        ui.vertical(|ui| {
                            ui.add_space(t.space.dialog);
                            ui.horizontal(|ui| {
                                ui.add_space(t.space.dialog);
                                ui.vertical(|ui| {
                                    ui.spacing_mut().item_spacing = vec2(t.space.gap, t.space.stack);
                                    w::label(ui, &t.ty.heading, c.text.primary, "Appearance");
                                    ui.add_space(t.space.stack);
                                    form(ui, t, "Theme", |ui| {
                                        w::combo(ui, t, St::Rest, "Follow the system", 190.0);
                                    });
                                    form(ui, t, "Density", |ui| {
                                        ui.spacing_mut().item_spacing.x = 2.0;
                                        let (on, off) = if t.compact { (1, 0) } else { (0, 1) };
                                        let names = ["Comfortable", "Compact"];
                                        w::button(ui, t, St::Rest, if on == 0 { Kind::On } else { Kind::Secondary }, None, names[0]);
                                        w::button(ui, t, St::Rest, if off == 0 { Kind::On } else { Kind::Secondary }, None, names[1]);
                                    });
                                    form(ui, t, "Text size", |ui| {
                                        let f = Field { numeric: true, unit: Some("px"), ..Default::default() };
                                        w::text_field(ui, t, St::Rest, &mut format!("{}", t.ty.body.size), 80.0, f);
                                        w::slider(ui, t, St::Rest, (t.ty.body.size - 11.0) / 5.0, 100.0);
                                    });
                                    form(ui, t, "UI scale", |ui| {
                                        w::combo(ui, t, St::Rest, "100%", 100.0);
                                    });
                                    form(ui, t, "Typeface", |ui| {
                                        w::combo(ui, t, St::Rest, "Default (Inter)", 190.0);
                                    });
                                    form(ui, t, "Monospace", |ui| {
                                        w::combo(ui, t, St::Rest, "Berkeley Mono", 190.0);
                                    });
                                    ui.horizontal(|ui| {
                                        ui.add_space(120.0 + t.space.gap);
                                        let (r, _) = ui.allocate_exact_size(vec2(14.0, t.ty.small.line), Sense::hover());
                                        w::glyph(ui, Glyph::Triangle, r.center(), 10.0, c.warning.text);
                                        w::label(ui, &t.ty.small, c.text.muted, "Not installed here; using the default");
                                    });
                                    ui.add_space(t.space.section - t.space.stack);
                                    w::hairline(ui, t, 380.0);
                                    ui.add_space(t.space.section - t.space.stack);
                                    w::switch(ui, t, St::Rest, true, "Animate hover, popups and Docks");
                                    w::switch(ui, t, St::Rest, false, "Use the system title bar");
                                    w::label(ui, &t.ty.small, c.text.muted, "Applies the next time REDA starts.");
                                    ui.add_space(t.space.dialog);
                                });
                                ui.add_space(t.space.dialog);
                            });
                        });
                    });
                });
            ui.add_space(10.0);
            note(ui, t, "Settings windows have a sidebar on the base colour and their page on the floating surface. Settings that apply at once are switches and need no OK button. (How these Preferences are presented is its own ticket; this only fixes the recipes.)");
        });
    });
}
