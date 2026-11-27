//! The wheel mapping applied to a frame's input (docs/spec/v0.3/input-and-defaults.md#wheel-mapping):
//! each scroll goes to the action the `canvas.wheel.*` table binds its modifiers to, a
//! touchpad's scrolls pan (under Touchpad Auto and Always pan), and a pinch zooms. It
//! knows nothing of a canvas: every editor's 2D canvas and the 3D view read their
//! [`WheelGesture`] from here and move their own camera by it.
//!
//! The raw wheel events are read rather than egui's smoothed delta, because egui turns
//! Ctrl+wheel into zoom and Shift+wheel into a sideways scroll on its own.

use egui::{Context, Event, Modifiers, MouseWheelSource, MouseWheelUnit, Vec2, vec2};
use reda_engine::preferences::{Touchpad, Wheel, WheelAction, WheelModifiers};

/// What a frame's wheel, touchpad and pinch input asks of a view.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WheelGesture {
    /// Zoom at the cursor, in wheel points (zoom speed and direction applied): positive
    /// zooms in.
    pub zoom: f32,
    /// A pinch's zoom factor; 1 is none.
    pub pinch: f32,
    /// How far to move the content on screen, in points (positive is right and down).
    pub pan: Vec2,
    /// Copper Layers to step: positive is the next one (the wheel turned towards the
    /// user), negative the previous. Only the PCB editor acts on it.
    pub layer_steps: i32,
}

impl Default for WheelGesture {
    fn default() -> WheelGesture {
        WheelGesture {
            zoom: 0.0,
            pinch: 1.0,
            pan: Vec2::ZERO,
            layer_steps: 0,
        }
    }
}

impl WheelGesture {
    /// Whether it asks for anything.
    pub fn is_some(&self) -> bool {
        *self != WheelGesture::default()
    }
}

/// What a view keeps between frames: the part of a Layer step a smooth scroll has made.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WheelState {
    layer_lines: f32,
}

/// Reads this frame's wheel input from egui with the mapping `prefs`.
pub fn read(ctx: &Context, prefs: &Wheel, state: &mut WheelState) -> WheelGesture {
    let line_points = ctx.options(|o| o.input_options.line_scroll_speed);
    ctx.input(|input| {
        let touch = input.multi_touch().map(|t| t.zoom_delta);
        let page_points = input.viewport_rect().height();
        gesture(&input.events, touch, prefs, line_points, page_points, state)
    })
}

/// The gesture of `events` under `prefs`: a wheel line is `line_points` points and a page
/// `page_points`. `touch_zoom` is a touch screen's pinch, which egui measures from the
/// touches; without one, the window system's pinch events count.
pub fn gesture(
    events: &[Event],
    touch_zoom: Option<f32>,
    prefs: &Wheel,
    line_points: f32,
    page_points: f32,
    state: &mut WheelState,
) -> WheelGesture {
    let mut out = WheelGesture::default();
    let mut pinch = 1.0;
    let mut layer_lines = 0.0;
    for event in events {
        match event {
            Event::Zoom(factor) => pinch *= factor,
            Event::MouseWheel {
                unit,
                delta,
                source,
                modifiers,
                ..
            } => {
                let delta = match unit {
                    MouseWheelUnit::Point => *delta,
                    MouseWheelUnit::Line => *delta * line_points,
                    MouseWheelUnit::Page => *delta * page_points,
                };
                if pans(prefs.touchpad, *source) {
                    out.pan += delta;
                    continue;
                }
                let amount = delta.x + delta.y;
                match prefs.action_for(held(modifiers)) {
                    Some(WheelAction::Zoom) => {
                        let direction = if prefs.reverse_zoom { -1.0 } else { 1.0 };
                        out.zoom += amount * prefs.zoom_speed as f32 * direction;
                    }
                    Some(WheelAction::ScrollX) => out.pan += vec2(amount, 0.0),
                    Some(WheelAction::ScrollY) => out.pan += vec2(0.0, amount),
                    Some(WheelAction::StepLayer) => layer_lines += amount / line_points,
                    None => {}
                }
            }
            _ => {}
        }
    }
    out.pinch = touch_zoom.unwrap_or(pinch);
    if layer_lines != 0.0 {
        state.layer_lines += layer_lines;
        let whole = state.layer_lines.trunc();
        state.layer_lines -= whole;
        // The wheel turned away (up) is the previous Layer.
        out.layer_steps = -(whole as i32);
    }
    out
}

/// Whether a scroll pans the view as a touchpad's does rather than following the table.
fn pans(mode: Touchpad, source: MouseWheelSource) -> bool {
    match mode {
        Touchpad::Pan => true,
        Touchpad::Wheel => false,
        Touchpad::Auto => is_touchpad(source),
    }
}

/// Whether the system reports the scroll as a touchpad's. A scroll of unknown source
/// follows the table as a wheel's does, even with precise (point) deltas: egui-winit
/// names the source on macOS only, and a delta's unit is no report of where it came from
/// (winit on Wayland gives points for any scroll without a wheel click).
fn is_touchpad(source: MouseWheelSource) -> bool {
    match source {
        MouseWheelSource::Trackpad | MouseWheelSource::Momentum => true,
        MouseWheelSource::Wheel | MouseWheelSource::Unknown => false,
    }
}

/// The table's combination of what egui reports held: Cmd counts as Ctrl.
fn held(modifiers: &Modifiers) -> WheelModifiers {
    WheelModifiers {
        ctrl: modifiers.ctrl || modifiers.command || modifiers.mac_cmd,
        alt: modifiers.alt,
        shift: modifiers.shift,
    }
}
