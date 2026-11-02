//! Draws bars from their data: the menu bar, the toolbar and the Active Bar. Drawing never
//! runs anything; each function returns the `(id, params)` of a clicked item, which the
//! window runs through the Shell.

use std::collections::BTreeMap;

use egui::{Align2, Button, Id, Order, Rect, Role, Ui, Vec2, WidgetInfo};
use serde_json::Value;

use reda_engine::model::Uuid;

use super::{Action, Bars, Item, Menu, Resolved, resolve_for};
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

/// The menu bar of `bars`.
pub fn menu_bar(ui: &mut Ui, cx: &Cx<'_>, bars: &Bars) -> Option<Invocation> {
    let mut clicked = None;
    egui::MenuBar::new().ui(ui, |ui| {
        for menu in &bars.menus {
            ui.menu_button(i18n::translate(&menu.title, &[]), |ui| {
                menu_items(ui, cx, &menu.items, &mut clicked);
            });
        }
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

/// A row of icon buttons, as on the toolbar and the Active Bar.
pub fn icon_row(ui: &mut Ui, cx: &Cx<'_>, items: &[Item]) -> Option<Invocation> {
    let theme = ui.ctx().theme();
    let mut clicked = None;
    for item in items {
        let action = match item {
            Item::Separator => {
                ui.separator();
                continue;
            }
            // A submenu on a bar opens as a menu under its button.
            Item::Submenu(menu) => {
                let mut inner = None;
                ui.menu_button(i18n::translate(&menu.title, &[]), |ui| {
                    menu_items(ui, cx, &menu.items, &mut inner);
                });
                clicked = clicked.or(inner);
                continue;
            }
            Item::Action(action) => action,
        };
        let item = resolve_for(cx.shell, action, cx.document);
        let enabled = item.run.is_ok();
        // The tooltip names the shortcut: "Undo (Ctrl+Z)".
        let heading = match cx.shortcut(ui, action) {
            Some(shortcut) => format!("{} ({shortcut})", item.title),
            None => item.title.clone(),
        };
        let button = match icons::image(&item.icon, icons::TOOLBAR, theme, enabled) {
            Some(image) => Button::new(image),
            None => Button::new(item.title.clone()),
        };
        let response = ui.add_enabled(
            enabled,
            button
                .selected(cx.is_checked(&item))
                .frame_when_inactive(false),
        );
        let name = item.title.clone();
        response.widget_info(|| WidgetInfo::labeled(Role::Button, enabled, name.clone()));
        match &item.run {
            Ok(run) => {
                let response = response.on_hover_ui(|ui| {
                    ui.strong(&heading);
                    ui.label(&item.description);
                });
                if response.clicked() {
                    clicked = Some(run.clone());
                }
            }
            Err(reason) => {
                response.on_disabled_hover_ui(|ui| {
                    ui.strong(&heading);
                    ui.label(reason);
                });
            }
        }
    }
    clicked
}

/// The toolbar under the menu bar.
pub fn toolbar(ui: &mut Ui, cx: &Cx<'_>, bars: &Bars) -> Option<Invocation> {
    ui.horizontal(|ui| icon_row(ui, cx, &bars.toolbar)).inner
}

/// The Active Bar of the canvas `id`, floating at the top centre of its rect `canvas`.
pub fn active_bar(
    ui: &mut Ui,
    cx: &Cx<'_>,
    bars: &Bars,
    id: Id,
    canvas: Rect,
) -> Option<Invocation> {
    if bars.active_bar.is_empty() {
        return None;
    }
    egui::Area::new(id.with("active-bar"))
        .order(Order::Middle)
        .fixed_pos(canvas.center_top() + Vec2::new(0.0, 8.0))
        .pivot(Align2::CENTER_TOP)
        .constrain_to(canvas)
        .show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style())
                .inner_margin(egui::Margin::symmetric(4, 2))
                .show(ui, |ui| {
                    ui.horizontal(|ui| icon_row(ui, cx, &bars.active_bar)).inner
                })
                .inner
        })
        .inner
}
