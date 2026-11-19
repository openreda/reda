//! The drill checks: hole size, annular ring and hole-to-hole spacing, over Vias, the Pads
//! of placed Footprints and Free Pads.

use crate::copper::shape::BoardShape as _;
use reda_model::geom::{CornerRect, Nm, Point};
use reda_model::{DrcCheck, Hole, Limit, PadKind, PcbDocument, RuleField, RuleKind, Uuid};

use super::{Board, Report};
use crate::copper::shape::{Core, Prim, Shape};
use crate::placed::{self, PlacedPad};
use crate::rules::{breach, hole_to_hole_between, largest, net_rules, resolve, resolve_pair};

pub(super) fn check(board: &Board<'_>, report: &mut Report) {
    via_spans(board.pcb, report);
    via_size(board, report);
    super::hdi::check(board.pcb, report);
    let holes = holes(board);
    for hole in holes.iter().filter(|h| h.via.is_none()) {
        let rules = net_rules(board.pcb, hole.net);
        let (narrow, wide) = (
            hole.size.width.min(hole.size.height),
            hole.size.width.max(hole.size.height),
        );
        let found = breach(rules.hole_min, None, narrow)
            .map(|b| (narrow, b))
            .or_else(|| breach(None, rules.hole_max, wide).map(|b| (wide, b)));
        let Some((actual, found)) = found else {
            continue;
        };
        let field = match found.0 {
            Limit::Min => RuleField::Min,
            Limit::Max => RuleField::Max,
        };
        let rule = resolve(board.pcb, hole.net, RuleKind::HoleSize, field)
            .expect("a breached limit applies")
            .rule;
        report.add(DrcCheck::HoleSize, |v| {
            super::copper::breach(v, actual, found, rule)
                .located(board.pcb.uuid, hole.shape.closest_points(&hole.shape)[0])
                .on_layers(board.pcb.copper_layers().map(|l| l.uuid))
                .objects(hole.objects.iter().copied())
        });
    }
    for hole in &holes {
        let Some(rings) = hole.ring else {
            continue;
        };
        let via = hole.via.and_then(|_| board.pcb.via(hole.objects[0]));
        let layers = match via {
            Some(via) => board.pcb.span_layers(via.span),
            None => board.pcb.copper_layers().map(|l| l.uuid).collect(),
        };
        let mut breaches = std::collections::BTreeMap::<_, Vec<_>>::new();
        for layer in layers {
            let ring = match rings {
                Rings::Via => {
                    let via = board.pcb.via(hole.objects[0]).expect("Via hole");
                    Nm((via.diameter_on(layer).0 - via.hole.0) / 2)
                }
                Rings::Pad {
                    top,
                    middle,
                    bottom,
                } => match board.pcb.layers().layer(layer).expect("copper Layer").side {
                    Some(reda_model::Side::Top) => top,
                    None => middle,
                    Some(reda_model::Side::Bottom) => bottom,
                },
            };
            if let Some(source) = crate::rules::resolve_on(
                board.pcb,
                hole.net,
                Some(layer),
                RuleKind::AnnularRing,
                RuleField::Min,
            ) && let Some(min) = source.value.length()
                && ring < min
            {
                breaches
                    .entry((source.rule, min, ring))
                    .or_default()
                    .push(layer);
            }
        }
        for ((rule, minimum, ring), layers) in breaches {
            report.add(DrcCheck::AnnularRing, |v| {
                v.length(ring, minimum, Limit::Min, rule)
                    .located(board.pcb.uuid, hole.shape.closest_points(&hole.shape)[0])
                    .on_layers(layers)
                    .arg("minimum", minimum)
                    .arg("actual", ring)
                    .objects(hole.objects.iter().copied())
            });
        }
    }
    if !report.is_off(DrcCheck::HoleToHole) {
        hole_to_hole(board.pcb, &holes, report);
    }
}

