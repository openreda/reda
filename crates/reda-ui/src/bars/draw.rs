//! Draws bars from their data: the top row (quick strip, menus, command field and the
//! Preferences gear) and the Active Bar. Drawing never
//! runs anything; each function returns the `(id, params)` of a clicked item, which the
//! window runs through the Shell.

use std::collections::BTreeMap;

use egui::{Align2, Button, Id, Order, Rect, Role, Ui, Vec2};
use serde_json::Value;

use reda_engine::model::Uuid;

use super::{Action, Bars, Item, Menu, Resolved, resolve_for};
use crate::a11y;
use crate::chrome;
use crate::i18n;
use crate::icons;
use crate::keymap;
use crate::shell::Shell;

/// A clicked item: the Command or Preset id and its params.
pub type Invocation = (String, Value);

/// What the bars are drawn against.
pub struct Cx<'a> {
    pub shell: &'a Shell,
    /// Per Command id, the params of the choice in force; an item that binds to the same
    /// Command and params shows as checked (the theme in View → Theme).
    pub checked: &'a BTreeMap<&'static str, Value>,
    /// The Document `$active_document` stands for: the active Document, or an Active
    /// Bar's own canvas's.
    pub document: Option<Uuid>,
}

impl Cx<'_> {
    fn is_checked(&self, item: &Resolved) -> bool {
        self.checked.get(item.command.as_str()) == Some(&item.params)
    }

    /// The key bound to `action` where these bars are drawn, as the platform writes it.
    fn shortcut(&self, ui: &Ui, action: &Action) -> Option<String> {
        let active = keymap::active_for(self.shell, self.document);
        let chord = self.shell.keymap().shortcut(&action.binding, &active)?;
        Some(chord.label(ui.ctx().os().is_mac()))
    }
}

/// The top row: from the left the quick strip (`bars.toolbar`), the menus, the command
/// field and, at the far right, the Preferences gear. One row, so the canvas keeps the
/// height a second one would take.
pub fn top_row(ui: &mut Ui, cx: &Cx<'_>, bars: &Bars) -> Option<Invocation> {
    ui.horizontal(|ui| {
        let row = ui.max_rect();
        let quick = icon_row_sized(
            ui,
            cx,
            &bars.toolbar,
            chrome::QUICK_BUTTON,
            chrome::QUICK_ICON,
        );
        if !bars.toolbar.is_empty() {
            chrome::bar_separator(ui, chrome::QUICK_ICON);
        }
        // The menus centre on the quick strip's row.
        let strip = ui.min_rect();
        let y = strip.center().y;
        let menus = Rect::from_x_y_ranges(ui.cursor().left()..=row.right(), strip.y_range());
        let (menu, menus_end) = ui
            .scope_builder(
                egui::UiBuilder::new()
                    .max_rect(menus)
                    .layout(egui::Layout::left_to_right(egui::Align::Center)),
                |ui| {
                    // The menu bar's row is as tall as the strip, its titles centred in it.
                    ui.spacing_mut().interact_size.y = strip.height();
                    menu_bar(ui, cx, bars)
                },
            )
            .inner;
        let gear_at = Rect::from_center_size(
            egui::pos2(row.right() - chrome::QUICK_BUTTON / 2.0, y),
            Vec2::splat(chrome::QUICK_BUTTON),
        );
        let field = command_field(ui, cx, menus_end..=gear_at.left(), row.center().x, y);
        let gear = gear(ui, cx, gear_at);
        quick.or(menu).or(field).or(gear)
    })
    .inner
}

/// Menu titles and items a size up from the rest of the UI.
fn larger(ui: &mut Ui) {
    ui.style_mut().override_font_id = Some(egui::FontId::proportional(chrome::MENU_TEXT));
    ui.spacing_mut().button_padding = chrome::MENU_PADDING;
}

