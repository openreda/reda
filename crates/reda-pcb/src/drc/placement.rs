//! The placement checks: Footprints outside the Board Outline and courtyards closer than
//! `courtyard_clearance`.
//!
//! A Footprint's extent is the area of its Pads and its courtyard. A courtyard is the area
//! its courtyard drawing encloses: rectangles, polygons and circles, and closed chains of
//! lines and arcs; open chains enclose nothing. A Footprint with no courtyard uses a
//! fallback box around its copper, silkscreen and assembly drawing, ignoring text.

use crate::copper::shape::BoardShape as _;
use std::collections::BTreeMap;

use reda_model::geom::{Millideg, Nm, Point};
use reda_model::{
    DrcCheck, FootprintInstance, FootprintLayer, LayerKind, Limit, PcbDocument, Restriction,
    RuleField, RuleKind, Shape, Side,
};

use super::{Board, Report};
use crate::copper::shape::{Shape as Copper, arc_points};
use crate::placed::{placed_graphics, rotate};
use crate::rules::resolve;
use crate::zone::poly::{self, Area, Contour, ip};

pub(super) fn check(board: &Board<'_>, report: &mut Report) {
    outside_board(board, report);
    courtyard_overlap(board, report);
}

fn outside_board(board: &Board<'_>, report: &mut Report) {
    if report.is_off(DrcCheck::FootprintOutsideBoard) {
        return;
    }
    let pcb = board.pcb;
    let Some(outline) = pcb.outline().filter(|o| o.vertices.len() >= 3) else {
        return;
    };
    let inside = poly::polygon(poly::contour(&outline.vertices));
    for instance in pcb.footprints() {
        let extent = extent(pcb, instance);
        if poly::difference(&extent, &inside).is_empty() {
            continue;
        }
        report.add(DrcCheck::FootprintOutsideBoard, |v| {
            v.arg("designator", instance.designator.clone())
                .object(instance.uuid)
                .object(outline.uuid)
        });
    }
}

/// Footprints whose courtyards on one side come closer than `courtyard_clearance`.
/// Courtyards that exactly touch pass at 0; overlapping ones never do.
fn courtyard_overlap(board: &Board<'_>, report: &mut Report) {
    if report.is_off(DrcCheck::CourtyardOverlap) {
        return;
    }
    let pcb = board.pcb;
    let (required, rule) = match resolve(pcb, None, RuleKind::CourtyardClearance, RuleField::Value)
    {
        Some(s) => (s.value.length().expect("a length"), s.rule),
        None => (Nm::ZERO, pcb.board_rule(RuleKind::CourtyardClearance).uuid),
    };
    let courtyards: Vec<(&FootprintInstance, BTreeMap<Side, Area>)> = pcb
        .footprints()
        .map(|f| (f, courtyards_or_box(pcb, f)))
        .filter(|(_, c)| !c.is_empty())
        .collect();
    for (i, (a, a_yards)) in courtyards.iter().enumerate() {
        for (b, b_yards) in &courtyards[i + 1..] {
            let closest = a_yards
                .iter()
                .filter_map(|(side, area)| {
                    let other = b_yards.get(side)?;
                    let (ba, bb) = (poly::bounds(area)?, poly::bounds(other)?);
                    if !poly::boxes_within(ba, bb, required.0) {
                        return None;
                    }
                    let (gap, points, overlaps) = poly::gap(area, other)?;
                    (overlaps || gap < required.0 as f64).then_some((gap, points, *side))
                })
                .min_by(|x, y| x.0.total_cmp(&y.0));
            let Some((gap, [pa, pb], side)) = closest else {
                continue;
            };
            let actual = Nm(gap.floor() as i64);
            report.add(DrcCheck::CourtyardOverlap, |v| {
                v.length(actual, required, Limit::Min, rule)
                    .distance_points(pa, pb)
                    .on_layers(
                        pcb.layers()
                            .find(LayerKind::Courtyard, Some(side))
                            .map(|l| l.uuid),
                    )
                    .arg("designator_a", a.designator.clone())
                    .arg("designator_b", b.designator.clone())
                    .arg("required", required)
                    .arg("actual", actual)
                    .object(a.uuid)
                    .object(b.uuid)
            });
        }
    }
}

