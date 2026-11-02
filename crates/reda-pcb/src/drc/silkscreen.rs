//! The silkscreen check: silkscreen over the solder-mask openings of Pads on its side.
//!
//! The silkscreen is the Footprints' silkscreen drawing and Designators, and the PCB
//! graphics and text on the silkscreen Layers. A Pad is exposed on the side its copper is
//! on (both for a Pad with a hole), over its copper grown by its solder-mask margin (none
//! when it sets none; a shrinking margin counts as none). Silkscreen that only touches an
//! opening is clear.

use std::collections::BTreeSet;

use reda_command::Fonts;
use reda_model::geom::Point;
use reda_model::{
    DrcCheck, FootprintLayer, GraphicShape, LayerKind, PadKind, PcbDocument, Shape, Side, Uuid,
};

use super::placement::{corners, polyline};
use super::{Board, Report};
use crate::copper::shape::{Core, P, Prim, Shape as Copper, arc_points};
use crate::placed::{Frame, placed_designator, placed_graphics};
use crate::text::{PlacedText, footprint_text_geometry, text_geometry};
use crate::zone::poly::{self, Area, Contour, ip};

/// One piece of silkscreen: the objects it belongs to, its side and its area.
struct Silk {
    objects: Vec<Uuid>,
    side: Side,
    area: Area,
}

/// One exposed Pad: the objects that name it, the sides it is open on and the opening.
struct Opening {
    objects: Vec<Uuid>,
    sides: Vec<Side>,
    area: Area,
}

pub(super) fn check(board: &Board<'_>, fonts: &Fonts, report: &mut Report) {
    if report.is_off(DrcCheck::SilkscreenOverPads) {
        return;
    }
    let pcb = board.pcb;
    let openings = openings(pcb);
    let mut seen = BTreeSet::new();
    for silk in silkscreen(pcb, fonts) {
        let Some(silk_box) = poly::bounds(&silk.area) else {
            continue;
        };
        for opening in &openings {
            if !opening.sides.contains(&silk.side) {
                continue;
            }
            let Some(open_box) = poly::bounds(&opening.area) else {
                continue;
            };
            if !boxes_meet(silk_box, open_box) || !poly::overlaps(&silk.area, &opening.area) {
                continue;
            }
            let mut objects = silk.objects.clone();
            for &o in &opening.objects {
                if !objects.contains(&o) {
                    objects.push(o);
                }
            }
            if seen.insert(objects.clone()) {
                report.add(DrcCheck::SilkscreenOverPads, |v| v.objects(objects));
            }
        }
    }
}

fn boxes_meet(a: (P, P), b: (P, P)) -> bool {
    a.0[0] <= b.1[0] && b.0[0] <= a.1[0] && a.0[1] <= b.1[1] && b.0[1] <= a.1[1]
}

/// Every Pad and Free Pad, as the solder-mask opening over it.
fn openings(pcb: &PcbDocument) -> Vec<Opening> {
    let both = || vec![Side::Top, Side::Bottom];
    let mut out = Vec::new();
    for instance in pcb.footprints() {
        let Some(copy) = pcb.footprint_copy(instance.footprint) else {
            continue;
        };
        let frame = Frame::of(instance);
        for pad in &copy.footprint.pads {
            let sides = match pad.kind {
                PadKind::Smd => vec![frame.side(Side::Top)],
                _ => both(),
            };
            let margin = pad.mask_margin.map_or(0, |m| m.0.max(0));
            out.push(Opening {
                objects: vec![instance.uuid, pad.uuid],
                sides,
                area: poly::union(poly::grown(&Copper::pad(instance, pad), margin)),
            });
        }
    }
    for pad in pcb.free_pads() {
        let sides = match (pad.kind, pad.side) {
            (PadKind::Smd, Some(side)) => vec![side],
            _ => both(),
        };
        out.push(Opening {
            objects: vec![pad.uuid],
            sides,
            area: poly::union(poly::grown(&Copper::free_pad(pad), 0)),
        });
    }
    out
}

/// The board side of a Footprint drawing Layer, for the silkscreen ones.
fn silk_side(layer: FootprintLayer) -> Option<Side> {
    match layer {
        FootprintLayer::SilkscreenTop => Some(Side::Top),
        FootprintLayer::SilkscreenBottom => Some(Side::Bottom),
        _ => None,
    }
}

