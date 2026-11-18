//! The collapsible section header of a Panel, and the section that keeps its own open
//! state (docs/spec/v0.1/design-language.md#panels).

use egui::{
    Align2, Rect, Response, Role, Sense, Ui, UiBuilder, Vec2, Widget, WidgetInfo, pos2, vec2,
};

use super::glyph::{self, Glyph};
use super::paint::{self, State};
use super::row::{CHEVRON_PULL, Selection, TREE_INDENT, ground};
use crate::theme::{TokensExt as _, motion};

/// A section header: a row with a chevron, the title in the `title` style and a muted
/// count. A click says the section should open or close; the caller keeps the state (or
/// use [`section`], which does).
pub fn section_header(title: impl Into<String>, open: bool) -> SectionHeader {
    SectionHeader {
        title: title.into(),
        count: None,
        open,
        width: None,
        state: State::Live,
    }
}

#[must_use = "add it with ui.add"]
pub struct SectionHeader {
    title: String,
    count: Option<String>,
    open: bool,
    width: Option<f32>,
    state: State,
}

impl SectionHeader {
    /// Muted text at the trailing end: how many things the section holds, or what it is
    /// set to.
    pub fn count(mut self, count: impl Into<String>) -> Self {
        self.count = Some(count.into());
        self
    }

    /// A width of the caller's own, instead of all that is available.
    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

    /// Draws one state whatever the pointer does: for the gallery.
    pub fn forced(mut self, state: State) -> Self {
        self.state = state;
        self
    }
}

impl Widget for SectionHeader {
    fn ui(self, ui: &mut Ui) -> Response {
        let t = ui.tokens();
        paint::stated(ui, self.state, |ui| {
            let c = &t.color;
            let width = self.width.unwrap_or_else(|| ui.available_width());
            let size = vec2(width, t.row_height());
            let (rect, response) = ui.allocate_exact_size(size, Sense::click());
            let enabled = ui.is_enabled();
            response.widget_info(|| {
                WidgetInfo::labeled(Role::DisclosureTriangle, enabled, &self.title)
            });
            ui.ctx()
                .accesskit_node_builder(response.id, |node| node.set_expanded(self.open));
            if !ui.is_rect_visible(rect) {
                return response;
            }
            let s = paint::seen(&response, self.state);
            ui.painter()
                .add(ground(&t, rect, Selection::None, false, s));
            let left = rect.left() + t.space.row_x;
            let chevron = Rect::from_min_size(
                pos2(left - CHEVRON_PULL, rect.center().y - glyph::BOX / 2.0),
                Vec2::splat(glyph::BOX),
            );
            // The chevron turns from right to down as the section opens.
            let turn = match self.state {
                State::Live => {
                    let id = response.id.with("turn");
                    motion::toggle(ui.ctx(), id, self.open, t.motion.section)
                }
                _ => f32::from(u8::from(self.open)),
            };
            let at = chevron.center();
            if turn >= 1.0 {
                Glyph::ChevronDown.paint(ui.painter(), at, c.text.muted);
            } else {
                paint::turned(ui, at, turn * std::f32::consts::FRAC_PI_2, || {
                    Glyph::ChevronRight.paint(ui.painter(), at, c.text.muted);
                });
            }
            let left = left + TREE_INDENT;
            let mut right = rect.right() - t.space.row_x;
            if let Some(count) = &self.count {
                let end = Rect::from_min_max(pos2(left, rect.top()), pos2(right, rect.bottom()));
                let style = &t.ty.numeric_small;
                let drawn =
                    paint::text_in(ui, end, Align2::RIGHT_CENTER, style, count, c.text.muted);
                right = drawn.left() - t.space.gap;
            }
            let label = Rect::from_min_max(pos2(left, rect.top()), pos2(right, rect.bottom()));
            if label.width() > 0.0 {
                paint::text_clipped(ui, label, &t.ty.title, &self.title, c.text.primary);
            }
            response
        })
    }
}

/// A collapsible section: its header, and its body while it is open. The open state is
/// kept per `Ui` and title (or [`Section::id_salt`]) and outlives the session's frames
/// the way a stock collapsing header's does.
pub fn section(title: impl Into<String>) -> Section {
    Section {
        title: title.into(),
        count: None,
        id_salt: None,
        default_open: false,
    }
}

#[must_use = "call show"]
pub struct Section {
    title: String,
    count: Option<String>,
    id_salt: Option<egui::IdSalt>,
    default_open: bool,
}

/// What showing a section gave.
pub struct SectionResponse<R> {
    /// The header row.
    pub header: Response,
    /// What the body returned, when the section is open.
    pub body: Option<R>,
}

impl Section {
    /// Muted text at the header's trailing end.
    pub fn count(mut self, count: impl Into<String>) -> Self {
        self.count = Some(count.into());
        self
    }

    /// What tells this section from another with the same title in the same `Ui`.
    pub fn id_salt(mut self, id_salt: impl egui::AsIdSalt) -> Self {
        self.id_salt = Some(egui::IdSalt::new(id_salt));
        self
    }

    /// Open until the user closes it. A section starts closed otherwise.
    pub fn default_open(mut self, open: bool) -> Self {
        self.default_open = open;
        self
    }

    pub fn show<R>(self, ui: &mut Ui, body: impl FnOnce(&mut Ui) -> R) -> SectionResponse<R> {
        let salt = self
            .id_salt
            .unwrap_or_else(|| egui::IdSalt::new(&self.title));
        let id = ui.make_persistent_id(salt);
        let mut open = ui.data_mut(|d| *d.get_persisted_mut_or(id, self.default_open));
        let mut header = section_header(self.title, open);
        header.count = self.count;
        let header = ui.add(header);
        if header.clicked() {
            open = !open;
            ui.data_mut(|d| d.insert_persisted(id, open));
        }
        let t = ui.tokens();
        let shown = motion::toggle(ui.ctx(), id.with("reveal"), open, t.motion.section);
        // While it closes the body is still drawn, but gives nothing back.
        let body = if shown > 0.0 {
            let inner = reveal(ui, id, shown, body);
            open.then_some(inner)
        } else {
            None
        };
        SectionResponse { header, body }
    }
}

/// Draws a section's `body` with the top `shown` of it revealed (0 to 1): laid out in
/// full, clipped, and taking only that much room. The full height is kept from the last
/// frame drawn, so a section that opens for the first time is revealed from its first
/// frame's height.
fn reveal<R>(ui: &mut Ui, id: egui::Id, shown: f32, body: impl FnOnce(&mut Ui) -> R) -> R {
    let height_id = id.with("height");
    let top = ui.available_rect_before_wrap();
    let builder = UiBuilder::new().id_salt((id, "body")).max_rect(top);
    let mut child = ui.new_child(builder);
    let full: Option<f32> = ui.data(|d| d.get_temp(height_id));
    if shown < 1.0 {
        let room = full.unwrap_or(0.0) * shown;
        let clip = Rect::from_min_size(top.min, vec2(top.width(), room));
        child.set_clip_rect(child.clip_rect().intersect(clip));
    }
    let inner = body(&mut child);
    let used = child.min_rect();
    ui.data_mut(|d| d.insert_temp(height_id, used.height()));
    let room = if shown < 1.0 {
        used.height() * shown
    } else {
        used.height()
    };
    ui.advance_cursor_after_rect(Rect::from_min_size(used.min, vec2(used.width(), room)));
    inner
}