/// Footprints whose Courtyard (or fallback box) on a Keepout's side overlaps a Keepout
/// that restricts `footprints`, with no clearance: touching passes. A Keepout on a top or
/// bottom copper Layer checks that side, one on every Layer both sides, and one on an
/// inner Layer neither.
pub(super) fn footprints_in_keepouts(board: &Board<'_>, report: &mut Report) {
    let pcb = board.pcb;
    let keepouts: Vec<_> = pcb
        .keepouts()
        .filter(|k| k.restricts(Restriction::Footprints))
        .collect();
    if keepouts.is_empty() {
        return;
    }
    let rule = pcb.board_rule(RuleKind::Clearance).uuid;
    for instance in pcb.footprints() {
        let yards = courtyards_or_box(pcb, instance);
        for keepout in &keepouts {
            let sides: Vec<Side> = match keepout.layer {
                None => vec![Side::Top, Side::Bottom],
                Some(layer) => pcb
                    .layers()
                    .layer(layer)
                    .and_then(|l| l.side)
                    .into_iter()
                    .collect(),
            };
            let area = crate::keepout::area(keepout);
            let hit = sides.iter().find_map(|side| {
                let yard = yards.get(side)?;
                let (_, points, overlaps) = poly::gap(&area, yard)?;
                overlaps.then_some((*side, points))
            });
            let Some((side, [pa, pb])) = hit else {
                continue;
            };
            report.add(DrcCheck::Keepout, |v| {
                v.length(Nm::ZERO, Nm::ZERO, Limit::Min, rule)
                    .distance_points(pa, pb)
                    .arg("designator", instance.designator.clone())
                    .object(instance.uuid)
                    .object(keepout.uuid)
                    .on_layers(
                        pcb.layers()
                            .find(LayerKind::Courtyard, Some(side))
                            .map(|l| l.uuid),
                    )
            });
        }
    }
}

/// A placed Footprint's courtyards, or with none, the fallback box: a box around its
/// copper, silkscreen and assembly drawing (text left out) on its side.
pub(crate) fn courtyards_or_box(
    pcb: &PcbDocument,
    instance: &FootprintInstance,
) -> BTreeMap<Side, Area> {
    let courtyards = courtyards(pcb, instance);
    if !courtyards.is_empty() {
        return courtyards;
    }
    let mut lo = [i64::MAX, i64::MAX];
    let mut hi = [i64::MIN, i64::MIN];
    let mut add = |p: [i64; 2], r: i64| {
        lo = [lo[0].min(p[0] - r), lo[1].min(p[1] - r)];
        hi = [hi[0].max(p[0] + r), hi[1].max(p[1] + r)];
    };
    if let Some(copy) = pcb.footprint_copy(instance.footprint) {
        for pad in &copy.footprint.pads {
            let (a, b) = Copper::pad(instance, pad).bounds();
            add(a, 0);
            add(b, 0);
        }
    }
    for graphic in placed_graphics(pcb, instance) {
        if graphic.layer.is_courtyard() {
            continue;
        }
        let points = corners(&graphic.shape)
            .or_else(|| polyline(&graphic.shape))
            .unwrap_or_default();
        for p in points {
            add([p.x.0, p.y.0], graphic.width.0 / 2);
        }
    }
    if lo[0] > hi[0] || lo[1] > hi[1] {
        return BTreeMap::new();
    }
    let contour: Contour = [
        [lo[0], lo[1]],
        [hi[0], lo[1]],
        [hi[0], hi[1]],
        [lo[0], hi[1]],
    ]
    .into_iter()
    .map(ip)
    .collect();
    BTreeMap::from([(instance.side, poly::polygon(contour))])
}

/// The area a placed Footprint takes up: its Pads and its courtyards (or fallback box).
fn extent(pcb: &PcbDocument, instance: &FootprintInstance) -> Area {
    let mut shapes: Vec<Vec<Contour>> = Vec::new();
    if let Some(copy) = pcb.footprint_copy(instance.footprint) {
        for pad in &copy.footprint.pads {
            shapes.extend(poly::grown(&Copper::pad(instance, pad), 0));
        }
    }
    for area in courtyards_or_box(pcb, instance).into_values() {
        shapes.extend(area);
    }
    poly::union(shapes)
}

