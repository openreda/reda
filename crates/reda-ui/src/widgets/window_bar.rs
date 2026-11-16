//! The window's own controls and the slim bar of a floating window
//! (docs/spec/v0.1/design-language.md#title-bar).
//!
//! The window controls are minimise, maximise or restore, and close, at the trailing end
//! of a bar on Windows and Linux: each 46 px wide and the full height of the bar, flat,
//! with a 1.5 px Glyph. Hover is `fill.hover`, except close, whose hover is
//! `color.close_hover` with a white Glyph. While the window is in the background the
//! Glyphs are muted.
//!
//! These widgets only say what was clicked ([`WindowAction`]); the window that places
//! them sends the matching [`egui::ViewportCommand`] ([`WindowAction::command`]).

use egui::{
    Align2, Color32, CornerRadius, Rect, Response, Role, Sense, Ui, Widget, WidgetInfo, vec2,
};

use super::glyph::Glyph;
use super::paint::{self, State};
use crate::theme::TokensExt as _;
use crate::tr;

/// The width of one window control.
pub const WINDOW_CONTROL: f32 = 46.0;

/// The Glyph on a close button that is hovered or pressed, on `color.close_hover`, in
/// both themes.
const ON_CLOSE: Color32 = Color32::WHITE;

/// What a window control asks of its window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowAction {
    Minimise,
    Maximise,
    Restore,
    Close,
}

impl WindowAction {
    /// The command that carries it out on the window the control is in.
    pub fn command(self) -> egui::ViewportCommand {
        match self {
            WindowAction::Minimise => egui::ViewportCommand::Minimized(true),
            WindowAction::Maximise => egui::ViewportCommand::Maximized(true),
            WindowAction::Restore => egui::ViewportCommand::Maximized(false),
            WindowAction::Close => egui::ViewportCommand::Close,
        }
    }

    fn glyph(self) -> Glyph {
        match self {
            WindowAction::Minimise => Glyph::Minimise,
            WindowAction::Maximise => Glyph::Maximise,
            WindowAction::Restore => Glyph::Restore,
            WindowAction::Close => Glyph::Close,
        }
    }

    /// The accessible name: "Minimise", "Maximise", "Restore", "Close".
    pub fn name(self) -> String {
        match self {
            WindowAction::Minimise => tr!("window-minimise"),
            WindowAction::Maximise => tr!("window-maximise"),
            WindowAction::Restore => tr!("window-restore"),
            WindowAction::Close => tr!("window-close"),
        }
    }
}

/// One window control, as tall as the Title bar unless told otherwise.
pub fn window_control(action: WindowAction) -> WindowControl {
    WindowControl {
        action,
        height: None,
        window_focused: None,
        corner: 0.0,
        state: State::Live,
    }
}

#[must_use = "add it with ui.add"]
pub struct WindowControl {
    action: WindowAction,
    height: Option<f32>,
    window_focused: Option<bool>,
    corner: f32,
    state: State,
}

impl WindowControl {
    /// The height of the bar it sits in.
    pub fn height(mut self, height: f32) -> Self {
        self.height = Some(height);
        self
    }

    /// Whether REDA's window is in front; behind, the Glyph is muted. Unset, it follows
    /// the window.
    pub fn window_focused(mut self, focused: bool) -> Self {
        self.window_focused = Some(focused);
        self
    }

    /// The window's corner radius at the control's top trailing corner: the close
    /// button's hover follows a rounded window's corner.
    pub fn corner(mut self, radius: f32) -> Self {
        self.corner = radius;
        self
    }

    pub fn forced(mut self, state: State) -> Self {
        self.state = state;
        self
    }
}

