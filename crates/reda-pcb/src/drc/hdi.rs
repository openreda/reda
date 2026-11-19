//! Optional drill aspect ratio and stacked microvia limits.
use super::Report;
use crate::rules::resolve;
use reda_model::geom::Point;
use reda_model::{
    DrcCheck, Limit, Measurement, PcbDocument, RuleField, RuleKind, RuleValue, StackupEntry, Uuid,
    Via,
};
use std::collections::{BTreeMap, BTreeSet};

/// Each copper face's depth, in nanometres multiplied by the number of blank sheets.
/// This keeps the fractional contribution of a shared sheet exact at the limit.
struct Depths {
    faces: BTreeMap<Uuid, (i128, i128)>,
    divisor: i128,
}

impl Depths {
    fn new(pcb: &PcbDocument) -> Self {
        let stack = pcb.stackup();
        let divisor = stack
            .dielectrics()
            .filter(|s| s.thickness.is_none())
            .count()
            .max(1) as i128;
        let remainder = i128::from(stack.totals().remainder.unwrap_or_default().0);
        let mut faces = BTreeMap::new();
        let mut cursor = 0;
        for entry in &stack.entries {
            match entry {
                StackupEntry::Copper(c) => {
                    let next = cursor + i128::from(c.thickness.0) * divisor;
                    faces.insert(c.layer, (cursor, next));
                    cursor = next;
                }
                StackupEntry::Dielectric(d) => {
                    cursor += d.thickness.map_or(remainder, |n| i128::from(n.0) * divisor);
                }
            }
        }
        Self { faces, divisor }
    }

    fn ratio(&self, via: &Via) -> (i128, i128) {
        let (a, b) = self.faces[&via.span.from];
        let (c, d) = self.faces[&via.span.to];
        (b.max(d) - a.min(c), self.divisor * i128::from(via.hole.0))
    }
}

pub(super) fn check(pcb: &PcbDocument, report: &mut Report) {
    if !report.is_off(DrcCheck::AspectRatio)
        && pcb
            .rules()
            .any(|r| r.enabled && r.values.get(RuleField::AspectRatioMax).is_some())
    {
        let mut depths = None;
        for via in pcb.vias() {
            let micro = pcb
                .stackup()
                .type_for_span(via.span)
                .is_some_and(|t| t.micro);
            let kind = if micro {
                RuleKind::MicroviaSize
            } else {
                RuleKind::ViaSize
            };
            let Some(source) = resolve(pcb, via.net, kind, RuleField::AspectRatioMax) else {
                continue;
            };
            let RuleValue::MilliRatio(max) = source.value else {
                continue;
            };
            let (depth, hole) = depths.get_or_insert_with(|| Depths::new(pcb)).ratio(via);
            if 1000 * depth > i128::from(max) * hole {
                let millionths = (depth * 1_000_000 + hole / 2) / hole;
                let actual = i64::try_from(millionths).unwrap_or(i64::MAX);
                let required = i64::from(max) * 1000;
                report.add(DrcCheck::AspectRatio, |v| {
                    v.comparison(
                        Measurement::RatioMillionths(actual),
                        Measurement::RatioMillionths(required),
                        Limit::Max,
                        source.rule,
                    )
                    .located(pcb.uuid, via.at)
                    .on_layers(pcb.span_layers(via.span))
                    .object(via.uuid)
                    .arg("actual", reda_model::geom::exact_decimal(actual, 1_000_000))
                    .arg(
                        "maximum",
                        reda_model::geom::exact_decimal(required, 1_000_000),
                    )
                });
            }
        }
    }
    if !report.is_off(DrcCheck::MaxStack)
        && pcb
            .rules()
            .any(|r| r.enabled && r.values.max_stack.is_some())
    {
        stack_limits(pcb, report);
    }
}

#[derive(Clone, Default)]
struct Stack {
    holes: Vec<Uuid>,
    lasers: u8,
    first: usize,
}

fn stack_limits(pcb: &PcbDocument, report: &mut Report) {
    let mut groups = BTreeMap::<(Uuid, Point), Vec<&Via>>::new();
    for via in pcb.vias() {
        if let Some(net) = via.net {
            groups.entry((net, via.at)).or_default().push(via);
        }
    }
    let copper: Vec<_> = pcb.copper_layers().map(|l| l.uuid).collect();
    for ((net, at), vias) in groups {
        let Some(source) = resolve(pcb, Some(net), RuleKind::MicroviaSize, RuleField::MaxStack)
        else {
            continue;
        };
        let RuleValue::Count(max) = source.value else {
            continue;
        };
        let mut starts = BTreeSet::new();
        let mut edges: Vec<_> = vias
            .into_iter()
            .filter_map(|via| {
                let kind = pcb.stackup().type_for_span(via.span)?;
                let a = pcb
                    .copper_number(via.span.from)
                    .expect("Via starts on copper")
                    - 1;
                let b = pcb.copper_number(via.span.to).expect("Via ends on copper") - 1;
                let first = a.min(b);
                starts.insert(first);
                Some((a.max(b), first, via.uuid, kind.micro))
            })
            .collect();
        edges.sort_unstable();
        // Physical order is a topological ordering of end-to-end spans. Keep the
        // highest microvia count ending at each Layer so overlapping holes never stack.
        let mut ends = BTreeMap::<usize, Stack>::new();
        for (last, first, uuid, micro) in edges {
            let mut path = ends.get(&first).cloned().unwrap_or_else(|| Stack {
                first,
                ..Stack::default()
            });
            path.holes.push(uuid);
            if micro {
                path.lasers += 1;
            }
            if ends.get(&last).is_none_or(|p| p.lasers < path.lasers) {
                ends.insert(last, path);
            }
        }
        for (last, path) in ends {
            if starts.contains(&last) || path.lasers <= max {
                continue;
            }
            report.add(DrcCheck::MaxStack, |v| {
                v.comparison(
                    Measurement::Count(path.lasers),
                    Measurement::Count(max),
                    Limit::Max,
                    source.rule,
                )
                .located(pcb.uuid, at)
                .on_layers(copper[path.first..=last].iter().copied())
                .objects(path.holes)
                .arg("actual", i64::from(path.lasers))
                .arg("maximum", i64::from(max))
            });
        }
    }
}
