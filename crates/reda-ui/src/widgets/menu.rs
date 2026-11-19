//! Menus and context menus (docs/spec/v0.1/design-language.md#menus-and-context-menus).
//!
//! A menu is a popup on `surface.floating` with the popup radius and shadow and the
//! `menu` padding around its items. An item is a row with the row radius: an Icon column
//! at the leading end (a checked item shows a tick there), the label, then a shortcut in
//! muted text or a muted chevron for a submenu. Separators are hairlines inset by `row_x`.
//!
//! ```ignore
//! widgets::menu_bar(ui, |ui| {
//!     widgets::menu_button("File").show(ui, |ui| {
//!         if ui.add(widgets::menu_item("Save").icon("save").shortcut("Ctrl+S")).clicked() { .. }
//!         widgets::menu_separator(ui);
//!         widgets::menu_button("Export").show(ui, |ui| { .. });   // a submenu
//!     });
//! });
//! widgets::context_menu(&response, |ui| { .. });
//! widgets::menu(&button_response).show(|ui| { .. });              // a menu under any widget
//! ```
//!
//! The popups are egui's, so they place themselves, close on a click or Escape, and take
//! the arrow keys: Up and Down move through the items, Right opens a submenu and Left
//! leaves it. A dropdown's list is the same popup and the same rows (`dropdown.rs`).

use egui::containers::menu::{MenuConfig, MenuState, SubMenu, find_menu_root, is_in_menu};
use egui::{
    Align2, Color32, Frame, Id, InnerResponse, Popup, PopupCloseBehavior, Rect, Response, Role,
    Sense, SetOpenCommand, Stroke, StrokeKind, Ui, Vec2, Widget, WidgetInfo, pos2, vec2,
};

use super::glyph::{self, Glyph};
use super::paint::{self, State};
use crate::theme::motion::{self, Look};
use crate::theme::{Tokens, TokensExt as _};

/// Between an item's Icon column and its label, and a menu button's text and chevron.
const PART_GAP: f32 = 6.0;
/// The height a separator takes.
const SEPARATOR_HEIGHT: f32 = 9.0;
/// Between the titles of a menu bar.
const BAR_GAP: f32 = 2.0;

/// The popup a menu or a list is shown in: `surface.floating`, a hairline rim, the popup
/// radius and shadow, with the `menu` padding around the rows.
pub fn menu_frame(t: &Tokens) -> Frame {
    Frame::new()
        .fill(t.color.surface.floating)
        .stroke(Stroke::new(t.stroke.hairline, t.color.stroke.hairline))
        .corner_radius(t.radius.popup)
        .shadow(t.elevation.popup)
        .inner_margin(t.space.menu)
}

/// What a menu changes of the style inside it: rows touch, and a stock button that is
/// still put in a menu is flat.
fn inside(style: &mut egui::Style) {
    egui::containers::menu::menu_style(style);
    style.spacing.item_spacing.y = 0.0;
}

/// A menu under `anchor`, opened by clicking it. Add the items in [`Menu::show`].
pub fn menu(anchor: &Response) -> Menu<'_> {
    Menu {
        popup: Popup::menu(anchor),
        fill: false,
        anchor: Some(anchor.rect),
    }
}

/// A menu hung below `anchor` that a click does not open: open it with [`Menu::open`],
/// as a key does. It closes like any menu.
pub fn menu_below(anchor: &Response) -> Menu<'_> {
    Menu {
        popup: Popup::from_response(anchor)
            .kind(egui::PopupKind::Menu)
            .layout(egui::Layout::top_down_justified(egui::Align::Min)),
        fill: false,
        anchor: Some(anchor.rect),
    }
}

/// The context menu of `response`: opened by a secondary click on it, at the pointer.
pub fn context_menu<R>(
    response: &Response,
    add: impl FnOnce(&mut Ui) -> R,
) -> Option<InnerResponse<R>> {
    Menu {
        popup: Popup::context_menu(response),
        fill: false,
        anchor: None,
    }
    .show(add)
}

