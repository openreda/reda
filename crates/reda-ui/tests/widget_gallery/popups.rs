//! The popups page: menus and context menus, tooltips, the dialog frame with its scrim
//! and button row, the floating surface, the card, and the Dock flyout.
//!
//! A popup cannot be held open in a still image, so each is drawn in place: its own
//! frame with its own rows in forced states.

use egui::{Align2, Rect, Sense, Ui, UiBuilder, Vec2, vec2};
use reda_ui::theme::TokensExt as _;
use reda_ui::widgets::{self, Glyph, State, TooltipText};

use crate::kit;

/// The size the page is snapshotted at.
pub const SIZE: Vec2 = vec2(1700.0, 1300.0);
/// The width of a state column that holds a menu item.
const ITEM: f32 = 190.0;
/// The widths of the menus and dialogs drawn in place.
const MENU: f32 = 260.0;
const SUBMENU: f32 = 170.0;
const DIALOG: f32 = 380.0;
/// The size of the flyout drawn beside its strip.
pub const FLYOUT: Vec2 = vec2(280.0, 300.0);

pub fn page(ui: &mut Ui) {
    let t = ui.tokens();
    ui.spacing_mut().item_spacing = vec2(t.space.section, t.space.section);
    kit::section(ui, "Menu items and menu buttons", items);
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| kit::section(ui, "Menus and context menus", menus));
        ui.vertical(|ui| {
            kit::section(ui, "Tooltips", tooltips);
            kit::section(ui, "Card and floating surface", surfaces);
        });
        ui.vertical(|ui| kit::section(ui, "Dock flyout", flyout));
    });
    kit::section(ui, "Dialog frame, scrim and button row", dialogs);
}

/// A cell of the states grid, as wide as a menu would be: a row fills what it is given.
fn cell(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    let t = ui.tokens();
    let size = vec2(ITEM, t.control_height());
    ui.allocate_ui_with_layout(size, egui::Layout::top_down(egui::Align::Min), |ui| {
        ui.set_max_width(ITEM);
        add(ui);
    });
}

pub fn items(ui: &mut Ui) {
    kit::states(
        ui,
        "menu-items",
        ITEM,
        &mut [
            ("Item", &mut |ui, s| {
                cell(ui, |ui| {
                    ui.add(widgets::menu_item("Show Ratsnest").forced(s));
                });
            }),
            ("With Icon and key", &mut |ui, s| {
                let item = widgets::menu_item("Copy").icon("copy").shortcut("Ctrl+C");
                cell(ui, |ui| {
                    ui.add(item.forced(s));
                });
            }),
            ("Checked", &mut |ui, s| {
                let item = widgets::menu_item("Snap to Grid").checked(true);
                cell(ui, |ui| {
                    ui.add(item.shortcut("Ctrl+G").forced(s));
                });
            }),
            ("Submenu", &mut |ui, s| {
                let item = widgets::menu_item("Find Similar").icon("select_similar");
                cell(ui, |ui| {
                    ui.add(item.submenu_arrow().forced(s));
                });
            }),
            ("Menu button", &mut |ui, s| {
                widgets::menu_button("New").forced(s).show(ui, |_| {});
            }),
            ("Menu bar title", &mut |ui, s| {
                cell(ui, |ui| {
                    widgets::menu_bar(ui, |ui| {
                        widgets::menu_button("Edit").forced(s).show(ui, |_| {});
                    });
                });
            }),
        ],
    );
}

/// A menu drawn in place, `width` wide.
fn menu(ui: &mut Ui, width: f32, add: impl FnOnce(&mut Ui)) {
    let t = ui.tokens();
    widgets::menu_frame(&t).show(ui, |ui| {
        ui.vertical(|ui| {
            ui.set_width(width);
            ui.spacing_mut().item_spacing.y = 0.0;
            add(ui);
        });
    });
}