impl Widget for WindowControl {
    fn ui(self, ui: &mut Ui) -> Response {
        let t = ui.tokens();
        paint::stated(ui, self.state, |ui| {
            let c = &t.color;
            let height = self.height.unwrap_or_else(|| t.title_bar_height());
            let (rect, response) =
                ui.allocate_exact_size(vec2(WINDOW_CONTROL, height), Sense::click());
            let enabled = ui.is_enabled();
            let name = self.action.name();
            response.widget_info(|| WidgetInfo::labeled(Role::Button, enabled, &name));
            if !ui.is_rect_visible(rect) {
                return response;
            }
            let s = paint::seen(&response, self.state);
            let close = self.action == WindowAction::Close;
            let lit = s.hover || s.press;
            let fill = if close && lit {
                c.close_hover
            } else {
                paint::state_fill(&t, Color32::TRANSPARENT, s)
            };
            let radius = CornerRadius {
                ne: self.corner.round() as u8,
                ..CornerRadius::ZERO
            };
            ui.painter().rect_filled(rect, radius, fill);
            let focused = self
                .window_focused
                .unwrap_or_else(|| ui.input(|i| i.focused));
            let ink = if close && lit {
                ON_CLOSE
            } else if focused {
                c.text.primary
            } else {
                c.text.muted
            };
            self.action.glyph().paint(ui.painter(), rect.center(), ink);
            if s.focus {
                let ring = rect.shrink(t.stroke.focus_ring + paint::RING_GAP);
                paint::focus_ring(ui, &t, ring, t.radius.control);
            }
            response
        })
    }
}

/// The three window controls side by side: minimise, maximise (restore while
/// `maximised`), close.
pub fn window_controls(maximised: bool) -> WindowControls {
    WindowControls {
        maximised,
        height: None,
        window_focused: None,
        corner: 0.0,
        states: [State::Live; 3],
    }
}

#[must_use = "call .show()"]
pub struct WindowControls {
    maximised: bool,
    height: Option<f32>,
    window_focused: Option<bool>,
    corner: f32,
    states: [State; 3],
}

impl WindowControls {
    /// The height of the bar they sit in.
    pub fn height(mut self, height: f32) -> Self {
        self.height = Some(height);
        self
    }

    pub fn window_focused(mut self, focused: bool) -> Self {
        self.window_focused = Some(focused);
        self
    }

    /// The window's corner radius, which the close button's hover follows. Zero while
    /// the window is maximised.
    pub fn corner(mut self, radius: f32) -> Self {
        self.corner = radius;
        self
    }

    /// Forces each control's state, in order, for the gallery.
    pub fn forced(mut self, states: [State; 3]) -> Self {
        self.states = states;
        self
    }

    /// The width the three take.
    pub fn width() -> f32 {
        3.0 * WINDOW_CONTROL
    }

    /// Draws them left to right from the cursor; returns what was clicked.
    pub fn show(self, ui: &mut Ui) -> Option<WindowAction> {
        let middle = if self.maximised {
            WindowAction::Restore
        } else {
            WindowAction::Maximise
        };
        let actions = [WindowAction::Minimise, middle, WindowAction::Close];
        let mut clicked = None;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            for (action, state) in actions.into_iter().zip(self.states) {
                let mut control = window_control(action).forced(state);
                if let Some(height) = self.height {
                    control = control.height(height);
                }
                if let Some(focused) = self.window_focused {
                    control = control.window_focused(focused);
                }
                if action == WindowAction::Close {
                    control = control.corner(self.corner);
                }
                if ui.add(control).clicked() {
                    clicked = Some(action);
                }
            }
        });
        clicked
    }
}

/// The slim bar of a floating window: on the base, the window's `title` at the leading
/// end in body text, drag space, and the window controls at the trailing end.
pub fn floating_bar(title: impl Into<String>) -> FloatingBar {
    FloatingBar {
        title: title.into(),
        leading_inset: 0.0,
        height: None,
        controls: true,
        drags: false,
        maximised: false,
        corner: None,
        window_focused: None,
        states: [State::Live; 3],
    }
}

