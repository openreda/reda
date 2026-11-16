//! Draws bars from their data: the menus and the quick strip, which the Title bar
//! (`crate::title_bar`) places, and the Active Bar. Drawing never
//! runs anything; each function returns the `(id, params)` of a clicked item, which the
//! window runs through the Shell.

use std::collections::BTreeMap;

use egui::{Align2, Id, Order, Rect, Role, Ui, Vec2};
use serde_json::Value;

use reda_engine::model::Uuid;

use super::{Action, Bars, Item, Menu, Resolved, resolve_for};
use crate::a11y;
use crate::i18n;
use crate::icons;
use crate::keymap;
use crate::shell::Shell;
use crate::theme::{Tokens, TokensExt as _};
use crate::widgets::{self, Tooltip as _};

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

/// The menu bar of `bars`, its titles muted when `muted` (the window in the background);
/// returns the item clicked.
pub fn menu_bar(ui: &mut Ui, cx: &Cx<'_>, bars: &Bars, muted: bool) -> Option<Invocation> {
    let mut clicked = None;
    widgets::menu_bar(ui, |ui| {
        for menu in &bars.menus {
            let title = i18n::translate(&menu.title, &[]);
            widgets::menu_button(title).muted(muted).show(ui, |ui| {
                menu_items(ui, cx, &menu.items, &mut clicked);
            });
        }
    });
    clicked
}

/// The Title bar's quick strip: a row of Icon buttons `side` points square holding
/// `icon`-point Icons, as on the Active Bar.
pub fn quick_strip(
    ui: &mut Ui,
    cx: &Cx<'_>,
    items: &[Item],
    side: f32,
    icon: f32,
) -> Option<Invocation> {
    icon_row_sized(ui, cx, items, side, icon)
}

/// One Icon button for `action`, with its tooltip and AccessKit name: the Title bar's
/// Preferences gear.
pub fn action_button(
    ui: &mut Ui,
    cx: &Cx<'_>,
    action: &Action,
    side: f32,
    icon: f32,
) -> Option<Invocation> {
    icon_button_for(ui, cx, action, side, icon)
}

/// The key bound to `action` where `cx` draws, as the platform writes it.
pub fn shortcut(ui: &Ui, cx: &Cx<'_>, action: &Action) -> Option<String> {
    cx.shortcut(ui, action)
}

pub(crate) fn submenu(ui: &mut Ui, cx: &Cx<'_>, menu: &Menu, clicked: &mut Option<Invocation>) {
    widgets::menu_button(i18n::translate(&menu.title, &[])).show(ui, |ui| {
        menu_items(ui, cx, &menu.items, clicked);
    });
}

pub(crate) fn menu_items(
    ui: &mut Ui,
    cx: &Cx<'_>,
    items: &[Item],
    clicked: &mut Option<Invocation>,
) {
    for item in items {
        match item {
            Item::Separator => widgets::menu_separator(ui),
            Item::Submenu(menu) => submenu(ui, cx, menu, clicked),
            Item::Action(action) => {
                let item = resolve_for(cx.shell, action, cx.document);
                let enabled = item.run.is_ok();
                let mut row = widgets::menu_item(item.title.clone())
                    .icon(&item.icon)
                    .checked(cx.is_checked(&item));
                if let Some(shortcut) = cx.shortcut(ui, action) {
                    row = row.shortcut(shortcut);
                }
                let response = ui.add_enabled(enabled, row);
                match &item.run {
                    Ok(run) => {
                        if response.tooltip(&item.description).clicked() {
                            *clicked = Some(run.clone());
                            ui.close();
                        }
                    }
                    Err(reason) => {
                        response.disabled_tooltip(reason);
                    }
                }
            }
        }
    }
}

/// A row of the Active Bar's Icon buttons, [`widgets::BAR_GAP`] apart.
pub fn icon_row(ui: &mut Ui, cx: &Cx<'_>, items: &[Item]) -> Option<Invocation> {
    let t = ui.tokens();
    icon_row_sized(ui, cx, items, widgets::bar_button_side(&t), t.icon.bar)
}