#[must_use = "call .show()"]
pub struct Menu<'a> {
    popup: Popup<'a>,
    /// The rows fill a width the caller gave (a dropdown's list is as wide as its
    /// control); otherwise the menu is as wide as its widest item.
    fill: bool,
    /// What it hangs from, which it travels away from as it opens; `None` for a context
    /// menu, which opens down from the pointer.
    anchor: Option<Rect>,
}

impl Menu<'_> {
    /// The popup's id, instead of one made from the anchor's.
    pub fn id(mut self, id: Id) -> Self {
        self.popup = self.popup.id(id);
        self
    }

    /// Opens at the pointer position when the menu was requested by a key.
    pub fn at_position(mut self, position: egui::Pos2) -> Self {
        self.popup = self.popup.at_position(position).open_memory(None);
        self.anchor = None;
        self
    }

    /// The popup's width; it grows to fit its items.
    pub fn width(mut self, width: f32) -> Self {
        self.popup = self.popup.width(width);
        self.fill = true;
        self
    }

    /// Keeps the menu open when something in it is clicked: a menu of toggles, or one
    /// that holds a field. A click outside, or Escape, still closes it.
    pub fn stay_open(mut self) -> Self {
        self.popup = self
            .popup
            .close_behavior(PopupCloseBehavior::CloseOnClickOutside);
        self
    }

    /// Opens the menu (or keeps it open) whatever was clicked.
    pub fn open(mut self) -> Self {
        self.popup = self.popup.open_memory(Some(SetOpenCommand::Bool(true)));
        self
    }

    /// Opens the menu when it is closed and closes it when it is open, when `toggle`: for
    /// a menu that a key opens as well as a click on its anchor.
    pub fn toggle(mut self, toggle: bool) -> Self {
        if toggle {
            self.popup = self.popup.open_memory(Some(SetOpenCommand::Toggle));
        }
        self
    }

    /// The popup's accessible name.
    pub fn named(mut self, name: impl Into<String>) -> Self {
        self.popup = self.popup.accessible_name(name.into());
        self
    }

    /// The id of the popup, to ask [`menu_is_open`] about.
    pub fn popup_id(&self) -> Id {
        self.popup.get_id()
    }

    /// Draws the menu while it is open; `None` while it is closed.
    pub fn show<R>(self, add: impl FnOnce(&mut Ui) -> R) -> Option<InnerResponse<R>> {
        let ctx = self.popup.ctx().clone();
        let t = ctx.tokens();
        let fill = self.fill;
        let anchor = self.anchor;
        let shown = self.popup.frame(menu_frame(&t)).style(inside).show(|ui| {
            if fill {
                ui.set_min_width(ui.available_width());
            }
            add(ui)
        });
        if let Some(shown) = &shown {
            arrive(&ctx, &shown.response, anchor);
        }
        shown
    }
}

/// A menu's arrival: `travel` pixels from its anchor, towards where it opened, as egui
/// fades it in (docs/spec/v0.1/design-language.md#motion-values). It leaves at once.
fn arrive(ctx: &egui::Context, popup: &Response, anchor: Option<Rect>) {
    let t = ctx.tokens();
    let d = t.motion.travel;
    let rect = popup.rect;
    let offset = match anchor {
        Some(a) if rect.bottom() <= a.top() + 1.0 => vec2(0.0, d),
        Some(a) if rect.left() >= a.right() - 1.0 => vec2(-d, 0.0),
        Some(a) if rect.right() <= a.left() + 1.0 => vec2(d, 0.0),
        _ => vec2(0.0, -d),
    };
    let look = Look::travel(offset).without_fade();
    motion::layer(ctx, popup.layer_id, look, t.motion.menu);
}

/// Whether the menu with this popup id is open.
pub fn menu_is_open(ctx: &egui::Context, popup_id: Id) -> bool {
    Popup::is_id_open(ctx, popup_id)
}

/// One item of a menu or a context menu.
pub fn menu_item(text: impl Into<String>) -> MenuItem<'static> {
    MenuItem {
        text: text.into(),
        icon: None,
        checked: None,
        trailing: String::new(),
        keyed: false,
        arrow: false,
        lit: false,
        state: State::Live,
    }
}