#[must_use = "call .show()"]
pub struct FloatingBar {
    title: String,
    leading_inset: f32,
    height: Option<f32>,
    controls: bool,
    drags: bool,
    maximised: bool,
    corner: Option<f32>,
    window_focused: Option<bool>,
    states: [State; 3],
}

/// What a floating bar reports.
pub struct FloatingBarResponse {
    /// The bar's empty stretch. It senses drags and double-clicks while the bar
    /// [`drags`](FloatingBar::drags) the window.
    pub response: Response,
    /// The window control clicked, if any.
    pub action: Option<WindowAction>,
}

impl FloatingBar {
    /// Room left at the leading end for controls the platform draws (macOS's traffic
    /// lights).
    pub fn leading_inset(mut self, inset: f32) -> Self {
        self.leading_inset = inset;
        self
    }

    /// The bar's height; unset, `floating_bar_height()`. On macOS the bar takes the
    /// native title-bar height, which the traffic lights are centred in.
    pub fn height(mut self, height: f32) -> Self {
        self.height = Some(height);
        self
    }

    /// Whether REDA draws the window controls (not on macOS).
    pub fn controls(mut self, controls: bool) -> Self {
        self.controls = controls;
        self
    }

    /// The empty stretch senses drags, for moving the window.
    pub fn drags(mut self, drags: bool) -> Self {
        self.drags = drags;
        self
    }

    pub fn maximised(mut self, maximised: bool) -> Self {
        self.maximised = maximised;
        self
    }

    /// The window's corner radius; unset, `radius.window` unless maximised.
    pub fn corner(mut self, radius: f32) -> Self {
        self.corner = Some(radius);
        self
    }

    pub fn window_focused(mut self, focused: bool) -> Self {
        self.window_focused = Some(focused);
        self
    }

    /// Forces the window controls' states, in order, for the gallery.
    pub fn forced(mut self, states: [State; 3]) -> Self {
        self.states = states;
        self
    }

    /// Draws the bar across the width available at the cursor.
    pub fn show(self, ui: &mut Ui) -> FloatingBarResponse {
        let t = ui.tokens();
        let height = self.height.unwrap_or_else(|| t.floating_bar_height());
        let sense = if self.drags {
            Sense::click_and_drag()
        } else {
            Sense::hover()
        };
        let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), height), sense);
        let corner = self
            .corner
            .unwrap_or(if self.maximised { 0.0 } else { t.radius.window });
        let top = CornerRadius {
            nw: corner.round() as u8,
            ne: corner.round() as u8,
            ..CornerRadius::ZERO
        };
        ui.painter().rect_filled(rect, top, t.color.surface.base);
        let focused = self
            .window_focused
            .unwrap_or_else(|| ui.input(|i| i.focused));
        let mut action = None;
        let mut text_end = rect.right() - t.space.control_x;
        if self.controls {
            let at = Rect::from_min_max(
                rect.right_top() - vec2(WindowControls::width(), 0.0),
                rect.max,
            );
            text_end = at.left();
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(at));
            action = window_controls(self.maximised)
                .height(height)
                .window_focused(focused)
                .corner(corner)
                .forced(self.states)
                .show(&mut child);
        }
        let ink = if focused {
            t.color.text.primary
        } else {
            t.color.text.muted
        };
        let left = rect.left() + self.leading_inset.max(t.space.control_x);
        let room = Rect::from_x_y_ranges(left..=text_end, rect.y_range());
        if paint::width(ui, &t.ty.body, &self.title) <= room.width() {
            paint::text_in(ui, room, Align2::LEFT_CENTER, &t.ty.body, &self.title, ink);
        } else {
            paint::text_clipped(ui, room, &t.ty.body, &self.title, ink);
        }
        let title = self.title.clone();
        response.widget_info(|| WidgetInfo::labeled(Role::TitleBar, true, &title));
        FloatingBarResponse { response, action }
    }
}
