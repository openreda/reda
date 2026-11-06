//! Where a placed Footprint's parts land on the board.
//!
//! A Footprint is drawn in its own frame. Placing it mirrors the frame left-right when it
//! is on the bottom side, turns it counter-clockwise by its rotation and moves its origin
//! to its position. Flipping a Footprint to the bottom therefore mirrors its Pads and its
//! drawing, and moves every sided Layer (copper, silkscreen, courtyard, assembly) to the
//! other face. The canvas, DRC and the manufacturing outputs all read placed geometry from
//! here.

pub use reda_model::geom::Bounds;
use reda_model::geom::{CornerRect, Millideg, Nm, Point};
use reda_model::{
    FootprintInstance, FootprintLayer, FootprintText, FreePad, Hole, PadKind, PadShape,
    PcbDocument, Shape, Side, Uuid, ZoneConnection,
};

/// `p` turned counter-clockwise by `angle` about `about`. Quarter turns are exact; other
/// angles round to the nearest nanometre.
pub fn rotate(p: Point, about: Point, angle: Millideg) -> Point {
    p.rotated_about(about, angle)
}

/// Whether `angle` is a whole number of quarter turns.
pub fn is_quarter_turn(angle: Millideg) -> bool {
    angle.0 % 90_000 == 0
}

/// A Footprint's frame on the board: mirror (bottom side), turn, then move.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Frame {
    pub at: Point,
    pub rotation: Millideg,
    pub side: Side,
}

impl Frame {
    pub fn of(instance: &FootprintInstance) -> Frame {
        Frame {
            at: instance.at,
            rotation: instance.rotation,
            side: instance.side,
        }
    }

    fn bottom(&self) -> bool {
        self.side == Side::Bottom
    }

    /// A point of the Footprint, on the board.
    pub fn point(&self, local: Point) -> Point {
        let local = match self.bottom() {
            true => Point::new(-local.x, local.y),
            false => local,
        };
        rotate(local + self.at, self.at, self.rotation)
    }

    /// The Footprint point that lands on board point `board`.
    pub fn local(&self, board: Point) -> Point {
        let p = rotate(board, self.at, -self.rotation) - self.at;
        match self.bottom() {
            true => Point::new(-p.x, p.y),
            false => p,
        }
    }

    /// A direction of the Footprint, on the board, in `[0, 360°)`.
    pub fn angle(&self, local: Millideg) -> Millideg {
        let local = if self.bottom() { -local } else { local };
        (local + self.rotation).normalized()
    }

    /// The Footprint direction that lands on board direction `board`.
    pub fn local_angle(&self, board: Millideg) -> Millideg {
        let a = board - self.rotation;
        (if self.bottom() { -a } else { a }).normalized()
    }

    /// The Layer a Footprint drawing Layer lands on.
    pub fn layer(&self, layer: FootprintLayer) -> FootprintLayer {
        if !self.bottom() {
            return layer;
        }
        use FootprintLayer::*;
        match layer {
            SilkscreenTop => SilkscreenBottom,
            SilkscreenBottom => SilkscreenTop,
            CourtyardTop => CourtyardBottom,
            CourtyardBottom => CourtyardTop,
            AssemblyTop => AssemblyBottom,
            AssemblyBottom => AssemblyTop,
        }
    }

    /// The board side a Footprint side lands on.
    pub fn side(&self, local: Side) -> Side {
        match (self.bottom(), local) {
            (false, s) => s,
            (true, Side::Top) => Side::Bottom,
            (true, Side::Bottom) => Side::Top,
        }
    }

    /// A Footprint shape, on the board. A rectangle that ends up turned off the axes
    /// becomes a polygon through its corners.
    pub fn shape(&self, shape: &Shape) -> Shape {
        let p = |q: &Point| self.point(*q);
        match shape {
            Shape::Line { start, end } => Shape::Line {
                start: p(start),
                end: p(end),
            },
            Shape::Rect { start, end } if is_quarter_turn(self.rotation) => Shape::Rect {
                start: p(start),
                end: p(end),
            },
            Shape::Rect { start, end } => Shape::Polygon {
                points: [
                    *start,
                    Point::new(end.x, start.y),
                    *end,
                    Point::new(start.x, end.y),
                ]
                .iter()
                .map(p)
                .collect(),
            },
            Shape::Arc {
                center,
                start,
                sweep,
            } => Shape::Arc {
                center: p(center),
                start: p(start),
                sweep: if self.bottom() { -*sweep } else { *sweep },
            },
            Shape::Circle { center, radius } => Shape::Circle {
                center: p(center),
                radius: *radius,
            },
            Shape::Polygon { points } => Shape::Polygon {
                points: points.iter().map(p).collect(),
            },
            Shape::Text {
                at,
                text,
                size,
                rotation,
            } => Shape::Text {
                at: p(at),
                text: text.clone(),
                size: *size,
                rotation: self.angle(*rotation),
            },
        }
    }