#[must_use = "add it with ui.add"]
pub struct MenuItem<'a> {
    text: String,
    icon: Option<&'a str>,
    checked: Option<bool>,
    trailing: String,
    /// The trailing text is a shortcut, which is read out after the label.
    keyed: bool,
    arrow: bool,
    pub(super) lit: bool,
    state: State,
}

impl<'a> MenuItem<'a> {
    /// The Icon with this key in the leading column. A key that names no Icon shows none.
    pub fn icon(self, key: &str) -> MenuItem<'_> {
        MenuItem {
            icon: Some(key),
            text: self.text,
            checked: self.checked,
            trailing: self.trailing,
            keyed: self.keyed,
            arrow: self.arrow,
            lit: self.lit,
            state: self.state,
        }
    }

    /// An item that is a choice in force or a toggle: checked, it shows a tick in the
    /// leading column in place of an Icon.
    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = Some(checked);
        self
    }

    /// The item's key, in muted text at the trailing end. A screen reader reads it after
    /// the label ("Save All Ctrl+S"), which also tells the item from a button of the
    /// same name on a bar.
    pub fn shortcut(mut self, keys: impl Into<String>) -> Self {
        self.trailing = keys.into();
        self.keyed = true;
        self
    }

    /// Any muted text at the trailing end, such as "not installed". It is not part of
    /// the item's name: a screen reader gets it as the item's description.
    pub fn note(mut self, note: impl Into<String>) -> Self {
        self.trailing = note.into();
        self.keyed = false;
        self
    }

    /// The muted chevron of an item that opens a submenu. [`menu_button`] inside a menu
    /// draws one of these and opens the submenu.
    pub fn submenu_arrow(mut self) -> Self {
        self.arrow = true;
        self
    }

    pub fn forced(mut self, state: State) -> Self {
        self.state = state;
        self
    }
}

impl Widget for MenuItem<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let t = ui.tokens();
        paint::stated(ui, self.state, |ui| {
            let c = &t.color;
            let lead = t.icon.small + PART_GAP;
            let trail = if self.arrow {
                t.space.gap + glyph::BOX
            } else if self.trailing.is_empty() {
                0.0
            } else {
                // A shortcut stands clear of the longest label by twice the gap.
                2.0 * t.space.gap + paint::width(ui, &t.ty.body, &self.trailing)
            };
            let needed =
                2.0 * t.space.row_x + lead + paint::width(ui, &t.ty.body, &self.text) + trail;
            let size = vec2(row_width(ui, needed), t.row_height());
            let (rect, response) = ui.allocate_exact_size(size, Sense::click());
            let enabled = ui.is_enabled();
            response.widget_info(|| {
                let name = if self.keyed && !self.trailing.is_empty() {
                    format!("{} {}", self.text, self.trailing)
                } else {
                    self.text.clone()
                };
                match self.checked {
                    Some(checked) => WidgetInfo::selected(Role::Button, enabled, checked, name),
                    None => WidgetInfo::labeled(Role::Button, enabled, name),
                }
            });
            // A note is not the item's name, but it is read out as its description.
            if !self.keyed && !self.trailing.is_empty() {
                ui.ctx().accesskit_node_builder(response.id, |node| {
                    node.set_description(self.trailing.clone());
                });
            }
            // A pointer click closes the menu from the popup; so does Enter on an item,
            // unless the menu stays open on clicks.
            if response.clicked()
                && !self.arrow
                && !response.clicked_by(egui::PointerButton::Primary)
                && is_in_menu(ui)
                && MenuConfig::find(ui).close_behavior == PopupCloseBehavior::CloseOnClick
            {
                ui.close();
            }
            if !ui.is_rect_visible(rect) {
                return response;
            }
            let mut s = paint::seen(&response, self.state);
            s.hover |= self.lit;
            let fill = paint::state_fill(&t, Color32::TRANSPARENT, s);
            ui.painter().rect_filled(rect, t.radius.row, fill);
            let inner = rect.shrink2(vec2(t.space.row_x, 0.0));
            let at = pos2(inner.left() + t.icon.small / 2.0, rect.center().y);
            if self.checked == Some(true) {
                Glyph::Tick.paint(ui.painter(), at, c.text.primary);
            } else if let Some(key) = self.icon
                && let Some(image) =
                    crate::icons::image(key, t.icon.small, ui.ctx().theme(), enabled)
            {
                image.paint_at(ui, Rect::from_center_size(at, Vec2::splat(t.icon.small)));
            }
            let label = inner.with_min_x(inner.left() + lead);
            let ink = c.text.primary;
            paint::text_in(ui, label, Align2::LEFT_CENTER, &t.ty.body, &self.text, ink);
            if self.arrow {
                let at = pos2(inner.right() - glyph::BOX / 2.0, rect.center().y);
                Glyph::ChevronRight.paint(ui.painter(), at, c.text.muted);
            } else if !self.trailing.is_empty() {
                let (style, text, ink) = (&t.ty.body, &self.trailing, c.text.muted);
                paint::text_in(ui, label, Align2::RIGHT_CENTER, style, text, ink);
            }
            if s.focus {
                // Inside the row, so the popup's edge does not clip it.
                ui.painter().rect_stroke(
                    rect,
                    t.radius.row,
                    Stroke::new(t.stroke.focus_ring, c.stroke.focus_ring),
                    StrokeKind::Inside,
                );
            }
            response
        })
    }
}

