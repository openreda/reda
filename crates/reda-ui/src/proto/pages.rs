//! PROTOTYPE (issue #276): the gallery. Every component in every state, one page per
//! family, drawn from `Tokens` alone.

mod data;
mod window;

use egui::{Align, Align2, Color32, Layout, Rect, Sense, Stroke, Ui, UiBuilder, vec2};

use super::widgets::{self as w, Check, Field, Glyph, Item, Kind, Sel, St};
use super::{Tokens, over};

type Page = fn(&mut Ui, &Tokens);

pub const PAGES: [(&str, Page); 7] = [
    ("1 · Controls", controls),
    ("2 · Menus and popups", popups),
    ("3 · Window: Title bar, tabs, Docks, status bar", window::page),
    ("4 · Panels and data", data::page),
    ("5 · Dialogs", window::dialogs),
    ("6 · Colour", colour),
    ("7 · Type, text size and typeface", type_page),
];

/// The whole gallery, filling `ui`.
pub fn gallery(ui: &mut Ui) {
    let t = Tokens::of(ui);
    let full = ui.max_rect();
    ui.painter().rect_filled(full, 0.0, t.color.surface.base);
    let inner = full.shrink2(vec2(20.0, 12.0));
    ui.scope_builder(UiBuilder::new().max_rect(inner), |ui| {
        ui.spacing_mut().item_spacing = vec2(t.space.gap, t.space.stack);
        ui.horizontal(|ui| {
            w::label(ui, &t.ty.heading, t.color.text.primary, PAGES[super::page()].0);
            ui.add_space(12.0);
            let about = format!(
                "{} · {} · text {} · {}",
                if t.dark { "Dark" } else { "Light" },
                if t.compact { "Compact" } else { "Comfortable" },
                super::text_size(),
                super::FACES[super::face()],
            );
            let (rect, _) = ui.allocate_exact_size(
                vec2(w::width(ui, &t.ty.body, &about), t.ty.heading.line),
                Sense::hover(),
            );
            w::text_in(ui, rect, Align2::LEFT_CENTER, &t.ty.body, &about, t.color.text.muted);
        });
        ui.add_space(6.0);
        (PAGES[super::page()].1)(ui, &t);
    });
}

// --- Layout helpers --------------------------------------------------------------------

/// A card holding one family of specimens.
pub(super) fn section<R>(ui: &mut Ui, t: &Tokens, title: &str, add: impl FnOnce(&mut Ui) -> R) -> R {
    egui::Frame::new()
        .fill(t.color.surface.card)
        .stroke(Stroke::new(t.hairline, t.color.stroke.hairline))
        .corner_radius(t.radius.card as u8)
        .shadow(t.elevation.card)
        .inner_margin(t.space.panel as i8)
        .show(ui, |ui| {
            ui.vertical(|ui| {
                w::label(ui, &t.ty.title, t.color.text.primary, title);
                ui.add_space(2.0);
                add(ui)
            })
            .inner
        })
        .inner
}

pub(super) fn column<R>(ui: &mut Ui, width: f32, add: impl FnOnce(&mut Ui) -> R) -> R {
    ui.allocate_ui_with_layout(vec2(width, 0.0), Layout::top_down(Align::Min), |ui| {
        ui.set_width(width);
        add(ui)
    })
    .inner
}

pub(super) fn note(ui: &mut Ui, t: &Tokens, text: &str) {
    let wrap = ui.available_width();
    w::paragraph(ui, &t.ty.small, t.color.text.muted, text, wrap);
}

const NAME_W: f32 = 104.0;

/// A named row of specimens.
fn spec(ui: &mut Ui, t: &Tokens, name: &str, add: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(vec2(NAME_W, t.control_height()), Sense::hover());
        w::text_in(ui, rect, Align2::LEFT_CENTER, &t.ty.small, name, t.color.text.muted);
        add(ui);
    });
}

fn cell(ui: &mut Ui, t: &Tokens, width: f32, add: impl FnOnce(&mut Ui)) {
    ui.allocate_ui_with_layout(
        vec2(width, t.control_height()),
        Layout::left_to_right(Align::Center),
        |ui| {
            ui.set_min_width(width);
            add(ui);
        },
    );
}

const STATES: [(St, &str); 5] = [
    (St::Rest, "Rest"),
    (St::Hover, "Hover"),
    (St::Press, "Press"),
    (St::Focus, "Focus"),
    (St::Disabled, "Disabled"),
];

fn state_header(ui: &mut Ui, t: &Tokens, width: f32) {
    ui.horizontal(|ui| {
        ui.add_space(NAME_W + t.space.gap);
        for (_, name) in STATES {
            let (rect, _) = ui.allocate_exact_size(vec2(width, t.ty.small.line), Sense::hover());
            w::text_in(ui, rect, Align2::LEFT_CENTER, &t.ty.small, name, t.color.text.muted);
        }
    });
}

/// A specimen in each of the five states.
fn states(ui: &mut Ui, t: &Tokens, name: &str, width: f32, mut add: impl FnMut(&mut Ui, St)) {
    spec(ui, t, name, |ui| {
        for (st, _) in STATES {
            cell(ui, t, width, |ui| add(ui, st));
        }
    });
}

fn s(text: &str) -> String {
    text.to_owned()
}

#[derive(Clone)]
struct Live {
    name: String,
    width: String,
    check: bool,
    switch: bool,
    radio: u8,
    toggle: bool,
}

impl Default for Live {
    fn default() -> Self {
        Live {
            name: "GND".into(),
            width: "0.250".into(),
            check: true,
            switch: true,
            radio: 0,
            toggle: true,
        }
    }
}

