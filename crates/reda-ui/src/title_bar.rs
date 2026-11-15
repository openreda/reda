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
pub mod resize;
#[cfg(all(
    unix,
    not(any(target_os = "macos", target_os = "ios", target_os = "android"))
))]
pub mod wayland;
mod window;

pub use arrange::{Arrangement, Field, Widths, arrange};
pub use draw::{FieldText, Outcome, field_text, show};
pub use window::{drive, edge};

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
    /// The radius of the window's corners where REDA rounds the window itself (a
    /// transparent, undecorated window): the bar rounds its top corners to it. Zero
    /// where the system draws the frame or the window is maximised or full screen.
    pub corner: f32,
    /// The window has no resize borders of its own, so REDA keeps the invisible
    /// edge-resize strip inside its edge ([`resize`]). Off while maximised.
    pub resizes: bool,
}

impl Chrome {
    /// Under the system title bar: no window controls, no dragging, no inset.
    pub const SYSTEM: Chrome = Chrome {
        leading_inset: 0.0,
        controls: false,
        drags: false,
        maximised: false,
        corner: 0.0,
        resizes: false,
    };
}

/// What the window `ctx` is drawing provides around its Title bar, from how REDA
/// `startup` and the window's state now (maximised, full screen).
pub fn chrome_for(ctx: &egui::Context, startup: Startup) -> Chrome {
    #[cfg(all(
        unix,
        not(any(target_os = "macos", target_os = "ios", target_os = "android"))
    ))]
    {
        use crate::theme::TokensExt as _;
        let radius = ctx.tokens().radius.window;
        return ctx.input(|i| wayland::chrome(startup, i.viewport(), radius));
    }
    #[allow(unreachable_code)]
    {
        let _ = (ctx, startup);
        Chrome::SYSTEM
    }
}

/// The options of a window REDA is about to open (the main window or a floating one),
/// as `startup` asks: the platform's undecorated window unless the system title bar is
/// in force.
pub fn viewport(startup: Startup, builder: egui::ViewportBuilder) -> egui::ViewportBuilder {
    #[cfg(all(
        unix,
        not(any(target_os = "macos", target_os = "ios", target_os = "android"))
    ))]
    return wayland::viewport(startup, builder);
    #[allow(unreachable_code)]
    {
        let _ = startup;
        builder
    }
}

/// Shows the system window menu at `at` (logical pixels in the main window), where the
/// platform has one that REDA can reach: only the main window's, through eframe.
pub fn show_window_menu(frame: &eframe::Frame, at: egui::Pos2) {
    #[cfg(all(
        unix,
        not(any(target_os = "macos", target_os = "ios", target_os = "android"))
    ))]
    wayland::show_window_menu(frame, at);
    #[cfg(not(all(
        unix,
        not(any(target_os = "macos", target_os = "ios", target_os = "android"))
    )))]
    let _ = (frame, at);
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
    /// is created.
    pub fn detect(prefs: &reda_engine::preferences::Preferences) -> Startup {
        #[allow(unused_mut)]
        let mut startup = Startup::of(prefs);
        #[cfg(all(
            unix,
            not(any(target_os = "macos", target_os = "ios", target_os = "android"))
        ))]
        {
            startup.forced = wayland::forces_system_title_bar(wayland::session(wayland::env));
        }
        startup
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
}

fn startup_id() -> egui::Id {
    egui::Id::NULL.with("reda-title-bar-startup")
}

impl Startup {
    /// Keeps on `ctx` that the main window was built as this start asks, so the floating
    /// windows it opens are built and decorated the same way ([`Startup::window`]).
    pub fn remember_window(self, ctx: &egui::Context) {
        ctx.data_mut(|d| d.insert_temp(window_id(), self));
    }

    /// The start the main window was built with, if the app built it as the platform
    /// does; `None` for a window drawn under the system title bar whatever the start
    /// (a test harness).
    pub fn window(ctx: &egui::Context) -> Option<Startup> {
        ctx.data(|d| d.get_temp(window_id()))
    }
}

fn window_id() -> egui::Id {
    egui::Id::NULL.with("reda-title-bar-window")
}
