//! The dropdown (the spec's combo box) and the filterable dropdown
//! (docs/spec/v0.1/design-language.md#fields).
//!
//! Closed, a dropdown is filled like a button, with a chevron Glyph at its trailing end.
//! Its list is a menu (`menu.rs`): the same popup and the same rows, with a tick in the
//! leading column of the chosen row. Up and Down move through the rows.
//!
//! The filterable dropdown is for long lists (the installed fonts): its popup starts
//! with a search field that has the keyboard, and typing narrows the rows.

use egui::{
    Align2, IdSalt, InnerResponse, Key, Popup, Response, Role, ScrollArea, Sense, Ui, WidgetInfo,
    pos2, vec2,
};

use super::glyph::{self, Glyph};
use super::paint::{self, State};
use crate::theme::{Tokens, TokensExt as _};

/// A dropdown's least width when it is given none.
pub const DROPDOWN_WIDTH: f32 = 100.0;
/// The height after which a dropdown's list scrolls.
pub const LIST_HEIGHT: f32 = 200.0;
/// Between a dropdown's text and its chevron, and a row's tick column and its text.
const PART_GAP: f32 = 6.0;
/// Between the search field of a filterable dropdown and its rows.
const FILTER_GAP: f32 = 4.0;

/// A dropdown showing `selected_text`. [`Dropdown::show`] draws it and, while it is
/// open, its list: add the rows there with [`choice`] or [`choice_value`].
pub fn dropdown(id_salt: impl egui::AsIdSalt, selected_text: impl Into<String>) -> Dropdown {
    Dropdown {
        id_salt: IdSalt::new(id_salt),
        text: selected_text.into(),
        width: None,
        max_width: None,
        state: State::Live,
    }
}

#[must_use = "call .show()"]
pub struct Dropdown {
    id_salt: IdSalt,
    text: String,
    width: Option<f32>,
    max_width: Option<f32>,
    state: State,
}

/// The closed control: what [`Dropdown`] and [`FilterDropdown`] share.
fn closed(
    ui: &mut Ui,
    t: &Tokens,
    state: State,
    text: &str,
    (min_width, max_width): (Option<f32>, Option<f32>),
    open: bool,
) -> Response {
    paint::stated(ui, state, |ui| {
        let c = &t.color;
        let needed =
            paint::width(ui, &t.ty.body, text) + PART_GAP + glyph::BOX + 2.0 * t.space.control_x;
        let width = min_width.unwrap_or(DROPDOWN_WIDTH).max(needed);
        // A dropdown held to a width cuts its text with an ellipsis.
        let width = max_width.map_or(width, |max| width.min(max));
        let size = vec2(width, t.control_height());
        let (rect, response) = ui.allocate_exact_size(size, Sense::click());
        let enabled = ui.is_enabled();
        response.widget_info(|| {
            let mut info = WidgetInfo::new(Role::ComboBox);
            info.enabled = enabled;
            info.current_text_value = Some(text.to_owned());
            info
        });
        if !ui.is_rect_visible(rect) {
            return response;
        }
        let mut s = paint::seen(&response, state);
        // An open dropdown stays lit while its list is up.
        s.hover |= open;
        let fill = paint::state_fill(t, c.fill.control, s);
        ui.painter().rect_filled(rect, t.radius.control, fill);
        let inner = rect.shrink2(vec2(t.space.control_x, 0.0));
        let label = inner.with_max_x(inner.right() - glyph::BOX - PART_GAP);
        paint::text_clipped(ui, label, &t.ty.body, text, c.text.primary);
        let at = pos2(inner.right() - glyph::BOX / 2.0, rect.center().y);
        Glyph::ChevronDown.paint(ui.painter(), at, c.text.muted);
        if s.focus {
            paint::focus_ring(ui, t, rect, t.radius.control);
        }
        response
    })
}

impl Dropdown {
    /// The least width; the dropdown grows to fit its text.
    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

    /// The most the dropdown grows to: a longer text is cut with an ellipsis. For a
    /// dropdown in a column that must fit a Panel.
    pub fn max_width(mut self, width: f32) -> Self {
        self.max_width = Some(width);
        self
    }