// --- 1 · Controls ---------------------------------------------------------------------

fn controls(ui: &mut Ui, t: &Tokens) {
    ui.horizontal_top(|ui| {
        column(ui, 770.0, |ui| {
            section(ui, t, "Buttons", |ui| {
                ui.set_width(770.0 - 2.0 * t.space.panel);
                let cw = 122.0;
                state_header(ui, t, cw);
                for (name, kind, label) in [
                    ("Primary", Kind::Primary, "Run DRC"),
                    ("Secondary", Kind::Secondary, "Cancel"),
                    ("Flat", Kind::Flat, "Reset"),
                    ("Destructive", Kind::Danger, "Delete"),
                    ("Toggle, on", Kind::On, "Snap"),
                ] {
                    states(ui, t, name, cw, |ui, st| {
                        w::button(ui, t, st, kind, None, label);
                    });
                }
                states(ui, t, "With icon", cw, |ui, st| {
                    w::button(ui, t, st, Kind::Secondary, Some("zoom_fit"), "Fit");
                });
                states(ui, t, "Icon button", cw, |ui, st| {
                    w::icon_button(ui, t, st, "wire", false, t.icon_bar);
                    w::icon_button(ui, t, st, "track", true, t.icon_bar);
                });
                states(ui, t, "Link", cw, |ui, st| {
                    w::link(ui, t, st, "Open the guide");
                });
                note(ui, t, "Buttons are filled at rest with no border. Hover and press lay the neutral fill over whatever is underneath, so one rule covers neutral, accent and error buttons. Focus is a 2px accent ring outside the control. Disabled is the whole widget at 45%. The second icon button in each pair is the on state (the active Tool).");
            });
            section(ui, t, "Check boxes, radio buttons, switches", |ui| {
                ui.set_width(770.0 - 2.0 * t.space.panel);
                let cw = 122.0;
                state_header(ui, t, cw);
                for (name, check) in [
                    ("Check, off", Check::Off),
                    ("Check, on", Check::On),
                    ("Check, mixed", Check::Mixed),
                ] {
                    states(ui, t, name, cw, |ui, st| {
                        w::checkbox(ui, t, st, check, "Visible");
                    });
                }
                for (name, on) in [("Radio, off", false), ("Radio, on", true)] {
                    states(ui, t, name, cw, |ui, st| {
                        w::radio(ui, t, st, on, "Metric");
                    });
                }
                for (name, on) in [("Switch, off", false), ("Switch, on", true)] {
                    states(ui, t, name, cw, |ui, st| {
                        w::switch(ui, t, st, on, "Autosave");
                    });
                }
                note(ui, t, "Proposed rule: a switch is for a setting that applies the moment it is flipped (Preferences, view options); a check box is for a choice confirmed with a button, and for lists and tables.");
            });
        });
        column(ui, 770.0, |ui| {
            section(ui, t, "Fields and combo boxes", |ui| {
                ui.set_width(770.0 - 2.0 * t.space.panel);
                let cw = 122.0;
                state_header(ui, t, cw);
                states(ui, t, "Text field", cw, |ui, st| {
                    w::text_field(ui, t, st, &mut s("GND"), cw - 12.0, Field::default());
                });
                states(ui, t, "Empty, hint", cw, |ui, st| {
                    let f = Field { hint: "Net name", ..Default::default() };
                    w::text_field(ui, t, st, &mut s(""), cw - 12.0, f);
                });
                states(ui, t, "Number field", cw, |ui, st| {
                    let f = Field { numeric: true, unit: Some("mm"), ..Default::default() };
                    w::text_field(ui, t, st, &mut s("0.250"), cw - 12.0, f);
                });
                states(ui, t, "Invalid", cw, |ui, st| {
                    let f = Field { numeric: true, unit: Some("mm"), invalid: true, ..Default::default() };
                    w::text_field(ui, t, st, &mut s("0.05"), cw - 12.0, f);
                });
                states(ui, t, "Search", cw, |ui, st| {
                    let f = Field { search: true, hint: "Filter", ..Default::default() };
                    w::text_field(ui, t, st, &mut s(""), cw - 12.0, f);
                });
                states(ui, t, "Combo box", cw, |ui, st| {
                    w::combo(ui, t, st, "Top Copper", cw - 12.0);
                });
                states(ui, t, "Slider", cw, |ui, st| {
                    w::slider(ui, t, st, 0.6, cw - 12.0);
                });
                spec(ui, t, "Invalid, message", |ui| {
                    let f = Field { numeric: true, unit: Some("mm"), invalid: true, ..Default::default() };
                    w::text_field(ui, t, St::Rest, &mut s("0.05"), 110.0, f);
                    w::glyph(ui, Glyph::Cross, ui.cursor().left_center() + vec2(6.0, 0.0), 9.0, t.color.error.text);
                    ui.add_space(14.0);
                    w::label(ui, &t.ty.body, t.color.error.text, "Below the 0.1 mm minimum of the fab preset");
                });
                note(ui, t, "Fields are wells: white (near-black in dark) with a 1px outline, so they read as a place to type, where buttons and combo boxes are filled. Number fields use tabular digits and show the unit in muted text. An invalid field keeps its error outline in every state and pairs it with a cross and a message.");
            });
            section(ui, t, "Small parts", |ui| {
                ui.set_width(770.0 - 2.0 * t.space.panel);
                spec(ui, t, "Counts", |ui| {
                    w::badge(ui, t, Some(&t.color.error), false, Some(Glyph::Cross), "3 Errors");
                    w::badge(ui, t, Some(&t.color.warning), false, Some(Glyph::Triangle), "12 Warnings");
                    w::badge(ui, t, None, false, None, "0 Errors");
                    w::badge(ui, t, Some(&t.color.success), false, Some(Glyph::Tick), "Clean");
                    ui.add_space(12.0);
                    w::badge(ui, t, Some(&t.color.warning), true, None, "Out of sync");
                    w::badge(ui, t, Some(&t.color.error), true, None, "Failed");
                    w::badge(ui, t, Some(&t.color.success), true, None, "Passed");
                });
                spec(ui, t, "Progress", |ui| {
                    w::progress(ui, t, &t.color.accent, 0.45, 150.0);
                    w::progress(ui, t, &t.color.success, 1.0, 150.0);
                    w::progress(ui, t, &t.color.error, 0.7, 150.0);
                });
                spec(ui, t, "Key chips", |ui| {
                    w::key_chip(ui, t, "Ctrl+K");
                    w::key_chip(ui, t, "W");
                    w::key_chip(ui, t, "Shift+F9");
                });
                spec(ui, t, "Data swatches", |ui| {
                    for hex in [0xc83434u32, 0x3d8fd6, 0x2fb36b, 0xd4c03c, 0xb85cd6, 0xf2f2f2, 0x202020] {
                        w::swatch(ui, t, Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8));
                    }
                    ui.add_space(12.0);
                    w::label(ui, &t.ty.small, t.color.text.muted, "one shape, neutral rim, never themed");
                });
                spec(ui, t, "Scrollbar", |ui| {
                    w::scrollbar(ui, t, false, t.control_height());
                    w::scrollbar(ui, t, true, t.control_height());
                    w::label(ui, &t.ty.small, t.color.text.muted, "4px at rest, 8px under the pointer, no track");
                });
            });
            section(ui, t, "Live: these follow the pointer and keyboard", |ui| {
                ui.set_width(770.0 - 2.0 * t.space.panel);
                let id = egui::Id::new("proto-live");
                let mut live = ui.data_mut(|d| d.get_temp::<Live>(id)).unwrap_or_default();
                ui.horizontal(|ui| {
                    w::button(ui, t, St::Live, Kind::Primary, None, "Run DRC");
                    w::button(ui, t, St::Live, Kind::Secondary, None, "Cancel");
                    w::button(ui, t, St::Live, Kind::Flat, Some("undo"), "Undo");
                    let kind = if live.toggle { Kind::On } else { Kind::Secondary };
                    if w::button(ui, t, St::Live, kind, None, "Snap").clicked() {
                        live.toggle = !live.toggle;
                    }
                    w::text_field(ui, t, St::Live, &mut live.name, 120.0, Field { hint: "Net name", ..Default::default() });
                    let f = Field { numeric: true, unit: Some("mm"), ..Default::default() };
                    w::text_field(ui, t, St::Live, &mut live.width, 100.0, f);
                    w::combo(ui, t, St::Live, "Top Copper", 120.0);
                });
                ui.horizontal(|ui| {
                    let check = if live.check { Check::On } else { Check::Off };
                    if w::checkbox(ui, t, St::Live, check, "Visible").clicked() {
                        live.check = !live.check;
                    }
                    ui.add_space(8.0);
                    for (i, name) in ["Metric", "Imperial"].into_iter().enumerate() {
                        if w::radio(ui, t, St::Live, live.radio == i as u8, name).clicked() {
                            live.radio = i as u8;
                        }
                    }
                    ui.add_space(8.0);
                    if w::switch(ui, t, St::Live, live.switch, "Autosave").clicked() {
                        live.switch = !live.switch;
                    }
                    ui.add_space(8.0);
                    w::link(ui, t, St::Live, "Open the guide");
                });
                ui.data_mut(|d| d.insert_temp(id, live));
            });
        });
    });
}