/// Every piece of silkscreen on the board, Footprints' first.
fn silkscreen(pcb: &PcbDocument, fonts: &Fonts) -> Vec<Silk> {
    let layer_of = |side| {
        pcb.layers()
            .find(LayerKind::Silkscreen, Some(side))
            .map_or(Uuid::from_u128(0), |l| l.uuid)
    };
    let mut out = Vec::new();
    for instance in pcb.footprints() {
        for graphic in placed_graphics(pcb, instance) {
            let Some(side) = silk_side(graphic.layer) else {
                continue;
            };
            let area = match &graphic.shape {
                Shape::Text {
                    at,
                    text,
                    size,
                    rotation,
                } => text_area(&footprint_text_geometry(
                    text,
                    *at,
                    *size,
                    *rotation,
                    side == Side::Bottom,
                    layer_of(side),
                )),
                shape => drawing_area(shape, graphic.width.0, graphic.fill),
            };
            out.push(Silk {
                objects: vec![instance.uuid],
                side,
                area,
            });
        }
        if let Some(text) = placed_designator(pcb, instance).filter(|t| t.visible)
            && let Some(side) = silk_side(text.layer)
        {
            let placed = footprint_text_geometry(
                &instance.designator,
                text.at,
                text.size,
                text.rotation,
                side == Side::Bottom,
                layer_of(side),
            );
            out.push(Silk {
                objects: vec![instance.uuid],
                side,
                area: text_area(&placed),
            });
        }
    }
    let side_of = |layer: Uuid| {
        pcb.layers()
            .layer(layer)
            .filter(|l| l.kind == LayerKind::Silkscreen)
            .and_then(|l| l.side)
    };
    for graphic in pcb.graphics() {
        let Some(side) = side_of(graphic.layer) else {
            continue;
        };
        let w = graphic.width.0;
        let area = match graphic.shape {
            GraphicShape::Line { from, to } => strokes_area(&[vec![from, to]], w),
            GraphicShape::Arc { from, to, sweep } => {
                strokes_area(&[arc_points(from, to, sweep)], w)
            }
            GraphicShape::Rect { from, to, filled } => drawing_area(
                &Shape::Rect {
                    start: from,
                    end: to,
                },
                w,
                filled,
            ),
        };
        out.push(Silk {
            objects: vec![graphic.uuid],
            side,
            area,
        });
    }
    for text in pcb.texts() {
        let Some(side) = side_of(text.layer) else {
            continue;
        };
        out.push(Silk {
            objects: vec![text.uuid],
            side,
            area: text_area(&text_geometry(text, fonts)),
        });
    }
    out
}

/// The area of polylines drawn with a round pen `width` wide.
fn strokes_area(lines: &[Vec<Point>], width: i64) -> Area {
    let prims = lines
        .iter()
        .flat_map(|line| line.windows(2))
        .map(|w| Prim {
            core: Core::Seg([w[0].x.0, w[0].y.0], [w[1].x.0, w[1].y.0]),
            radius: width / 2,
        })
        .collect();
    poly::union(poly::grown(&Copper { prims }, 0))
}

/// The area of a Footprint drawing shape drawn with a round pen `width` wide, its inside
/// too when `fill` is set.
fn drawing_area(shape: &Shape, width: i64, fill: bool) -> Area {
    let Some(corners) = corners(shape) else {
        return polyline(shape).map_or_else(Vec::new, |line| strokes_area(&[line], width));
    };
    let mut ring = corners.clone();
    ring.extend(corners.first().copied());
    let mut shapes: Vec<Vec<Contour>> = Vec::new();
    if fill && corners.len() >= 3 {
        shapes.push(vec![corners.iter().map(|p| ip([p.x.0, p.y.0])).collect()]);
    }
    shapes.extend(strokes_area(&[ring], width));
    poly::union(shapes)
}

/// The area of laid-out text: its pen strokes and its filled glyphs.
fn text_area(text: &PlacedText) -> Area {
    let lines: Vec<Vec<Point>> = text.strokes.iter().map(|s| vec![s.a, s.b]).collect();
    let mut shapes: Vec<Vec<Contour>> = strokes_area(&lines, text.stroke_width.0);
    for tri in text.fill.indices.chunks_exact(3) {
        let corners: Contour = tri
            .iter()
            .map(|&i| {
                let p = text.fill.points[i as usize];
                ip([p.x.0, p.y.0])
            })
            .collect();
        shapes.push(vec![corners]);
    }
    poly::union(shapes)
}
