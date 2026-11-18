//! The tab switcher: the popup Ctrl+Tab opens, listing the open tabs most recently used
//! first with the pick selected. It stays open while Ctrl is held; Ctrl+Tab and
//! Ctrl+Shift+Tab move the pick (`layout.tab.switch`), releasing Ctrl or pressing Enter
//! switches, Escape closes it without switching.

use egui::{Align2, Context, Id, Key, Modifiers, Order, Role, UiBuilder, Vec2};

use crate::a11y;
use crate::docking::{tab_icon_key, tab_title};
use crate::shell::Shell;
use crate::theme::TokensExt as _;
use crate::tr;
use crate::widgets;

/// The width of the switcher's rows.
const WIDTH: f32 = 240.0;
/// Before the Keymap sees this frame's keys: Escape and Enter go to an open switcher.
pub fn keys(ctx: &Context, shell: &mut Shell) {
    if shell.tab_switch().is_none() {
        return;
    }
    let (escape, enter) = ctx.input_mut(|i| {
        let escape = i.consume_key(Modifiers::NONE, Key::Escape)
            || i.consume_key(Modifiers::CTRL, Key::Escape)
            || i.consume_key(Modifiers::CTRL | Modifiers::SHIFT, Key::Escape);
        let enter = i.consume_key(Modifiers::NONE, Key::Enter)
            || i.consume_key(Modifiers::CTRL, Key::Enter);
        (escape, enter)
    });
    if escape {
        shell.end_tab_switch(false);
    } else if enter {
        shell.end_tab_switch(true);
    }
}

/// After the Keymap ran this frame's keys: an open switcher switches once Ctrl is up.
pub fn release(ctx: &Context, shell: &mut Shell) {
    if shell.tab_switch().is_some() && !ctx.input(|i| i.modifiers.ctrl) {
        shell.end_tab_switch(true);
    }
}

/// Draws the switcher while it is open; a click on a tab switches to it.
pub fn show(ctx: &Context, shell: &mut Shell) {
    let Some(switch) = shell.tab_switch().cloned() else {
        return;
    };
    let t = ctx.tokens();
    let title = tr!("tab-switcher-title");
    let mut clicked = None;
    // A popup: the floating surface with the popup radius and shadow. Its rows are list
    // rows with the tab's Icon, and the pick is the live selection.
    egui::Area::new(Id::NULL.with("tab-switcher"))
        .order(Order::Foreground)
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .show(ctx, |ui| {
            widgets::menu_frame(&t).show(ui, |ui| {
                ui.set_width(WIDTH);
                egui::Frame::new()
                    .inner_margin(egui::Margin::symmetric(
                        t.space.row_x as i8,
                        t.space.row_y as i8,
                    ))
                    .show(ui, |ui| {
                        ui.label(t.ty.small.rich(&title).color(t.color.text.muted));
                    });
                let list = UiBuilder::new()
                    .accessibility_role(Role::List)
                    .accessibility_label(title.clone());
                ui.scope_builder(list, |ui| {
                    widgets::rows(ui, |ui| {
                        for (i, tab) in switch.tabs.iter().enumerate() {
                            let name = tab_title(shell, tab);
                            let picked = i == switch.index;
                            let mut row = widgets::list_row(name.as_str()).selected(picked);
                            if let Some(key) = tab_icon_key(shell, tab) {
                                row = row.icon(key);
                            }
                            let response = ui.add(row);
                            a11y::name(&response, Role::ListItem, true, &name, Some(picked), None);
                            if picked {
                                response.scroll_to_me(None);
                            }
                            if response.clicked() {
                                clicked = Some(i);
                            }
                        }
                    });
                });
            });
        });
    if let Some(i) = clicked {
        shell.pick_tab_switch(i);
        shell.end_tab_switch(true);
    }
}
