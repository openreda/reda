//! Fields: the text field, the number field and the search field
//! (docs/spec/v0.1/design-language.md#fields).
//!
//! A field is a well: `surface.well` with a 1 px `stroke.strong` outline. Hover darkens
//! the outline, focus swaps it for the focus ring, and invalid for a line in the error
//! text colour. The editing itself is egui's `TextEdit`, laid out from a text-style token
//! so that the field is a control tall with any face.

use egui::{
    Align2, Id, Rect, Response, Sense, Shape, Stroke, StrokeKind, Ui, UiBuilder, Widget, pos2, vec2,
};

use super::glyph::{self, Glyph};
use super::paint::{self, State};
use crate::theme::{Text, Tokens, TokensExt as _};

/// A field's width when it is given none. The `size` family replaces it when it lands.
pub const FIELD_WIDTH: f32 = 280.0;
/// The width of an invalid field's outline.
const INVALID_LINE: f32 = 1.5;
/// Between a field's text and its unit, and between the search glass and the text.
pub(super) const PART_GAP: f32 = 6.0;

/// A one-line text field editing `text`.
pub fn text_field(text: &mut String) -> TextField<'_> {
    TextField {
        text,
        id: None,
        hint: String::new(),
        width: None,
        invalid: false,
        numeric: false,
        unit: None,
        search: false,
        mono: false,
        rows: None,
        state: State::Live,
    }
}

/// A number field: a text field in the numeric style (tabular digits), with the unit in
/// muted text when given one. The caller reads the number out of the text.
pub fn number_field(text: &mut String) -> TextField<'_> {
    TextField {
        numeric: true,
        ..text_field(text)
    }
}

#[must_use = "add it with ui.add"]
pub struct TextField<'a> {
    text: &'a mut String,
    id: Option<Id>,
    hint: String,
    width: Option<f32>,
    invalid: bool,
    numeric: bool,
    unit: Option<String>,
    search: bool,
    mono: bool,
    rows: Option<usize>,
    state: State,
}

impl TextField<'_> {
    /// A fixed id, for a field that is asked whether it has focus or is given it.
    pub fn id(mut self, id: Id) -> Self {
        self.id = Some(id);
        self
    }

    /// Shown in muted text while the field is empty.
    pub fn hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = hint.into();
        self
    }

    /// The field's width; `f32::INFINITY` takes all there is.
    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

    /// The error outline. Pair it with [`invalid_message`].
    pub fn invalid(mut self, invalid: bool) -> Self {
        self.invalid = invalid;
        self
    }

    /// The unit, in muted text at the trailing end.
    pub fn unit(mut self, unit: impl Into<String>) -> Self {
        self.unit = Some(unit.into());
        self
    }

    /// A search field: the search glass Glyph at the leading end.
    pub fn search(mut self) -> Self {
        self.search = true;
        self
    }

    /// The monospace style: file names and patterns, command and key text.
    pub fn mono(mut self) -> Self {
        self.mono = true;
        self
    }

    /// A field of several lines, `rows` of them tall. Enter starts a new line.
    pub fn rows(mut self, rows: usize) -> Self {
        self.rows = Some(rows.max(1));
        self
    }

    pub fn forced(mut self, state: State) -> Self {
        self.state = state;
        self
    }
}

impl Widget for TextField<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let t = ui.tokens();
        paint::stated(ui, self.state, |ui| {
            let c = &t.color;
            let style = if self.numeric {
                t.ty.numeric.clone()
            } else if self.mono {
                t.ty.mono.clone()
            } else {
                t.ty.body.clone()
            };
            let rows = self.rows.unwrap_or(1) as f32;
            let width = match self.width {
                Some(width) if width.is_finite() => width,
                Some(_) => ui.available_width(),
                None => FIELD_WIDTH,
            };
            let size = vec2(width, rows * style.line + 2.0 * t.space.control_y);
            let (rect, outer) = ui.allocate_exact_size(size, Sense::hover());
            // The text keeps clear of the focus ring, which is drawn inside the well.
            let inset = t.space.control_x - t.stroke.focus_ring;
            let unit_width = self
                .unit
                .as_deref()
                .map_or(0.0, |unit| paint::width(ui, &t.ty.body, unit) + PART_GAP);
            let search_width = if self.search {
                glyph::BOX + PART_GAP
            } else {
                0.0
            };
            let inner = Rect::from_min_max(
                pos2(
                    rect.left() + inset + search_width,
                    rect.top() + t.space.control_y,
                ),
                pos2(
                    rect.right() - inset - unit_width,
                    rect.bottom() - t.space.control_y,
                ),
            );
            // The well goes behind the text, but its outline depends on whether the text
            // edit has focus: reserve the two shapes now and set them at the end.
            let well = ui.painter().add(Shape::Noop);
            let outline_slot = ui.painter().add(Shape::Noop);
            let mut s = paint::seen(&outer, self.state);
            let mut response = outer;
            if self.state == State::Live {
                let hint = style.rich(self.hint.as_str()).color(c.text.muted);
                let edited = edit(ui, inner, self.text, &style, &t, hint, self.id, self.rows);
                // A click on the padding around the text lands in the field all the same.
                let clicked_beside = response.hovered()
                    && !edited.hovered()
                    && ui.input(|i| i.pointer.primary_clicked());
                if clicked_beside {
                    edited.request_focus();
                }
                s.focus = edited.has_focus();
                if s.focus {
                    paint::draws_focus(ui.ctx(), edited.id);
                }
                s.hover = edited.hovered() || response.hovered();
                response = edited;
            } else if self.text.is_empty() {
                let ink = c.text.muted;
                paint::text_in(ui, inner, Align2::LEFT_TOP, &style, &self.hint, ink);
            } else {
                let ink = c.text.primary;
                let drawn = paint::text_in(ui, inner, Align2::LEFT_CENTER, &style, self.text, ink);
                if s.focus {
                    let caret = Stroke::new(glyph::STROKE, c.accent.text);
                    let x = drawn.right() + caret.width / 2.0;
                    ui.painter().vline(x, drawn.y_range(), caret);
                }
            }
            let line = outline(&t, self.invalid, s);
            ui.painter().set(
                well,
                egui::epaint::RectShape::filled(rect, t.radius.control, c.surface.well),
            );
            ui.painter().set(
                outline_slot,
                egui::epaint::RectShape::stroke(rect, t.radius.control, line, StrokeKind::Inside),
            );
            if let Some(unit) = &self.unit {
                let at = rect.shrink2(vec2(inset, 0.0));
                let ink = c.text.muted;
                paint::text_in(ui, at, Align2::RIGHT_CENTER, &t.ty.body, unit, ink);
            }
            if self.search {
                let at = pos2(rect.left() + inset + glyph::BOX / 2.0, rect.center().y);
                Glyph::Search.paint(ui.painter(), at, c.text.muted);
            }
            response
        })
    }
}

