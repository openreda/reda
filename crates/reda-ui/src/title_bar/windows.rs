//! The Title bar on Windows (docs/spec/v0.1/app.md#title-bar, ADR-0014).
//!
//! The window is undecorated: winit removes the frame the documented way (`WM_NCCALCSIZE`)
//! and keeps the caption, sizing and maximise styles, so the system's move and size loops,
//! Aero snap, Win+Arrow and the work-area clamp of a maximised window all still work. REDA
//! draws the window controls, the bar's empty space drags the window, and an invisible
//! strip inside the edge resizes it ([`super::resize`]), because winit answers no
//! `WM_NCHITTEST` and an undecorated window has no resize borders of its own.
//!
//! The window is not made transparent, as it is on Wayland: wgpu's Direct3D 12 surface on
//! a window handle presents opaque only, so corners REDA cut itself would show black, and
//! Windows 11 does not round a per-pixel-alpha window. Windows 11 rounds the corners of a
//! window that keeps its caption and sizing styles itself and squares them while it is
//! maximised or snapped, so REDA draws no corners here ([`Chrome::corner`] is 0).
//!
//! What winit cannot give without `unsafe` code or a fork is an accepted gap (ADR-0014):
//! no snap-layouts flyout on the maximise button (winit#3884), whose answer would be
//! `HTMAXBUTTON` from [`super::hit`].
//!
//! Everything here but [`show_window_menu`] is a pure function, compiled on every
//! platform so it is tested anywhere.

use egui::{ViewportBuilder, ViewportInfo};

use super::{Chrome, Platform, Startup};

/// Windows' decoration of REDA's windows, for drawing it on another platform (the window
/// tests). On Windows it is what [`Platform::NATIVE`] does.
pub const PLATFORM: Platform = Platform::new(chrome_in, viewport);

/// What the window around the Title bar provides in a window with the state `viewport`:
/// REDA's window controls, a bar that drags and the edge-resize strip, unless the system
/// title bar is in force. Maximised, the strip is off; in full screen the controls hide
/// and the bar neither drags nor resizes.
pub fn chrome(startup: Startup, viewport: &ViewportInfo) -> Chrome {
    if startup.system_title_bar() {
        return Chrome::SYSTEM;
    }
    let maximised = viewport.maximized == Some(true);
    let fullscreen = viewport.fullscreen == Some(true);
    Chrome {
        leading_inset: 0.0,
        controls: !fullscreen,
        drags: !fullscreen,
        maximised,
        // Windows 11 rounds the window and squares it maximised; see the module docs.
        corner: 0.0,
        resizes: !(maximised || fullscreen),
    }
}

/// [`chrome`] for the window `ctx` draws.
pub fn chrome_in(ctx: &egui::Context, startup: Startup) -> Chrome {
    ctx.input(|i| chrome(startup, i.viewport()))
}

/// The window's options before it is created: undecorated, and opaque, unless the system
/// title bar is in force.
pub fn viewport(startup: Startup, builder: ViewportBuilder) -> ViewportBuilder {
    if startup.system_title_bar() {
        return builder;
    }
    builder.with_decorations(false)
}

/// Opens the system window menu (Restore, Move, Size, Minimise, Maximise, Close) at `at`,
/// in logical pixels of the main window: winit's `GetSystemMenu` and `TrackPopupMenu`.
#[cfg(target_os = "windows")]
pub fn show_window_menu(frame: &eframe::Frame, at: egui::Pos2) {
    if let Some(window) = frame.winit_window() {
        let at = winit::dpi::LogicalPosition::new(f64::from(at.x), f64::from(at.y));
        window.show_window_menu(at);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CUSTOM: Startup = Startup {
        preference: false,
        forced: false,
    };

    fn info(maximized: bool, fullscreen: bool) -> ViewportInfo {
        ViewportInfo {
            maximized: Some(maximized),
            fullscreen: Some(fullscreen),
            ..Default::default()
        }
    }

    #[test]
    fn a_restored_window_draws_controls_drags_and_resizes_and_leaves_the_corners_to_windows() {
        let chrome = chrome(CUSTOM, &info(false, false));
        assert!(chrome.controls && chrome.drags && chrome.resizes);
        assert!(!chrome.maximised);
        assert_eq!(chrome.corner, 0.0);
        assert_eq!(chrome.leading_inset, 0.0);
    }

    #[test]
    fn a_window_whose_state_is_not_known_yet_is_restored() {
        let chrome = chrome(CUSTOM, &ViewportInfo::default());
        assert!(chrome.controls && chrome.drags && chrome.resizes && !chrome.maximised);
    }

    #[test]
    fn a_maximised_window_shows_restore_and_does_not_resize() {
        let chrome = chrome(CUSTOM, &info(true, false));
        assert!(chrome.controls && chrome.drags && chrome.maximised);
        assert!(!chrome.resizes);
        assert_eq!(chrome.corner, 0.0);
    }

    #[test]
    fn a_full_screen_window_hides_its_controls() {
        let chrome = chrome(CUSTOM, &info(false, true));
        assert!(!chrome.controls && !chrome.drags && !chrome.resizes);
    }

    #[test]
    fn the_window_is_undecorated_and_opaque() {
        let builder = viewport(CUSTOM, ViewportBuilder::default());
        assert_eq!(builder.decorations, Some(false));
        assert_eq!(builder.transparent, None);
    }

    #[test]
    fn the_system_title_bar_whether_chosen_or_forced_leaves_the_window_alone() {
        for startup in [
            Startup {
                preference: true,
                forced: false,
            },
            Startup {
                preference: false,
                forced: true,
            },
        ] {
            assert_eq!(chrome(startup, &info(false, false)), Chrome::SYSTEM);
            assert_eq!(chrome(startup, &info(true, false)), Chrome::SYSTEM);
            let builder = viewport(startup, ViewportBuilder::default());
            assert_eq!(builder.decorations, None);
            assert_eq!(builder.transparent, None);
        }
    }
}
