//! The silkscreen checks: `silk_to_mask`, silkscreen too close to exposed copper or to
//! the solder-mask openings on its side, and `silk_to_edge`, silkscreen too close to the
//! Board Outline or a Board Cutout.
//!
//! The silkscreen is the Footprints' silkscreen drawing and Designators, and the PCB
//! graphics and text on the silkscreen Layers. The openings are the [resolved
//! ones](crate::openings): a Pad is open on the side its copper is on (both for a Pad with
//! a hole), over its copper grown by its mask expansion. In `exposed_copper` mode the
//! silkscreen keeps its distance from the copper inside each opening; in `mask_opening`
//! mode from the opening itself.

use std::collections::BTreeSet;

use reda_command::Fonts;
use reda_model::geom::{Nm, Point};
use reda_model::{
    DrcCheck, FootprintLayer, GraphicShape, LayerKind, Limit, PcbDocument, RuleField, RuleKind,
    Shape, Side, SilkMaskMode, Uuid,
};

use super::edge::Edge;
use super::placement::{corners, polyline};
use super::{Board, Report};
use crate::copper::shape::{Core, Prim, Shape as Copper, arc_points};
use crate::placed::{PlacedPad, placed_graphics};
use crate::rules::resolve;
use crate::text::{PlacedText, footprint_texts, text_geometry};
use crate::zone::poly::{self, Area, Contour, ip};

/// One piece of silkscreen: the objects it belongs to, its side and its area.
pub(super) struct Silk {
    pub objects: Vec<Uuid>,
    pub side: Side,
    pub area: Area,
}

/// What silkscreen keeps clear of on one side: the objects that name it and its area.
struct Target {
    objects: Vec<Uuid>,
    side: Side,
    area: Area,
}

pub(super) fn check(board: &Board<'_>, fonts: &Fonts, report: &mut Report) {
    if report.is_off(DrcCheck::SilkToMask) && report.is_off(DrcCheck::SilkToEdge) {
        return;
    }
    let silk = silkscreen(board.pcb, fonts);
    if !report.is_off(DrcCheck::SilkToMask) {
        silk_to_mask(board, &silk, report);
    }
    if !report.is_off(DrcCheck::SilkToEdge) {
        silk_to_edge(board, &silk, report);
    }
}

fn silk_layer(pcb: &PcbDocument, side: Side) -> Uuid {
    pcb.layers()
        .find(LayerKind::Silkscreen, Some(side))
        .expect("silk is on a board Layer")
        .uuid
}

fn silk_to_mask(board: &Board<'_>, silk: &[Silk], report: &mut Report) {
    let pcb = board.pcb;
    let Some(rule) = resolve(pcb, None, RuleKind::SilkToMask, RuleField::Value) else {
        return;
    };
    let required = rule.value.length().expect("a length");
    let mode = resolve(pcb, None, RuleKind::SilkToMask, RuleField::Mode)
        .and_then(|m| m.value.mode())
        .unwrap_or_default();
    let targets = targets(pcb, mode);
    let mut seen = BTreeSet::new();
    for piece in silk {
        let Some(silk_box) = poly::bounds(&piece.area) else {
            continue;
        };
        for target in targets.iter().filter(|t| t.side == piece.side) {
            let Some(target_box) = poly::bounds(&target.area) else {
                continue;
            };
            if !poly::boxes_within(silk_box, target_box, required.0) {
                continue;
            }
            let Some((gap, [a, b], overlaps)) = poly::gap(&piece.area, &target.area) else {
                continue;
            };
            if !overlaps && gap >= required.0 as f64 {
                continue;
            }
            let mut objects = piece.objects.clone();
            for &o in &target.objects {
                if !objects.contains(&o) {
                    objects.push(o);
                }
            }
            if !seen.insert(objects.clone()) {
                continue;
            }
            let actual = Nm(gap.floor() as i64);
            report.add(DrcCheck::SilkToMask, |v| {
                v.length(actual, required, Limit::Min, rule.rule)
                    .distance_points(a, b)
                    .on_layers([silk_layer(pcb, piece.side)])
                    .arg("required", required)
                    .arg("actual", actual)
                    .arg(
                        "mode",
                        match mode {
                            SilkMaskMode::ExposedCopper => "exposed_copper",
                            SilkMaskMode::MaskOpening => "mask_opening",
                        },
                    )
                    .objects(objects)
            });
        }
    }
}

