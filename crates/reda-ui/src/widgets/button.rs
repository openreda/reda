//! Buttons: filled, primary, flat and destructive, the toggle, the Icon button and the
//! link (docs/spec/v0.1/design-language.md#buttons).

use egui::{
    Align2, Color32, CursorIcon, Image, Rect, Response, Role, Sense, Stroke, Ui, Vec2, Widget,
    WidgetInfo, pos2, vec2,
};

use super::glyph::Glyph;
use super::paint::{self, State};
use crate::theme::TokensExt as _;

/// Between a button's icon and its label.
pub const ICON_GAP: f32 = 6.0;
/// How far below a link's text its underline sits from the bottom of the line.
const LINK_UNDERLINE_RISE: f32 = 3.0;
/// How far a link's focus ring stands off its text sideways.
const LINK_RING_PAD: f32 = 2.0;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Filled,
    Primary,
    Flat,
    Danger,
}

/// A button: filled at rest with no border. `.primary()` is the one primary button of a
/// dialog, `.danger()` the confirm button of a destructive one, `.flat()` has no fill
/// until hovered, and `.on(..)` makes it a toggle.
pub fn button(text: impl Into<String>) -> Button<'static> {
    Button {
        text: text.into(),
        kind: Kind::Filled,
        on: None,
        icon: None,
        state: State::Live,
    }
}

#[must_use = "add it with ui.add"]
pub struct Button<'a> {
    text: String,
    kind: Kind,
    on: Option<bool>,
    icon: Option<&'a str>,
    state: State,
}

impl<'a> Button<'a> {
    /// The accent solid with its on-colour. One per dialog.
    pub fn primary(mut self) -> Self {
        self.kind = Kind::Primary;
        self
    }

    /// No fill until hovered: row actions and bars.
    pub fn flat(mut self) -> Self {
        self.kind = Kind::Flat;
        self
    }

    /// The error solid with its on-colour.
    pub fn danger(mut self) -> Self {
        self.kind = Kind::Danger;
        self
    }

    /// A toggle: the accent subtle fill when on.
    pub fn on(mut self, on: bool) -> Self {
        self.on = Some(on);
        self
    }

    /// The Icon with this key, before the label.
    pub fn icon(self, key: &str) -> Button<'_> {
        Button {
            icon: Some(key),
            text: self.text,
            kind: self.kind,
            on: self.on,
            state: self.state,
        }
    }

    /// Draws one state whatever the pointer does: for the gallery.
    pub fn forced(mut self, state: State) -> Self {
        self.state = state;
        self
    }
}

impl Widget for Button<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let t = ui.tokens();
        paint::stated(ui, self.state, |ui| {
            let c = &t.color;
            let style = match self.kind {
                Kind::Primary | Kind::Danger => &t.ty.body_strong,
                Kind::Filled | Kind::Flat => &t.ty.body,
            };
            let icon_width = if self.icon.is_some() {
                t.icon.small + ICON_GAP
            } else {
                0.0
            };
            let size = vec2(
                paint::width(ui, style, &self.text) + icon_width + 2.0 * t.space.control_x,
                t.control_height(),
            );
            let (rect, response) = ui.allocate_exact_size(size, Sense::click());
            let enabled = ui.is_enabled();
            response.widget_info(|| match self.on {
                Some(on) => WidgetInfo::selected(Role::Button, enabled, on, &self.text),
                None => WidgetInfo::labeled(Role::Button, enabled, &self.text),
            });
            if !ui.is_rect_visible(rect) {
                return response;
            }
            let s = paint::seen(&response, self.state);
            let (under, ink) = match (self.kind, self.on) {
                (Kind::Primary, _) => (c.accent.solid, c.accent.on_solid),
                (Kind::Danger, _) => (c.error.solid, c.error.on_solid),
                (_, Some(true)) => (c.accent.subtle, c.text.primary),
                (Kind::Filled, _) => (c.fill.control, c.text.primary),
                (Kind::Flat, _) => (Color32::TRANSPARENT, c.text.primary),
            };
            ui.painter()
                .rect_filled(rect, t.radius.control, paint::state_fill(&t, under, s));
            let mut left = rect.left() + t.space.control_x;
            if let Some(key) = self.icon {
                let at = pos2(left + t.icon.small / 2.0, rect.center().y);
                paint_icon(ui, key, at, t.icon.small, enabled);
                left += icon_width;
            }
            let label = Rect::from_min_max(pos2(left, rect.top()), rect.max);
            paint::text_in(ui, label, Align2::LEFT_CENTER, style, &self.text, ink);
            if s.focus {
                paint::focus_ring(ui, &t, rect, t.radius.control);
            }
            response
        })
    }
}

/// The Icon `key` centred on `at`: its role colours when enabled, its greyscale copy when
/// not (the disabled `Ui` fades it once, with its button).
pub(super) fn paint_icon(ui: &Ui, key: &str, at: egui::Pos2, size: f32, enabled: bool) {
    if let Some(image) = crate::icons::image(key, size, ui.ctx().theme(), enabled) {
        image.paint_at(ui, Rect::from_center_size(at, Vec2::splat(size)));
    }
}

#[derive(Clone, Copy)]
enum IconPx {
    Small,
    Bar,
    Custom(f32),
}

enum Art<'a> {
    Key(&'a str),
    Image(Image<'static>),
    Glyph(Glyph),
}

/// An Icon button: a square of the control height holding the Icon `key`, flat at rest
/// and the accent subtle fill when on. Name it for AccessKit with [`crate::a11y::name`].
pub fn icon_button(key: &str) -> IconButton<'_> {
    IconButton::new(Art::Key(key))
}