/// The menu bar of `bars`; returns the item clicked and where the last menu title ends.
pub fn menu_bar(ui: &mut Ui, cx: &Cx<'_>, bars: &Bars) -> (Option<Invocation>, f32) {
    let mut clicked = None;
    let mut end = ui.cursor().left();
    egui::MenuBar::new().ui(ui, |ui| {
        larger(ui);
        for menu in &bars.menus {
            let response = ui.menu_button(i18n::translate(&menu.title, &[]), |ui| {
                larger(ui);
                menu_items(ui, cx, &menu.items, &mut clicked);
            });
            end = response.response.rect.right();
        }
    });
    (clicked, end)
}

/// The command field's place: a field reading what the Palette searches, with its key.
/// It is left out while the row has no room for it between the menus and the gear.
fn command_field(
    ui: &mut Ui,
    cx: &Cx<'_>,
    room: std::ops::RangeInclusive<f32>,
    centre: f32,
    y: f32,
) -> Option<Invocation> {
    const WIDTH: f32 = 360.0;
    const NARROWEST: f32 = 160.0;
    const GAP: f32 = 12.0;
    let (left, right) = (room.start() + GAP, room.end() - GAP);
    let width = WIDTH.min(right - left);
    if width < NARROWEST {
        return None;
    }
    let centre = centre.clamp(left + width / 2.0, right - width / 2.0);
    let action = Action {
        id: "top_row.command_field".to_owned(),
        binding: super::Binding::Preset("app.dialog.open:palette".to_owned()),
        title: None,
        icon: None,
    };
    let item = resolve_for(cx.shell, &action, cx.document);
    let mut button =
        Button::new(egui::RichText::new(crate::tr!("command-field-placeholder")).weak())
            .fill(ui.visuals().extreme_bg_color)
            .truncate();
    if let Some(shortcut) = cx.shortcut(ui, &action) {
        button = button.right_text(egui::RichText::new(shortcut).weak());
    }
    let at = Rect::from_center_size(egui::pos2(centre, y), Vec2::new(width, chrome::QUICK_ICON));
    crate::palette::remember_command_field(ui.ctx(), at);
    let response = ui.put(at, button).on_hover_text(&item.title);
    response.clicked().then_some(item.run.ok()).flatten()
}

/// The Preferences gear at the far right of the top row, at `at`.
fn gear(ui: &mut Ui, cx: &Cx<'_>, at: Rect) -> Option<Invocation> {
    let action = Action {
        id: "top_row.preferences".to_owned(),
        binding: super::Binding::Preset("app.dialog.open:preferences".to_owned()),
        title: None,
        icon: None,
    };
    let mut clicked = None;
    ui.scope_builder(egui::UiBuilder::new().max_rect(at), |ui| {
        clicked = icon_button_for(ui, cx, &action, chrome::QUICK_BUTTON, chrome::QUICK_ICON);
    });
    clicked
}

fn submenu(ui: &mut Ui, cx: &Cx<'_>, menu: &Menu, clicked: &mut Option<Invocation>) {
    ui.menu_button(i18n::translate(&menu.title, &[]), |ui| {
        menu_items(ui, cx, &menu.items, clicked);
    });
}

fn menu_items(ui: &mut Ui, cx: &Cx<'_>, items: &[Item], clicked: &mut Option<Invocation>) {
    let theme = ui.ctx().theme();
    for item in items {
        match item {
            Item::Separator => {
                ui.separator();
            }
            Item::Submenu(menu) => submenu(ui, cx, menu, clicked),
            Item::Action(action) => {
                let item = resolve_for(cx.shell, action, cx.document);
                let enabled = item.run.is_ok();
                let image = icons::image(&item.icon, icons::MENU, theme, enabled);
                // Button::new keeps the icon at its size; Button::image_and_text would
                // shrink it to the text height.
                let mut button = match image {
                    Some(image) => Button::new((image, item.title.clone())),
                    None => Button::new(item.title.clone()),
                }
                .selected(cx.is_checked(&item));
                if let Some(shortcut) = cx.shortcut(ui, action) {
                    button = button.shortcut_text(shortcut);
                }
                let response = ui.add_enabled(enabled, button);
                match &item.run {
                    Ok(run) => {
                        if response.on_hover_text(&item.description).clicked() {
                            *clicked = Some(run.clone());
                            ui.close();
                        }
                    }
                    Err(reason) => {
                        response.on_disabled_hover_text(reason);
                    }
                }
            }
        }
    }
}

