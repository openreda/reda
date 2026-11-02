//! What part of a Document a frame shows, and at what size.

use reda_model::geom::Point;

use crate::viewport::Viewport;

/// A view of a Document: the Document point at the centre of the image, the zoom, and the
/// image size in pixels. Document y grows upwards; the image's grows downwards.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View {
    /// The Document point (nm) at the centre of the image.
    pub center: (f64, f64),
    /// Nanometres per pixel; larger is further out.
    pub nm_per_px: f64,
    /// Image width in pixels.
    pub width: u32,
    /// Image height in pixels.
    pub height: u32,
    /// Mirrors left to right: the PCB view from the bottom.
    pub mirrored: bool,
}

impl View {
    /// Frames the box `lo`..`hi` in an image `width` × `height` pixels, leaving `margin`
    /// (a fraction of the image, such as 0.02) clear on every side.
    pub fn fit_into(lo: Point, hi: Point, width: u32, height: u32, margin: f64) -> View {
        let (w, h) = ((hi.x.0 - lo.x.0) as f64, (hi.y.0 - lo.y.0) as f64);
        let usable = 1.0 - 2.0 * margin;
        let zoom_x = w / (f64::from(width.max(1)) * usable);
        let zoom_y = h / (f64::from(height.max(1)) * usable);
        View {
            center: (
                (lo.x.0 as f64 + hi.x.0 as f64) / 2.0,
                (lo.y.0 as f64 + hi.y.0 as f64) / 2.0,
            ),
            nm_per_px: zoom_x.max(zoom_y).max(1.0),
            width,
            height,
            mirrored: false,
        }
    }

    /// The view that shows exactly `viewport`'s rectangle at its pixel size.
    pub fn of(viewport: &Viewport) -> View {
        View::fit_into(
            viewport.min,
            viewport.max,
            viewport.width,
            viewport.height,
            0.0,
        )
    }

    /// The Document point under the pixel centre `(px, py)`, counted from the image's top
    /// left corner.
    pub fn document_point(&self, px: f64, py: f64) -> (f64, f64) {
        let dx = (px - f64::from(self.width) / 2.0) * self.nm_per_px;
        let dy = (f64::from(self.height) / 2.0 - py) * self.nm_per_px;
        let dx = if self.mirrored { -dx } else { dx };
        (self.center.0 + dx, self.center.1 + dy)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reda_model::geom::Nm;

    fn p(x: i64, y: i64) -> Point {
        Point::new(Nm(x), Nm(y))
    }

    #[test]
    fn a_viewport_maps_to_the_view_of_its_rectangle() {
        let v = View::of(&Viewport::fit(p(0, 0), p(297_000_000, 210_000_000), 1024));
        assert_eq!((v.width, v.height), (1024, 724));
        assert_eq!(v.center, (148_500_000.0, 105_000_000.0));
    }

    #[test]
    fn the_top_left_pixel_maps_to_the_boxs_top_left_corner() {
        let v = View::fit_into(p(0, 0), p(1000, 1000), 10, 10, 0.0);
        assert_eq!(v.document_point(0.0, 0.0), (0.0, 1000.0));
        assert_eq!(v.document_point(10.0, 10.0), (1000.0, 0.0));
    }
}
