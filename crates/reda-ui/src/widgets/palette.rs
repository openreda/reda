//! The Command Palette's frame and rows
//! (docs/spec/v0.1/design-language.md#command-palette).
//!
//! The Palette is a window dropped from the command field: [`palette_frame`]. A row is a menu row: the row radius and padding, an Icon column, the title in
//! body text, and at the trailing end its notes in muted text (a Command's place in the
//! menus, why it cannot run, "recently used") and its shortcut as a key chip. The row the
//! arrow keys have reached is the live selection (the accent subtle fill); hover lays the
//! neutral fill over it.
//!
//! ```ignore
//! let row = widgets::palette_row(title).icon(&icon).note(path).shortcut(keys).selected(i == lit);
//! if ui.add_enabled(available, row).clicked() { .. }
//! ```

use egui::{
    Align2, Frame, Rect, Response, Role, Sense, Stroke, Ui, Widget, WidgetInfo, pos2, vec2,
};

use super::button::{ICON_GAP, paint_icon};
use super::paint::{self, State};
use super::row::{Selection, ground};
use super::tooltip::{key_chip_size, paint_key_chip};
use crate::theme::{Tokens, TokensExt as _};

/// The Palette's window: `surface.floating` with a hairline rim, the window radius and
/// shadow, and the `menu` padding around its field and rows.
pub fn palette_frame(t: &Tokens) -> Frame {
    Frame::new()
        .fill(t.color.surface.floating)
        .stroke(Stroke::new(t.stroke.hairline, t.color.stroke.hairline))
        .corner_radius(t.radius.window)
        .shadow(t.elevation.window)
        .inner_margin(t.space.menu)
}

/// What a [`palette_frame`] adds on each side: its padding and rim.
pub fn palette_frame_inset(t: &Tokens) -> f32 {
    t.space.menu + t.stroke.hairline
}

/// Between the notes at a row's trailing end.
const NOTE_SEPARATOR: &str = "  ·  ";

/// What a Palette row shows at its trailing end, in order.
enum Part {
    Note(String),
    Shortcut(String),
}

/// One row of the Command Palette, titled `text`.
pub fn palette_row(text: impl Into<String>) -> PaletteRow<'static> {
    PaletteRow {
        text: text.into(),
        icon: None,
        parts: Vec::new(),
        selected: false,
        state: State::Live,
    }
}

#[must_use = "add it with ui.add"]
pub struct PaletteRow<'a> {
    text: String,
    icon: Option<&'a str>,
    parts: Vec<Part>,
    selected: bool,
    state: State,
}

impl<'a> PaletteRow<'a> {
    /// The Icon with this key in the leading column. A key that names no Icon shows none.
    pub fn icon(self, key: &str) -> PaletteRow<'_> {
        PaletteRow {
            icon: Some(key),
            text: self.text,
            parts: self.parts,
            selected: self.selected,
            state: self.state,
        }
    }

    /// Muted text at the trailing end, after the notes already given. Notes are part of
    /// the row's accessible name, after its title.
    pub fn note(mut self, note: impl Into<String>) -> Self {
        self.parts.push(Part::Note(note.into()));
        self
    }

    /// The key that runs the row's Command, as a key chip at the trailing end. It is read
    /// out in its place among the notes.
    pub fn shortcut(mut self, keys: impl Into<String>) -> Self {
        self.parts.push(Part::Shortcut(keys.into()));
        self
    }

    /// The row the arrow keys have reached: the live selection.
    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    pub fn forced(mut self, state: State) -> Self {
        self.state = state;
        self
    }

    /// The accessible name: the title, then the trailing parts in order.
    fn name(&self) -> String {
        let parts: Vec<&str> = (self.parts.iter())
            .map(|p| match p {
                Part::Note(s) | Part::Shortcut(s) => s.as_str(),
            })
            .collect();
        if parts.is_empty() {
            self.text.clone()
        } else {
            format!("{} {}", self.text, parts.join(NOTE_SEPARATOR))
        }
    }
}

impl Widget for PaletteRow<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let t = ui.tokens();
        paint::stated(ui, self.state, |ui| {
            let c = &t.color;
            let size = vec2(ui.available_width(), t.row_height());
            let (rect, response) = ui.allocate_exact_size(size, Sense::click());
            let enabled = ui.is_enabled();
            let name = self.name();
            let selected = self.selected;
            response.widget_info(|| WidgetInfo::selected(Role::Button, enabled, selected, &name));
            if !ui.is_rect_visible(rect) {
                return response;
            }
            let s = paint::seen(&response, self.state);
            let selection = if selected {
                Selection::Live
            } else {
                Selection::None
            };
            ui.painter().add(ground(&t, rect, selection, false, s));

            let inner = rect.shrink2(vec2(t.space.row_x, 0.0));
            let mut left = inner.left();
            if let Some(key) = self.icon {
                let at = pos2(left + t.icon.small / 2.0, rect.center().y);
                paint_icon(ui, key, at, t.icon.small, enabled);
            }
            left += t.icon.small + ICON_GAP;

            // The trailing parts, from the trailing end: the chip, then the notes before it.
            let mut right = inner.right();
            let shortcut = self.parts.iter().find_map(|p| match p {
                Part::Shortcut(keys) => Some(keys.as_str()),
                Part::Note(_) => None,
            });
            if let Some(keys) = shortcut {
                let chip = key_chip_size(ui, &t, keys);
                let at = pos2(right - chip.x, rect.center().y - chip.y / 2.0);
                let chip = Rect::from_min_size(pos2(at.x.round(), at.y.round()), chip);
                paint_key_chip(ui, &t, chip, keys);
                right = chip.left() - t.space.gap;
            }
            let notes: Vec<&str> = (self.parts.iter())
                .filter_map(|p| match p {
                    Part::Note(note) => Some(note.as_str()),
                    Part::Shortcut(_) => None,
                })
                .collect();
            if !notes.is_empty() {
                let notes = notes.join(NOTE_SEPARATOR);
                let room = Rect::from_min_max(pos2(left, rect.top()), pos2(right, rect.bottom()));
                let (style, ink) = (&t.ty.body, c.text.muted);
                let drawn = paint::text_in(ui, room, Align2::RIGHT_CENTER, style, &notes, ink);
                right = drawn.left() - 2.0 * t.space.gap;
            }
            let label = Rect::from_min_max(pos2(left, rect.top()), pos2(right, rect.bottom()));
            if label.width() > 0.0 {
                paint::text_clipped(ui, label, &t.ty.body, &self.text, c.text.primary);
            }
            response
        })
    }
}