/// egui's one-line `TextEdit` in `inner`, with no frame of its own, laid out from `style`.
/// It takes no room in `ui`: the caller already took the field's.
pub(super) fn edit_line(
    ui: &mut Ui,
    inner: Rect,
    text: &mut String,
    style: &Text,
    t: &Tokens,
    hint: egui::RichText,
    id: Option<Id>,
) -> Response {
    let line = Rect::from_center_size(inner.center(), vec2(inner.width(), style.line));
    edit(ui, line, text, style, t, hint, id, None)
}

/// egui's `TextEdit` filling `inner`: one line, or `rows` lines that wrap at its width.
#[expect(clippy::too_many_arguments)]
fn edit(
    ui: &mut Ui,
    inner: Rect,
    text: &mut String,
    style: &Text,
    t: &Tokens,
    hint: egui::RichText,
    id: Option<Id>,
    rows: Option<usize>,
) -> Response {
    let ink = t.color.text.primary;
    let lay = style.clone();
    let wraps = rows.is_some();
    let mut layouter = move |ui: &Ui, buffer: &dyn egui::TextBuffer, wrap: f32| {
        let mut job = lay.job(buffer.as_str(), ink);
        if wraps {
            job.wrap.max_width = wrap;
        }
        ui.painter().layout_job(job)
    };
    let mut edit = match rows {
        Some(rows) => egui::TextEdit::multiline(text).desired_rows(rows),
        None => egui::TextEdit::singleline(text),
    };
    edit = edit
        .frame(egui::Frame::NONE)
        .margin(egui::Margin::ZERO)
        .desired_width(inner.width())
        .align(Align2::LEFT_TOP)
        .hint_text(hint)
        .layouter(&mut layouter);
    if let Some(id) = id {
        edit = edit.id(id);
    }
    let mut child = ui.new_child(UiBuilder::new().max_rect(inner));
    // egui grows a text edit to `interact_size.y`; the line height is the token's.
    child.spacing_mut().interact_size.y = style.line;
    child.add(edit)
}

/// A field's outline: `stroke.strong`, darker when hovered, the focus ring when it has
/// the keyboard, and the error line when invalid.
pub(super) fn outline(t: &Tokens, invalid: bool, s: paint::Seen) -> Stroke {
    let c = &t.color;
    if invalid {
        Stroke::new(INVALID_LINE, c.error.text)
    } else if s.focus {
        Stroke::new(t.stroke.focus_ring, c.stroke.focus_ring)
    } else if s.hover {
        Stroke::new(t.stroke.hairline, c.text.muted)
    } else {
        Stroke::new(t.stroke.hairline, c.stroke.strong)
    }
}

/// What goes beside or under an invalid field: a cross and the reason, in the error text
/// colour.
pub fn invalid_message(text: impl Into<String>) -> InvalidMessage {
    InvalidMessage { text: text.into() }
}

#[must_use = "add it with ui.add"]
pub struct InvalidMessage {
    text: String,
}

impl Widget for InvalidMessage {
    fn ui(self, ui: &mut Ui) -> Response {
        let t = ui.tokens();
        let ink = t.color.error.text;
        let galley = paint::galley(ui, &t.ty.body, &self.text, ink);
        let size = vec2(glyph::BOX + galley.size().x, galley.size().y);
        let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
        response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, &self.text));
        if ui.is_rect_visible(rect) {
            let at = pos2(rect.left() + glyph::BOX / 2.0, rect.center().y);
            Glyph::Cross.paint(ui.painter(), at, ink);
            ui.painter()
                .galley(pos2(rect.left() + glyph::BOX, rect.top()), galley, ink);
        }
        response
    }
}