/// A row of square icon buttons, as on the Active Bar.
pub fn icon_row(ui: &mut Ui, cx: &Cx<'_>, items: &[Item]) -> Option<Invocation> {
    icon_row_sized(ui, cx, items, chrome::BAR_BUTTON, icons::TOOLBAR)
}

/// A row of square icon buttons `side` points wide holding `icon`-point icons.
fn icon_row_sized(
    ui: &mut Ui,
    cx: &Cx<'_>,
    items: &[Item],
    side: f32,
    icon: f32,
) -> Option<Invocation> {
    let mut clicked = None;
    ui.spacing_mut().item_spacing.x = chrome::BAR_SPACING;
    for item in items {
        let action = match item {
            Item::Separator => {
                chrome::bar_separator(ui, side);
                continue;
            }
            // A submenu on a bar opens as a menu under its button: its icon, or else its
            // title.
            Item::Submenu(menu) => {
                let mut inner = None;
                let title = i18n::translate(&menu.title, &[]);
                let image = menu
                    .icon
                    .as_deref()
                    .and_then(|key| icons::image(key, icon, ui.ctx().theme(), true));
                match image {
                    Some(image) => {
                        // Shown as active while any of its items is.
                        let checked = menu.items.iter().any(|i| match i {
                            Item::Action(a) => {
                                cx.is_checked(&resolve_for(cx.shell, a, cx.document))
                            }
                            _ => false,
                        });
                        let response = chrome::icon_button(ui, image, icon, side, checked, true)
                            .on_hover_text(&title);
                        a11y::name(&response, Role::Button, true, &title, None, None);
                        egui::Popup::menu(&response).show(|ui| {
                            menu_items(ui, cx, &menu.items, &mut inner);
                        });
                    }
                    None => {
                        ui.menu_button(title, |ui| {
                            menu_items(ui, cx, &menu.items, &mut inner);
                        });
                    }
                }
                clicked = clicked.or(inner);
                continue;
            }
            Item::Action(action) => action,
        };
        clicked = clicked.or(icon_button_for(ui, cx, action, side, icon));
    }
    clicked
}

/// One square icon button for `action`, with its tooltip and AccessKit name.
fn icon_button_for(
    ui: &mut Ui,
    cx: &Cx<'_>,
    action: &Action,
    side: f32,
    icon: f32,
) -> Option<Invocation> {
    icon_button_toggled(ui, cx, action, side, icon, None)
}