/// A button of the same shape holding a Glyph in the text colour: the lock and funnel
/// toggles of list headers and rows.
pub fn glyph_button(glyph: Glyph) -> IconButton<'static> {
    IconButton::new(Art::Glyph(glyph))
}

/// An Icon button around an image the caller already loaded (at its own size, and already
/// the greyscale copy when it is disabled).
pub fn icon_button_image(image: Image<'static>) -> IconButton<'static> {
    IconButton::new(Art::Image(image))
}

#[must_use = "add it with ui.add"]
pub struct IconButton<'a> {
    art: Art<'a>,
    on: bool,
    enabled: bool,
    icon: IconPx,
    side: Option<f32>,
    muted: bool,
    state: State,
}

impl<'a> IconButton<'a> {
    fn new(art: Art<'a>) -> Self {
        IconButton {
            art,
            on: false,
            enabled: true,
            icon: IconPx::Small,
            side: None,
            muted: false,
            state: State::Live,
        }
    }

    /// A Glyph in muted text: a Title bar's Glyphs while the window is in the
    /// background. An Icon keeps its role colours.
    pub fn muted(mut self, muted: bool) -> Self {
        self.muted = muted;
        self
    }

    /// On: the active Tool, a switch that is set, an open flyout.
    pub fn on(mut self, on: bool) -> Self {
        self.on = on;
        self
    }

    /// A disabled Icon button still senses the pointer, so that it can say why in a
    /// tooltip. (A disabled `Ui` disables it too, as it does every widget.)
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// The 20 px Icon of the quick strip, Tool bars, the Active Bar and Dock strips,
    /// instead of the 16 px one.
    pub fn bar(mut self) -> Self {
        self.icon = IconPx::Bar;
        self
    }

    /// An Icon size of the caller's own. Bars that are not converted yet pass theirs.
    pub fn icon_size(mut self, size: f32) -> Self {
        self.icon = IconPx::Custom(size);
        self
    }

    /// A side of the caller's own, instead of the control height.
    pub fn side(mut self, side: f32) -> Self {
        self.side = Some(side);
        self
    }

    pub fn forced(mut self, state: State) -> Self {
        self.state = state;
        self
    }
}

impl Widget for IconButton<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let t = ui.tokens();
        paint::stated(ui, self.state, |ui| {
            let side = self.side.unwrap_or(t.control_height());
            let icon = match self.icon {
                IconPx::Small => t.icon.small,
                IconPx::Bar => t.icon.bar,
                IconPx::Custom(size) => size,
            };
            let enabled = self.enabled && ui.is_enabled();
            let sense = if self.enabled {
                Sense::click()
            } else {
                Sense::hover()
            };
            let (rect, response) = ui.allocate_exact_size(Vec2::splat(side), sense);
            if !ui.is_rect_visible(rect) {
                return response;
            }
            let s = if enabled {
                paint::seen(&response, self.state)
            } else {
                paint::seen(&response, State::Rest)
            };
            let under = if self.on {
                t.color.accent.subtle
            } else {
                Color32::TRANSPARENT
            };
            // Disabled by its own flag while the `Ui` is enabled: fade it here, once.
            let fade = !self.enabled && ui.is_enabled();
            let painter = if fade {
                let mut painter = ui.painter().clone();
                painter.multiply_opacity(t.opacity.disabled);
                painter
            } else {
                ui.painter().clone()
            };
            painter.rect_filled(rect, t.radius.control, paint::state_fill(&t, under, s));
            let at = Rect::from_center_size(rect.center(), Vec2::splat(icon));
            let image = match self.art {
                Art::Key(key) => crate::icons::image(key, icon, ui.ctx().theme(), enabled),
                Art::Image(image) => Some(image),
                Art::Glyph(glyph) => {
                    let ink = if self.muted {
                        t.color.text.muted
                    } else {
                        t.color.text.primary
                    };
                    glyph.paint(&painter, rect.center(), ink);
                    None
                }
            };
            if let Some(image) = image {
                let image = if fade {
                    let alpha = (t.opacity.disabled * 255.0).round() as u8;
                    image.tint(Color32::from_white_alpha(alpha))
                } else {
                    image
                };
                image.paint_at(ui, at);
            }
            if s.focus {
                paint::focus_ring(ui, &t, rect, t.radius.control);
            }
            response
        })
    }
}

/// A link: accent text, underlined when hovered or focused.
pub fn link(text: impl Into<String>) -> Link {
    Link {
        text: text.into(),
        state: State::Live,
    }
}

#[must_use = "add it with ui.add"]
pub struct Link {
    text: String,
    state: State,
}

impl Link {
    pub fn forced(mut self, state: State) -> Self {
        self.state = state;
        self
    }
}

impl Widget for Link {
    fn ui(self, ui: &mut Ui) -> Response {
        let t = ui.tokens();
        paint::stated(ui, self.state, |ui| {
            let ink = t.color.accent.text;
            let galley = paint::galley(ui, &t.ty.body, &self.text, ink);
            let (rect, response) = ui.allocate_exact_size(galley.size(), Sense::click());
            let enabled = ui.is_enabled();
            response.widget_info(|| WidgetInfo::labeled(Role::Link, enabled, &self.text));
            if !ui.is_rect_visible(rect) {
                return response;
            }
            let s = paint::seen(&response, self.state);
            ui.painter().galley(rect.min, galley, ink);
            if s.hover || s.focus {
                let y = rect.bottom() - LINK_UNDERLINE_RISE;
                let line = Stroke::new(t.stroke.hairline, ink);
                ui.painter().hline(rect.x_range(), y, line);
            }
            if s.focus {
                let ring = rect.expand2(vec2(LINK_RING_PAD, 0.0));
                paint::focus_ring(ui, &t, ring, t.radius.mark);
            }
            if response.hovered() {
                ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
            }
            response
        })
    }
}
