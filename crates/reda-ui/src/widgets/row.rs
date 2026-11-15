//! Rows: the row of a list and the row of a tree with its disclosure Glyph
//! (docs/spec/v0.1/design-language.md#lists-trees-and-tables).
//!
//! A row is the body line plus twice `row_y` high, has the row radius and sits inside its
//! card's padding. There are no stripes and no rules between rows: stack them with no
//! gap. The ground a row is drawn on ([`ground`]) is shared with the table's rows and the
//! section header.

use egui::{
    Align2, Color32, EventFilter, Key, Rect, Response, Role, Sense, Shape, Stroke, StrokeKind, Ui,
    Vec2, Widget, WidgetInfo, epaint::RectShape, pos2, vec2,
};

use super::button::{ICON_GAP, paint_icon};
use super::glyph::{self, Glyph};
use super::mark::{SWATCH, Status, paint_swatch};
use super::paint::{self, Seen, State};
use crate::theme::{Tokens, TokensExt as _};

/// A tree's indent per level.
pub const TREE_INDENT: f32 = 16.0;
/// How far a disclosure chevron's box is pulled towards the row's leading edge. A
/// chevron's ink starts a few pixels into its box, and this lines the ink up with the
/// row's padding.
pub(super) const CHEVRON_PULL: f32 = 3.0;

/// How a selected row shows
/// (docs/spec/v0.1/design-language.md#selection-and-window-focus).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Selection {
    /// Not selected.
    #[default]
    None,
    /// Selected where the keyboard focus is: the accent subtle tint.
    Live,
    /// Selected in a Panel that does not hold keyboard focus, or while REDA's window is in
    /// the background: the neutral `fill.selected_unfocused`.
    Unfocused,
}

impl Selection {
    /// The selection a row shows: live only when its Panel holds keyboard focus and
    /// REDA's window is in front.
    pub fn of(ui: &Ui, selected: bool, panel_focused: bool) -> Selection {
        if !selected {
            Selection::None
        } else if panel_focused && ui.input(|i| i.focused) {
            Selection::Live
        } else {
            Selection::Unfocused
        }
    }

    pub fn is_selected(self) -> bool {
        self != Selection::None
    }
}

/// The ground of a row in `rect`: its selection tint, the neutral hover or press fill
/// over it, the accent frame of a drop target, and the focus ring. The ring and the frame
/// are drawn inside the row, because rows stack with no gap.
pub(super) fn ground(
    t: &Tokens,
    rect: Rect,
    selection: Selection,
    drop_target: bool,
    s: Seen,
) -> Shape {
    let c = &t.color;
    let under = if drop_target {
        c.accent.subtle
    } else {
        match selection {
            Selection::None => Color32::TRANSPARENT,
            Selection::Live => c.accent.subtle,
            Selection::Unfocused => c.fill.selected_unfocused,
        }
    };
    let mut shapes = Vec::new();
    let fill = paint::state_fill(t, under, s);
    if fill != Color32::TRANSPARENT {
        shapes.push(Shape::rect_filled(rect, t.radius.row, fill));
    }
    let frame = if s.focus {
        Some(Stroke::new(t.stroke.focus_ring, c.stroke.focus_ring))
    } else if drop_target {
        Some(Stroke::new(t.stroke.accent_line, c.accent.text))
    } else {
        None
    };
    if let Some(stroke) = frame {
        shapes.push(Shape::Rect(RectShape::stroke(
            rect,
            t.radius.row,
            stroke,
            StrokeKind::Inside,
        )));
    }
    Shape::Vec(shapes)
}

/// A stack of rows with no gap between them, as rows are meant to sit: a list, a tree.
pub fn rows<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        add(ui)
    })
    .inner
}

/// What a row leads with, before its text.
enum Lead<'a> {
    Icon(&'a str),
    Glyph(Glyph),
    Status(Status),
    Swatch(Color32),
}

impl Lead<'_> {
    fn width(&self, t: &Tokens) -> f32 {
        match self {
            Lead::Icon(_) => t.icon.small,
            Lead::Glyph(_) | Lead::Status(_) => glyph::BOX,
            Lead::Swatch(_) => SWATCH,
        }
    }
}

/// The row of a list: its text, with an optional Icon, status mark or swatch before it
/// and muted text at its trailing end. It fills the width it is given.
pub fn list_row(text: impl Into<String>) -> Row<'static> {
    Row {
        text: text.into(),
        trailing: None,
        lead: None,
        tree: None,
        selected: false,
        panel_focused: true,
        selection: None,
        drop_target: false,
        muted: false,
        width: None,
        sense: Sense::click(),
        state: State::Live,
    }
}

