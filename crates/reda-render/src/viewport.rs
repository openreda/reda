//! What part of a Scene an image shows, and at what pixel size.

use reda_model::Uuid;
use reda_model::geom::Point;

use crate::color::{ColorKey, Palette, Rgba, SchematicColor};
use crate::scene::{Item, SceneLayer};

/// A rectangle of the Document (nm, y up) shown in an image `width` x `height` pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Viewport {
    pub min: Point,
    pub max: Point,
    pub width: u32,
    pub height: u32,
}

impl Viewport {
    /// The rectangle `min`..`max` at `long_side` pixels on its longer side, keeping its
    /// aspect ratio (at least one pixel each way).
    pub fn fit(min: Point, max: Point, long_side: u32) -> Viewport {
        let w = (max.x - min.x).0.max(1) as u128;
        let h = (max.y - min.y).0.max(1) as u128;
        let long = u128::from(long_side.max(1));
        let scale = |short: u128, long_len: u128| {
            u32::try_from(((short * long + long_len / 2) / long_len).max(1)).unwrap_or(u32::MAX)
        };
        let (width, height) = if w >= h {
            (long_side.max(1), scale(h, w))
        } else {
            (scale(w, h), long_side.max(1))
        };
        Viewport {
            min,
            max,
            width,
            height,
        }
    }
}

/// Dimmed ink is blended this far (in 1/256) toward the paper (or the canvas, on a board).
const DIM: u32 = 180;

/// The colour `item` is drawn in: its palette colour, or, while anything is highlighted,
/// dimmed toward the paper unless it is highlighted itself. The paper is never dimmed.
pub(crate) fn ink(item: &Item, palette: &Palette, highlight: &[Uuid]) -> Rgba {
    let color = palette.color(item.color);
    let paper_key = ColorKey::Schematic(SchematicColor::Paper);
    if highlight.is_empty() || item.color == paper_key || highlight.contains(&item.object) {
        return color;
    }
    // A board has no paper: it dims toward the canvas around it.
    let paper = match item.layer {
        SceneLayer::Schematic => palette.color(paper_key),
        _ => palette.background(),
    };
    let mut out = color.0;
    for (c, p) in out.iter_mut().zip(paper.0).take(3) {
        *c = ((u32::from(*c) * (256 - DIM) + u32::from(p) * DIM) / 256) as u8;
    }
    Rgba(out)
}
