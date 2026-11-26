//! The assembly tree: products, the uses of one product in another, and where each use is
//! placed.
//!
//! - A product's shape is the representation its `SHAPE_DEFINITION_REPRESENTATION` names,
//!   together with the representations tied to it by plain
//!   `SHAPE_REPRESENTATION_RELATIONSHIP`s (AP203 and AP242 keep the B-rep in one of those).
//! - A `NEXT_ASSEMBLY_USAGE_OCCURRENCE` puts a child product in a parent; its
//!   `CONTEXT_DEPENDENT_SHAPE_REPRESENTATION` places it through an
//!   `ITEM_DEFINED_TRANSFORMATION` from a placement in the child's shape to one in the
//!   parent's.
//! - Roots are the products no other product uses. Shapes no product reaches become roots of
//!   their own, so a model with broken product structure is still drawn.

use std::collections::{HashMap, HashSet};

use super::geometry::{Frame, Transform};
use super::{Mesher, Node, Part};
use crate::records::*;

/// How deep an assembly may nest before the rest is cut off (it guards against cycles).
const MAX_DEPTH: usize = 64;

struct Use {
    child: RecordId,
    name: String,
    transform: Transform,
}

pub(crate) fn build(mesher: &mut Mesher) -> (Vec<Part>, Vec<Node>) {
    let m = mesher.model;

    // Each product definition's shape representations.
    let mut own: HashMap<RecordId, Vec<RecordId>> = HashMap::new();
    let mut definitions: Vec<RecordId> = m.iter::<ProductDefinition>().map(|(id, _)| id).collect();
    for (_, sdr) in m.iter::<ShapeDefinitionRepresentation>() {
        let Some(pds) = m.get::<ProductDefinitionShape>(sdr.definition) else {
            continue;
        };
        if m.get::<ProductDefinition>(pds.definition).is_some() {
            own.entry(pds.definition)
                .or_default()
                .push(sdr.used_representation);
        }
    }
    let claimed: HashSet<RecordId> = own.values().flatten().copied().collect();
    let mut links: HashMap<RecordId, Vec<RecordId>> = HashMap::new();
    for (_, srr) in m.iter::<ShapeRepresentationRelationship>() {
        links.entry(srr.rep_1).or_default().push(srr.rep_2);
        links.entry(srr.rep_2).or_default().push(srr.rep_1);
    }
    let shape_of = |pd: RecordId| -> Vec<RecordId> {
        let mut seen: Vec<RecordId> = own.get(&pd).cloned().unwrap_or_default();
        let mut i = 0;
        while i < seen.len() {
            for &next in links.get(&seen[i]).map_or(&[][..], |v| v) {
                if !seen.contains(&next) && !claimed.contains(&next) {
                    seen.push(next);
                }
            }
            i += 1;
        }
        seen
    };
    let shapes: HashMap<RecordId, Vec<RecordId>> =
        definitions.iter().map(|&pd| (pd, shape_of(pd))).collect();

    // The uses of each product in its parents, with their placements.
    let mut uses: HashMap<RecordId, Vec<Use>> = HashMap::new();
    let mut placed: HashSet<RecordId> = HashSet::new();
    let mut is_child: HashSet<RecordId> = HashSet::new();
    for (_, cdsr) in m.iter::<ContextDependentShapeRepresentation>() {
        let Some(pds) = m.get::<ProductDefinitionShape>(cdsr.represented_product_relation) else {
            continue;
        };
        let Some(nauo) = m.get::<NextAssemblyUsageOccurrence>(pds.definition) else {
            continue;
        };
        let Some(srt) = m.get::<ShapeRepresentationTransformation>(cdsr.representation_relation)
        else {
            mesher.warn(format!(
                "#{}: the placement of '{}' is not an item-defined transformation; it is placed \
                 at its parent's origin",
                cdsr.representation_relation.0 + 1,
                nauo.name
            ));
            continue;
        };
        let parent_shape = shapes.get(&nauo.relating_product_definition);
        let swapped = parent_shape.is_some_and(|s| s.contains(&srt.rep_1));
        let transform = match placement(m, srt.transformation_operator, swapped) {
            Some(t) => t,
            None => {
                mesher.warn(format!(
                    "#{}: the placement of '{}' cannot be read; it is placed at its parent's \
                     origin",
                    srt.transformation_operator.0 + 1,
                    nauo.name
                ));
                Transform::IDENTITY
            }
        };
        placed.insert(pds.definition);
        is_child.insert(nauo.related_product_definition);
        uses.entry(nauo.relating_product_definition)
            .or_default()
            .push(Use {
                child: nauo.related_product_definition,
                name: nauo.name.clone(),
                transform,
            });
    }
    // Uses with no placement at all sit at the parent's origin.
    for (id, nauo) in m.iter::<NextAssemblyUsageOccurrence>() {
        if placed.contains(&id) {
            continue;
        }
        is_child.insert(nauo.related_product_definition);
        uses.entry(nauo.relating_product_definition)
            .or_default()
            .push(Use {
                child: nauo.related_product_definition,
                name: nauo.name.clone(),
                transform: Transform::IDENTITY,
            });
    }

    // Each product's own solids, meshed once.
    let mut parts: Vec<Part> = Vec::new();
    let mut part_of: HashMap<RecordId, usize> = HashMap::new();
    let mut drawn_reps: HashSet<RecordId> = HashSet::new();
    definitions.retain(|pd| shapes.contains_key(pd));
    for &pd in &definitions {
        let mut solids = Vec::new();
        for &rep in &shapes[&pd] {
            if !drawn_reps.insert(rep) {
                continue;
            }
            if let Some(r) = m.get::<Representation>(rep) {
                for &item in &r.items {
                    solids.extend(mesher.solids(item));
                }
            }
        }
        if !solids.is_empty() {
            part_of.insert(pd, parts.len());
            parts.push(Part {
                name: product_name(m, pd),
                solids,
            });
        }
    }

    fn node(
        m: &crate::StepModel,
        pd: RecordId,
        occurrence: Option<String>,
        transform: Transform,
        uses: &HashMap<RecordId, Vec<Use>>,
        part_of: &HashMap<RecordId, usize>,
        stack: &mut Vec<RecordId>,
    ) -> Node {
        stack.push(pd);
        let mut children = Vec::new();
        if stack.len() < MAX_DEPTH {
            for u in uses.get(&pd).map_or(&[][..], |v| v) {
                if !stack.contains(&u.child) {
                    children.push(node(
                        m,
                        u.child,
                        Some(u.name.clone()),
                        u.transform,
                        uses,
                        part_of,
                        stack,
                    ));
                }
            }
        }
        stack.pop();
        Node {
            name: product_name(m, pd),
            occurrence,
            transform,
            part: part_of.get(&pd).copied(),
            children,
        }
    }

    let mut roots = Vec::new();
    for &pd in &definitions {
        if is_child.contains(&pd) {
            continue;
        }
        let n = node(
            m,
            pd,
            None,
            Transform::IDENTITY,
            &uses,
            &part_of,
            &mut Vec::new(),
        );
        if n.part.is_some() || !n.children.is_empty() {
            roots.push(n);
        }
    }

    // Shapes no product reaches.
    let in_assembly: HashSet<RecordId> = m
        .iter::<ShapeRepresentationTransformation>()
        .flat_map(|(_, s)| [s.rep_1, s.rep_2])
        .collect();
    for (id, rep) in m.iter::<Representation>() {
        if drawn_reps.contains(&id) || rep.kind == RepresentationKind::Definitional {
            continue;
        }
        if in_assembly.contains(&id) || !rep.items.iter().any(|&i| mesher.is_solid_item(i)) {
            continue;
        }
        let mut solids = Vec::new();
        for &item in &rep.items {
            solids.extend(mesher.solids(item));
        }
        if solids.is_empty() {
            continue;
        }
        roots.push(Node {
            name: rep.name.clone(),
            occurrence: None,
            transform: Transform::IDENTITY,
            part: Some(parts.len()),
            children: Vec::new(),
        });
        parts.push(Part {
            name: rep.name.clone(),
            solids,
        });
    }
    (parts, roots)
}