// --- 2 · Menus and popups -------------------------------------------------------------

/// A menu or popup surface around `add`.
pub(super) fn menu<R>(ui: &mut Ui, t: &Tokens, add: impl FnOnce(&mut Ui) -> R) -> R {
    egui::Frame::new()
        .fill(t.color.surface.floating)
        .stroke(Stroke::new(t.hairline, t.color.stroke.hairline))
        .corner_radius(t.radius.popup as u8)
        .shadow(t.elevation.popup)
        .inner_margin(t.space.menu as i8)
        .show(ui, |ui| {
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                add(ui)
            })
            .inner
        })
        .inner
}

fn popups(ui: &mut Ui, t: &Tokens) {
    ui.horizontal_top(|ui| {
        column(ui, 300.0, |ui| {
            w::label(ui, &t.ty.small, t.color.text.muted, "Main menu, open from the Title bar");
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                w::button(ui, t, St::Rest, Kind::Flat, None, "File");
                w::button(ui, t, St::Press, Kind::Flat, None, "Edit");
                w::button(ui, t, St::Hover, Kind::Flat, None, "View");
                w::button(ui, t, St::Rest, Kind::Flat, None, "Place");
            });
            menu(ui, t, |ui| {
                let mw = 260.0;
                let item = |icon, text, shortcut| Item { icon, text, shortcut, ..Default::default() };
                w::menu_item(ui, t, St::Rest, mw, item(Some("undo"), "Undo Move", "Ctrl+Z"));
                w::menu_item(ui, t, St::Disabled, mw, item(Some("redo"), "Redo", "Ctrl+Shift+Z"));
                w::menu_separator(ui, t, mw);
                w::menu_item(ui, t, St::Rest, mw, item(Some("cut"), "Cut", "Ctrl+X"));
                w::menu_item(ui, t, St::Hover, mw, item(Some("copy"), "Copy", "Ctrl+C"));
                w::menu_item(ui, t, St::Rest, mw, item(Some("paste"), "Paste", "Ctrl+V"));
                w::menu_item(ui, t, St::Rest, mw, item(Some("delete"), "Delete", "Del"));
                w::menu_separator(ui, t, mw);
                w::menu_item(ui, t, St::Rest, mw, Item { text: "Snap to Grid", checked: true, shortcut: "Ctrl+G", ..Default::default() });
                w::menu_item(ui, t, St::Rest, mw, Item { text: "Show Ratsnest", ..Default::default() });
                w::menu_item(ui, t, St::Focus, mw, Item { icon: Some("find"), text: "Find Similar", submenu: true, ..Default::default() });
                w::menu_separator(ui, t, mw);
                w::menu_item(ui, t, St::Rest, mw, item(Some("rules"), "Preferences…", "Ctrl+,"));
            });
            ui.add_space(8.0);
            note(ui, t, "Items are rows with 6px corners inside the popup's 12px, so the corners nest. Hover is the neutral fill, never the accent. A check takes the icon column. Delete stays neutral. Keyboard focus is a ring inside the row.");
        });
        ui.add_space(12.0);
        column(ui, 420.0, |ui| {
            w::label(ui, &t.ty.small, t.color.text.muted, "Context menu with a submenu");
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = -4.0;
                menu(ui, t, |ui| {
                    let mw = 190.0;
                    w::menu_item(ui, t, St::Rest, mw, Item { icon: Some("tab"), text: "Close", shortcut: "Ctrl+W", ..Default::default() });
                    w::menu_item(ui, t, St::Rest, mw, Item { text: "Close Others", ..Default::default() });
                    w::menu_separator(ui, t, mw);
                    w::menu_item(ui, t, St::Hover, mw, Item { icon: Some("tab_move"), text: "Move to", submenu: true, ..Default::default() });
                    w::menu_item(ui, t, St::Rest, mw, Item { icon: Some("split"), text: "Split Right", ..Default::default() });
                });
                ui.vertical(|ui| {
                    ui.add_space(2.0 * t.row_height());
                    menu(ui, t, |ui| {
                        let mw = 170.0;
                        w::menu_item(ui, t, St::Rest, mw, Item { text: "New Window", ..Default::default() });
                        w::menu_item(ui, t, St::Hover, mw, Item { text: "Tab Group Below", ..Default::default() });
                        w::menu_item(ui, t, St::Rest, mw, Item { text: "Left Dock", ..Default::default() });
                    });
                });
            });
            ui.add_space(16.0);
            w::label(ui, &t.ty.small, t.color.text.muted, "Combo box, open");
            w::combo(ui, t, St::Press, "Top Copper", 200.0);
            ui.add_space(-t.space.stack + 4.0);
            menu(ui, t, |ui| {
                let mw = 200.0 - 2.0 * t.space.menu;
                for (i, name) in ["Top Copper", "Inner 1", "Inner 2", "Bottom Copper"].into_iter().enumerate() {
                    let st = if i == 1 { St::Hover } else { St::Rest };
                    w::menu_item(ui, t, st, mw, Item { text: name, checked: i == 0, ..Default::default() });
                }
            });
            ui.add_space(16.0);
            w::label(ui, &t.ty.small, t.color.text.muted, "Tooltips: a Tool (name and key), and one with a sentence");
            ui.horizontal_top(|ui| {
                tooltip(ui, t, "Route Track", Some("X"), None);
                ui.add_space(8.0);
                tooltip(ui, t, "Out of sync", None, Some("The PCB has not taken the last schematic changes. Run Update PCB."));
            });
            ui.add_space(12.0);
            w::label(ui, &t.ty.small, t.color.text.muted, "Floating bar (the Active Bar on the canvas)");
            egui::Frame::new()
                .fill(t.color.surface.floating)
                .stroke(Stroke::new(t.hairline, t.color.stroke.hairline))
                .corner_radius(t.radius.popup as u8)
                .shadow(t.elevation.popup)
                .inner_margin(4)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 2.0;
                        for (key, st, on) in [
                            ("select", St::Rest, false),
                            ("wire", St::Rest, true),
                            ("net", St::Hover, false),
                            ("port", St::Rest, false),
                            ("text", St::Rest, false),
                            ("line", St::Disabled, false),
                        ] {
                            w::icon_button(ui, t, st, key, on, t.icon_bar);
                        }
                    });
                });
        });
        ui.add_space(12.0);
        column(ui, 600.0, |ui| {
            w::label(ui, &t.ty.small, t.color.text.muted, "Command Palette");
            palette(ui, t, 600.0);
            ui.add_space(8.0);
            note(ui, t, "The Command Palette is a window (16px corners, the window shadow) dropped from the Title bar's command field. Its rows are menu rows. The selected row is the live selection: accent subtle with ordinary text. Shortcuts are key chips.");
        });
    });
}

