//! The command field in the Title bar (docs/spec/v0.1/design-language.md#title-bar): a
//! filled control with the search glass at its leading end, the window's title centred
//! in it, and the Palette's key in small muted text at the trailing end when there is
//! room. Clicking it opens the Command Palette, which the caller does on `.clicked()`.

use egui::{Align2, Rect, Response, Role, Sense, Ui, Vec2, Widget, WidgetInfo, pos2, vec2};

use super::glyph::{self, Glyph};
use super::paint::{self, State};
use crate::theme::TokensExt as _;

/// The command field's least width: narrower, the Title bar shows it as an Icon button.
pub const COMMAND_FIELD_MIN: f32 = 180.0;
/// The command field's greatest width.
pub const COMMAND_FIELD_MAX: f32 = 420.0;

/// A command field showing `text`: the window's title, "Project name — active Document".
pub fn command_field(text: impl Into<String>) -> CommandField {
    CommandField {
        text: text.into(),
        hint: false,
        shortcut: None,
        name: None,
        width: COMMAND_FIELD_MAX,
        window_focused: None,
        state: State::Live,
    }
}

#[must_use = "add it with ui.add"]
pub struct CommandField {
    text: String,
    hint: bool,
    shortcut: Option<String>,
    name: Option<String>,
    width: f32,
    window_focused: Option<bool>,
    state: State,
}

impl CommandField {
    /// The text is a prompt, not a title ("Search commands" with no Project open): it is
    /// muted.
    pub fn hint(mut self, hint: bool) -> Self {
        self.hint = hint;
        self
    }

    /// The key that opens the Palette, shown at the trailing end when there is room.
    pub fn shortcut(mut self, keys: impl Into<String>) -> Self {
        self.shortcut = Some(keys.into());
        self
    }

    /// The accessible name, when it differs from the text shown. The text shown is then
    /// the node's value.
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// The field's width; the Title bar keeps it within [`COMMAND_FIELD_MIN`] and
    /// [`COMMAND_FIELD_MAX`].
    pub fn width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }

    /// Whether REDA's window is in front. Behind, the title is muted. Unset, it follows
    /// the window.
    pub fn window_focused(mut self, focused: bool) -> Self {
        self.window_focused = Some(focused);
        self
    }

    pub fn forced(mut self, state: State) -> Self {
        self.state = state;
        self
    }
}

impl Widget for CommandField {
    fn ui(self, ui: &mut Ui) -> Response {
        let t = ui.tokens();
        paint::stated(ui, self.state, |ui| {
            let c = &t.color;
            let size = vec2(self.width, t.control_height());
            let (rect, response) = ui.allocate_exact_size(size, Sense::click());
            let enabled = ui.is_enabled();
            let (name, value) = match &self.name {
                Some(name) => (name.clone(), Some(self.text.clone())),
                None => (self.text.clone(), None),
            };
            response.widget_info(|| {
                let mut info = WidgetInfo::labeled(Role::Button, enabled, &name);
                info.current_text_value.clone_from(&value);
                info
            });
            if !ui.is_rect_visible(rect) {
                return response;
            }
            let s = paint::seen(&response, self.state);
            let fill = paint::state_fill(&t, c.fill.control, s);
            ui.painter().rect_filled(rect, t.radius.control, fill);
            let inner = rect.shrink2(vec2(t.space.control_x, 0.0));
            let glass = pos2(inner.left() + glyph::BOX / 2.0, rect.center().y);
            Glyph::Search.paint(ui.painter(), glass, c.text.muted);
            let focused = self
                .window_focused
                .unwrap_or_else(|| ui.input(|i| i.focused));
            let ink = if self.hint || !focused {
                c.text.muted
            } else {
                c.text.primary
            };
            // The text keeps clear of the glass, and of the key on both sides, so it
            // stays centred in the field.
            let text_w = paint::width(ui, &t.ty.body, &self.text);
            let lead = glyph::BOX + t.space.gap;
            let key = self
                .shortcut
                .as_deref()
                .map(|k| (k, paint::width(ui, &t.ty.small, k) + t.space.gap))
                .filter(|(_, w)| text_w + 2.0 * lead.max(*w) <= inner.width());
            let side = key.map_or(lead, |(_, w)| lead.max(w));
            if let Some((keys, _)) = key {
                paint::text_in(
                    ui,
                    inner,
                    Align2::RIGHT_CENTER,
                    &t.ty.small,
                    keys,
                    c.text.muted,
                );
            }
            let room = inner.shrink2(Vec2::new(side, 0.0));
            if text_w <= room.width() {
                paint::text_in(ui, room, Align2::CENTER_CENTER, &t.ty.body, &self.text, ink);
            } else {
                let room = Rect::from_min_max(pos2(inner.left() + lead, inner.top()), inner.max);
                paint::text_clipped(ui, room, &t.ty.body, &self.text, ink);
            }
            if s.focus {
                paint::focus_ring(ui, &t, rect, t.radius.control);
            }
            response
        })
    }
}