pub fn menus(ui: &mut Ui) {
    let t = ui.tokens();
    ui.spacing_mut().item_spacing.y = t.space.stack;
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            kit::caption(ui, "The main menu, open under its title");
            let bar = vec2(MENU, t.control_height());
            ui.allocate_ui(bar, |ui| {
                widgets::menu_bar(ui, |ui| {
                    for (title, state) in [
                        ("File", State::Rest),
                        ("Edit", State::Hover),
                        ("View", State::Rest),
                        ("Place", State::Rest),
                    ] {
                        widgets::menu_button(title).forced(state).show(ui, |_| {});
                    }
                });
            });
            menu(ui, MENU, |ui| {
                let item = |text: &str, icon: &'static str, keys: &str| {
                    widgets::menu_item(text).icon(icon).shortcut(keys)
                };
                ui.add(item("Undo Move", "undo", "Ctrl+Z").forced(State::Rest));
                ui.add(item("Redo", "redo", "Ctrl+Shift+Z").forced(State::Disabled));
                widgets::menu_separator(ui);
                ui.add(item("Cut", "cut", "Ctrl+X").forced(State::Rest));
                ui.add(item("Copy", "copy", "Ctrl+C").forced(State::Hover));
                ui.add(item("Paste", "paste", "Ctrl+V").forced(State::Rest));
                ui.add(item("Delete", "delete", "Del").forced(State::Rest));
                widgets::menu_separator(ui);
                let snap = widgets::menu_item("Snap to Grid").checked(true);
                ui.add(snap.shortcut("Ctrl+G").forced(State::Rest));
                let ratsnest = widgets::menu_item("Show Ratsnest").checked(false);
                ui.add(ratsnest.forced(State::Rest));
                let similar = widgets::menu_item("Find Similar").icon("select_similar");
                ui.add(similar.submenu_arrow().forced(State::Focus));
                widgets::menu_separator(ui);
                ui.add(item("Preferences…", "preferences", "Ctrl+,").forced(State::Rest));
            });
        });
        ui.vertical(|ui| {
            kit::caption(ui, "A context menu with its submenu open");
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                menu(ui, ITEM, |ui| {
                    let close = widgets::menu_item("Close").icon("close");
                    ui.add(close.shortcut("Ctrl+W").forced(State::Rest));
                    ui.add(widgets::menu_item("Close Others").forced(State::Rest));
                    widgets::menu_separator(ui);
                    let to = widgets::menu_item("Move to").icon("tab_move");
                    ui.add(to.submenu_arrow().forced(State::Hover));
                    let split = widgets::menu_item("Split Right").icon("split");
                    ui.add(split.forced(State::Rest));
                });
                ui.vertical(|ui| {
                    // Beside the item that opened it.
                    ui.add_space(2.0 * t.row_height() + 9.0);
                    menu(ui, SUBMENU, |ui| {
                        ui.add(widgets::menu_item("New Window").forced(State::Rest));
                        ui.add(widgets::menu_item("Tab Group Below").forced(State::Hover));
                        ui.add(widgets::menu_item("Left Dock").forced(State::Rest));
                    });
                });
            });
        });
    });
}

/// A tooltip drawn in place.
fn tooltip(ui: &mut Ui, text: TooltipText<'_>) {
    let t = ui.tokens();
    widgets::tooltip_frame(&t).show(ui, |ui| {
        ui.vertical(|ui| widgets::tooltip_contents(ui, text));
    });
}

pub fn tooltips(ui: &mut Ui) {
    let t = ui.tokens();
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = t.space.section;
        let name = TooltipText {
            title: "Route Track",
            key: Some("X"),
            sentence: None,
        };
        tooltip(ui, name);
        let plain = TooltipText {
            title: "Zoom to Fit",
            key: None,
            sentence: None,
        };
        tooltip(ui, plain);
        let more = TooltipText {
            title: "Out of sync",
            key: None,
            sentence: Some("The PCB has not taken the last schematic changes. Run Update PCB."),
        };
        tooltip(ui, more);
    });
    ui.add_space(t.space.stack);
    ui.horizontal(|ui| {
        kit::caption(ui, "Key chips");
        for keys in ["X", "Ctrl+K", "Shift+F9", "Home"] {
            ui.add(widgets::key_chip(keys));
        }
    });
}