/// [`icon_button_for`], shown toggled on or off by `toggled` when it is a toggle.
fn icon_button_toggled(
    ui: &mut Ui,
    cx: &Cx<'_>,
    action: &Action,
    side: f32,
    icon: f32,
    toggled: Option<bool>,
) -> Option<Invocation> {
    let theme = ui.ctx().theme();
    let mut clicked = None;
    let item = resolve_for(cx.shell, action, cx.document);
    let enabled = item.run.is_ok();
    // The tooltip names the shortcut: "Undo (Ctrl+Z)".
    let shortcut = cx.shortcut(ui, action);
    let heading = match &shortcut {
        Some(shortcut) => format!("{} ({shortcut})", item.title),
        None => item.title.clone(),
    };
    let checked = toggled.unwrap_or_else(|| cx.is_checked(&item));
    // A disabled button still senses the pointer, for the tooltip saying why.
    let response = match icons::image(&item.icon, icons::TOOLBAR, theme, enabled) {
        Some(image) => chrome::icon_button(ui, image, icon, side, checked, enabled),
        None => ui.add_enabled(
            enabled,
            Button::new(item.title.clone())
                .selected(checked)
                .min_size(Vec2::splat(side)),
        ),
    };
    let response = match &item.run {
        Ok(run) => {
            let response = response.on_hover_ui(|ui| {
                ui.strong(&heading);
                ui.label(&item.description);
            });
            if response.clicked() {
                clicked = Some(run.clone());
            }
            response
        }
        Err(reason) => {
            let tooltip = |ui: &mut Ui| {
                ui.strong(&heading);
                ui.label(reason);
            };
            if response.enabled() {
                response.on_hover_ui(tooltip)
            } else {
                response.on_disabled_hover_ui(tooltip)
            }
        }
    };
    // A choice among several (the active Tool) or a toggle reads as on or off.
    let selected = toggled
        .or_else(|| (cx.checked.contains_key(item.command.as_str())).then(|| cx.is_checked(&item)));
    a11y::name(
        &response,
        Role::Button,
        enabled,
        &item.title,
        selected,
        shortcut.as_deref(),
    );
    clicked
}

/// How far below the canvas top the Active Bar floats.
pub(crate) const ACTIVE_BAR_TOP: f32 = 12.0;
/// The Active Bar's frame: its inner margin and window stroke, on each side.
pub(crate) const ACTIVE_BAR_MARGIN: i8 = 3;
/// What the frame adds to the row's width: margin and stroke on both sides.
const ACTIVE_BAR_FRAME: f32 = 2.0 * (ACTIVE_BAR_MARGIN as f32 + 1.0);
/// How close the Active Bar comes to the canvas's sides.
const ACTIVE_BAR_INSET: f32 = 8.0;
/// How far below the canvas top the Active Bar ends, for overlays that sit under it.
pub const ACTIVE_BAR_BOTTOM: f32 = ACTIVE_BAR_TOP + ACTIVE_BAR_FRAME + chrome::BAR_BUTTON;

/// How far above the canvas bottom a Tool's bar (the Routing or Placement Bar) ends,
/// clear of the grid readout.
pub const TOOL_BAR_BOTTOM: f32 = 36.0;
/// The height of the Layer strip along a board canvas's bottom.
pub const LAYER_STRIP_HEIGHT: f32 = 24.0;
/// How far above the canvas bottom the grid readout ends.
pub const GRID_READOUT_BOTTOM: f32 = 8.0;

/// What the Layer strip takes from the canvas bottom: [`LAYER_STRIP_HEIGHT`] while it
/// shows, else nothing. Everything anchored to the bottom moves up by this much.
pub fn bottom_inset(strip: bool) -> f32 {
    if strip { LAYER_STRIP_HEIGHT } else { 0.0 }
}

/// Where a Tool's bar sits on `canvas`: its bottom centre, 36 px above the canvas's
/// bottom edge, and a further 24 px up while the Layer `strip` shows. A Tool's bar is
/// pivoted on this point (`Align2::CENTER_BOTTOM`).
pub fn tool_bar_anchor(canvas: Rect, strip: bool) -> egui::Pos2 {
    canvas.center_bottom() - Vec2::new(0.0, TOOL_BAR_BOTTOM + bottom_inset(strip))
}

/// Where the grid readout sits on `canvas`: its bottom left corner, moved up while the
/// Layer `strip` shows.
pub fn grid_readout_anchor(canvas: Rect, strip: bool) -> egui::Pos2 {
    canvas.left_bottom() + Vec2::new(8.0, -(GRID_READOUT_BOTTOM + bottom_inset(strip)))
}

