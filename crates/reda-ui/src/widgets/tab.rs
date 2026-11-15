//! A tab on a tab group's strip (docs/spec/v0.1/design-language.md#tabs).
//!
//! The front tab of a group is a card joined to the panel below it: the card's surface
//! with no line between tab and panel. The active Document's tab adds the 2 px accent
//! line along its top edge and the `body_strong` style. Back tabs sit on the base in
//! muted text, with the hover fill as a rounded pill. Each tab shows its 16 px Icon, its
//! title, the out-of-sync badge, and a modified dot that gives way to the close × on
//! hover; the front or hovered tab shows the close ×.

use egui::emath::GuiRounding as _;
use egui::{
    Align2, Color32, CornerRadius, Rect, Response, Role, Sense, Shape, Stroke, StrokeKind, Ui,
    Vec2, WidgetInfo, pos2, vec2,
};

use super::button::{ICON_GAP, paint_icon};
use super::glyph::{self, Glyph};
use super::paint::{self, State};
use crate::theme::{Tokens, TokensExt as _, motion};

/// How far the first tab of a strip stands in from its card's edge.
pub const TAB_INSET: f32 = 6.0;
/// How much smaller than the card's radius the top corners of a front tab are.
const CORNER_LESS: f32 = 2.0;
/// Between a tab's title and what follows it.
const PART_GAP: f32 = 6.0;
/// How far a back tab's hover pill stands in from the tab's edges.
const PILL_INSET: Vec2 = vec2(2.0, 3.0);
/// How far the focus ring stands in from the tab's edges, so it stays on the strip.
const RING_INSET: f32 = 2.0;
/// How much larger than the Glyph box the hover fill behind the close × is.
const CLOSE_GROW: f32 = 2.0;

/// Between the Icon buttons of a collapsed Dock's strip.
const STRIP_GAP: f32 = 2.0;

/// Lays out a tab group's strip in `ui`: the tabs `add` shows touch, the first one
/// [`TAB_INSET`] in from the card's edge.
pub fn tab_strip<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    ui.spacing_mut().item_spacing.x = 0.0;
    ui.add_space(TAB_INSET);
    add(ui)
}

/// Lays out a collapsed Dock's strip in `ui`: the Icon buttons `add` shows, in a row for
/// the bottom Dock (`across`) and in a column for a side Dock, on the base with no frame.
pub fn dock_strip<R>(ui: &mut Ui, across: bool, add: impl FnOnce(&mut Ui) -> R) -> R {
    let spaced = |ui: &mut Ui| {
        ui.spacing_mut().item_spacing = Vec2::splat(STRIP_GAP);
        add(ui)
    };
    if across {
        ui.horizontal_centered(spaced).inner
    } else {
        ui.vertical_centered(spaced).inner
    }
}

/// Which of the three tabs this is.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TabKind {
    /// The front tab of the active Document: a card with the accent line.
    Active,
    /// The front tab of any other group, or a Panel's: a card, no line.
    Front,
    /// A tab behind the front one: on the base.
    Back,
}

/// A tab titled `title`. Show it with [`Tab::show`] on a strip that is
/// [`Tokens::tab_height`] tall and lets it paint one point below itself.
pub fn tab(title: &str) -> Tab<'_> {
    Tab {
        title,
        icon: None,
        kind: TabKind::Back,
        modified: false,
        out_of_sync: false,
        window_focused: None,
        state: State::Live,
    }
}

#[must_use = "call .show()"]
pub struct Tab<'a> {
    title: &'a str,
    icon: Option<&'a str>,
    kind: TabKind,
    modified: bool,
    out_of_sync: bool,
    window_focused: Option<bool>,
    state: State,
}

/// What showing a tab gave.
pub struct TabResponse {
    /// The whole tab: clicks, drags and keyboard focus.
    pub response: Response,
    /// The close ×, on the front or hovered tab. Name it for AccessKit.
    pub close: Option<Response>,
}

impl<'a> Tab<'a> {
    /// The 16 px Icon before the title, in its role colours on every tab.
    pub fn icon(mut self, key: Option<&'a str>) -> Self {
        self.icon = key;
        self
    }