    pub fn forced(mut self, state: State) -> Self {
        self.state = state;
        self
    }

    /// Draws the dropdown, and its list with `rows` while it is open. The inner value is
    /// `None` while it is closed.
    pub fn show<R>(self, ui: &mut Ui, rows: impl FnOnce(&mut Ui) -> R) -> InnerResponse<Option<R>> {
        let t = ui.tokens();
        let popup_id = ui.make_persistent_id(self.id_salt).with("popup");
        let open = Popup::is_id_open(ui.ctx(), popup_id);
        let widths = (self.width, self.max_width);
        let response = closed(ui, &t, self.state, &self.text, widths, open);
        let height = LIST_HEIGHT;
        let inner = super::menu(&response)
            .id(popup_id)
            .width(response.rect.width())
            .show(|ui| {
                ScrollArea::vertical()
                    .max_height(height)
                    .show(ui, rows)
                    .inner
            })
            .map(|shown| shown.inner);
        if inner.is_some() {
            ui.ctx().accesskit_node_builder(popup_id, |node| {
                node.set_role(Role::ListBox);
            });
        }
        InnerResponse { inner, response }
    }
}

/// One row of a dropdown's list: a menu item. `selected` shows the tick.
pub fn choice(selected: bool, text: impl Into<String>) -> ChoiceRow {
    super::menu_item(text).checked(selected)
}

/// A dropdown's row is a menu's item.
pub type ChoiceRow = super::MenuItem<'static>;

/// A row that sets `current` to `value` when clicked, and shows the tick when they are
/// equal. The response is marked changed when the click changed `current`.
pub fn choice_value<T: PartialEq>(
    ui: &mut Ui,
    current: &mut T,
    value: T,
    text: impl Into<String>,
) -> Response {
    let mut response = ui.add(choice(*current == value, text));
    if response.clicked() && *current != value {
        *current = value;
        response.mark_changed();
    }
    response
}

/// One entry of a filterable dropdown.
#[derive(Clone, Copy, Debug, Default)]
pub struct Choice<'a> {
    pub text: &'a str,
    /// Muted text at the trailing end, such as "not installed".
    pub note: &'a str,
    /// A rule under this entry, shown while nothing is typed: it sets the default apart
    /// from the list.
    pub rule_below: bool,
}

impl<'a> Choice<'a> {
    pub fn new(text: &'a str) -> Self {
        Choice {
            text,
            ..Choice::default()
        }
    }

    pub fn note(mut self, note: &'a str) -> Self {
        self.note = note;
        self
    }

    pub fn rule_below(mut self) -> Self {
        self.rule_below = true;
        self
    }
}

/// What [`FilterDropdown::show`] returns.
pub struct Picked {
    /// The closed control.
    pub response: Response,
    /// The index of the entry picked this frame.
    pub index: Option<usize>,
}

/// A dropdown whose list can be narrowed by typing. `selected_text` is what it shows
/// closed.
pub fn filter_dropdown(
    id_salt: impl egui::AsIdSalt,
    selected_text: impl Into<String>,
) -> FilterDropdown {
    FilterDropdown {
        id_salt: IdSalt::new(id_salt),
        text: selected_text.into(),
        width: None,
        hint: String::new(),
        none: String::new(),
        state: State::Live,
    }
}

#[must_use = "call .show()"]
pub struct FilterDropdown {
    id_salt: IdSalt,
    text: String,
    width: Option<f32>,
    hint: String,
    none: String,
    state: State,
}

/// What a filterable dropdown remembers while it is open.
#[derive(Clone, Default)]
struct Filtering {
    typed: String,
    /// The lit row, as an index into the rows that match.
    lit: usize,
    /// The search field was given the keyboard.
    focused: bool,
}

impl FilterDropdown {
    /// The least width; the dropdown grows to fit its text.
    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