/// The width a row takes: all there is, or what it needs while its popup is measured.
fn row_width(ui: &Ui, needed: f32) -> f32 {
    if ui.is_sizing_pass() {
        needed
    } else {
        ui.available_width().max(needed)
    }
}

/// A hairline between items, inset by `row_x`.
pub fn menu_separator(ui: &mut Ui) {
    let t = ui.tokens();
    let size = vec2(row_width(ui, 0.0), SEPARATOR_HEIGHT);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    ui.painter().hline(
        rect.shrink2(vec2(t.space.row_x, 0.0)).x_range(),
        rect.center().y,
        Stroke::new(t.stroke.hairline, t.color.stroke.hairline),
    );
}

/// A row of menu buttons: the main menu. Hovering another title while one menu is open
/// switches to that menu.
pub fn menu_bar<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> InnerResponse<R> {
    egui::MenuBar::new().ui(ui, |ui| {
        ui.spacing_mut().item_spacing.x = BAR_GAP;
        add(ui)
    })
}

/// The width a [`menu_bar`] of these titles takes: each title's text with the control
/// padding at both sides, the titles `BAR_GAP` apart.
pub fn menu_titles_width<'a>(ui: &Ui, titles: impl IntoIterator<Item = &'a str>) -> f32 {
    let t = ui.tokens();
    let widths: Vec<f32> = titles
        .into_iter()
        .map(|title| paint::width(ui, &t.ty.body, title) + 2.0 * t.space.control_x)
        .collect();
    widths.iter().sum::<f32>() + BAR_GAP * widths.len().saturating_sub(1) as f32
}

/// The id under which a menu bar remembers its open menu, when `ui` is directly in one.
fn bar_memory(ui: &Ui) -> Option<Id> {
    if is_in_menu(ui) {
        return None;
    }
    let root = find_menu_root(ui);
    root.info
        .tags
        .contains(MenuConfig::MENU_CONFIG_TAG)
        .then(|| root.unique_id.with("reda-open-menu"))
}

/// A button that opens a menu. In a [`menu_bar`] it is a flat title; inside a menu it is
/// an item with a chevron that opens a submenu; anywhere else it is a filled button with
/// a chevron.
pub fn menu_button(text: impl Into<String>) -> MenuButton<'static> {
    MenuButton {
        text: text.into(),
        icon: None,
        muted: false,
        state: State::Live,
    }
}

#[must_use = "call .show()"]
pub struct MenuButton<'a> {
    text: String,
    icon: Option<&'a str>,
    muted: bool,
    state: State,
}

/// Where a menu button is, which decides how it is drawn.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Place {
    Bar,
    Alone,
}

