//! Delete Layer-local geometry, shorten endpoints and reconcile the rules using them.
use super::*;
use reda_model::{LayerSpan, RuleLayers};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) fn remove_plan(pcb: &PcbDocument, layer: Uuid) -> Result<Plan, CommandError> {
    let mut plan = Plan::new(pcb);
    let i = inner(pcb, layer)?;
    let order: Vec<_> = pcb.copper_layers().map(|l| l.uuid).collect();
    let above = order[i - 1];
    let below = order[i + 1];
    let shorten = |span: LayerSpan| -> Option<LayerSpan> {
        let toward = |other| {
            if pcb.copper_number(other).unwrap() <= i {
                above
            } else {
                below
            }
        };
        let from = if span.from == layer {
            toward(span.to)
        } else {
            span.from
        };
        let to = if span.to == layer {
            toward(span.from)
        } else {
            span.to
        };
        (from != to).then_some(LayerSpan { from, to })
    };
    let mut replacements = BTreeMap::new();
    // Reserve all unchanged spans first, independent of Via type table order.
    let unchanged: Vec<_> = plan
        .stack
        .via_types
        .iter()
        .filter(|v| v.from != layer && v.to != layer)
        .map(|v| (v.span(), v.uuid))
        .collect();
    plan.stack.via_types.retain_mut(|kind| {
        if kind.from != layer && kind.to != layer {
            return true;
        }
        if let Some(next) = shorten(kind.span()) {
            if let Some((_, other)) = unchanged.iter().find(|(s, _)| s.joins(next)) {
                replacements.insert(kind.uuid, Some(*other));
            } else {
                kind.from = next.from;
                kind.to = next.to;
                plan.counts.via_types_changed += 1;
                return true;
            }
        } else {
            replacements.insert(kind.uuid, None);
        }
        plan.counts.via_types_removed += 1;
        false
    });
    for v in pcb.vias() {
        if let Some(span) = shorten(v.span) {
            let mut via = v.clone();
            via.set_span(span);
            if span != v.span {
                plan.counts.vias_shortened += 1;
            }
            // The surviving type can have the opposite drill direction. Keep the
            // diameters on the same physical Layers when adopting that direction.
            if let Some(kind) = plan.stack.type_for_span(span) {
                via.set_span(kind.span());
            }
            if &via != v {
                plan.changed
                    .push(Object::Via(InDocument::new(pcb.uuid, via)));
            }
        } else {
            plan.counts.vias_deleted += 1;
            plan.deleted.push(v.uuid);
        }
    }
    for rule in pcb.rules() {
        let mut next = rule.clone();
        if let Some(RuleLayers::Named(ids)) = &mut next.scope.layers {
            ids.retain(|id| *id != layer);
            if ids.is_empty() {
                plan.deleted.push(rule.uuid);
                plan.counts.rules_removed += 1;
                continue;
            }
        }
        if let Some(allowed) = &mut next.values.allowed {
            let was_nonempty = !allowed.is_empty();
            let mut seen = BTreeSet::new();
            allowed.retain_mut(|id| {
                match replacements.get(id) {
                    Some(Some(survivor)) => *id = *survivor,
                    Some(None) => return false,
                    None => {}
                }
                seen.insert(*id)
            });
            if was_nonempty && allowed.is_empty() {
                plan.counts.allowed_lists_emptied += 1;
                if let Some(class) = rule.scope.net_class.and_then(|id| pcb.net_class(id)) {
                    plan.counts.classes_allowing_all.push(class.name.clone());
                }
            }
        }
        if &next != rule {
            plan.changed
                .push(Object::DesignRule(InDocument::new(pcb.uuid, next)));
            plan.counts.rules_changed += 1;
        }
    }
    plan.counts.classes_allowing_all.sort();
    plan.delete_objects(
        "track",
        pcb.tracks().filter(|o| o.layer == layer).map(|o| o.uuid),
    );
    plan.delete_objects(
        "zone",
        pcb.zones().filter(|o| o.layer == layer).map(|o| o.uuid),
    );
    plan.delete_objects(
        "zone_cutout",
        pcb.zone_cutouts()
            .filter(|o| o.layer == layer)
            .map(|o| o.uuid),
    );
    plan.delete_objects(
        "keepout",
        pcb.keepouts()
            .filter(|o| o.layer == Some(layer))
            .map(|o| o.uuid),
    );
    plan.delete_objects(
        "text",
        pcb.texts().filter(|o| o.layer == layer).map(|o| o.uuid),
    );
    plan.delete_objects(
        "graphic",
        pcb.graphics().filter(|o| o.layer == layer).map(|o| o.uuid),
    );
    plan.delete_objects(
        "dimension",
        pcb.dimensions()
            .filter(|o| o.layer == layer)
            .map(|o| o.uuid),
    );
    let entries = &mut plan.stack.entries;
    let at = entries.iter().position(|e| e.uuid() == layer).unwrap();
    let StackupEntry::Dielectric(lower) = entries.remove(at + 1) else {
        unreachable!()
    };
    entries.remove(at);
    let StackupEntry::Dielectric(upper) = &mut entries[at - 1] else {
        unreachable!()
    };
    upper.thickness = upper.thickness.zip(lower.thickness).map(|(a, b)| a + b);
    plan.layers.layers.retain(|l| l.uuid != layer);
    Ok(plan)
}