pub fn surfaces(ui: &mut Ui) {
    let t = ui.tokens();
    // A card is shown on the base, where cards sit.
    egui::Frame::new()
        .fill(t.color.surface.base)
        .corner_radius(t.radius.card)
        .inner_margin(t.space.section)
        .show(ui, |ui| {
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = t.space.section;
                ui.vertical(|ui| {
                    kit::caption(ui, "Card");
                    widgets::card_frame(&t).show(ui, |ui| {
                        ui.set_width(170.0);
                        ui.spacing_mut().item_spacing.y = t.space.stack;
                        ui.label(t.ty.title.rich("Layers").color(t.color.text.primary));
                        for (i, name) in ["Top Copper", "Inner 1", "Bottom Copper"]
                            .into_iter()
                            .enumerate()
                        {
                            let mut on = i != 1;
                            ui.add(widgets::checkbox(&mut on, name));
                        }
                    });
                });
                ui.vertical(|ui| {
                    kit::caption(ui, "Floating surface (a bar on the canvas)");
                    widgets::floating_frame(&t).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 2.0;
                            for (key, state, on) in [
                                ("select", State::Rest, false),
                                ("wire", State::Rest, true),
                                ("net_label", State::Hover, false),
                                ("port", State::Rest, false),
                                ("text", State::Rest, false),
                                ("line", State::Disabled, false),
                            ] {
                                ui.add(widgets::icon_button(key).bar().on(on).forced(state));
                            }
                        });
                    });
                });
            });
        });
}

/// A collapsed Dock's strip on the base with its flyout open beside it, as the window
/// shows them. `side` is where the strip is: at the left or at the right of the flyout.
pub fn strip_and_flyout(ui: &mut Ui, strip_at_left: bool, size: Vec2) {
    let t = ui.tokens();
    let strip = |ui: &mut Ui| {
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing = vec2(0.0, 2.0);
            let chevron = if strip_at_left {
                Glyph::ChevronRight
            } else {
                Glyph::ChevronLeft
            };
            ui.add(widgets::glyph_button(chevron));
            for (key, on) in [
                ("panel_projects", false),
                ("panel_layers", true),
                ("panel_library", false),
                ("panel_find", false),
            ] {
                ui.add(widgets::icon_button(key).bar().on(on));
            }
        });
    };
    let card = |ui: &mut Ui| {
        widgets::flyout_frame(&t).show(ui, |ui| {
            let inner = size - Vec2::splat(2.0 * t.space.panel);
            let down = egui::Layout::top_down(egui::Align::Min);
            ui.allocate_ui_with_layout(inner, down, |ui| {
                ui.set_min_size(inner);
                widgets::flyout_title(ui, Some("panel_layers"), "Layers");
                ui.spacing_mut().item_spacing.y = 0.0;
                let layers = [
                    "Top Copper",
                    "Inner 1",
                    "Inner 2",
                    "Bottom Copper",
                    "Top Silkscreen",
                    "Bottom Silkscreen",
                    "Board Outline",
                ];
                for (i, name) in layers.into_iter().enumerate() {
                    let mut on = i != 2 && i != 5;
                    ui.add(widgets::checkbox(&mut on, name));
                }
            });
        });
    };
    egui::Frame::new()
        .fill(t.color.surface.base)
        .inner_margin(widgets::CARD_GAP)
        .show(ui, |ui| {
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = widgets::CARD_GAP;
                if strip_at_left {
                    strip(ui);
                    card(ui);
                } else {
                    card(ui);
                    strip(ui);
                }
            });
        });
}

pub fn flyout(ui: &mut Ui) {
    kit::caption(ui, "Open beside the strip of a collapsed right Dock");
    strip_and_flyout(ui, false, FLYOUT);
}