    /// A Footprint text, on the board.
    pub fn text(&self, text: &FootprintText) -> FootprintText {
        FootprintText {
            at: self.point(text.at),
            rotation: self.angle(text.rotation),
            layer: self.layer(text.layer),
            ..text.clone()
        }
    }
}

/// A Pad of a placed Footprint (or a Free Pad), on the board.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlacedPad {
    /// The Footprint copy's Pad, or the Free Pad.
    pub pad: Uuid,
    pub number: String,
    pub kind: PadKind,
    pub shape: PadShape,
    /// The anchor on the board: the hole sits there and Tracks connect there.
    pub at: Point,
    /// The turn of the Pad's frame on the board.
    pub rotation: Millideg,
    /// The copper outline in the Pad's frame on the board, about `at`: mirrored when its
    /// Footprint is on the bottom.
    pub outline: CornerRect,
    /// The hole, its slot turned in the Pad's frame on the board.
    pub hole: Option<Hole>,
    /// The copper side of an SMD Pad; `None` for a Pad with a hole, which is on both.
    pub side: Option<Side>,
    pub net: Option<Uuid>,
    /// The Pad's own solder-mask margin, overriding the board's; Free Pads have none.
    pub mask_margin: Option<Nm>,
    /// The Pad's own solder-paste margin, overriding the board's; Free Pads have none.
    pub paste_margin: Option<Nm>,
    pub zone_connection: Option<ZoneConnection>,
}

impl PlacedPad {
    /// A point of the Pad's frame, on the board.
    pub fn point(&self, local: Point) -> Point {
        rotate(local + self.at, self.at, self.rotation)
    }

    /// The centre of the copper on the board.
    pub fn copper_center(&self) -> Point {
        self.point(self.outline.center)
    }

    /// A box around the Pad's copper.
    pub fn bounds(&self) -> Bounds {
        outline_bounds(self.at, self.rotation, &self.outline)
    }

    /// The ends of the hole's centre line on the board, both at `at` for a round hole.
    pub fn hole_ends(&self) -> Option<(Point, Point)> {
        let (a, b) = self.hole?.ends();
        Some((self.point(a), self.point(b)))
    }
}

/// The corners of an outline of a Pad anchored at `at` and turned by `rotation`, boxed.
fn outline_bounds(at: Point, rotation: Millideg, outline: &CornerRect) -> Bounds {
    let corners = outline.box_corners().map(|p| rotate(p + at, at, rotation));
    Bounds::of_points(corners).expect("four corners")
}

/// The Pads of a placed Footprint, in the copy's order, with their Nets.
pub fn placed_pads(pcb: &PcbDocument, instance: &FootprintInstance) -> Vec<PlacedPad> {
    let Some(copy) = pcb.footprint_copy(instance.footprint) else {
        return Vec::new();
    };
    let frame = Frame::of(instance);
    copy.footprint
        .pads
        .iter()
        .map(|pad| {
            let bottom = frame.side == Side::Bottom;
            let outline = pad.outline();
            let hole = pad.hole.map(|h| Hole {
                rotation: if bottom { -h.rotation } else { h.rotation },
                ..h
            });
            PlacedPad {
                pad: pad.uuid,
                number: pad.number.clone(),
                kind: pad.kind,
                shape: pad.shape,
                at: frame.point(pad.at),
                rotation: frame.angle(pad.rotation),
                outline: if bottom { outline.mirrored() } else { outline },
                hole,
                side: pad.copper_side().map(|s| frame.side(s)),
                net: instance.pad_net(pad.uuid),
                mask_margin: pad.mask_margin,
                paste_margin: pad.paste_margin,
                zone_connection: pad.zone_connection,
            }
        })
        .collect()
}

