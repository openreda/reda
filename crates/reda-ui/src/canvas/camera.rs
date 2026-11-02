//! The canvas camera: which Document point sits at the canvas centre, and the zoom.
//! Screen positions are egui points (y down); Document coordinates are nm (y up).

use egui::{Pos2, Rect, Vec2, pos2};
use reda_engine::model::geom::Point;
use reda_engine::render::View;

/// The share of the canvas left clear on each side when fitting a Document.
const FIT_MARGIN: f64 = 0.03;
/// How strongly the wheel zooms: one 50-point wheel notch zooms by about 1.22×.
const WHEEL_ZOOM_PER_POINT: f64 = 0.004;

/// A canvas's view of its Document.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    /// The Document point (nm) at the canvas centre.
    pub center: (f64, f64),
    /// Nanometres per egui point; larger is further out.
    pub nm_per_point: f64,
}

/// What the user did to a canvas in one frame, as far as the camera cares.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CanvasInput {
    /// The pointer position, if it is over the canvas.
    pub pointer: Option<Pos2>,
    /// Wheel scroll in points; positive (up) zooms in.
    pub scroll: f32,
    /// A pinch or Ctrl+wheel zoom factor; 0 or 1 means none.
    pub zoom: f32,
    /// How far a right- or middle-button drag moved this frame, in points.
    pub pan: Vec2,
}

impl Camera {
    /// The closest zoom: 10 nm per point.
    pub const MIN_NM_PER_POINT: f64 = 10.0;
    /// The furthest zoom: 10 m across a 1000-point canvas.
    pub const MAX_NM_PER_POINT: f64 = 10_000_000.0;

    /// Frames the box `lo`..`hi` in a canvas of `size` points.
    pub fn fit(lo: Point, hi: Point, size: Vec2) -> Camera {
        let (w, h) = ((hi.x.0 - lo.x.0) as f64, (hi.y.0 - lo.y.0) as f64);
        let usable = 1.0 - 2.0 * FIT_MARGIN;
        let zoom_x = w / (f64::from(size.x.max(1.0)) * usable);
        let zoom_y = h / (f64::from(size.y.max(1.0)) * usable);
        Camera {
            center: (
                (lo.x.0 as f64 + hi.x.0 as f64) / 2.0,
                (lo.y.0 as f64 + hi.y.0 as f64) / 2.0,
            ),
            nm_per_point: zoom_x
                .max(zoom_y)
                .clamp(Self::MIN_NM_PER_POINT, Self::MAX_NM_PER_POINT),
        }
    }

    /// The Document point under the screen position `pos` of a canvas in `rect`.
    pub fn document_point(&self, rect: Rect, pos: Pos2) -> (f64, f64) {
        let d = pos - rect.center();
        (
            self.center.0 + f64::from(d.x) * self.nm_per_point,
            self.center.1 - f64::from(d.y) * self.nm_per_point,
        )
    }

    /// Where the Document point `p` shows on a canvas in `rect`.
    pub fn screen_point(&self, rect: Rect, p: (f64, f64)) -> Pos2 {
        let c = rect.center();
        pos2(
            c.x + ((p.0 - self.center.0) / self.nm_per_point) as f32,
            c.y - ((p.1 - self.center.1) / self.nm_per_point) as f32,
        )
    }

    /// Zooms in by `factor` (below 1 zooms out), keeping the Document point under `pos`
    /// where it is on screen.
    pub fn zoom_at(&mut self, rect: Rect, pos: Pos2, factor: f64) {
        let before = self.document_point(rect, pos);
        self.nm_per_point =
            (self.nm_per_point / factor).clamp(Self::MIN_NM_PER_POINT, Self::MAX_NM_PER_POINT);
        let after = self.document_point(rect, pos);
        self.center.0 += before.0 - after.0;
        self.center.1 += before.1 - after.1;
    }

    /// Moves the Document by `delta` points on screen, as when dragging it.
    pub fn pan(&mut self, delta: Vec2) {
        self.center.0 -= f64::from(delta.x) * self.nm_per_point;
        self.center.1 += f64::from(delta.y) * self.nm_per_point;
    }

    /// Applies one frame's input; returns whether the camera moved.
    pub fn apply(&mut self, rect: Rect, input: &CanvasInput) -> bool {
        let before = *self;
        if input.pan != Vec2::ZERO {
            self.pan(input.pan);
        }
        if let Some(pointer) = input.pointer.filter(|p| rect.contains(*p)) {
            let mut factor = (f64::from(input.scroll) * WHEEL_ZOOM_PER_POINT).exp();
            if input.zoom > 0.0 {
                factor *= f64::from(input.zoom);
            }
            if factor != 1.0 {
                self.zoom_at(rect, pointer, factor);
            }
        }
        *self != before
    }

    /// The renderer's view of a canvas in `rect` on a screen with `pixels_per_point`.
    pub fn view(&self, rect: Rect, pixels_per_point: f32) -> View {
        let ppp = f64::from(pixels_per_point);
        // Rounded edge by edge, as egui sizes the callback's viewport, so the offscreen
        // frame matches it pixel for pixel.
        let span = |lo: f32, hi: f32| {
            ((f64::from(hi) * ppp).round() - (f64::from(lo) * ppp).round()).max(1.0) as u32
        };
        View {
            center: self.center,
            nm_per_px: self.nm_per_point / ppp,
            width: span(rect.left(), rect.right()),
            height: span(rect.top(), rect.bottom()),
            mirrored: false,
        }
    }
}

/// The smallest gap between grid dots, in pixels, before the grid coarsens.
const MIN_GRID_PX: f64 = 8.0;
/// How many times the grid may coarsen by ten before it is hidden.
const MAX_COARSENING: u32 = 2;

/// The pitch (nm) to draw a `base` grid at when one pixel is `nm_per_px`: `base`, or ten
/// or a hundred times it when the dots would crowd closer than 8 px; `None` hides the grid.
pub fn grid_pitch(base: f64, nm_per_px: f64) -> Option<f64> {
    let mut pitch = base;
    for _ in 0..=MAX_COARSENING {
        if pitch / nm_per_px >= MIN_GRID_PX {
            return Some(pitch);
        }
        pitch *= 10.0;
    }
    None
}