fn silk_to_edge(board: &Board<'_>, silk: &[Silk], report: &mut Report) {
    let pcb = board.pcb;
    let Some(rule) = resolve(pcb, None, RuleKind::SilkToEdge, RuleField::Value) else {
        return;
    };
    let required = rule.value.length().expect("a length");
    let edges = Edge::all(pcb);
    for piece in silk {
        let Some(first) = piece.area.first().and_then(|shape| shape.first()) else {
            continue;
        };
        let Some(anchor) = first.first().map(|p| [p.x, p.y]) else {
            continue;
        };
        let outline = poly::boundary(&piece.area);
        for edge in &edges {
            let (gap, [a, b]) = edge.gap(&outline);
            let wrong_side = edge.contains(anchor) != edge.keeps_inside;
            let actual = if wrong_side {
                Nm::ZERO
            } else if gap < required.0 as f64 {
                Nm(gap.max(0.0).floor() as i64)
            } else {
                continue;
            };
            report.add(DrcCheck::SilkToEdge, |v| {
                v.length(actual, required, Limit::Min, rule.rule)
                    .distance_points(a, b)
                    .on_layers([silk_layer(pcb, piece.side)])
                    .arg("required", required)
                    .arg("actual", actual)
                    .objects(piece.objects.iter().copied())
                    .object(edge.uuid)
            });
        }
    }
}

/// What silkscreen keeps clear of in `mode`: each resolved mask opening, or the copper
/// inside it.
fn targets(pcb: &PcbDocument, mode: SilkMaskMode) -> Vec<Target> {
    crate::openings::resolve(pcb)
        .into_iter()
        .filter(|o| o.kind == crate::openings::Kind::Mask)
        .filter_map(|o| {
            let opening = pad_area(o.geometry.as_ref()?);
            let area = match mode {
                SilkMaskMode::MaskOpening => opening,
                SilkMaskMode::ExposedCopper => poly::intersection(&opening, &pad_area(&o.copper)),
            };
            (!area.is_empty()).then_some(Target {
                objects: o.objects,
                side: o.side,
                area,
            })
        })
        .collect()
}

/// The area of a placed Pad's outline.
fn pad_area(pad: &PlacedPad) -> Area {
    let contour = pad
        .outline
        .polygon(Nm(1_000))
        .into_iter()
        .map(|p| {
            let p = pad.point(p);
            ip([p.x.0, p.y.0])
        })
        .collect();
    poly::polygon(contour)
}

/// The board side of a Footprint drawing Layer, for the silkscreen ones.
fn silk_side(layer: FootprintLayer) -> Option<Side> {
    match layer {
        FootprintLayer::SilkscreenTop => Some(Side::Top),
        FootprintLayer::SilkscreenBottom => Some(Side::Bottom),
        _ => None,
    }
}

/// Every piece of silkscreen on the board, Footprints' first. Footprint text is laid out
/// by [`footprint_texts`], as the canvas and the Gerber silkscreen lay it out.
pub(super) fn silkscreen(pcb: &PcbDocument, fonts: &Fonts) -> Vec<Silk> {
    let side_of = |layer: Uuid| {
        pcb.layers()
            .layer(layer)
            .filter(|l| l.kind == LayerKind::Silkscreen)
            .and_then(|l| l.side)
    };
    let mut out = Vec::new();
    for instance in pcb.footprints() {
        for graphic in placed_graphics(pcb, instance) {
            if matches!(graphic.shape, Shape::Text { .. }) {
                continue;
            }
            let Some(side) = silk_side(graphic.layer) else {
                continue;
            };
            out.push(Silk {
                objects: vec![instance.uuid],
                side,
                area: drawing_area(&graphic.shape, graphic.width.0, graphic.fill),
            });
        }
        for text in footprint_texts(pcb, instance, fonts) {
            let Some(side) = side_of(text.layer) else {
                continue;
            };
            out.push(Silk {
                objects: vec![instance.uuid],
                side,
                area: text_area(&text_geometry(&text, fonts)),
            });
        }
    }
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
