//! What a point in a window REDA decorates itself is, for the window around it
//! (docs/spec/v0.1/app.md#title-bar, design-language.md#title-bar): the edge-resize strip
//! with its corners, a window control, the Title bar's empty space that drags the window,
//! something on the bar, or the window's content.
//!
//! This is the answer a native frame gives the OS for each point (Windows'
//! `WM_NCHITTEST`: `HTLEFT` to `HTBOTTOMRIGHT`, `HTMINBUTTON`, `HTMAXBUTTON`, `HTCLOSE`,
//! `HTCAPTION`, `HTCLIENT`). winit answers none of it, so REDA's widgets carry each part
//! out ([`super::resize`], the window controls, [`super::drive`]); this function says what
//! they must add up to, and the window tests check the drawn bar against it.

use egui::viewport::ResizeDirection;
use egui::{Pos2, Rect};

use super::{Chrome, resize};
use crate::widgets::{WindowAction, window_control_rects};

/// What a point in the window is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hit {
    /// The edge-resize strip: a press resizes the window towards this direction.
    Resize(ResizeDirection),
    /// A window control.
    Control(WindowAction),
    /// The Title bar's empty space: a drag moves the window, a double-click maximises or
    /// restores it.
    Drag,
    /// Something on the Title bar that takes its own clicks: the quick strip, a menu, the
    /// command field, the gear. Under the system title bar, the whole bar.
    Item,
    /// The window's content below the bar.
    Client,
}

/// What `pointer` is in `window`, whose Title bar is drawn at `bar` with `chrome`, and
/// whose bar holds clickable `items` (the quick strip's buttons, the menus, the command
/// field, the gear). The resize strip wins over everything at the edge, the close button
/// included; then the window controls; then the bar's items; the rest of the bar drags.
pub fn hit(window: Rect, bar: Rect, chrome: Chrome, items: &[Rect], pointer: Pos2) -> Hit {
    if chrome.resizes
        && let Some(direction) = resize::hit(window, pointer)
    {
        return Hit::Resize(direction);
    }
    if !bar.contains(pointer) {
        return Hit::Client;
    }
    if chrome.controls {
        let control = window_control_rects(bar, chrome.maximised)
            .into_iter()
            .find(|(_, rect)| rect.contains(pointer));
        if let Some((action, _)) = control {
            return Hit::Control(action);
        }
    }
    if chrome.drags && !items.iter().any(|item| item.contains(pointer)) {
        Hit::Drag
    } else {
        Hit::Item
    }
}

#[cfg(test)]
mod tests {
    use egui::pos2;

    use super::*;
    use ResizeDirection::*;

    /// A window 1200 by 720 at the origin, its 40 px bar across the top.
    fn window() -> Rect {
        Rect::from_min_max(pos2(0.0, 0.0), pos2(1200.0, 720.0))
    }

    fn bar() -> Rect {
        Rect::from_min_max(pos2(0.0, 0.0), pos2(1200.0, 40.0))
    }

    /// The quick strip and a menu title.
    fn items() -> [Rect; 2] {
        [
            Rect::from_min_max(pos2(6.0, 8.0), pos2(100.0, 32.0)),
            Rect::from_min_max(pos2(110.0, 8.0), pos2(150.0, 32.0)),
        ]
    }

    const RESTORED: Chrome = Chrome {
        controls: true,
        drags: true,
        resizes: true,
        ..Chrome::SYSTEM
    };

    const MAXIMISED: Chrome = Chrome {
        controls: true,
        drags: true,
        maximised: true,
        ..Chrome::SYSTEM
    };

    fn at(chrome: Chrome, x: f32, y: f32) -> Hit {
        hit(window(), bar(), chrome, &items(), pos2(x, y))
    }

    #[test]
    fn the_controls_take_the_last_138_px_of_the_bar() {
        assert_eq!(
            at(RESTORED, 1080.0, 20.0),
            Hit::Control(WindowAction::Minimise)
        );
        assert_eq!(
            at(RESTORED, 1120.0, 20.0),
            Hit::Control(WindowAction::Maximise)
        );
        assert_eq!(
            at(RESTORED, 1170.0, 20.0),
            Hit::Control(WindowAction::Close)
        );
        assert_eq!(
            at(MAXIMISED, 1120.0, 20.0),
            Hit::Control(WindowAction::Restore)
        );
        // Just left of minimise is the drag gap.
        assert_eq!(at(RESTORED, 1060.0, 20.0), Hit::Drag);
        assert_eq!(at(RESTORED, 1053.0, 20.0), Hit::Drag);
    }

    #[test]
    fn the_bar_drags_where_nothing_is_on_it() {
        assert_eq!(at(RESTORED, 600.0, 20.0), Hit::Drag);
        assert_eq!(at(RESTORED, 105.0, 20.0), Hit::Drag);
        assert_eq!(at(RESTORED, 50.0, 20.0), Hit::Item);
        assert_eq!(at(RESTORED, 120.0, 20.0), Hit::Item);
        assert_eq!(at(RESTORED, 600.0, 39.0), Hit::Drag);
        assert_eq!(at(RESTORED, 600.0, 41.0), Hit::Client);
        assert_eq!(at(RESTORED, 600.0, 400.0), Hit::Client);
    }

    #[test]
    fn the_resize_strip_wins_at_the_edges_and_corners() {
        // Over the close button, the top and right edges resize.
        assert_eq!(at(RESTORED, 1199.0, 1.0), Hit::Resize(NorthEast));
        assert_eq!(at(RESTORED, 1190.0, 2.0), Hit::Resize(NorthEast));
        assert_eq!(at(RESTORED, 1197.0, 20.0), Hit::Resize(East));
        assert_eq!(at(RESTORED, 1170.0, 3.0), Hit::Resize(North));
        assert_eq!(at(RESTORED, 1170.0, 6.0), Hit::Control(WindowAction::Close));
        // Over the quick strip, the top-left corner and the left edge.
        assert_eq!(at(RESTORED, 2.0, 2.0), Hit::Resize(NorthWest));
        assert_eq!(at(RESTORED, 2.0, 11.0), Hit::Resize(NorthWest));
        assert_eq!(at(RESTORED, 2.0, 12.0), Hit::Resize(West));
        assert_eq!(at(RESTORED, 600.0, 718.0), Hit::Resize(South));
        assert_eq!(at(RESTORED, 1195.0, 715.0), Hit::Resize(SouthEast));
    }

    #[test]
    fn a_maximised_window_has_no_resize_strip() {
        assert_eq!(
            at(MAXIMISED, 1199.0, 1.0),
            Hit::Control(WindowAction::Close)
        );
        assert_eq!(at(MAXIMISED, 600.0, 1.0), Hit::Drag);
        assert_eq!(at(MAXIMISED, 1.0, 20.0), Hit::Drag);
        assert_eq!(at(MAXIMISED, 1.0, 400.0), Hit::Client);
    }

    #[test]
    fn under_the_system_title_bar_the_bar_neither_drags_nor_has_controls() {
        assert_eq!(at(Chrome::SYSTEM, 600.0, 20.0), Hit::Item);
        assert_eq!(at(Chrome::SYSTEM, 1170.0, 20.0), Hit::Item);
        assert_eq!(at(Chrome::SYSTEM, 1199.0, 1.0), Hit::Item);
        assert_eq!(at(Chrome::SYSTEM, 1199.0, 400.0), Hit::Client);
    }
}