fn via_spans(pcb: &PcbDocument, report: &mut Report) {
    if report.is_off(DrcCheck::ViaSpan) {
        return;
    }
    let board_rule = pcb.board_rule(RuleKind::ViaSpan);
    for via in pcb.vias() {
        let layers = pcb.span_layers(via.span);
        let (reason, rule) = match pcb.stackup().type_for_span(via.span) {
            None if board_rule.enabled => ("no_type", board_rule.uuid),
            Some(kind) if kind.micro && layers.len() > 3 && board_rule.enabled => {
                ("micro_too_long", board_rule.uuid)
            }
            Some(kind) => {
                let Some((rule, allowed)) = crate::rules::via_span_permission(pcb, via.net) else {
                    continue;
                };
                if allowed.is_empty() || allowed.contains(&kind.uuid) {
                    continue;
                }
                ("not_allowed", rule)
            }
            None => continue,
        };
        report.add(DrcCheck::ViaSpan, |v| {
            v.arg("reason", reason)
                .with_rule(rule)
                .object(via.uuid)
                .located(pcb.uuid, via.at)
                .on_layers(layers)
        });
    }
}

/// Applies the drilling method's limits to the hole and each distinct physical pad size.
/// A uniform Via produces the same single diameter measurement on its span as before.
fn via_size(board: &Board<'_>, report: &mut Report) {
    use RuleField::*;
    let pcb = board.pcb;
    for via in pcb.vias() {
        let stack = pcb.stackup();
        let via_type = stack
            .type_for_span(via.span)
            .unwrap_or_else(|| stack.through_type());
        let (kind, check) = if via_type.micro {
            (RuleKind::MicroviaSize, DrcCheck::MicroviaSize)
        } else {
            (RuleKind::ViaSize, DrcCheck::ViaSize)
        };
        if report.is_off(check) {
            continue;
        }
        let (pads, hole) = crate::rules::via_ranges(pcb, via.net, via_type);
        let layers = pcb.span_layers(via.span);
        let mut by_size = std::collections::BTreeMap::<Nm, Vec<Uuid>>::new();
        for &layer in &layers {
            by_size
                .entry(via.diameter_on(layer))
                .or_default()
                .push(layer);
        }
        let diameters = by_size
            .into_iter()
            .map(|(size, on)| ("diameter", size, on, pads, [DiameterMin, DiameterMax]));
        for (part, size, on, range, [min, max]) in
            diameters.chain([("hole", via.hole, layers, hole, [HoleMin, HoleMax])])
        {
            let Some(found) = range.breach(size) else {
                continue;
            };
            let field = if found.0 == Limit::Min { min } else { max };
            let rule = resolve(pcb, via.net, kind, field)
                .expect("a breached limit applies")
                .rule;
            report.add(check, |v| {
                super::copper::breach(v, size, found, rule)
                    .located(pcb.uuid, via.at)
                    .on_layers(on)
                    .arg("part", part)
                    .object(via.uuid)
            });
        }
    }
}

/// One drilled hole on the board.
pub(super) struct DrillHole {
    /// The Via, or the Footprint instance then the Pad, or the Free Pad.
    pub objects: Vec<Uuid>,
    size: Hole,
    /// Its Net, which picks its rules.
    net: Option<Uuid>,
    /// A Via's centre and its normalized physical span, used to recognize a stacked pair.
    via: Option<ViaBore>,
    /// The copper ring around a plated hole; `None` for a bare (non-plated) hole.
    pub(super) ring: Option<Rings>,
    pub shape: Shape,
}

/// A Via follows its drilling span. Pad rings are measured in board-side order.
#[derive(Clone, Copy)]
pub(super) enum Rings {
    Via,
    Pad { top: Nm, middle: Nm, bottom: Nm },
}

#[derive(Clone, Copy)]
struct ViaBore {
    at: Point,
    first: usize,
    last: usize,
}

