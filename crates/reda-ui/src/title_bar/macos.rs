//! The Title bar on macOS: content under a transparent system title bar, with AppKit's own
//! traffic lights kept and centred in REDA's bar (docs/spec/v0.1/app.md#title-bar,
//! design-language.md#title-bar, ADR-0014).
//!
//! Everything here that decides something is a pure function, so it is tested on any
//! platform: what the window gives the bar ([`chrome`]), how the window is built
//! ([`viewport`]), where the traffic lights go ([`traffic_lights`]) and what a
//! double-click does ([`double_click`], [`DoubleClick::parse`]). Only the calls that reach
//! the OS (eframe's traffic-light placement, reading the system setting) are compiled on
//! macOS alone.

use egui::{Rangef, Rect, ViewportBuilder, ViewportCommand, ViewportInfo};

use super::{Chrome, Startup};

/// The room the traffic lights take at the leading end of the bar.
pub const TRAFFIC_LIGHTS_INSET: f32 = 78.0;

/// The close button's distance from the window's left edge: the three buttons (54 px with
/// AppKit's spacing) sit in the middle of [`TRAFFIC_LIGHTS_INSET`].
pub const TRAFFIC_LIGHTS_MARGIN: f32 = 12.0;

/// AppKit's standard title-bar height, which a floating window's slim bar takes: its
/// traffic lights cannot be moved (eframe reaches the main window only), so the bar is
/// made the height they are centred in.
pub const NATIVE_TITLE_BAR_HEIGHT: f32 = 28.0;

/// What the window gives the Title bar: the traffic lights at the leading end, no
/// REDA-drawn controls, and a bar that drags, unless the system title bar is in force. In
/// full screen the traffic lights live in the system's own strip, so the inset closes up,
/// and a full-screen window cannot be moved.
pub fn chrome(startup: Startup, viewport: &ViewportInfo) -> Chrome {
    if startup.system_title_bar() {
        return Chrome::SYSTEM;
    }
    let maximised = viewport.maximized == Some(true);
    if viewport.fullscreen == Some(true) {
        return Chrome {
            maximised,
            ..Chrome::SYSTEM
        };
    }
    Chrome {
        leading_inset: TRAFFIC_LIGHTS_INSET,
        drags: true,
        maximised,
        ..Chrome::SYSTEM
    }
}

/// A window REDA is about to open: its content fills the window under a transparent title
/// bar with no title text, keeping the traffic lights. The system title bar leaves the
/// window as it is.
pub fn viewport(startup: Startup, builder: ViewportBuilder) -> ViewportBuilder {
    if startup.system_title_bar() {
        return builder;
    }
    builder
        .with_fullsize_content_view(true)
        .with_titlebar_shown(false)
        .with_title_shown(false)
}

/// Where the traffic lights go in the main window's bar.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TrafficLights {
    /// The bar's height range from the top of the window, which the buttons centre in.
    pub y: Rangef,
    /// The close button's distance from the window's left edge.
    pub left: f32,
}

/// The traffic lights' place for the bar drawn at `bar`, in the window's points: centred
/// in its height, inside the inset. None where AppKit keeps them where it likes: under
/// the system title bar and in full screen, where the bar leaves no inset.
pub fn traffic_lights(chrome: Chrome, bar: Rect) -> Option<TrafficLights> {
    if chrome.leading_inset <= 0.0 {
        return None;
    }
    Some(TrafficLights {
        y: bar.y_range(),
        left: bar.left() + TRAFFIC_LIGHTS_MARGIN,
    })
}

/// What a double-click on a window's title bar does, as set in System Settings (Desktop
/// & Dock, "Double-click a window's title bar to").
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DoubleClick {
    /// Zoom, or Fill: the window fills the screen, and a second double-click restores it.
    #[default]
    Zoom,
    Minimise,
    Nothing,
}

impl DoubleClick {
    /// The setting from what `defaults read -g AppleActionOnDoubleClick` printed (`action`),
    /// or failing that the older `AppleMiniaturizeOnDoubleClick` (`miniaturise`). Either
    /// is None when the key is unset; with neither set the system zooms.
    pub fn parse(action: Option<&str>, miniaturise: Option<&str>) -> DoubleClick {
        match action.map(str::trim) {
            Some("Minimize") => return DoubleClick::Minimise,
            Some("None") => return DoubleClick::Nothing,
            Some("Maximize" | "Fill" | "Zoom") => return DoubleClick::Zoom,
            _ => {}
        }
        match miniaturise.map(str::trim) {
            Some("1" | "YES" | "true") => DoubleClick::Minimise,
            _ => DoubleClick::Zoom,
        }
    }

    /// The setting as the user has it now. Read on each double-click, so a change in
    /// System Settings applies at once.
    #[cfg(target_os = "macos")]
    pub fn system() -> DoubleClick {
        fn read(key: &str) -> Option<String> {
            let out = std::process::Command::new("/usr/bin/defaults")
                .args(["read", "-g", key])
                .output()
                .ok()?;
            out.status
                .success()
                .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
        }
        let action = read("AppleActionOnDoubleClick");
        let miniaturise = read("AppleMiniaturizeOnDoubleClick");
        DoubleClick::parse(action.as_deref(), miniaturise.as_deref())
    }
}

/// The command a double-click on the bar's empty space sends, as `setting` asks, to a
/// window that is `maximised` (zoomed) or not.
pub fn double_click(setting: DoubleClick, maximised: bool) -> Option<ViewportCommand> {
    match setting {
        DoubleClick::Zoom => Some(ViewportCommand::Maximized(!maximised)),
        DoubleClick::Minimise => Some(ViewportCommand::Minimized(true)),
        DoubleClick::Nothing => None,
    }
}

/// Centres the main window's traffic lights in the bar drawn at `bar` this frame, through
/// eframe (which reaches the main window only). AppKit moves them back whenever it lays
/// the window out again, so this runs every frame.
#[cfg(target_os = "macos")]
pub fn place_traffic_lights(frame: &eframe::Frame, ctx: &egui::Context, chrome: Chrome, bar: Rect) {
    if let Some(at) = traffic_lights(chrome, bar) {
        frame.set_traffic_lights_position(ctx, at.y, at.left);
    }
}

#[cfg(test)]
mod tests;