/// The areas a placed Footprint's courtyard drawing encloses, per board side.
fn courtyards(pcb: &PcbDocument, instance: &FootprintInstance) -> BTreeMap<Side, Area> {
    let mut contours: BTreeMap<Side, Vec<Vec<Contour>>> = BTreeMap::new();
    let mut chains: BTreeMap<Side, Vec<Vec<Point>>> = BTreeMap::new();
    for graphic in placed_graphics(pcb, instance) {
        let side = match graphic.layer {
            FootprintLayer::CourtyardTop => Side::Top,
            FootprintLayer::CourtyardBottom => Side::Bottom,
            _ => continue,
        };
        let Some(closed) = corners(&graphic.shape) else {
            if let Some(line) = polyline(&graphic.shape) {
                chains.entry(side).or_default().push(line);
            }
            continue;
        };
        if closed.len() >= 3 {
            contours.entry(side).or_default().push(vec![
                closed.into_iter().map(|p| ip([p.x.0, p.y.0])).collect(),
            ]);
        }
    }
    for (side, pieces) in chains {
        for contour in loops(pieces) {
            contours.entry(side).or_default().push(vec![contour]);
        }
    }
    contours
        .into_iter()
        .map(|(side, shapes)| (side, poly::union(shapes)))
        .filter(|(_, area)| !area.is_empty())
        .collect()
}

/// The corners of a closed Footprint drawing shape on the board: a rectangle, a polygon or
/// a circle (as chords within 1 µm). `None` for lines, arcs and text.
pub(super) fn corners(shape: &Shape) -> Option<Vec<Point>> {
    match shape {
        Shape::Rect { start, end } => Some(vec![
            *start,
            Point::new(end.x, start.y),
            *end,
            Point::new(start.x, end.y),
        ]),
        Shape::Polygon { points } => Some(points.clone()),
        Shape::Circle { center, radius } => {
            let from = Point::new(center.x + *radius, center.y);
            let half = Point::new(center.x - *radius, center.y);
            let mut points = arc_points(from, half, Millideg(180_000));
            points.extend(
                arc_points(half, from, Millideg(180_000))
                    .into_iter()
                    .skip(1),
            );
            points.pop();
            Some(points)
        }
        _ => None,
    }
}

/// The points along an open Footprint drawing shape on the board: a line, or an arc as
/// chords within 1 µm. `None` for the other shapes.
pub(super) fn polyline(shape: &Shape) -> Option<Vec<Point>> {
    match shape {
        Shape::Line { start, end } => Some(vec![*start, *end]),
        Shape::Arc {
            center,
            start,
            sweep,
        } => Some(arc_points(*start, rotate(*start, *center, *sweep), *sweep)),
        _ => None,
    }
}

/// The closed loops that open polylines form where their ends meet exactly.
fn loops(mut pieces: Vec<Vec<Point>>) -> Vec<Contour> {
    let mut out = Vec::new();
    while let Some(mut chain) = pieces.pop() {
        loop {
            let (first, last) = (chain[0], chain[chain.len() - 1]);
            if first == last && chain.len() > 3 {
                chain.pop();
                out.push(chain.iter().map(|p| ip([p.x.0, p.y.0])).collect());
                break;
            }
            let next = pieces
                .iter()
                .position(|p| p[0] == last || p[p.len() - 1] == last);
            let Some(i) = next else { break };
            let mut piece = pieces.swap_remove(i);
            if piece[0] != last {
                piece.reverse();
            }
            chain.extend(piece.into_iter().skip(1));
        }
    }
    out
}

/// The board-wide `courtyard_clearance`: 0 when no rule sets it.
pub(crate) fn courtyard_clearance(pcb: &PcbDocument) -> Nm {
    resolve(pcb, None, RuleKind::CourtyardClearance, RuleField::Value)
        .and_then(|s| s.value.length())
        .unwrap_or(Nm::ZERO)
}

/// Whether two Footprints' courtyards (or fallback boxes) on one side come closer than
/// `required`, as the courtyard check finds them. Touching passes at 0.
pub(crate) fn too_close(a: &BTreeMap<Side, Area>, b: &BTreeMap<Side, Area>, required: Nm) -> bool {
    a.iter().any(|(side, area)| {
        let Some(other) = b.get(side) else {
            return false;
        };
        let (Some(ba), Some(bb)) = (poly::bounds(area), poly::bounds(other)) else {
            return false;
        };
        if !poly::boxes_within(ba, bb, required.0) {
            return false;
        }
        poly::gap(area, other).is_some_and(|(gap, _, overlaps)| overlaps || gap < required.0 as f64)
    })
}