/// The width of a row of bar items.
fn row_width(items: &[Item]) -> f32 {
    let widths: f32 = items
        .iter()
        .map(|item| match item {
            Item::Separator => chrome::BAR_SEPARATOR,
            _ => chrome::BAR_BUTTON,
        })
        .sum();
    widths + chrome::BAR_SPACING * items.len().saturating_sub(1) as f32
}

/// How many of the Active Bar's `items` fit on a canvas `width` points wide. The rest go
/// in a menu behind a "More Tools" button at the end of the row. A separator never ends
/// the row.
pub fn active_bar_fit(items: &[Item], width: f32) -> usize {
    let room = width - 2.0 * ACTIVE_BAR_INSET - ACTIVE_BAR_FRAME;
    if row_width(items) <= room {
        return items.len();
    }
    let trimmed = |n: usize| {
        let mut n = n;
        while n > 0 && matches!(items[n - 1], Item::Separator) {
            n -= 1;
        }
        n
    };
    let more = chrome::BAR_SPACING + chrome::BAR_BUTTON;
    (1..items.len())
        .rev()
        .map(trimmed)
        .find(|&n| n > 0 && row_width(&items[..n]) + more <= room)
        .unwrap_or(0)
}

/// The Active Bar of the canvas `id`, floating at the top centre of its rect `canvas`.
/// What does not fit on a narrow canvas goes in a menu at the end of the row.
pub fn active_bar(
    ui: &mut Ui,
    cx: &Cx<'_>,
    bars: &Bars,
    id: Id,
    canvas: Rect,
) -> Option<Invocation> {
    let items = &bars.active_bar;
    if items.is_empty() {
        return None;
    }
    let shown = active_bar_fit(
        items,
        canvas.width() - chrome::BAR_BUTTON - chrome::BAR_SPACING,
    );
    let (row, rest) = items.split_at(shown);
    let rest = &rest[rest
        .iter()
        .take_while(|i| matches!(i, Item::Separator))
        .count()..];
    egui::Area::new(id.with("active-bar"))
        .order(Order::Middle)
        .fixed_pos(canvas.center_top() + Vec2::new(0.0, ACTIVE_BAR_TOP))
        .pivot(Align2::CENTER_TOP)
        .constrain_to(canvas.shrink2(Vec2::new(ACTIVE_BAR_INSET, 0.0)))
        .show(ui.ctx(), |ui| {
            egui::Frame::window(ui.style())
                .inner_margin(egui::Margin::same(ACTIVE_BAR_MARGIN))
                .corner_radius(egui::CornerRadius::same(5))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let mut clicked = icon_row(ui, cx, row);
                        if !rest.is_empty() {
                            clicked = clicked.or(more_tools(ui, cx, rest));
                        }
                        if let Some(document) = cx.document {
                            clicked = clicked
                                .or(crate::selection::filter_ui::button(ui, cx.shell, document))
                                .or(crate::snap::palette_button(ui, cx.shell, document));
                        }
                        clicked
                    })
                    .inner
                })
                .inner
        })
        .inner
}

/// The Active Bar's "More Tools" button, whose menu holds the items that did not fit.
fn more_tools(ui: &mut Ui, cx: &Cx<'_>, items: &[Item]) -> Option<Invocation> {
    let title = crate::tr!("active-bar-more");
    let button = Button::new("»")
        .frame_when_inactive(false)
        .min_size(Vec2::splat(chrome::BAR_BUTTON));
    let response = ui.add(button).on_hover_text(&title);
    a11y::name(&response, Role::Button, true, &title, None, None);
    let mut clicked = None;
    egui::Popup::menu(&response).show(|ui| {
        menu_items(ui, cx, items, &mut clicked);
    });
    clicked
}

/// How a Tool's bar shows one of its items now.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ToolBarItem {
    /// Left off the bar (the Routing Bar's width with no route in flight).
    pub hidden: bool,
    /// The icon that shows the mode, instead of the item's own.
    pub icon: Option<String>,
    /// Shown toggled on or off; `None` is a plain button.
    pub toggled: Option<bool>,
    /// Shown as this text instead of an icon (the width's value).
    pub text: Option<String>,
}

