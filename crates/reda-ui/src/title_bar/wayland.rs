//! The Title bar on Linux and the BSDs (docs/spec/v0.1/app.md#title-bar, ADR-0014).
//!
//! Under Wayland the window asks for no server-side decoration: REDA draws the window
//! controls, the bar drags the window, an invisible strip resizes it ([`super::resize`])
//! and the window is transparent so its corners can take the window radius. A compositor
//! that draws a title bar for every window whatever the client asks (sway, Hyprland) is
//! recognised from the environment before the window is created, and the system title
//! bar is then forced on.
//!
//! winit picks Wayland when `WAYLAND_DISPLAY` (or `WAYLAND_SOCKET`) is set and X11
//! otherwise. Under X11 the window manager decorates the window, so the system title bar
//! is forced there too: the custom bar is a Wayland slice.
//!
//! Everything here but [`env`] and the window menu is a pure function of what it is given,
//! compiled on every platform so it is tested anywhere.

use egui::{ViewportBuilder, ViewportInfo};

use super::{Chrome, Platform, Startup};

/// Wayland's decoration of REDA's windows. On Linux and the BSDs it is what
/// [`Platform::NATIVE`] does; elsewhere it draws Wayland's chrome in the window tests.
pub const PLATFORM: Platform = Platform::new(
    chrome_in,
    viewport,
    crate::theme::Tokens::floating_bar_height,
    super::maximise_or_restore,
);

/// The display server winit will open the window on, and the compositor where that
/// matters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Session {
    Wayland(Compositor),
    /// X11, or no display at all.
    X11,
}

/// The Wayland compositors REDA tells apart.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Compositor {
    /// Draws the title bar of every tiled window (wlroots' `server_side` answer).
    Sway,
    /// Answers `server_side` to every window.
    Hyprland,
    /// GNOME, KDE and the rest: they leave the decorations to a client that asks.
    Other,
}

/// The session described by the environment variable lookup `var` (unset and empty
/// variables alike are `None`), following winit's own choice of backend.
pub fn session(var: impl Fn(&str) -> Option<String>) -> Session {
    let set = |name: &str| var(name).filter(|v| !v.is_empty());
    if set("WAYLAND_DISPLAY")
        .or_else(|| set("WAYLAND_SOCKET"))
        .is_none()
    {
        return Session::X11;
    }
    let desktop = set("XDG_CURRENT_DESKTOP").unwrap_or_default();
    let names = || desktop.split(':').map(str::trim);
    if set("HYPRLAND_INSTANCE_SIGNATURE").is_some()
        || names().any(|n| n.eq_ignore_ascii_case("hyprland"))
    {
        Session::Wayland(Compositor::Hyprland)
    } else if set("SWAYSOCK").is_some() || names().any(|n| n.eq_ignore_ascii_case("sway")) {
        Session::Wayland(Compositor::Sway)
    } else {
        Session::Wayland(Compositor::Other)
    }
}

/// Whether the desktop draws a title bar for every window, so the system title bar is
/// forced on whatever the Preference says.
pub fn forces_system_title_bar(session: Session) -> bool {
    !matches!(session, Session::Wayland(Compositor::Other))
}

/// The process environment, for [`session`].
pub fn env(name: &str) -> Option<String> {
    std::env::var(name).ok()
}

/// What the window around the Title bar provides in a window with the state `viewport`.
pub fn chrome(startup: Startup, viewport: &ViewportInfo, radius: f32) -> Chrome {
    if startup.system_title_bar() {
        return Chrome::SYSTEM;
    }
    let maximised = viewport.maximized.unwrap_or(false);
    let fullscreen = viewport.fullscreen.unwrap_or(false);
    let square = maximised || fullscreen;
    Chrome {
        leading_inset: 0.0,
        controls: !fullscreen,
        drags: !fullscreen,
        maximised,
        corner: if square { 0.0 } else { radius },
        resizes: !square,
    }
}

/// [`chrome`] for the window `ctx` draws, its corners at the window radius.
pub fn chrome_in(ctx: &egui::Context, startup: Startup) -> Chrome {
    use crate::theme::TokensExt as _;
    let radius = ctx.tokens().radius.window;
    ctx.input(|i| chrome(startup, i.viewport(), radius))
}

/// The window's options before it is created: no server-side decoration and a
/// transparent surface (for the round corners) unless the system title bar is in force.
pub fn viewport(startup: Startup, builder: ViewportBuilder) -> ViewportBuilder {
    if startup.system_title_bar() {
        return builder;
    }
    builder.with_decorations(false).with_transparent(true)
}