/// The child-to-parent transform of an `ITEM_DEFINED_TRANSFORMATION`: it takes placement
/// `item_1` (in the child) onto `item_2` (in the parent), or the other way when `swapped`.
fn placement(m: &crate::StepModel, id: RecordId, swapped: bool) -> Option<Transform> {
    let idt = m.get::<ItemDefinedTransformation>(id)?;
    let (from, to) = if swapped {
        (idt.transform_item_2, idt.transform_item_1)
    } else {
        (idt.transform_item_1, idt.transform_item_2)
    };
    let from = Transform::of_frame(&Frame::of(m, from)?);
    let to = Transform::of_frame(&Frame::of(m, to)?);
    Some(to.compose(&from.inverse()))
}

fn product_name(m: &crate::StepModel, pd: RecordId) -> String {
    let Some(pd) = m.get::<ProductDefinition>(pd) else {
        return String::new();
    };
    let product = m
        .get::<ProductDefinitionFormation>(pd.formation)
        .map(|f| f.of_product)
        .or_else(|| {
            m.get::<ProductDefinitionFormationWithSpecifiedSource>(pd.formation)
                .map(|f| f.of_product)
        });
    product
        .and_then(|p| m.get::<Product>(p))
        .map(|p| p.name.clone())
        .unwrap_or_default()
}