/// A Free Pad as a placed Pad.
pub fn placed_free_pad(pad: &FreePad) -> PlacedPad {
    PlacedPad {
        pad: pad.uuid,
        number: String::new(),
        kind: pad.kind,
        shape: pad.shape,
        at: pad.at,
        rotation: pad.rotation,
        outline: pad.outline(),
        hole: pad.hole,
        side: pad.form().copper_side(),
        net: pad.net,
        mask_margin: None,
        paste_margin: None,
        zone_connection: pad.zone_connection,
    }
}

/// One drawing shape of a placed Footprint, on the board.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlacedGraphic {
    /// The Footprint copy's graphic.
    pub graphic: Uuid,
    pub layer: FootprintLayer,
    pub shape: Shape,
    pub width: Nm,
    pub fill: bool,
}

/// The silkscreen, courtyard and assembly drawing of a placed Footprint.
pub fn placed_graphics(pcb: &PcbDocument, instance: &FootprintInstance) -> Vec<PlacedGraphic> {
    let Some(copy) = pcb.footprint_copy(instance.footprint) else {
        return Vec::new();
    };
    let frame = Frame::of(instance);
    copy.footprint
        .graphics
        .iter()
        .map(|g| PlacedGraphic {
            graphic: g.uuid,
            layer: frame.layer(g.layer),
            shape: frame.shape(&g.shape),
            width: g.width,
            fill: g.fill,
        })
        .collect()
}

/// The Designator text of a Footprint relative to its origin: the instance's own when it
/// has been moved, else the Footprint's [`designator_text`](reda_model::Footprint::designator_text).
pub fn designator_text(pcb: &PcbDocument, instance: &FootprintInstance) -> Option<FootprintText> {
    instance.designator_text.clone().or_else(|| {
        pcb.footprint_copy(instance.footprint)
            .and_then(|c| c.footprint.designator_text().cloned())
    })
}

/// Every text a placed Footprint shows, relative to its origin: the Footprint copy's
/// `texts`, with its Designator text replaced by the instance's own when it has been moved.
pub fn instance_texts(pcb: &PcbDocument, instance: &FootprintInstance) -> Vec<FootprintText> {
    let mut texts = pcb
        .footprint_copy(instance.footprint)
        .map(|c| c.footprint.texts.clone())
        .unwrap_or_default();
    if let Some(moved) = &instance.designator_text {
        let primary = pcb
            .footprint_copy(instance.footprint)
            .and_then(|c| c.footprint.designator_text())
            .map(|t| t.uuid);
        texts.retain(|t| Some(t.uuid) != primary);
        texts.push(moved.clone());
    }
    texts
}

/// Where a Footprint's Designator text is drawn on the board.
pub fn placed_designator(pcb: &PcbDocument, instance: &FootprintInstance) -> Option<FootprintText> {
    designator_text(pcb, instance).map(|t| Frame::of(instance).text(&t))
}

/// A box around a placed Footprint's Pads and drawing (its origin when it has neither),
/// used to align and arrange Footprints.
pub fn footprint_bounds(pcb: &PcbDocument, instance: &FootprintInstance) -> Bounds {
    let pads = placed_pads(pcb, instance).into_iter().map(|p| p.bounds());
    let graphics = placed_graphics(pcb, instance)
        .into_iter()
        .filter_map(|g| shape_bounds(&g.shape));
    pads.chain(graphics)
        .reduce(Bounds::union)
        .unwrap_or(Bounds::of_point(instance.at))
}

/// A box around a Free Pad's copper (or hole).
pub fn free_pad_bounds(pad: &FreePad) -> Bounds {
    outline_bounds(pad.at, pad.rotation, &pad.outline())
}

/// A box around a shape's points (arcs and circles by their full circle).
pub(crate) fn shape_bounds(shape: &Shape) -> Option<Bounds> {
    match shape {
        Shape::Line { start, end } | Shape::Rect { start, end } => {
            Bounds::of_points([*start, *end])
        }
        Shape::Arc { center, start, .. } => {
            let (dx, dy) = ((start.x - center.x).0 as f64, (start.y - center.y).0 as f64);
            Some(Bounds::around(*center, Nm(dx.hypot(dy).ceil() as i64)))
        }
        Shape::Circle { center, radius } => Some(Bounds::around(*center, *radius)),
        Shape::Polygon { points } => Bounds::of_points(points.iter().copied()),
        Shape::Text { at, .. } => Some(Bounds::of_point(*at)),
    }
}
