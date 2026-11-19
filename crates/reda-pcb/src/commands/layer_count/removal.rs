//! Delete Layer-local geometry, shorten endpoints and reconcile the rules using them.
use super::*;
use reda_model::{LayerSpan, RuleLayers};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) fn remove_plan(pcb: &PcbDocument, layer: Uuid) -> Result<Plan, CommandError> {
    let mut plan = Plan::new(pcb);
    inner(pcb, layer)?;
    let removed = BTreeSet::from([layer]);
    remove_copper(&mut plan, pcb, &removed);
    reconcile(&mut plan, pcb, &removed);
    resolve_blanks(&mut plan.stack);
    let entries = &mut plan.stack.entries;
    let at = entries.iter().position(|e| e.uuid() == layer).unwrap();
    let StackupEntry::Dielectric(lower) = entries.remove(at + 1) else {
        unreachable!()
    };
    entries.remove(at);
    let StackupEntry::Dielectric(upper) = &mut entries[at - 1] else {
        unreachable!()
    };
    let joined = upper.thickness.unwrap_or(Nm::ZERO) + lower.thickness.unwrap_or(Nm::ZERO);
    upper.thickness = (joined > Nm::ZERO).then_some(joined);
    Ok(plan)
}

/// Drop removed endpoints inward, keeping the original drilling direction.
fn surviving_span(
    pcb: &PcbDocument,
    removed: &BTreeSet<Uuid>,
    span: LayerSpan,
) -> Option<LayerSpan> {
    if !removed.contains(&span.from) && !removed.contains(&span.to) {
        return Some(span);
    }
    let layers = pcb.span_layers(span);
    let mut kept = layers.iter().copied().filter(|l| !removed.contains(l));
    let first = kept.next()?;
    let last = kept.next_back()?;
    let (from, to) = if layers.first() == Some(&span.from) {
        (first, last)
    } else {
        (last, first)
    };
    Some(LayerSpan { from, to })
}

/// Reserve unchanged types first, then shorten the others in table order. Collisions
/// keep the existing type, including two types that collapse onto the same new span.
pub(super) fn remove_copper(plan: &mut Plan, pcb: &PcbDocument, removed: &BTreeSet<Uuid>) {
    let mut occupied: Vec<_> = plan
        .stack
        .via_types
        .iter()
        .filter(|v| !removed.contains(&v.from) && !removed.contains(&v.to))
        .map(|v| v.span())
        .collect();
    plan.stack.via_types.retain_mut(|kind| {
        if !removed.contains(&kind.from) && !removed.contains(&kind.to) {
            return true;
        }
        if let Some(span) = surviving_span(pcb, removed, kind.span())
            && !occupied.iter().any(|s| s.joins(span))
        {
            kind.from = span.from;
            kind.to = span.to;
            occupied.push(span);
            return true;
        }
        false
    });
    plan.layers.layers.retain(|l| !removed.contains(&l.uuid));
    for (layer, kind, uuid) in pcb.layer_objects() {
        if removed.contains(&layer) {
            *plan.counts.objects_deleted.entry(kind.into()).or_default() += 1;
            plan.deleted.push(uuid);
        }
    }
}

/// Repair board references against the final Layers and Via types. Counting the final
/// edit gives one outcome for each object even when several Layers disappear.
pub(super) fn reconcile(plan: &mut Plan, pcb: &PcbDocument, removed: &BTreeSet<Uuid>) {
    let mut replacements = BTreeMap::new();
    plan.counts.via_types_changed = 0;
    for old in &pcb.stackup().via_types {
        if let Some(next) = plan.stack.via_types.iter().find(|v| v.uuid == old.uuid) {
            let length = |layers: &LayerSet, from, to| {
                layers
                    .copper_number(from)
                    .unwrap()
                    .abs_diff(layers.copper_number(to).unwrap())
            };
            let changed = next != old
                || length(&plan.layers, next.from, next.to)
                    > length(pcb.layers(), old.from, old.to);
            plan.counts.via_types_changed += usize::from(changed);
        } else {
            let next = surviving_span(pcb, removed, old.span())
                .and_then(|s| plan.stack.type_for_span(s))
                .map(|v| v.uuid);
            replacements.insert(old.uuid, next);
            plan.counts.via_types_removed += 1;
        }
    }
    for v in pcb.vias() {
        if let Some(span) = surviving_span(pcb, removed, v.span) {
            let mut via = v.clone();
            via.set_span(span);
            if span != v.span {
                plan.counts.vias_shortened += 1;
            }
            // The surviving type can have the opposite drill direction. Keep the
            // diameters on the same physical Layers when adopting that direction.
            if let Some(kind) = plan.stack.type_for_span(span) {
                via.set_span(kind.span());
            } else if pcb.stackup().type_for_span(v.span).is_some() {
                plan.counts.vias_without_type += 1;
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
            ids.retain(|id| !removed.contains(id));
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
}
