//! The placement checks: Footprints outside the Board Outline and overlapping courtyards.
//!
//! A Footprint's extent is the area of its Pads and its courtyard. A courtyard is the area
//! its courtyard drawing encloses: rectangles, polygons and circles, and closed chains of
//! lines and arcs; open chains enclose nothing.

use crate::copper::shape::BoardShape as _;
use std::collections::BTreeMap;

use reda_model::geom::{Millideg, Point};
use reda_model::{DrcCheck, FootprintInstance, FootprintLayer, PcbDocument, Shape, Side};

use super::{Board, Report};
use crate::copper::shape::{Shape as Copper, arc_points};
use crate::placed::{placed_graphics, rotate};
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

fn courtyard_overlap(board: &Board<'_>, report: &mut Report) {
    if report.is_off(DrcCheck::CourtyardOverlap) {
        return;
    }
    let pcb = board.pcb;
    let courtyards: Vec<(&FootprintInstance, BTreeMap<Side, Area>)> = pcb
        .footprints()
        .map(|f| (f, courtyards(pcb, f)))
        .filter(|(_, c)| !c.is_empty())
        .collect();
    for (i, (a, a_yards)) in courtyards.iter().enumerate() {
        for (b, b_yards) in &courtyards[i + 1..] {
            let overlaps = a_yards
                .iter()
                .any(|(side, area)| b_yards.get(side).is_some_and(|o| poly::overlaps(area, o)));
            if overlaps {
                report.add(DrcCheck::CourtyardOverlap, |v| {
                    v.arg("designator_a", a.designator.clone())
                        .arg("designator_b", b.designator.clone())
                        .object(a.uuid)
                        .object(b.uuid)
                });
            }
        }
    }
}

/// The area a placed Footprint takes up: its Pads and its courtyards.
fn extent(pcb: &PcbDocument, instance: &FootprintInstance) -> Area {
    let mut shapes: Vec<Vec<Contour>> = Vec::new();
    if let Some(copy) = pcb.footprint_copy(instance.footprint) {
        for pad in &copy.footprint.pads {
            shapes.extend(poly::grown(&Copper::pad(instance, pad), 0));
        }
    }
    for area in courtyards(pcb, instance).into_values() {
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