/// A dialog drawn in place, `width` wide.
fn dialog(ui: &mut Ui, width: f32, title: &str, add: impl FnOnce(&mut Ui)) {
    let t = ui.tokens();
    widgets::dialog_frame(&t).show(ui, |ui| {
        ui.set_width(width - 2.0 * t.space.dialog);
        ui.spacing_mut().item_spacing.y = t.space.stack;
        widgets::title_row(ui, title, true);
        add(ui);
    });
}

fn body(ui: &mut Ui, text: &str) {
    let t = ui.tokens();
    ui.label(t.ty.body.rich(text).color(t.color.text.primary));
}

pub fn dialogs(ui: &mut Ui) {
    let t = ui.tokens();
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = t.space.section * 2.0;
        ui.vertical(|ui| {
            kit::caption(ui, "A modal over the scrim; a destructive confirm");
            let size = vec2(DIALOG + 80.0, 230.0);
            let (area, _) = ui.allocate_exact_size(size, Sense::hover());
            // What the scrim covers: a card with a list in it.
            let under = UiBuilder::new().max_rect(area.shrink(t.space.gap));
            ui.scope_builder(under, |ui| {
                widgets::card_frame(&t).show(ui, |ui| {
                    ui.set_min_size(ui.available_size());
                    for name in ["Top Copper", "Inner 1", "Inner 2", "Bottom Copper"] {
                        let mut on = true;
                        ui.add(widgets::checkbox(&mut on, name));
                    }
                });
            });
            ui.painter()
                .rect_filled(area, t.radius.card, t.color.surface.scrim);
            let width = DIALOG - 40.0;
            let at = Align2::CENTER_CENTER.align_size_within_rect(vec2(width, 150.0), area);
            let over = UiBuilder::new().max_rect(Rect::from_min_size(at.min, vec2(width, 0.0)));
            ui.scope_builder(over, |ui| {
                dialog(ui, width, "Delete 14 Tracks?", |ui| {
                    body(ui, "The Tracks on Net GND between U1 and J2 are removed.");
                    widgets::button_row(ui, |ui| {
                        ui.add(widgets::button("Delete").danger());
                        ui.add(widgets::button("Cancel").forced(State::Focus));
                    });
                });
            });
        });
        ui.vertical(|ui| {
            kit::caption(ui, "Three choices: the primary button last");
            dialog(ui, DIALOG, "Save changes to usb_c_mcu?", |ui| {
                body(ui, "Two Documents have changes that are not saved.");
                widgets::button_row(ui, |ui| {
                    ui.add(widgets::button("Save All").primary());
                    ui.add(widgets::button("Cancel"));
                    ui.add(widgets::button("Don't Save"));
                });
            });
        });
        ui.vertical(|ui| {
            kit::caption(ui, "A form; the action waits for a valid value");
            dialog(ui, DIALOG, "Paste Array", |ui| {
                for (name, value, unit) in [("Columns", "4", ""), ("Column pitch", "2.540", "mm")] {
                    ui.horizontal(|ui| {
                        let size = vec2(120.0, t.control_height());
                        let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
                        let text = t.ty.body.job(name, t.color.text.primary);
                        let galley = ui.painter().layout_job(text);
                        let at = Align2::LEFT_CENTER.align_size_within_rect(galley.size(), rect);
                        ui.painter().galley(at.min, galley, t.color.text.primary);
                        let mut text = value.to_owned();
                        let field = widgets::number_field(&mut text).width(120.0);
                        let field = if unit.is_empty() {
                            field
                        } else {
                            field.unit(unit)
                        };
                        ui.add(field).widget_info(|| {
                            egui::WidgetInfo::labeled(egui::Role::TextInput, true, name)
                        });
                    });
                }
                widgets::button_row(ui, |ui| {
                    ui.add_enabled(false, widgets::button("Paste").primary());
                    ui.add(widgets::button("Cancel"));
                });
            });
        });
    });
}