/// A row of Icon buttons `side` points square holding `icon`-point Icons.
fn icon_row_sized(
    ui: &mut Ui,
    cx: &Cx<'_>,
    items: &[Item],
    side: f32,
    icon: f32,
) -> Option<Invocation> {
    widgets::bar_row(ui, |ui| {
        let mut clicked = None;
        for item in items {
            let action = match item {
                Item::Separator => {
                    widgets::bar_separator_sized(ui, side);
                    continue;
                }
                // A submenu on a bar opens as a menu under its button: its icon, or else
                // its title.
                Item::Submenu(menu) => {
                    clicked = clicked.or(submenu_button(ui, cx, menu, side, icon));
                    continue;
                }
                Item::Action(action) => action,
            };
            clicked = clicked.or(icon_button_for(ui, cx, action, side, icon));
        }
        clicked
    })
    .inner
}

/// A bar's button for a submenu: an Icon button that opens it as a menu, or a menu button
/// with its title when it has no Icon.
fn submenu_button(
    ui: &mut Ui,
    cx: &Cx<'_>,
    menu: &Menu,
    side: f32,
    icon: f32,
) -> Option<Invocation> {
    let mut inner = None;
    let title = i18n::translate(&menu.title, &[]);
    match menu.icon.as_deref().filter(|key| icons::has(key)) {
        Some(key) => {
            // Shown as active while any of its items is.
            let checked = menu.items.iter().any(|i| match i {
                Item::Action(a) => cx.is_checked(&resolve_for(cx.shell, a, cx.document)),
                _ => false,
            });
            let button = widgets::icon_button(key)
                .side(side)
                .icon_size(icon)
                .on(checked);
            let response = ui.add(button).tooltip(&title);
            a11y::name(&response, Role::Button, true, &title, None, None);
            widgets::menu(&response).show(|ui| {
                menu_items(ui, cx, &menu.items, &mut inner);
            });
        }
        None => {
            widgets::menu_button(title).show(ui, |ui| {
                menu_items(ui, cx, &menu.items, &mut inner);
            });
        }
    }
    inner
}

/// One Icon button for `action`, with its tooltip and AccessKit name.
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
    icon_button_with(ui, cx, action, side, icon, toggled).0
}

/// An Active Bar button for `action`, with its response (a group's button adds its menu).
pub(super) fn icon_button_response(
    ui: &mut Ui,
    cx: &Cx<'_>,
    action: &Action,
) -> (Option<Invocation>, egui::Response) {
    let t = ui.tokens();
    let side = widgets::bar_button_side(&t);
    icon_button_with(ui, cx, action, side, t.icon.bar, None)
}

/// A bar button's tooltip: its title with the key that runs it ("Undo (Ctrl+Z)"), and a
/// sentence under it.
fn tooltip(ui: &mut Ui, title: &str, key: Option<&str>, sentence: Option<&str>) {
    let heading = match key {
        Some(key) => format!("{title} ({key})"),
        None => title.to_owned(),
    };
    widgets::tooltip_contents(
        ui,
        widgets::TooltipText {
            title: &heading,
            key: None,
            sentence,
        },
    );
}

