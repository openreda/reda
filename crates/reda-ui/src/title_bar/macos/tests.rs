use egui::{Rect, ViewportBuilder, ViewportCommand, ViewportInfo, pos2};

use super::*;

const CUSTOM: Startup = Startup {
    preference: false,
    forced: false,
};
const CHOSEN: Startup = Startup {
    preference: true,
    forced: false,
};

fn window(maximized: bool, fullscreen: bool) -> ViewportInfo {
    ViewportInfo {
        maximized: Some(maximized),
        fullscreen: Some(fullscreen),
        ..ViewportInfo::default()
    }
}

#[test]
fn a_restored_window_leaves_the_traffic_lights_78_px_and_drags_with_no_controls() {
    let chrome = chrome(CUSTOM, &window(false, false));
    assert_eq!(chrome.leading_inset, 78.0);
    assert!(chrome.drags);
    assert!(!chrome.controls);
    assert!(!chrome.maximised);
}

#[test]
fn a_zoomed_window_keeps_the_inset_and_reports_itself_maximised() {
    let chrome = chrome(CUSTOM, &window(true, false));
    assert_eq!(chrome.leading_inset, 78.0);
    assert!(chrome.drags);
    assert!(chrome.maximised);
}

#[test]
fn an_unknown_window_state_is_a_restored_window() {
    let chrome = chrome(CUSTOM, &ViewportInfo::default());
    assert_eq!(chrome.leading_inset, 78.0);
    assert!(chrome.drags);
    assert!(!chrome.maximised);
}

#[test]
fn full_screen_closes_up_the_inset_and_stops_dragging() {
    let chrome = chrome(CUSTOM, &window(false, true));
    assert_eq!(chrome.leading_inset, 0.0);
    assert!(!chrome.drags);
    assert!(!chrome.controls);
}

#[test]
fn the_system_title_bar_gives_the_plain_row_whether_chosen_or_forced() {
    let forced = Startup {
        preference: false,
        forced: true,
    };
    for startup in [CHOSEN, forced] {
        assert_eq!(chrome(startup, &window(false, false)), Chrome::SYSTEM);
        assert_eq!(chrome(startup, &window(true, true)), Chrome::SYSTEM);
    }
}

#[test]
fn macos_draws_nothing_else_of_the_window() {
    // AppKit rounds, shadows and resizes the window: every other field stays at the
    // system title bar's value.
    let expected = Chrome {
        leading_inset: 78.0,
        drags: true,
        ..Chrome::SYSTEM
    };
    assert_eq!(chrome(CUSTOM, &window(false, false)), expected);
}

#[test]
fn the_window_is_built_with_content_under_a_transparent_title_bar() {
    let built = viewport(CUSTOM, ViewportBuilder::default().with_title("REDA"));
    assert_eq!(built.fullsize_content_view, Some(true));
    assert_eq!(built.titlebar_shown, Some(false));
    assert_eq!(built.title_shown, Some(false));
    // The traffic lights stay, and the window keeps its frame: no borderless window.
    assert_eq!(built.titlebar_buttons_shown, None);
    assert_eq!(built.decorations, None);
    assert_eq!(built.title.as_deref(), Some("REDA"));
}

#[test]
fn the_system_title_bar_leaves_the_window_as_it_was_built() {
    let built = viewport(CHOSEN, ViewportBuilder::default());
    assert_eq!(built, ViewportBuilder::default());
}

#[test]
fn the_traffic_lights_centre_in_the_bar_inside_the_inset() {
    let bar = Rect::from_min_max(pos2(0.0, 0.0), pos2(1280.0, 40.0));
    let at = traffic_lights(chrome(CUSTOM, &window(false, false)), bar).unwrap();
    assert_eq!(at.y, egui::Rangef::new(0.0, 40.0));
    assert_eq!(at.left, 12.0);
    // Close, minimise and zoom are 14 px with 6 px between them: they end inside 78.
    assert!(at.left + 3.0 * 14.0 + 2.0 * 6.0 <= TRAFFIC_LIGHTS_INSET - TRAFFIC_LIGHTS_MARGIN);
}

#[test]
fn the_traffic_lights_follow_a_compact_bar() {
    let bar = Rect::from_min_max(pos2(0.0, 0.0), pos2(900.0, 36.0));
    let at = traffic_lights(chrome(CUSTOM, &window(false, false)), bar).unwrap();
    assert_eq!(at.y, egui::Rangef::new(0.0, 36.0));
}

#[test]
fn appkit_keeps_the_traffic_lights_in_full_screen_and_under_the_system_title_bar() {
    let bar = Rect::from_min_max(pos2(0.0, 0.0), pos2(1280.0, 40.0));
    assert_eq!(
        traffic_lights(chrome(CUSTOM, &window(false, true)), bar),
        None
    );
    assert_eq!(
        traffic_lights(chrome(CHOSEN, &window(false, false)), bar),
        None
    );
}

#[test]
fn the_double_click_setting_is_read_from_its_defaults_value() {
    assert_eq!(
        DoubleClick::parse(Some("Maximize\n"), None),
        DoubleClick::Zoom
    );
    assert_eq!(DoubleClick::parse(Some("Fill\n"), None), DoubleClick::Zoom);
    assert_eq!(
        DoubleClick::parse(Some("Minimize\n"), None),
        DoubleClick::Minimise
    );
    assert_eq!(
        DoubleClick::parse(Some("None\n"), None),
        DoubleClick::Nothing
    );
}

#[test]
fn the_older_minimise_key_counts_when_the_newer_is_unset() {
    assert_eq!(DoubleClick::parse(None, Some("1\n")), DoubleClick::Minimise);
    assert_eq!(DoubleClick::parse(None, Some("0\n")), DoubleClick::Zoom);
    // The newer key wins when both are set.
    assert_eq!(
        DoubleClick::parse(Some("None"), Some("1")),
        DoubleClick::Nothing
    );
}

#[test]
fn with_nothing_set_a_double_click_zooms() {
    assert_eq!(DoubleClick::parse(None, None), DoubleClick::Zoom);
    assert_eq!(DoubleClick::parse(Some("Bogus"), None), DoubleClick::Zoom);
}

#[test]
fn a_double_click_zooms_restores_minimises_or_does_nothing() {
    assert_eq!(
        double_click(DoubleClick::Zoom, false),
        Some(ViewportCommand::Maximized(true))
    );
    assert_eq!(
        double_click(DoubleClick::Zoom, true),
        Some(ViewportCommand::Maximized(false))
    );
    assert_eq!(
        double_click(DoubleClick::Minimise, false),
        Some(ViewportCommand::Minimized(true))
    );
    assert_eq!(double_click(DoubleClick::Nothing, true), None);
}