/// A Tool's bar (the Routing Bar): `items` in the Active Bar's frame and buttons, pivoted
/// on its bottom centre at `anchor` ([`tool_bar_anchor`]), each item as `state` says. A
/// `note` shows just above it. Returns the item clicked.
pub fn tool_bar(
    ui: &Ui,
    cx: &Cx<'_>,
    items: &[Item],
    id: Id,
    anchor: egui::Pos2,
    note: Option<&str>,
    state: &dyn Fn(&Action) -> ToolBarItem,
) -> Option<Invocation> {
    let shown = egui::Area::new(id.with("tool-bar"))
        .order(Order::Middle)
        .fixed_pos(anchor)
        .pivot(Align2::CENTER_BOTTOM)
        .show(ui.ctx(), |ui| {
            egui::Frame::window(ui.style())
                .inner_margin(egui::Margin::same(ACTIVE_BAR_MARGIN))
                .corner_radius(egui::CornerRadius::same(5))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = chrome::BAR_SPACING;
                        let mut clicked = None;
                        for item in items {
                            let action = match item {
                                Item::Separator => {
                                    chrome::bar_separator(ui, chrome::BAR_BUTTON);
                                    continue;
                                }
                                Item::Submenu(_) => continue,
                                Item::Action(action) => action,
                            };
                            let look = state(action);
                            if look.hidden {
                                continue;
                            }
                            let hit = match &look.text {
                                Some(text) => text_button(ui, cx, action, text),
                                None => {
                                    let mut action = action.clone();
                                    if look.icon.is_some() {
                                        action.icon = look.icon.clone();
                                    }
                                    icon_button_toggled(
                                        ui,
                                        cx,
                                        &action,
                                        chrome::BAR_BUTTON,
                                        icons::TOOLBAR,
                                        look.toggled,
                                    )
                                }
                            };
                            clicked = clicked.or(hit);
                        }
                        clicked
                    })
                    .inner
                })
                .inner
        });
    if let Some(note) = note {
        let top = shown.response.rect.center_top() - Vec2::new(0.0, 4.0);
        egui::Area::new(id.with("tool-bar-note"))
            .order(Order::Middle)
            .fixed_pos(top)
            .pivot(Align2::CENTER_BOTTOM)
            .interactable(false)
            .show(ui.ctx(), |ui| {
                egui::Frame::window(ui.style())
                    .inner_margin(egui::Margin::symmetric(6, 2))
                    .corner_radius(egui::CornerRadius::same(4))
                    .show(ui, |ui| {
                        let response = ui.label(note);
                        a11y::name(&response, Role::Label, true, note, None, None);
                    });
            });
    }
    shown.inner
}

/// A bar button showing `text` (a value) instead of an icon, named "<title>: <text>".
fn text_button(ui: &mut Ui, cx: &Cx<'_>, action: &Action, text: &str) -> Option<Invocation> {
    let item = resolve_for(cx.shell, action, cx.document);
    let shortcut = cx.shortcut(ui, action);
    let heading = match &shortcut {
        Some(shortcut) => format!("{} ({shortcut})", item.title),
        None => item.title.clone(),
    };
    let response = ui
        .add_enabled(
            item.run.is_ok(),
            Button::new(text)
                .frame_when_inactive(false)
                .min_size(Vec2::new(chrome::BAR_BUTTON, chrome::BAR_BUTTON)),
        )
        .on_hover_ui(|ui| {
            ui.strong(&heading);
            ui.label(&item.description);
        });
    let name = format!("{}: {text}", item.title);
    a11y::name(
        &response,
        Role::Button,
        item.run.is_ok(),
        &name,
        None,
        shortcut.as_deref(),
    );
    if response.clicked() {
        item.run.ok()
    } else {
        None
    }
}