/// The row of a tree, `depth` levels in (the roots are at 0). `.open(..)` gives it the
/// disclosure chevron of a row that has children.
pub fn tree_row(text: impl Into<String>, depth: usize) -> Row<'static> {
    let mut row = list_row(text);
    row.tree = Some(Tree { depth, open: None });
    row
}

#[derive(Clone, Copy)]
struct Tree {
    depth: usize,
    open: Option<bool>,
}

#[must_use = "add it with ui.add, or call show for a tree row's disclosure"]
pub struct Row<'a> {
    text: String,
    trailing: Option<String>,
    lead: Option<Lead<'a>>,
    tree: Option<Tree>,
    selected: bool,
    panel_focused: bool,
    selection: Option<Selection>,
    drop_target: bool,
    muted: bool,
    width: Option<f32>,
    sense: Sense,
    state: State,
}

/// What showing a row gave.
pub struct RowResponse {
    /// The whole row.
    pub response: Response,
    /// The row's disclosure was used this frame: its chevron was clicked, or the arrow
    /// key that opens or closes it was pressed while the row held focus. Flip the open
    /// state. A click on the chevron is still a click on the row, so look at this before
    /// `response.clicked()` when the chevron should not also choose the row.
    pub toggled: bool,
}

impl<'a> Row<'a> {
    /// Selected. The tint is live while the row's Panel holds keyboard focus (see
    /// [`Row::panel_focused`]) and REDA's window is in front, and neutral otherwise.
    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    /// Whether the Panel this row is in holds keyboard focus. Only one place shows a live
    /// selection. A row assumes it does until told otherwise.
    pub fn panel_focused(mut self, focused: bool) -> Self {
        self.panel_focused = focused;
        self
    }

    /// The selection to show, whatever the window's focus is: for the gallery.
    pub fn selection(mut self, selection: Selection) -> Self {
        self.selection = Some(selection);
        self
    }

    /// Something dragged is over this row and would drop on it.
    pub fn drop_target(mut self, drop_target: bool) -> Self {
        self.drop_target = drop_target;
        self
    }

    /// Muted text: a waived Violation, a hidden Layer, an entry that is off.
    pub fn muted(mut self, muted: bool) -> Self {
        self.muted = muted;
        self
    }