pub(super) fn tooltip(ui: &mut Ui, t: &Tokens, title: &str, key: Option<&str>, body: Option<&str>) {
    egui::Frame::new()
        .fill(t.color.surface.floating)
        .stroke(Stroke::new(t.hairline, t.color.stroke.hairline))
        .corner_radius(t.radius.control as u8)
        .shadow(t.elevation.popup)
        .inner_margin(egui::Margin::symmetric(10, 6))
        .show(ui, |ui| {
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                ui.horizontal(|ui| {
                    w::label(ui, &t.ty.body, t.color.text.primary, title);
                    if let Some(key) = key {
                        w::key_chip(ui, t, key);
                    }
                });
                if let Some(body) = body {
                    w::paragraph(ui, &t.ty.small, t.color.text.muted, body, 220.0);
                }
            });
        });
}

pub(super) fn palette(ui: &mut Ui, t: &Tokens, width: f32) {
    egui::Frame::new()
        .fill(t.color.surface.floating)
        .stroke(Stroke::new(t.hairline, t.color.stroke.hairline))
        .corner_radius(t.radius.window as u8)
        .shadow(t.elevation.window)
        .inner_margin(8)
        .show(ui, |ui| {
            ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            let inner = width - 16.0;
            let (rect, _) = ui.allocate_exact_size(vec2(inner, t.control_height() + 6.0), Sense::hover());
            w::glyph(ui, Glyph::Search, rect.left_center() + vec2(14.0, 0.0), 14.0, t.color.text.muted);
            let typed = w::text_in(ui, rect.with_min_x(rect.left() + 32.0), Align2::LEFT_CENTER, &t.ty.body, "zone", t.color.text.primary);
            ui.painter().vline(typed.right() + 1.0, typed.y_range().shrink(2.0), Stroke::new(1.5, t.color.accent.text));
            ui.add_space(6.0);
            w::hairline(ui, t, inner);
            ui.add_space(6.0);
            let rows = [
                ("zone", "Place Zone", "PCB", "Z", Sel::Live, St::Rest),
                ("zone", "Fill All Zones", "PCB", "B", Sel::None, St::Rest),
                ("zone", "Unfill All Zones", "PCB", "Ctrl+B", Sel::None, St::Hover),
                ("keepout", "Place Keepout Zone", "PCB", "", Sel::None, St::Rest),
                ("rules", "Board Setup: Zone Defaults", "Settings", "", Sel::None, St::Rest),
                ("zoom_fit", "Zoom to Fit", "View", "Home", Sel::None, St::Rest),
            ];
            for (key, name, group, keys, sel, st) in rows {
                let (rect, _) = w::row(ui, t, st, sel, inner, t.row_height() + 4.0);
                let x = rect.left() + t.space.row_x + 2.0;
                w::icon(ui, key, egui::pos2(x + t.icon / 2.0, rect.center().y), t.icon, true);
                let at = rect.with_min_x(x + t.icon + 10.0);
                let drawn = w::text_in(ui, at, Align2::LEFT_CENTER, &t.ty.body, name, t.color.text.primary);
                w::text_in(ui, at.with_min_x(drawn.right() + 10.0), Align2::LEFT_CENTER, &t.ty.small, group, t.color.text.muted);
                if !keys.is_empty() {
                    let kw = w::width(ui, &t.ty.small, keys) + 10.0;
                    let chip = Rect::from_center_size(
                        egui::pos2(rect.right() - t.space.row_x - kw / 2.0, rect.center().y),
                        vec2(kw, t.ty.small.line + 2.0),
                    );
                    ui.painter().rect_filled(chip, t.radius.mark, t.color.fill.control);
                    w::text_in(ui, chip, Align2::CENTER_CENTER, &t.ty.small, keys, t.color.text.muted);
                }
            }
            ui.add_space(6.0);
            w::hairline(ui, t, inner);
            let (rect, _) = ui.allocate_exact_size(vec2(inner, t.ty.small.line + 10.0), Sense::hover());
            w::text_in(ui, rect.shrink2(vec2(8.0, 0.0)), Align2::LEFT_CENTER, &t.ty.small, "↑↓ to move · Enter to run · Tab for params", t.color.text.muted);
            w::text_in(ui, rect.shrink2(vec2(8.0, 0.0)), Align2::RIGHT_CENTER, &t.ty.numeric_small, "6 of 214", t.color.text.muted);
            });
        });
}

