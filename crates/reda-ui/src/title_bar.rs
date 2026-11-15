//! The Title bar: the one row at the top of the main window that holds the quick strip,
//! the menus, the command field centred in the window, the Preferences gear and, where
//! REDA draws them, the window controls (docs/spec/v0.1/app.md#title-bar,
//! docs/spec/v0.1/design-language.md#title-bar, ADR-0014).
//!
//! The bar is the same on every platform. What differs is the window around it, which
//! the bar is told through [`Chrome`]: whether REDA draws the window controls, whether
//! the bar's empty stretches drag the window, and how far in from the leading edge the
//! quick strip starts (macOS's traffic lights). Until the platform slices land, every
//! window keeps the system title bar above the bar, so the app draws it with
//! [`Chrome::SYSTEM`].
//!
//! Where things are placed is worked out by [`arrange`], a pure function of widths, so
//! the narrow-window steps are tested without drawing.

pub mod arrange;
mod draw;

pub use arrange::{Arrangement, Field, Widths, arrange};
pub use draw::{FieldText, Outcome, field_text, show};

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