    pub fn kind(mut self, kind: TabKind) -> Self {
        self.kind = kind;
        self
    }

    /// Changes not yet saved: a dot where the close × is, until the tab is hovered.
    pub fn modified(mut self, modified: bool) -> Self {
        self.modified = modified;
        self
    }

    /// The out-of-sync badge: a warning triangle in the warning solid.
    pub fn out_of_sync(mut self, out_of_sync: bool) -> Self {
        self.out_of_sync = out_of_sync;
        self
    }

    /// Whether REDA's window is in front, instead of asking the `Ui`. In the background
    /// the accent line goes neutral.
    pub fn window_focused(mut self, focused: bool) -> Self {
        self.window_focused = Some(focused);
        self
    }

    pub fn forced(mut self, state: State) -> Self {
        self.state = state;
        self
    }

    /// The top corners of a front tab.
    fn corners(t: &Tokens) -> CornerRadius {
        let r = (t.radius.card - CORNER_LESS) as u8;
        CornerRadius {
            nw: r,
            ne: r,
            sw: 0,
            se: 0,
        }
    }

    pub fn show(self, ui: &mut Ui) -> TabResponse {
        let t = ui.tokens();
        paint::stated(ui, self.state, |ui| self.show_stated(ui, &t))
    }

    fn show_stated(self, ui: &mut Ui, t: &Tokens) -> TabResponse {
        let c = &t.color;
        let front = self.kind != TabKind::Back;
        let style = if self.kind == TabKind::Active {
            &t.ty.body_strong
        } else {
            &t.ty.body
        };
        let icon_width = if self.icon.is_some() {
            t.icon.small + ICON_GAP
        } else {
            0.0
        };
        let badge_width = if self.out_of_sync { glyph::BOX } else { 0.0 };
        // | tab_x | icon gap | title | gap | badge | close | tab_x |
        let width = 2.0 * t.space.tab_x
            + icon_width
            + paint::width(ui, style, self.title)
            + PART_GAP
            + badge_width
            + glyph::BOX;
        let (rect, response) =
            ui.allocate_exact_size(vec2(width, t.tab_height()), Sense::click_and_drag());
        let enabled = ui.is_enabled();
        response.widget_info(|| WidgetInfo::selected(Role::Tab, enabled, front, self.title));
        let s = paint::seen(&response, self.state);
        // Hovering the close × is hovering the tab.
        let pointed = match self.state {
            State::Live => ui.rect_contains_pointer(rect),
            _ => s.hover,
        };
        let close_at = pos2(
            rect.right() - t.space.tab_x - glyph::BOX / 2.0,
            rect.center().y,
        );
        let close_rect = Rect::from_center_size(close_at, Vec2::splat(glyph::BOX));
        let close = (front || pointed)
            .then(|| ui.interact(close_rect, response.id.with("close"), Sense::click()));
        if !ui.is_rect_visible(rect) {
            return TabResponse { response, close };
        }

        let p = ui.painter();
        if front {
            let top = Self::corners(t);
            p.rect_filled(rect, top, c.surface.card);
            // The rim runs up one side, over the top and down the other. Below, the tab
            // is open: it covers the rim of the card it is joined to.
            let rim = Stroke::new(t.stroke.hairline, c.stroke.hairline);
            let open = rect.with_max_y(rect.bottom() + t.radius.card);
            p.with_clip_rect(rect.with_max_y(rect.bottom() + t.stroke.hairline))
                .rect_stroke(open, top, rim, StrokeKind::Inside);
            join_tab(ui, t, rect);
            // The accent line fades in on the tab that becomes active, and leaves the
            // last one at once. What the tab holds swaps at once.
            let active = self.kind == TabKind::Active;
            let line = motion::toggle(ui.ctx(), response.id.with("accent"), active, t.motion.tab_line);
            if active || line > 0.0 {
                let focused = self
                    .window_focused
                    .unwrap_or_else(|| ui.input(|i| i.focused));
                let ink = if focused {
                    c.accent.text
                } else {
                    c.stroke.strong
                };
                p.rect_filled(accent_line(t, rect), under_corners(t), ink.gamma_multiply(line));
            }
        } else if pointed || s.hover_fill > 0.0 {
            let fill = paint::state_fill(t, Color32::TRANSPARENT, s);
            let fill = if fill == Color32::TRANSPARENT && pointed {
                c.fill.hover
            } else {
                fill
            };
            p.rect_filled(rect.shrink2(PILL_INSET), t.radius.control, fill);
        }
        let mut left = rect.left() + t.space.tab_x;
        if let Some(key) = self.icon {
            let at = pos2(left + t.icon.small / 2.0, rect.center().y);
            paint_icon(ui, key, at, t.icon.small, true);
            left += icon_width;
        }
        let ink = if front { c.text.primary } else { c.text.muted };
        let label = Rect::from_min_max(pos2(left, rect.top()), rect.max);
        let drawn = paint::text_in(ui, label, Align2::LEFT_CENTER, style, self.title, ink);
        if self.out_of_sync {
            let at = pos2(drawn.right() + PART_GAP + glyph::BOX / 2.0, rect.center().y);
            out_of_sync_badge(ui, t, at);
        }
        if self.modified && !pointed {
            Glyph::Dot.paint(ui.painter(), close_at, ink);
        } else if let Some(close) = &close {
            let over = match self.state {
                State::Live => close.hovered(),
                _ => s.hover && front,
            };
            if over {
                let fill = if close.is_pointer_button_down_on() {
                    c.fill.press
                } else {
                    c.fill.hover
                };
                let ground = close_rect.expand(CLOSE_GROW / 2.0);
                ui.painter().rect_filled(ground, t.radius.mark, fill);
            }
            Glyph::Close.paint(ui.painter(), close_at, c.text.muted);
        }
        if s.focus {
            paint::focus_ring(ui, t, rect.shrink(RING_INSET), t.radius.control);
        }
        TabResponse { response, close }
    }
}