// --- 6 · Colour -----------------------------------------------------------------------

fn hex(c: Color32) -> String {
    if c.a() == 255 {
        format!("#{:02X}{:02X}{:02X}", c.r(), c.g(), c.b())
    } else {
        let [r, g, b, a] = c.to_srgba_unmultiplied();
        format!("#{r:02X}{g:02X}{b:02X} {:.0}%", f32::from(a) / 2.55)
    }
}

fn chip(ui: &mut Ui, t: &Tokens, name: &str, color: Color32, under: Color32) {
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(vec2(44.0, 28.0), Sense::hover());
        ui.painter().rect_filled(rect, t.radius.row, under);
        ui.painter().rect_filled(rect, t.radius.row, color);
        ui.painter().rect_stroke(rect, t.radius.row, Stroke::new(1.0, t.color.stroke.hairline), egui::StrokeKind::Inside);
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            w::label(ui, &t.ty.small, t.color.text.primary, name);
            w::label(ui, &t.ty.numeric_small, t.color.text.muted, &hex(color));
        });
    });
}

fn colour(ui: &mut Ui, t: &Tokens) {
    let c = &t.color;
    ui.horizontal_top(|ui| {
        column(ui, 250.0, |ui| {
            section(ui, t, "Surfaces, text, strokes", |ui| {
                ui.set_width(250.0 - 2.0 * t.space.panel);
                chip(ui, t, "surface.base", c.surface.base, c.surface.base);
                chip(ui, t, "surface.card", c.surface.card, c.surface.card);
                chip(ui, t, "surface.floating", c.surface.floating, c.surface.floating);
                chip(ui, t, "surface.well", c.surface.well, c.surface.well);
                chip(ui, t, "surface.scrim", c.surface.scrim, c.surface.card);
                ui.add_space(6.0);
                chip(ui, t, "text.primary", c.text.primary, c.surface.card);
                chip(ui, t, "text.muted", c.text.muted, c.surface.card);
                ui.add_space(6.0);
                chip(ui, t, "stroke.hairline", c.stroke.hairline, c.surface.card);
                chip(ui, t, "stroke.strong", c.stroke.strong, c.surface.card);
                chip(ui, t, "stroke.focus_ring", c.stroke.focus_ring, c.surface.card);
                ui.add_space(6.0);
                chip(ui, t, "fill.control", c.fill.control, c.surface.card);
                chip(ui, t, "fill.hover", c.fill.hover, c.surface.card);
                chip(ui, t, "fill.press", c.fill.press, c.surface.card);
                chip(ui, t, "fill.selected_unfocused", c.fill.selected_unfocused, c.surface.card);
                ui.add_space(6.0);
                chip(ui, t, "touch", c.touch, c.surface.card);
                chip(ui, t, "close_hover", c.close_hover, c.surface.card);
            });
        });
        column(ui, 640.0, |ui| {
            section(ui, t, "The accent and the three semantic colours, in three strengths", |ui| {
                ui.set_width(640.0 - 2.0 * t.space.panel);
                for (name, tone, mark, text) in [
                    ("accent", &c.accent, Glyph::Dot, "Selected"),
                    ("error", &c.error, Glyph::Cross, "Clearance violation"),
                    ("warning", &c.warning, Glyph::Triangle, "Unconnected pin"),
                    ("success", &c.success, Glyph::Tick, "No violations"),
                ] {
                    ui.horizontal(|ui| {
                        column(ui, 132.0, |ui| chip(ui, t, &format!("{name}.text"), tone.text, c.surface.card));
                        column(ui, 150.0, |ui| chip(ui, t, &format!("{name}.subtle"), tone.subtle, c.surface.card));
                        column(ui, 132.0, |ui| chip(ui, t, &format!("{name}.solid"), tone.solid, c.surface.card));
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = 2.0;
                            ui.horizontal(|ui| {
                                let (r, _) = ui.allocate_exact_size(vec2(14.0, t.ty.body.line), Sense::hover());
                                w::glyph(ui, mark, r.center(), 10.0, tone.text);
                                w::label(ui, &t.ty.body, tone.text, text);
                            });
                            ui.horizontal(|ui| {
                                w::badge(ui, t, Some(tone), false, Some(mark), text);
                                w::badge(ui, t, Some(tone), true, None, "Solid");
                            });
                        });
                    });
                    ui.add_space(4.0);
                }
                note(ui, t, "Red, amber and green re-picked against Paper's warm surfaces. Text strength keeps 4.5:1 on base, card, floating and their hover fills; each solid keeps 4.5:1 for its on-colour, also hovered and pressed; ordinary and muted text keep 4.5:1 on each subtle tint. The warning solid is a real amber with dark text on it; the other solids carry white.");
            });
            section(ui, t, "Selection", |ui| {
                ui.set_width(640.0 - 2.0 * t.space.panel);
                ui.horizontal_top(|ui| {
                    for (title, sel, st) in [
                        ("Focused Panel", Sel::Live, St::Rest),
                        ("Panel without focus", Sel::Unfocused, St::Rest),
                        ("Hover only", Sel::None, St::Hover),
                    ] {
                        column(ui, 196.0, |ui| {
                            ui.spacing_mut().item_spacing.y = 0.0;
                            w::label(ui, &t.ty.small, t.color.text.muted, title);
                            for (i, name) in ["R1  10k", "R2  4k7", "C1  100n"].into_iter().enumerate() {
                                let (sel, st) = if i == 1 { (sel, st) } else { (Sel::None, St::Rest) };
                                let (rect, _) = w::row(ui, t, st, sel, 190.0, t.row_height());
                                w::text_in(ui, rect.shrink2(vec2(t.space.row_x, 0.0)), Align2::LEFT_CENTER, &t.ty.body, name, c.text.primary);
                            }
                        });
                    }
                });
                note(ui, t, "Only one place shows a live blue selection. A selected row elsewhere, or anywhere while REDA's window is in the background, is the neutral fill.");
            });
        });
        column(ui, 640.0, |ui| {
            section(ui, t, "Icon role colours on each surface", |ui| {
                ui.set_width(640.0 - 2.0 * t.space.panel);
                for (name, ground) in [
                    ("base", c.surface.base),
                    ("card", c.surface.card),
                    ("floating", c.surface.floating),
                    ("card + hover", over(c.fill.hover, c.surface.card)),
                    ("accent subtle", over(c.accent.subtle, c.surface.card)),
                ] {
                    ui.horizontal(|ui| {
                        let (rect, _) = ui.allocate_exact_size(vec2(360.0, 32.0), Sense::hover());
                        ui.painter().rect_filled(rect, t.radius.control, ground);
                        ui.painter().rect_stroke(rect, t.radius.control, Stroke::new(1.0, c.stroke.hairline), egui::StrokeKind::Inside);
                        let keys = ["select", "wire", "track", "net", "measure", "keepout", "zone", "via", "pad", "text", "drc", "layers"];
                        for (i, key) in keys.into_iter().enumerate() {
                            w::icon(ui, key, rect.left_center() + vec2(22.0 + 28.0 * i as f32, 0.0), t.icon_bar, true);
                        }
                        w::label(ui, &t.ty.small, c.text.muted, name);
                    });
                }
                note(ui, t, "Icons keep their six role colours at full strength on every surface, including the on state's accent tint. The general blue is the accent hue.");
            });
            section(ui, t, "Contrast rules", |ui| {
                ui.set_width(640.0 - 2.0 * t.space.panel);
                let rules = super::contrast_rules(t);
                let mut worst: Vec<(f64, &str)> = rules
                    .iter()
                    .map(|(name, fg, bg, _)| (crate::theme::contrast_ratio(*fg, *bg), name.as_str()))
                    .collect();
                let failures = rules
                    .iter()
                    .filter(|(_, fg, bg, need)| crate::theme::contrast_ratio(*fg, *bg) < *need)
                    .count();
                worst.sort_by(|a, b| a.0.total_cmp(&b.0));
                let tone = if failures == 0 { &c.success } else { &c.error };
                let mark = if failures == 0 { Glyph::Tick } else { Glyph::Cross };
                w::badge(ui, t, Some(tone), false, Some(mark), &format!("{} rules, {failures} fail", rules.len()));
                w::label(ui, &t.ty.small, c.text.muted, "The tightest pairs in this theme:");
                for (ratio, name) in worst.iter().filter(|(_, n)| !n.contains("line") && !n.contains("ring")).take(7) {
                    ui.horizontal(|ui| {
                        let (r, _) = ui.allocate_exact_size(vec2(40.0, t.ty.small.line), Sense::hover());
                        w::text_in(ui, r, Align2::RIGHT_CENTER, &t.ty.numeric_small, &format!("{ratio:.2}"), c.text.primary);
                        w::label(ui, &t.ty.small, c.text.muted, name);
                    });
                }
            });
        });
    });
}