/// Asks the compositor for its window menu at `at`, in logical pixels of the main window.
/// The compositor may show none (xdg-shell gives no guarantee).
#[cfg(all(
    unix,
    not(any(target_os = "macos", target_os = "ios", target_os = "android"))
))]
pub fn show_window_menu(frame: &eframe::Frame, at: egui::Pos2) {
    if let Some(window) = frame.winit_window() {
        let at = winit::dpi::LogicalPosition::new(f64::from(at.x), f64::from(at.y));
        window.show_window_menu(at);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session_of(vars: &[(&str, &str)]) -> Session {
        let vars: Vec<(String, String)> = (vars.iter())
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        session(|name| vars.iter().find(|(k, _)| k == name).map(|(_, v)| v.clone()))
    }

    #[test]
    fn gnome_and_kde_on_wayland_take_the_custom_bar() {
        let gnome = session_of(&[
            ("WAYLAND_DISPLAY", "wayland-0"),
            ("XDG_CURRENT_DESKTOP", "GNOME"),
            ("DISPLAY", ":0"),
        ]);
        assert_eq!(gnome, Session::Wayland(Compositor::Other));
        assert!(!forces_system_title_bar(gnome));
        let kde = session_of(&[
            ("WAYLAND_DISPLAY", "wayland-0"),
            ("XDG_CURRENT_DESKTOP", "KDE"),
        ]);
        assert_eq!(kde, Session::Wayland(Compositor::Other));
        let ubuntu = session_of(&[
            ("WAYLAND_DISPLAY", "wayland-0"),
            ("XDG_CURRENT_DESKTOP", "ubuntu:GNOME"),
        ]);
        assert!(!forces_system_title_bar(ubuntu));
    }

    #[test]
    fn sway_and_hyprland_force_the_system_title_bar() {
        let by_socket = session_of(&[
            ("WAYLAND_DISPLAY", "wayland-1"),
            ("SWAYSOCK", "/run/user/1000/sway-ipc.sock"),
        ]);
        assert_eq!(by_socket, Session::Wayland(Compositor::Sway));
        assert!(forces_system_title_bar(by_socket));
        let by_desktop = session_of(&[
            ("WAYLAND_DISPLAY", "wayland-1"),
            ("XDG_CURRENT_DESKTOP", "sway"),
        ]);
        assert_eq!(by_desktop, Session::Wayland(Compositor::Sway));
        let hypr = session_of(&[
            ("WAYLAND_DISPLAY", "wayland-1"),
            ("HYPRLAND_INSTANCE_SIGNATURE", "abc_123"),
        ]);
        assert_eq!(hypr, Session::Wayland(Compositor::Hyprland));
        assert!(forces_system_title_bar(hypr));
        let hypr_desktop = session_of(&[
            ("WAYLAND_DISPLAY", "wayland-1"),
            ("XDG_CURRENT_DESKTOP", "Hyprland"),
        ]);
        assert_eq!(hypr_desktop, Session::Wayland(Compositor::Hyprland));
    }

    #[test]
    fn wayland_is_found_as_winit_finds_it() {
        // An empty WAYLAND_DISPLAY is unset; WAYLAND_SOCKET alone is Wayland.
        let empty = session_of(&[("WAYLAND_DISPLAY", ""), ("DISPLAY", ":0")]);
        assert_eq!(empty, Session::X11);
        let socket = session_of(&[("WAYLAND_SOCKET", "5")]);
        assert_eq!(socket, Session::Wayland(Compositor::Other));
        // Sway's variables left over in an X11 session are not Wayland.
        let x11 = session_of(&[("DISPLAY", ":0"), ("SWAYSOCK", "/tmp/s")]);
        assert_eq!(x11, Session::X11);
    }

    #[test]
    fn x11_keeps_the_system_title_bar() {
        assert!(forces_system_title_bar(session_of(&[("DISPLAY", ":0")])));
        assert!(forces_system_title_bar(session_of(&[])));
    }

    fn info(maximized: bool, fullscreen: bool) -> ViewportInfo {
        ViewportInfo {
            maximized: Some(maximized),
            fullscreen: Some(fullscreen),
            ..Default::default()
        }
    }

    const CUSTOM: Startup = Startup {
        preference: false,
        forced: false,
    };

    #[test]
    fn a_restored_window_draws_controls_drags_resizes_and_rounds_its_corners() {
        let chrome = chrome(CUSTOM, &info(false, false), 16.0);
        assert!(chrome.controls && chrome.drags && chrome.resizes);
        assert!(!chrome.maximised);
        assert_eq!(chrome.corner, 16.0);
        assert_eq!(chrome.leading_inset, 0.0);
    }

    #[test]
    fn a_maximised_window_is_square_and_does_not_resize() {
        let chrome = chrome(CUSTOM, &info(true, false), 16.0);
        assert!(chrome.controls && chrome.drags && chrome.maximised);
        assert!(!chrome.resizes);
        assert_eq!(chrome.corner, 0.0);
    }

    #[test]
    fn a_full_screen_window_hides_its_controls() {
        let chrome = chrome(CUSTOM, &info(false, true), 16.0);
        assert!(!chrome.controls && !chrome.drags && !chrome.resizes);
        assert_eq!(chrome.corner, 0.0);
    }

    #[test]
    fn the_system_title_bar_whether_chosen_or_forced_draws_no_window_chrome() {
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
            assert_eq!(chrome(startup, &info(false, false), 16.0), Chrome::SYSTEM);
            let builder = viewport(startup, ViewportBuilder::default());
            assert_eq!(builder.decorations, None);
            assert_eq!(builder.transparent, None);
        }
        let builder = viewport(CUSTOM, ViewportBuilder::default());
        assert_eq!(builder.decorations, Some(false));
        assert_eq!(builder.transparent, Some(true));
    }
}