/// Joins the front tab at `tab` to the card below it: covers the card's rim under the
/// tab with the card's surface, so no line runs between tab and panel. The tab does this
/// itself; call it again after anything that repaints the card's rim.
pub fn join_tab(ui: &Ui, t: &Tokens, tab: Rect) {
    ui.painter().hline(
        (tab.left() + t.stroke.hairline)..=(tab.right() - t.stroke.hairline),
        tab.bottom() + t.stroke.hairline / 2.0,
        Stroke::new(t.stroke.hairline, t.color.surface.card),
    );
}

/// Where the accent line of the active Document's tab is: along the top edge, between
/// the rounded corners.
pub fn accent_line(t: &Tokens, tab: Rect) -> Rect {
    let inset = t.radius.card - CORNER_LESS;
    Rect::from_min_max(
        pos2(tab.left() + inset, tab.top()),
        pos2(tab.right() - inset, tab.top() + t.stroke.accent_line),
    )
}

/// The accent line's lower corners are rounded by its own width.
fn under_corners(t: &Tokens) -> CornerRadius {
    let r = t.stroke.accent_line as u8;
    CornerRadius {
        nw: 0,
        ne: 0,
        sw: r,
        se: r,
    }
}

/// The out-of-sync badge centred on `at`: the warning triangle filled with the warning
/// solid, its mark in the solid's on-colour. A shape and a colour, never colour alone.
fn out_of_sync_badge(ui: &Ui, t: &Tokens, at: egui::Pos2) {
    let c = &t.color.warning;
    let p = ui.painter();
    let origin = (at - Vec2::splat(glyph::BOX / 2.0)).round_to_pixels(p.pixels_per_point());
    let to = |x: f32, y: f32| origin + vec2(x, y);
    p.add(Shape::convex_polygon(
        vec![to(8.0, 2.5), to(14.0, 13.0), to(2.0, 13.0)],
        c.solid,
        Stroke::NONE,
    ));
    let pen = Stroke::new(glyph::STROKE, c.on_solid);
    p.line_segment([to(8.0, 6.5), to(8.0, 9.25)], pen);
    p.circle_filled(to(8.0, 11.25), glyph::STROKE / 2.0, c.on_solid);
}