/// Every hole, Vias first, then Footprint Pads, then Free Pads, each in UUID order.
pub(super) fn holes(board: &Board<'_>) -> Vec<DrillHole> {
    let pcb = board.pcb;
    let mut out = Vec::new();
    for via in pcb.vias() {
        let size = Hole::new(via.hole, via.hole);
        let from = pcb
            .copper_number(via.span.from)
            .expect("Via starts on copper");
        let to = pcb.copper_number(via.span.to).expect("Via ends on copper");
        out.push(DrillHole {
            objects: vec![via.uuid],
            size,
            net: via.net,
            via: Some(ViaBore {
                at: via.at,
                first: from.min(to),
                last: from.max(to),
            }),
            ring: Some(Rings::Via),
            shape: hole_shape((via.at, via.at), size),
        });
    }
    let pad_hole = |objects: Vec<Uuid>, pad: &PlacedPad| {
        let size = pad.hole?;
        let ring = (pad.kind == PadKind::PlatedThroughHole).then(|| {
            let top = annular_ring(&pad.outline, size);
            Rings::Pad {
                top,
                middle: pad.middle.map_or(top, |(_, o)| annular_ring(&o, size)),
                bottom: pad.bottom.map_or(top, |(_, o)| annular_ring(&o, size)),
            }
        });
        Some(DrillHole {
            objects,
            size,
            net: pad.net,
            via: None,
            ring,
            shape: hole_shape(pad.hole_ends()?, size),
        })
    };
    for instance in pcb.footprints() {
        for pad in placed::placed_pads(pcb, instance) {
            out.extend(pad_hole(vec![instance.uuid, pad.pad], &pad));
        }
    }
    for pad in pcb.free_pads() {
        out.extend(pad_hole(vec![pad.uuid], &placed::placed_free_pad(pad)));
    }
    out
}

/// The narrowest copper between a Pad's hole and the edge of its copper, negative when
/// the hole reaches outside it. The copper is convex, so the narrowest point is at an end
/// of the slot's centre line.
fn annular_ring(outline: &CornerRect, hole: Hole) -> Nm {
    let (a, b) = hole.ends();
    let inside = outline.edge_distance(a).min(outline.edge_distance(b));
    Nm((inside - hole.diameter().0 as f64 / 2.0).floor() as i64)
}

/// A hole's outline: a disc, or a slot between the `ends` of its centre line.
fn hole_shape(ends: (Point, Point), size: Hole) -> Shape {
    let end = |p: Point| [p.x.0, p.y.0];
    Shape {
        prims: vec![Prim {
            core: Core::Seg(end(ends.0), end(ends.1)),
            radius: size.diameter().0 / 2,
        }],
    }
}

/// Holes whose edges come closer than the spacing their Nets require (the larger of the
/// two), overlapping ones included. One Violation per pair, in the order of [`holes`].
fn hole_to_hole(pcb: &PcbDocument, holes: &[DrillHole], report: &mut Report) {
    let reach = largest(pcb, RuleKind::HoleToHole, RuleField::Min);
    let mut order: Vec<usize> = (0..holes.len()).collect();
    let left = |i: usize| holes[i].shape.envelope().lower()[0];
    let right = |i: usize| holes[i].shape.envelope().upper()[0];
    order.sort_by_key(|&i| left(i));
    let mut pairs = Vec::new();
    for (k, &i) in order.iter().enumerate() {
        for &j in &order[k + 1..] {
            if left(j) - right(i) >= reach.0 {
                break;
            }
            if let (Some(a), Some(b)) = (holes[i].via, holes[j].via)
                && a.at == b.at
                && holes[i].net.is_some()
                && holes[i].net == holes[j].net
                && (a.last == b.first || b.last == a.first)
            {
                continue;
            }
            let Some(required) = hole_to_hole_between(pcb, holes[i].net, holes[j].net) else {
                continue;
            };
            let gap = holes[i].shape.distance(&holes[j].shape);
            if gap < required.0 as f64 {
                let actual = Nm(gap.max(0.0).floor() as i64);
                pairs.push((i.min(j), i.max(j), actual, required));
            }
        }
    }
    pairs.sort();
    for (i, j, actual, required) in pairs {
        let source = resolve_pair(
            pcb,
            holes[i].net,
            holes[j].net,
            RuleKind::HoleToHole,
            RuleField::Min,
        )
        .expect("spacing limit applies");
        let [a, b] = holes[i].shape.closest_points(&holes[j].shape);
        report.add(DrcCheck::HoleToHole, |v| {
            v.length(actual, required, Limit::Min, source.rule)
                .distance_points(a, b)
                .on_layers(pcb.copper_layers().map(|l| l.uuid))
                .arg("required", required)
                .arg("actual", actual)
                .objects(holes[i].objects.iter().copied())
                .objects(holes[j].objects.iter().copied())
        });
    }
}