    /// The Icon with this key, before the text.
    pub fn icon(self, key: &str) -> Row<'_> {
        self.lead(Lead::Icon(key))
    }

    /// A Glyph in the text colour, before the text.
    pub fn glyph(self, glyph: Glyph) -> Self {
        self.lead(Lead::Glyph(glyph))
    }

    /// A status mark before the text.
    pub fn status(self, status: Status) -> Self {
        self.lead(Lead::Status(status))
    }

    /// The swatch of a data colour before the text.
    pub fn swatch(self, color: Color32) -> Self {
        self.lead(Lead::Swatch(color))
    }

    fn lead(self, lead: Lead<'_>) -> Row<'_> {
        Row {
            lead: Some(lead),
            text: self.text,
            trailing: self.trailing,
            tree: self.tree,
            selected: self.selected,
            panel_focused: self.panel_focused,
            selection: self.selection,
            drop_target: self.drop_target,
            muted: self.muted,
            width: self.width,
            sense: self.sense,
            state: self.state,
        }
    }

    /// Small muted text at the trailing end: a count, a state.
    pub fn trailing(mut self, text: impl Into<String>) -> Self {
        self.trailing = Some(text.into());
        self
    }

    /// A tree row with children: `Some(true)` draws the open chevron, `Some(false)` the
    /// closed one. A leaf has none and keeps the chevron's room, so siblings line up.
    pub fn open(mut self, open: Option<bool>) -> Self {
        if let Some(tree) = &mut self.tree {
            tree.open = open;
        }
        self
    }

    /// A width of the caller's own, instead of all that is available.
    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

    /// What the row senses, when a click is not all: a row that can be dragged.
    pub fn sense(mut self, sense: Sense) -> Self {
        self.sense = sense;
        self
    }

    /// Draws one state whatever the pointer does: for the gallery.
    pub fn forced(mut self, state: State) -> Self {
        self.state = state;
        self
    }

    /// Shows the row and says whether its disclosure was used.
    pub fn show(self, ui: &mut Ui) -> RowResponse {
        let t = ui.tokens();
        paint::stated(ui, self.state, |ui| self.show_stated(ui, &t))
    }

    fn show_stated(self, ui: &mut Ui, t: &Tokens) -> RowResponse {
        let c = &t.color;
        let width = self.width.unwrap_or_else(|| ui.available_width());
        let (rect, response) = ui.allocate_exact_size(vec2(width, t.row_height()), self.sense);
        let enabled = ui.is_enabled();
        let selection = self
            .selection
            .unwrap_or_else(|| Selection::of(ui, self.selected, self.panel_focused));
        let role = if self.tree.is_some() {
            Role::TreeItem
        } else {
            Role::ListItem
        };
        response.widget_info(|| {
            WidgetInfo::selected(role, enabled, selection.is_selected(), &self.text)
        });
        let open = self.tree.and_then(|tree| tree.open);
        ui.ctx().accesskit_node_builder(response.id, |node| {
            node.set_selected(selection.is_selected());
            if let Some(tree) = self.tree {
                node.set_level(tree.depth + 1);
                if let Some(open) = tree.open {
                    node.set_expanded(open);
                }
            }
        });

        let mut left = rect.left() + t.space.row_x;
        let mut chevron = None;
        if let Some(tree) = self.tree {
            left += TREE_INDENT * tree.depth as f32;
            chevron = Some(Rect::from_min_size(
                pos2(left - CHEVRON_PULL, rect.center().y - glyph::BOX / 2.0),
                Vec2::splat(glyph::BOX),
            ));
            left += TREE_INDENT;
        }
        let toggled = open.is_some_and(|open| {
            let slot = chevron.unwrap_or(Rect::NOTHING);
            let by_pointer = response.clicked()
                && response
                    .interact_pointer_pos()
                    .is_some_and(|at| at.x < slot.right() && at.x >= slot.left());
            by_pointer || arrow_toggles(ui, &response, open)
        });
        if !ui.is_rect_visible(rect) {
            return RowResponse { response, toggled };
        }

        let s = paint::seen(&response, self.state);
        ui.painter()
            .add(ground(t, rect, selection, self.drop_target, s));
        let ink = if self.muted {
            c.text.muted
        } else {
            c.text.primary
        };
        if let (Some(open), Some(slot)) = (open, chevron) {
            let glyph = if open {
                Glyph::ChevronDown
            } else {
                Glyph::ChevronRight
            };
            glyph.paint(ui.painter(), slot.center(), c.text.muted);
        }
        if let Some(lead) = &self.lead {
            let side = lead.width(t);
            let at = pos2(left + side / 2.0, rect.center().y);
            match lead {
                Lead::Icon(key) => paint_icon(ui, key, at, side, enabled),
                Lead::Glyph(glyph) => glyph.paint(ui.painter(), at, ink),
                Lead::Status(status) => status.paint(ui.painter(), at, t),
                Lead::Swatch(color) => {
                    let mark = Rect::from_center_size(at, Vec2::splat(side));
                    paint_swatch(ui.painter(), t, mark, *color);
                }
            }
            left += side + ICON_GAP;
        }
        let mut right = rect.right() - t.space.row_x;
        if let Some(trailing) = &self.trailing {
            let style = &t.ty.numeric_small;
            let end = Rect::from_min_max(pos2(left, rect.top()), pos2(right, rect.bottom()));
            let drawn =
                paint::text_in(ui, end, Align2::RIGHT_CENTER, style, trailing, c.text.muted);
            right = drawn.left() - t.space.gap;
        }
        let label = Rect::from_min_max(pos2(left, rect.top()), pos2(right, rect.bottom()));
        if label.width() > 0.0 {
            paint::text_clipped(ui, label, &t.ty.body, &self.text, ink);
        }
        RowResponse { response, toggled }
    }
}

/// Whether an arrow key used the disclosure of the focused row: right opens a closed
/// one, left closes an open one. The arrows are kept from moving the focus sideways.
fn arrow_toggles(ui: &Ui, response: &Response, open: bool) -> bool {
    if !response.has_focus() {
        return false;
    }
    ui.memory_mut(|m| {
        m.set_focus_lock_filter(
            response.id,
            EventFilter {
                horizontal_arrows: true,
                ..Default::default()
            },
        );
    });
    let key = if open {
        Key::ArrowLeft
    } else {
        Key::ArrowRight
    };
    ui.input(|i| i.key_pressed(key))
}

impl Widget for Row<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        self.show(ui).response
    }
}