    /// The search field's hint, such as "Filter".
    pub fn hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = hint.into();
        self
    }

    /// What the list says when nothing matches.
    pub fn none_text(mut self, none: impl Into<String>) -> Self {
        self.none = none.into();
        self
    }

    pub fn forced(mut self, state: State) -> Self {
        self.state = state;
        self
    }

    /// Draws the dropdown and, while it is open, the search field and the entries that
    /// match what is typed. `selected` is the chosen entry. Up and Down move through the
    /// rows, Enter picks the lit one and Escape closes the list.
    pub fn show(self, ui: &mut Ui, choices: &[Choice<'_>], selected: Option<usize>) -> Picked {
        let t = ui.tokens();
        let id = ui.make_persistent_id(self.id_salt);
        let popup_id = id.with("popup");
        let was_open = Popup::is_id_open(ui.ctx(), popup_id);
        let response = closed(ui, &t, self.state, &self.text, (self.width, None), was_open);
        let height = LIST_HEIGHT;
        let mut filtering: Filtering = if was_open {
            ui.data(|d| d.get_temp(id)).unwrap_or_default()
        } else {
            Filtering::default()
        };
        let mut picked = None;
        let shown = super::menu(&response)
            .id(popup_id)
            .width(response.rect.width())
            .stay_open()
            .show(|ui| {
                let before = filtering.typed.clone();
                let field = ui.add(
                    super::text_field(&mut filtering.typed)
                        .id(id.with("filter"))
                        .search()
                        .hint(self.hint.as_str())
                        .width(f32::INFINITY),
                );
                let hint = self.hint.clone();
                field.widget_info(|| WidgetInfo::labeled(Role::SearchInput, true, &hint));
                if !filtering.focused {
                    field.request_focus();
                    filtering.focused = true;
                }
                if filtering.typed != before {
                    filtering.lit = 0;
                }
                ui.add_space(FILTER_GAP);
                let needle = filtering.typed.trim().to_lowercase();
                let matches: Vec<usize> = (0..choices.len())
                    .filter(|i| choices[*i].text.to_lowercase().contains(&needle))
                    .collect();
                let (down, up, enter) = ui.input(|i| {
                    (
                        i.key_pressed(Key::ArrowDown),
                        i.key_pressed(Key::ArrowUp),
                        i.key_pressed(Key::Enter),
                    )
                });
                let last = matches.len().saturating_sub(1);
                if down {
                    filtering.lit = (filtering.lit + 1).min(last);
                }
                if up {
                    filtering.lit = filtering.lit.saturating_sub(1);
                }
                filtering.lit = filtering.lit.min(last);
                if enter && let Some(index) = matches.get(filtering.lit) {
                    picked = Some(*index);
                }
                ScrollArea::vertical().max_height(height).show(ui, |ui| {
                    if matches.is_empty() {
                        let size = vec2(ui.available_width(), t.row_height());
                        let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
                        let at = rect.shrink2(vec2(t.space.row_x, 0.0));
                        let ink = t.color.text.muted;
                        paint::text_in(ui, at, Align2::LEFT_CENTER, &t.ty.body, &self.none, ink);
                    }
                    for (row, index) in matches.iter().enumerate() {
                        let entry = &choices[*index];
                        let mut line =
                            choice(selected == Some(*index), entry.text).note(entry.note);
                        line.lit = row == filtering.lit;
                        let line = ui.add(line);
                        if (down || up) && row == filtering.lit {
                            line.scroll_to_me(None);
                        }
                        if line.clicked() {
                            picked = Some(*index);
                        }
                        if entry.rule_below && needle.is_empty() && row < last {
                            super::menu_separator(ui);
                        }
                    }
                });
            });
        if shown.is_some() {
            ui.ctx().accesskit_node_builder(popup_id, |node| {
                node.set_role(Role::ListBox);
            });
            ui.data_mut(|d| d.insert_temp(id, filtering));
        } else if was_open {
            ui.data_mut(|d| d.remove::<Filtering>(id));
        }
        if picked.is_some() {
            Popup::close_id(ui.ctx(), popup_id);
            ui.data_mut(|d| d.remove::<Filtering>(id));
            response.request_focus();
        }
        Picked {
            response,
            index: picked,
        }
    }
}
