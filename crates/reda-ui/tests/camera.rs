//! The canvas camera: fitting a Document, wheel zoom at the cursor, right/middle-drag pan,
//! the View handed to the renderer, and the grid pitch.

use egui::{Pos2, Rect, pos2, vec2};
use reda_ui::canvas::{Camera, CanvasInput, grid_pitch};
use reda_ui::engine::model::geom::{Nm, Point};

const MM: f64 = 1_000_000.0;

fn rect() -> Rect {
    Rect::from_min_size(pos2(100.0, 50.0), vec2(400.0, 300.0))
}

fn close(a: (f64, f64), b: (f64, f64)) -> bool {
    (a.0 - b.0).abs() < 1e-3 && (a.1 - b.1).abs() < 1e-3
}

fn camera() -> Camera {
    Camera {
        center: (50.0 * MM, 40.0 * MM),
        nm_per_point: 0.25 * MM,
    }
}

#[test]
fn fitting_frames_the_document_in_the_canvas() {
    let lo = Point::new(Nm(0), Nm(0));
    let hi = Point::new(Nm(297_000_000), Nm(210_000_000));
    let cam = Camera::fit(lo, hi, rect().size());
    assert_eq!(cam.center, (148.5 * MM, 105.0 * MM));
    // The width limits: 297 mm across 400 points, less a margin on each side.
    let edge = cam.screen_point(rect(), (0.0, 105.0 * MM));
    assert!(
        edge.x > rect().left() && edge.x < rect().left() + 20.0,
        "{edge:?}"
    );
}

#[test]
fn the_canvas_centre_shows_the_camera_centre_and_y_grows_upwards() {
    let cam = camera();
    assert!(close(
        cam.document_point(rect(), rect().center()),
        cam.center
    ));
    // 4 points right and 4 points up is 1 mm right and 1 mm up.
    let p = rect().center() + vec2(4.0, -4.0);
    assert!(close(cam.document_point(rect(), p), (51.0 * MM, 41.0 * MM)));
    let back = cam.screen_point(rect(), (51.0 * MM, 41.0 * MM));
    assert!((back - p).length() < 1e-3);
}

#[test]
fn zooming_keeps_the_point_under_the_cursor_still() {
    let mut cam = camera();
    let cursor = pos2(130.0, 290.0);
    let under = cam.document_point(rect(), cursor);
    cam.zoom_at(rect(), cursor, 2.0);
    assert_eq!(cam.nm_per_point, 0.125 * MM);
    assert!(close(cam.document_point(rect(), cursor), under));
}

#[test]
fn zoom_stops_at_its_limits() {
    let mut cam = camera();
    for _ in 0..200 {
        cam.zoom_at(rect(), rect().center(), 10.0);
    }
    assert_eq!(cam.nm_per_point, Camera::MIN_NM_PER_POINT);
    for _ in 0..200 {
        cam.zoom_at(rect(), rect().center(), 0.1);
    }
    assert_eq!(cam.nm_per_point, Camera::MAX_NM_PER_POINT);
}

#[test]
fn panning_moves_the_document_with_the_pointer() {
    let mut cam = camera();
    let grabbed = cam.screen_point(rect(), (60.0 * MM, 30.0 * MM));
    cam.pan(vec2(12.0, -8.0));
    let now = cam.screen_point(rect(), (60.0 * MM, 30.0 * MM));
    assert!((now - (grabbed + vec2(12.0, -8.0))).length() < 1e-3);
}

#[test]
fn the_view_is_in_physical_pixels() {
    let view = camera().view(rect(), 2.0);
    assert_eq!((view.width, view.height), (800, 600));
    assert_eq!(view.nm_per_px, 0.125 * MM);
    assert_eq!(view.center, camera().center);
    assert!(!view.mirrored);
}

#[test]
fn wheel_up_zooms_in_at_the_pointer() {
    let mut cam = camera();
    let cursor = pos2(450.0, 100.0);
    let under = cam.document_point(rect(), cursor);
    let changed = cam.apply(
        rect(),
        &CanvasInput {
            pointer: Some(cursor),
            scroll: 50.0,
            ..CanvasInput::default()
        },
    );
    assert!(changed);
    assert!(cam.nm_per_point < camera().nm_per_point);
    assert!(close(cam.document_point(rect(), cursor), under));

    let mut cam = camera();
    cam.apply(
        rect(),
        &CanvasInput {
            pointer: Some(cursor),
            scroll: -50.0,
            ..CanvasInput::default()
        },
    );
    assert!(cam.nm_per_point > camera().nm_per_point);
}

#[test]
fn a_pinch_zooms_like_the_wheel() {
    let mut cam = camera();
    cam.apply(
        rect(),
        &CanvasInput {
            pointer: Some(rect().center()),
            zoom: 1.5,
            ..CanvasInput::default()
        },
    );
    assert!((cam.nm_per_point - camera().nm_per_point / 1.5).abs() < 1e-6);
}

#[test]
fn a_pan_drag_moves_the_view_and_nothing_is_a_no_op() {
    let mut cam = camera();
    let changed = cam.apply(
        rect(),
        &CanvasInput {
            pointer: Some(rect().center()),
            pan: vec2(10.0, 0.0),
            ..CanvasInput::default()
        },
    );
    assert!(changed);
    assert_eq!(cam.center.0, camera().center.0 - 10.0 * 0.25 * MM);

    let mut cam = camera();
    assert!(!cam.apply(rect(), &CanvasInput::default()));
    assert_eq!(cam, camera());
}

#[test]
fn the_wheel_does_nothing_without_the_pointer_over_the_canvas() {
    let mut cam = camera();
    let input = CanvasInput {
        pointer: None,
        scroll: 50.0,
        ..CanvasInput::default()
    };
    assert!(!cam.apply(rect(), &input));
    let outside: Pos2 = pos2(0.0, 0.0);
    let input = CanvasInput {
        pointer: Some(outside),
        scroll: 50.0,
        ..CanvasInput::default()
    };
    assert!(!cam.apply(rect(), &input));
}

#[test]
fn the_grid_coarsens_by_tens_when_its_dots_crowd() {
    let mil50 = 1_270_000.0;
    // 50 mil spans 20 px: drawn as it is.
    assert_eq!(grid_pitch(mil50, mil50 / 20.0), Some(mil50));
    // 50 mil spans 3 px: every tenth dot, 500 mil.
    assert_eq!(grid_pitch(mil50, mil50 / 3.0), Some(10.0 * mil50));
    // 50 mil spans 0.05 px: even 5000 mil is under 8 px, so no grid.
    assert_eq!(grid_pitch(mil50, mil50 / 0.05), None);
}