fn icon_button_with(
    ui: &mut Ui,
    cx: &Cx<'_>,
    action: &Action,
    side: f32,
    icon: f32,
    toggled: Option<bool>,
) -> (Option<Invocation>, egui::Response) {
    let mut clicked = None;
    let item = resolve_for(cx.shell, action, cx.document);
    let enabled = item.run.is_ok();
    // The tooltip names the shortcut: "Undo (Ctrl+Z)".
    let shortcut = cx.shortcut(ui, action);
    let checked = toggled.unwrap_or_else(|| cx.is_checked(&item));
    // A disabled Icon button still senses the pointer, for the tooltip saying why.
    let response = if icons::has(&item.icon) {
        let button = widgets::icon_button(&item.icon)
            .side(side)
            .icon_size(icon)
            .on(checked)
            .enabled(enabled);
        ui.add(button)
    } else {
        ui.add_enabled(enabled, widgets::button(item.title.clone()).on(checked))
    };
    let response = match &item.run {
        Ok(run) => {
            let sentence = (!item.preset).then_some(item.description.as_str());
            let response =
                response.tooltip_ui(|ui| tooltip(ui, &item.title, shortcut.as_deref(), sentence));
            if response.clicked() {
                clicked = Some(run.clone());
            }
            response
        }
        Err(reason) => {
            let tip = |ui: &mut Ui| tooltip(ui, &item.title, shortcut.as_deref(), Some(reason));
            if response.enabled() {
                response.tooltip_ui(tip)
            } else {
                response.disabled_tooltip_ui(tip)
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
    (clicked, response)
}

/// How far below the canvas top the Active Bar floats.
pub(crate) const ACTIVE_BAR_TOP: f32 = 12.0;
/// How close the Active Bar comes to the canvas's sides.
const ACTIVE_BAR_INSET: f32 = 8.0;

/// What the bar frame adds to a row's width or height: its inset on both sides.
fn frame_width(t: &Tokens) -> f32 {
    2.0 * widgets::bar_frame_inset(t)
}

/// How far below the canvas top the Active Bar ends, for overlays that sit under it.
pub fn active_bar_bottom(t: &Tokens) -> f32 {
    ACTIVE_BAR_TOP + frame_width(t) + widgets::bar_button_side(t)
}

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
fn row_width(t: &Tokens, items: &[Item]) -> f32 {
    let widths: f32 = items
        .iter()
        .map(|item| match item {
            Item::Separator => widgets::BAR_SEPARATOR,
            _ => widgets::bar_button_side(t),
        })
        .sum();
    widths + widgets::BAR_GAP * items.len().saturating_sub(1) as f32
}

/// What one more button adds to a row: the gap before it and its side.
fn one_more(t: &Tokens) -> f32 {
    widgets::BAR_GAP + widgets::bar_button_side(t)
}

/// How many of the Active Bar's `items` fit on a canvas `width` points wide, at the sizes
/// of `t`. The rest go in a menu behind a "More Tools" button at the end of the row. A
/// separator never ends the row.
pub fn active_bar_fit(t: &Tokens, items: &[Item], width: f32) -> usize {
    let room = width - 2.0 * ACTIVE_BAR_INSET - frame_width(t);
    if row_width(t, items) <= room {
        return items.len();
    }
    let trimmed = |n: usize| {
        let mut n = n;
        while n > 0 && matches!(items[n - 1], Item::Separator) {
            n -= 1;
        }
        n
    };
    (1..items.len())
        .rev()
        .map(trimmed)
        .find(|&n| n > 0 && row_width(t, &items[..n]) + one_more(t) <= room)
        .unwrap_or(0)
}

/// How many of the Active Bar's `items` show on a canvas `width` points wide, when
/// `popups` pop-up buttons (the Selection Filter and the snap palette) follow them; the
/// rest go in the More Tools menu. `None` when the canvas is too narrow for even the More
/// Tools button and the pop-ups, so the bar hides instead of being clipped.
pub fn active_bar_layout(t: &Tokens, items: &[Item], width: f32, popups: usize) -> Option<usize> {
    let tail = popups as f32 * one_more(t);
    let shown = active_bar_fit(t, items, width - tail);
    let minimal = widgets::bar_button_side(t) + tail + 2.0 * ACTIVE_BAR_INSET + frame_width(t);
    (shown > 0 || width >= minimal).then_some(shown)
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
    let t = ui.tokens();
    // Grouped by Preference, where the Document type lists groups.
    let grouped = cx
        .document
        .and_then(|d| super::document_type(cx.shell, d))
        .filter(|_| cx.shell.preferences().display.group_active_bar_tools)
        .filter(|_| !bars.active_bar_groups.is_empty());
    let items = match grouped {
        Some(_) => &bars.active_bar_groups,
        None => &bars.active_bar,
    };
    if items.is_empty() {
        return None;
    }
    let popups = if cx.document.is_some() { 2 } else { 0 };
    let Some(shown) = active_bar_layout(&t, items, canvas.width(), popups) else {
        // Too narrow for even More Tools and the pop-ups: hidden rather than clipped.
        return None;
    };
    let (row, rest) = items.split_at(shown);
    let rest = &rest[rest
        .iter()
        .take_while(|i| matches!(i, Item::Separator))
        .count()..];
    let width = row_width(&t, row)
        + frame_width(&t)
        + (popups + usize::from(!rest.is_empty())) as f32 * one_more(&t)
        - if row.is_empty() {
            widgets::BAR_GAP
        } else {
            0.0
        };
    egui::Area::new(id.with("active-bar"))
        .order(Order::Middle)
        // Placed by its own width, which is known, rather than pivoted on its centre:
        // a pivot uses the last frame's size, so a bar that changed (the Preference
        // grouping it) would sit off centre, and away from its buttons, for a frame.
        .fixed_pos(canvas.center_top() + Vec2::new(-0.5 * width, ACTIVE_BAR_TOP))
        .pivot(Align2::LEFT_TOP)
        .constrain_to(canvas.shrink2(Vec2::new(ACTIVE_BAR_INSET, 0.0)))
        .show(ui.ctx(), |ui| {
            widgets::bar_frame(&t)
                .show(ui, |ui| {
                    widgets::bar_row(ui, |ui| {
                        let mut clicked = match grouped {
                            Some(ty) => grouped_row(ui, cx, ty, row),
                            None => icon_row(ui, cx, row),
                        };
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

/// A grouped Active Bar's row: a group's button for each group, else the item's own.
fn grouped_row(
    ui: &mut Ui,
    cx: &Cx<'_>,
    ty: super::DocumentType,
    items: &[Item],
) -> Option<Invocation> {
    widgets::bar_row(ui, |ui| {
        let mut clicked = None;
        for item in items {
            let run = match item {
                Item::Submenu(group) => super::groups::button(ui, cx, ty, group),
                other => icon_row(ui, cx, std::slice::from_ref(other)),
            };
            clicked = clicked.or(run);
        }
        clicked
    })
    .inner
}

/// The Active Bar's "More Tools" button, whose menu holds the items that did not fit.
fn more_tools(ui: &mut Ui, cx: &Cx<'_>, items: &[Item]) -> Option<Invocation> {
    let title = crate::tr!("active-bar-more");
    let side = widgets::bar_button_side(&ui.tokens());
    let button = widgets::glyph_button(widgets::Glyph::ChevronDown).side(side);
    let response = ui.add(button).tooltip(&title);
    a11y::name(&response, Role::Button, true, &title, None, None);
    let mut clicked = None;
    widgets::menu(&response).show(|ui| {
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
/// `note` shows just above it, in the same frame. Returns the item clicked.
pub fn tool_bar(
    ui: &Ui,
    cx: &Cx<'_>,
    items: &[Item],
    id: Id,
    anchor: egui::Pos2,
    note: Option<&str>,
    state: &dyn Fn(&Action) -> ToolBarItem,
) -> Option<Invocation> {
    let t = ui.tokens();
    let side = widgets::bar_button_side(&t);
    // Not egui's fade-in: the bar fades and travels 4 px as `motion.canvas_bar` says
    // (below), coming and going with its Tool or the selection.
    let shown = egui::Area::new(id.with("tool-bar"))
        .fade_in(false)
        .order(Order::Middle)
        .fixed_pos(anchor)
        .pivot(Align2::CENTER_BOTTOM)
        .show(ui.ctx(), |ui| {
            widgets::bar_frame(&t)
                .show(ui, |ui| {
                    widgets::bar_row(ui, |ui| {
                        let mut clicked = None;
                        for item in items {
                            let action = match item {
                                Item::Separator => {
                                    widgets::bar_separator(ui);
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
                                    let icon = t.icon.bar;
                                    icon_button_toggled(ui, cx, &action, side, icon, look.toggled)
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
    let motion = t.motion;
    let look = crate::theme::motion::Look::travel(Vec2::new(0.0, motion.travel));
    crate::theme::motion::layer(ui.ctx(), shown.response.layer_id, look, motion.canvas_bar);
    if let Some(note) = note {
        let top = shown.response.rect.center_top() - Vec2::new(0.0, widgets::BAR_GAP);
        egui::Area::new(id.with("tool-bar-note"))
            .order(Order::Middle)
            .fixed_pos(top)
            .pivot(Align2::CENTER_BOTTOM)
            .interactable(false)
            .show(ui.ctx(), |ui| {
                widgets::bar_frame(&t).show(ui, |ui| {
                    let text = t.ty.body.rich(note).color(t.color.text.primary);
                    let response = ui.label(text);
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
    let response = ui
        .add_enabled(item.run.is_ok(), widgets::button(text).flat())
        .tooltip_ui(|ui| {
            let sentence = Some(item.description.as_str());
            tooltip(ui, &item.title, shortcut.as_deref(), sentence);
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