impl<'a> MenuButton<'a> {
    /// The Icon of a submenu's item.
    pub fn icon(self, key: &str) -> MenuButton<'_> {
        MenuButton {
            icon: Some(key),
            text: self.text,
            muted: self.muted,
            state: self.state,
        }
    }

    /// The title in muted text: a Title bar's menus while the window is in the
    /// background.
    pub fn muted(mut self, muted: bool) -> Self {
        self.muted = muted;
        self
    }

    pub fn forced(mut self, state: State) -> Self {
        self.state = state;
        self
    }

    /// Draws the button and, while it is open, its menu with `add`. The inner value is
    /// `None` while the menu is closed.
    pub fn show<R>(self, ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> InnerResponse<Option<R>> {
        if is_in_menu(ui) {
            return self.submenu(ui, add);
        }
        let memory = bar_memory(ui);
        let place = if memory.is_some() {
            Place::Bar
        } else {
            Place::Alone
        };
        // The id the button is about to take, and so its popup's.
        let was_open = Popup::is_id_open(ui.ctx(), ui.next_auto_id().with("popup"));
        let response = self.title(ui, place, was_open);
        let mut menu = menu(&response);
        let popup_id = menu.popup_id();
        // In a bar, hovering another title while a menu is open switches to this one.
        if let Some(memory) = memory
            && response.hovered()
            && !response.clicked()
            && let Some(other) = ui.data(|d| d.get_temp::<Id>(memory))
            && other != popup_id
            && Popup::is_id_open(ui.ctx(), other)
        {
            menu = menu.open();
        }
        let inner = menu.show(add).map(|shown| shown.inner);
        if let Some(memory) = memory
            && inner.is_some()
        {
            ui.data_mut(|d| d.insert_temp(memory, popup_id));
        }
        InnerResponse { inner, response }
    }

    /// The button itself, outside a menu.
    fn title(&self, ui: &mut Ui, place: Place, open: bool) -> Response {
        let t = ui.tokens();
        paint::stated(ui, self.state, |ui| {
            let c = &t.color;
            let chevron = if place == Place::Alone {
                PART_GAP + glyph::BOX
            } else {
                0.0
            };
            let text = paint::width(ui, &t.ty.body, &self.text);
            let size = vec2(text + chevron + 2.0 * t.space.control_x, t.control_height());
            let (rect, response) = ui.allocate_exact_size(size, Sense::click());
            let enabled = ui.is_enabled();
            response.widget_info(|| WidgetInfo::labeled(Role::Button, enabled, &self.text));
            if !ui.is_rect_visible(rect) {
                return response;
            }
            let mut s = paint::seen(&response, self.state);
            // An open menu's button stays lit while the menu is up.
            s.hover |= open;
            let under = match place {
                Place::Alone => c.fill.control,
                Place::Bar => Color32::TRANSPARENT,
            };
            let fill = paint::state_fill(&t, under, s);
            ui.painter().rect_filled(rect, t.radius.control, fill);
            let inner = rect.shrink2(vec2(t.space.control_x, 0.0));
            let ink = if self.muted {
                c.text.muted
            } else {
                c.text.primary
            };
            paint::text_in(ui, inner, Align2::LEFT_CENTER, &t.ty.body, &self.text, ink);
            if place == Place::Alone {
                let at = pos2(inner.right() - glyph::BOX / 2.0, rect.center().y);
                Glyph::ChevronDown.paint(ui.painter(), at, c.text.muted);
            }
            if s.focus {
                paint::focus_ring(ui, &t, rect, t.radius.control);
            }
            response
        })
    }

    /// Inside a menu: an item with a chevron, and its submenu beside it.
    fn submenu<R>(self, ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> InnerResponse<Option<R>> {
        let id = ui.next_auto_id();
        let open = MenuState::from_ui(ui, |state, _| {
            state.open_item == Some(SubMenu::id_from_widget_id(id))
        });
        let mut item = menu_item(self.text).submenu_arrow().forced(self.state);
        item.lit = open;
        let response = match self.icon {
            Some(key) => ui.add(item.icon(key)),
            None => ui.add(item),
        };
        // The submenu takes its parent's config: its style and what a click closes.
        let inner = SubMenu::new().show(ui, &response, add).map(|shown| {
            arrive(ui.ctx(), &shown.response, Some(response.rect));
            shown.inner
        });
        InnerResponse { inner, response }
    }
}
