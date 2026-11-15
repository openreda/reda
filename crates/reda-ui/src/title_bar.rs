//! The Title bar: the one row at the top of the main window that holds the quick strip,
//! the menus, the command field centred in the window, the Preferences gear and, where
//! REDA draws them, the window controls (docs/spec/v0.1/app.md#title-bar,
//! docs/spec/v0.1/design-language.md#title-bar, ADR-0014).
//!
//! The bar is the same on every platform. What differs is the window around it, which
//! the bar is told through [`Chrome`]: whether REDA draws the window controls, whether
//! the bar's empty stretches drag the window, and how far in from the leading edge the
//! quick strip starts (macOS's traffic lights). Each platform's file says what its window
//! gives ([`chrome_for`]) and how the window is built ([`viewport`]); a platform with no
//! slice of its own keeps the system title bar above the bar, [`Chrome::SYSTEM`].
//!
//! Where things are placed is worked out by [`arrange`], a pure function of widths, so
//! the narrow-window steps are tested without drawing.

pub mod arrange;
mod draw;
// Compiled everywhere, so its pure functions are tested on any platform; what reaches
// AppKit inside it is macOS-only.
pub mod macos;
mod window;

pub use arrange::{Arrangement, Field, Widths, arrange};
pub use draw::{FieldText, Outcome, field_text, show};
pub use window::drive;

/// The drag gap kept beside the window controls while the bar drags the window.
pub const DRAG_GAP: f32 = 48.0;

/// The width of the invisible edge-resize strip inside an undecorated window's edges,
/// on Windows and Wayland; off while the window is maximised.
pub const RESIZE_EDGE: f32 = 6.0;
/// The same strip's reach at the corners, along each of the two edges.
pub const RESIZE_CORNER: f32 = 12.0;

/// What the window around the Title bar provides, which decides what the bar draws.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Chrome {
    /// The room left at the leading end for controls the platform draws (macOS's
    /// traffic lights, 78 px). Zero elsewhere.
    pub leading_inset: f32,
    /// REDA draws minimise, maximise or restore, and close at the trailing end.
    pub controls: bool,
    /// The bar's empty stretches move the window, so a drag gap is kept beside the
    /// window controls.
    pub drags: bool,
    /// The window is maximised: the restore Glyph in place of maximise, square corners.
    pub maximised: bool,
}

impl Chrome {
    /// Under the system title bar: no window controls, no dragging, no inset.
    pub const SYSTEM: Chrome = Chrome {
        leading_inset: 0.0,
        controls: false,
        drags: false,
        maximised: false,
    };
}

/// What the window `ctx` is drawing provides around its Title bar, from how REDA
/// `startup` and the window's state now (maximised, full screen).
pub fn chrome_for(ctx: &egui::Context, startup: Startup) -> Chrome {
    #[cfg(target_os = "macos")]
    return ctx.input(|i| macos::chrome(startup, i.viewport()));
    #[allow(unreachable_code)]
    {
        let _ = (ctx, startup);
        Chrome::SYSTEM
    }
}

/// The options of a window REDA is about to open (the main window or a floating one),
/// as `startup` asks: the platform's own window for REDA's bar unless the system title
/// bar is in force.
pub fn viewport(startup: Startup, builder: egui::ViewportBuilder) -> egui::ViewportBuilder {
    #[cfg(target_os = "macos")]
    return macos::viewport(startup, builder);
    #[allow(unreachable_code)]
    {
        let _ = startup;
        builder
    }
}

/// The height of a floating window's slim bar: the native title bar's on macOS, where
/// the traffic lights stay where AppKit puts them, and `floating_bar_height()` elsewhere.
pub fn floating_bar_height(tokens: &crate::theme::Tokens) -> f32 {
    #[cfg(target_os = "macos")]
    {
        let _ = tokens;
        return macos::NATIVE_TITLE_BAR_HEIGHT;
    }
    #[allow(unreachable_code)]
    {
        tokens.floating_bar_height()
    }
}

/// What a double-click on the bar's empty space sends to a window that is `maximised`
/// or not: on macOS what System Settings asks, elsewhere maximise or restore.
pub fn double_click(maximised: bool) -> Option<egui::ViewportCommand> {
    #[cfg(target_os = "macos")]
    return macos::double_click(macos::DoubleClick::system(), maximised);
    #[allow(unreachable_code)]
    {
        Some(egui::ViewportCommand::Maximized(!maximised))
    }
}

/// Puts what the platform draws itself in the main window's bar, drawn at `bar` this
/// frame: macOS's traffic lights, centred in it. Through `frame`, which holds the main
/// window only; nothing elsewhere.
pub fn place_native_controls(
    frame: &eframe::Frame,
    ctx: &egui::Context,
    chrome: Chrome,
    bar: egui::Rect,
) {
    #[cfg(target_os = "macos")]
    macos::place_traffic_lights(frame, ctx, chrome, bar);
    #[cfg(not(target_os = "macos"))]
    let _ = (frame, ctx, chrome, bar);
}

/// How the Title bar was set up when REDA started. **Use the system title bar** applies
/// on restart, so the Appearance page compares the stored Preference with this to say
/// whether a change waits for the next start.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Startup {
    /// `general.system_title_bar` as it was read at the start.
    pub preference: bool,
    /// The desktop insists on drawing a title bar for every window (sway, Hyprland),
    /// detected before the window is created. The system title bar is then in force
    /// whatever the Preference says, and the stored value is left alone.
    pub forced: bool,
}

impl Startup {
    /// The start as the Preferences ask, with nothing forcing the system title bar.
    pub fn of(prefs: &reda_engine::preferences::Preferences) -> Startup {
        Startup {
            preference: prefs.general.system_title_bar,
            forced: false,
        }
    }

    /// The start as the Preferences ask and the desktop allows: `forced` where the
    /// platform detects a desktop that draws every window's title bar, before the window
    /// is created. macOS never forces it.
    pub fn detect(prefs: &reda_engine::preferences::Preferences) -> Startup {
        Startup::of(prefs)
    }

    /// Whether the window uses the system title bar.
    pub fn system_title_bar(self) -> bool {
        self.preference || self.forced
    }

    /// Keeps this start on `ctx`, where the Appearance page reads it.
    pub fn remember(self, ctx: &egui::Context) {
        ctx.data_mut(|d| d.insert_temp(startup_id(), self));
    }

    /// The start kept on `ctx`, if the app has kept one.
    pub fn recalled(ctx: &egui::Context) -> Option<Startup> {
        ctx.data(|d| d.get_temp(startup_id()))
    }

    /// Marks the windows drawn on `ctx` as built from this start, so REDA draws the
    /// platform's window chrome in them ([`Startup::decorating`]). Only the real app
    /// does this; a test harness's windows keep the system title bar's row.
    pub fn remember_decorating(self, ctx: &egui::Context) {
        self.remember(ctx);
        ctx.data_mut(|d| d.insert_temp(decorating_id(), true));
    }

    /// The start kept on `ctx` when its windows are built from it, for a floating
    /// window to be built and decorated like the main one.
    pub fn decorating(ctx: &egui::Context) -> Option<Startup> {
        let decorating = ctx.data(|d| d.get_temp::<bool>(decorating_id()));
        decorating.and_then(|_| Startup::recalled(ctx))
    }
}

fn startup_id() -> egui::Id {
    egui::Id::NULL.with("reda-title-bar-startup")
}

fn decorating_id() -> egui::Id {
    egui::Id::NULL.with("reda-title-bar-decorating")
}