// --- 7 · Type -------------------------------------------------------------------------

fn type_page(ui: &mut Ui, t: &Tokens) {
    let c = &t.color;
    ui.horizontal_top(|ui| {
        column(ui, 500.0, |ui| {
            section(ui, t, "The type scale", |ui| {
                ui.set_width(500.0 - 2.0 * t.space.panel);
                for (name, style, sample) in [
                    ("type.heading", &t.ty.heading, "Board Setup"),
                    ("type.title", &t.ty.title, "Design Rules"),
                    ("type.body_strong", &t.ty.body_strong, "usb_c_mcu.pcb"),
                    ("type.body", &t.ty.body, "Clearance between copper"),
                    ("type.small", &t.ty.small, "Top Copper · 0.25 mm"),
                    ("type.numeric", &t.ty.numeric, "X 121.920  Y 48.260"),
                    ("type.mono", &t.ty.mono, "pcb.zone.fill {\"all\": true}"),
                ] {
                    ui.horizontal(|ui| {
                        let (r, _) = ui.allocate_exact_size(vec2(190.0, style.line), Sense::hover());
                        w::text_in(ui, r, Align2::LEFT_CENTER, &t.ty.small, &format!("{name}  {}/{}", style.size, style.line), c.text.muted);
                        let g = w::galley(ui, style, sample, c.text.primary);
                        let (rect, _) = ui.allocate_exact_size(g.size(), Sense::hover());
                        // The line box, to show the glyphs centred in it.
                        ui.painter().rect_filled(rect, 0.0, c.fill.hover);
                        ui.painter().galley(rect.min, g, c.text.primary);
                    });
                }
                note(ui, t, "Each style is family, weight, size and line height together (size/line shown). The tinted box is the line height: a token per size (about 1.5 em on a 2px grid), not the face's own. Weights are regular, medium (body_strong, the active tab, primary buttons) and semibold (title, heading).");
            });
            section(ui, t, "Line height is the token's, whatever the face", |ui| {
                ui.set_width(500.0 - 2.0 * t.space.panel);
                let faces: [(&str, egui::FontFamily); 3] = [
                    ("Inter", t.ty.body.family.clone()),
                    ("DejaVu Sans", egui::FontFamily::Name("proto-dejavu".into())),
                    ("Hack", egui::FontFamily::Monospace),
                ];
                ui.horizontal(|ui| {
                    w::label(ui, &t.ty.small, c.text.muted, "REDA widgets:");
                    for (name, family) in &faces {
                        let style = super::Text { family: family.clone(), ..t.ty.body.clone() };
                        let g = w::galley(ui, &style, &format!("{name} Hxgjy"), c.text.primary);
                        let (rect, _) = ui.allocate_exact_size(g.size() + vec2(2.0 * t.space.control_x, 2.0 * t.space.control_y), Sense::hover());
                        ui.painter().rect_filled(rect, t.radius.control, c.fill.control);
                        ui.painter().galley(rect.min + vec2(t.space.control_x, t.space.control_y), g, c.text.primary);
                    }
                });
                ui.horizontal(|ui| {
                    w::label(ui, &t.ty.small, c.text.muted, "Stock egui buttons, RichText with the token's line height:");
                });
                ui.horizontal(|ui| {
                    ui.spacing_mut().button_padding = vec2(t.space.control_x, t.space.control_y);
                    ui.spacing_mut().interact_size.y = 0.0;
                    for (name, family) in &faces {
                        let style = super::Text { family: family.clone(), ..t.ty.body.clone() };
                        let r = ui.add(egui::Button::new(style.rich(&format!("{name} Hxgjy")).color(c.text.primary)));
                        w::label(ui, &t.ty.numeric_small, c.text.muted, &format!("{}px", r.rect.height()));
                    }
                });
                ui.horizontal(|ui| {
                    w::label(ui, &t.ty.small, c.text.muted, "Stock egui buttons, plain text (the face's own row height):");
                });
                ui.horizontal(|ui| {
                    ui.spacing_mut().button_padding = vec2(t.space.control_x, t.space.control_y);
                    ui.spacing_mut().interact_size.y = 0.0;
                    for (name, family) in &faces {
                        let text = egui::RichText::new(format!("{name} Hxgjy"))
                            .font(egui::FontId::new(t.ty.body.size, family.clone()))
                            .color(c.text.primary);
                        let r = ui.add(egui::Button::new(text));
                        w::label(ui, &t.ty.numeric_small, c.text.muted, &format!("{}px", r.rect.height()));
                    }
                });
                note(ui, t, "Checked: `TextFormat::line_height` (and `RichText::line_height`) fixes the row height and epaint centres the glyphs in it from the face's ascent and descent, as CSS does. Text given to a stock widget as a plain string takes the face's own row height, so every REDA widget builds its text from a text-style token. A stock `TextEdit` takes the same through its layouter.");
            });
        });
        column(ui, 500.0, |ui| {
            section(ui, t, "Tabular digits", |ui| {
                ui.set_width(500.0 - 2.0 * t.space.panel);
                ui.horizontal_top(|ui| {
                    let prop = super::Text { family: egui::FontFamily::Name("proto-inter-proportional".into()), ..t.ty.numeric.clone() };
                    for (title, style) in [("Inter as shipped", &prop), ("Bundled Inter, tnum baked in", &t.ty.numeric)] {
                        column(ui, 220.0, |ui| {
                            ui.spacing_mut().item_spacing.y = 0.0;
                            w::label(ui, &t.ty.small, c.text.muted, title);
                            for n in ["111.111", "121.920", "48.260", "1.117", "0.250", "888.888"] {
                                let (rect, _) = ui.allocate_exact_size(vec2(120.0, style.line), Sense::hover());
                                w::text_in(ui, rect, Align2::RIGHT_CENTER, style, n, c.text.primary);
                                ui.painter().hline(rect.x_range(), rect.bottom(), Stroke::new(1.0, c.stroke.hairline));
                            }
                        });
                    }
                });
                note(ui, t, "egui cannot switch OpenType features on, so the bundled Inter carries tabular digits as its default: a small in-house step reads the font's tnum substitutions and rewrites its character map (no new dependency). Numeric recipes: number fields, coordinates and deltas in the status bar, number columns in tables, counts, the Command Palette's result count. Running text keeps the chosen face; a numeric recipe uses the chosen face when its digits measure equal, otherwise the bundled Inter.");
            });
            section(ui, t, "Monospace candidates", |ui| {
                ui.set_width(500.0 - 2.0 * t.space.panel);
                for (name, key) in super::MONO_CANDIDATES {
                    let style = super::Text { family: egui::FontFamily::Name(key.into()), ..t.ty.mono.clone() };
                    w::label(ui, &t.ty.small, c.text.muted, name);
                    w::label(ui, &style, c.text.primary, "pcb.track.add {\"net\": \"GND\", \"width\": \"0.25mm\"}");
                    w::label(ui, &style, c.text.primary, "0O 1lI| {}[]() => != <= 0123456789");
                    ui.add_space(4.0);
                }
                note(ui, t, "Only faces already on this machine are shown. JetBrains Mono, Geist Mono and IBM Plex Mono are the other candidates; they need a download to compare.");
            });
            section(ui, t, "Weight never carries meaning alone", |ui| {
                ui.set_width(500.0 - 2.0 * t.space.panel);
                note(ui, t, "Switch the typeface (F6) to a regular-only face: the active tab still has its card and accent line, the primary button its fill, titles their position and size, table headers their muted colour. Medium and semibold only reinforce.");
            });
        });
        column(ui, 520.0, |ui| {
            section(ui, t, "Text size 11, 13 and 16 (same Density)", |ui| {
                ui.set_width(520.0 - 2.0 * t.space.panel);
                for size in [11.0, 13.0, 16.0] {
                    let theme = if t.dark { egui::Theme::Dark } else { egui::Theme::Light };
                    let tt = Tokens::new(theme, t.compact, size);
                    w::label(ui, &t.ty.small, c.text.muted, &format!(
                        "Body {size}px: control {}px, row {}px, tab {}px; small {}px, heading {}px",
                        tt.control_height(), tt.row_height(), tt.tab_height(), tt.ty.small.size, tt.ty.heading.size
                    ));
                    ui.horizontal(|ui| {
                        w::button(ui, &tt, St::Rest, Kind::Primary, None, "Run DRC");
                        w::button(ui, &tt, St::Rest, Kind::Secondary, Some("zoom_fit"), "Fit");
                        let f = Field { numeric: true, unit: Some("mm"), ..Default::default() };
                        w::text_field(ui, &tt, St::Rest, &mut s("0.250"), 7.0 * size, f);
                        w::combo(ui, &tt, St::Rest, "Top Copper", 9.0 * size);
                        w::checkbox(ui, &tt, St::Rest, Check::On, "Visible");
                    });
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        w::tab(ui, &tt, St::Rest, w::Tab { kind: w::TabKind::Active, icon: "document_pcb", title: "usb_c_mcu.pcb", modified: false, out_of_sync: false, window_active: true });
                        w::tab(ui, &tt, St::Rest, w::Tab { kind: w::TabKind::Back, icon: "document_schematic", title: "Sheet 1", modified: true, out_of_sync: false, window_active: true });
                    });
                    ui.scope(|ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        for (i, (name, value)) in [("R1", "10k"), ("C1", "100n")].into_iter().enumerate() {
                            let sel = if i == 0 { Sel::Live } else { Sel::None };
                            let (rect, _) = w::row(ui, &tt, St::Rest, sel, 300.0, tt.row_height());
                            let inner = rect.shrink2(vec2(tt.space.row_x, 0.0));
                            w::text_in(ui, inner, Align2::LEFT_CENTER, &tt.ty.body, name, c.text.primary);
                            w::text_in(ui, inner, Align2::RIGHT_CENTER, &tt.ty.small, value, c.text.muted);
                        }
                    });
                    ui.add_space(6.0);
                }
                note(ui, t, "Icons, radii, strokes and the check box stay fixed; paddings stay the Density's; heights grow with the line height; field widths follow the text size. At 11px the small style would be 9px and stops at the 10px floor.");
            });
        });
    });
}
