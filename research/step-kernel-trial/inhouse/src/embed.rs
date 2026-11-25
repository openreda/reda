//! The re-embedder: copies a component STEP file's entities into the output as renumbered
//! text, finds the component's root product, and instances it under the `trial-board` assembly.

use crate::board::{Contexts, Emitter, ProductIds, cos_sin, ref_list, write_product};
use crate::p21::{self, Index, StepFile, Val};
use std::collections::{BTreeMap, HashSet};

const Z: [f64; 3] = [0.0, 0.0, 1.0];
const X: [f64; 3] = [1.0, 0.0, 0.0];

/// Which application protocol a FILE_SCHEMA string names.
pub fn classify_schema(schema: &str) -> &'static str {
    let s = schema.to_ascii_uppercase();
    if s.contains("AP242") {
        "AP242"
    } else if s.contains("AUTOMOTIVE_DESIGN") {
        "AP214"
    } else if s.contains("CONFIG_CONTROL_DESIGN") || s.contains("AP203") {
        "AP203"
    } else {
        "unknown"
    }
}

/// The strings inside the FILE_SCHEMA header statement, joined with `, `.
pub fn declared_schema(file: &StepFile) -> String {
    for h in &file.header {
        if p21::type_name(h) == "FILE_SCHEMA"
            && let Ok(e) = p21::parse_entity(h)
        {
            let mut names = Vec::new();
            collect_strings(&e.parts[0].1, &mut names);
            return names.join(", ");
        }
    }
    String::new()
}

fn collect_strings(vals: &[Val], out: &mut Vec<String>) {
    for v in vals {
        match v {
            Val::Str(_) => out.push(v.as_str().unwrap_or_default()),
            Val::List(l) => collect_strings(l, out),
            _ => {}
        }
    }
}

/// Where the component's root product sits in the component file (ids before renumbering).
struct Root {
    definition: u64,
    representation: u64,
    /// An identity AXIS2_PLACEMENT_3D among the representation's items, if it has one.
    origin: Option<u64>,
    product_name: String,
    children: usize,
}

fn is_product_definition(ty: &str) -> bool {
    ty == "PRODUCT_DEFINITION" || ty == "PRODUCT_DEFINITION_WITH_ASSOCIATED_DOCUMENTS"
}

fn triple(ix: &Index, id: u64, ty: &str) -> Option<[f64; 3]> {
    let a = ix.args(id, ty).ok()?;
    let l = a.get(1)?.as_list()?;
    Some([
        l.first()?.as_f64()?,
        l.get(1)?.as_f64()?,
        l.get(2).and_then(Val::as_f64).unwrap_or(0.0),
    ])
}

fn near(a: [f64; 3], b: [f64; 3]) -> bool {
    (0..3).all(|i| (a[i] - b[i]).abs() < 1e-12)
}

/// True when `id` is an AXIS2_PLACEMENT_3D at the origin with the default axes.
fn is_identity_placement(ix: &Index, id: u64) -> bool {
    let Ok(a) = ix.args(id, "AXIS2_PLACEMENT_3D") else {
        return false;
    };
    let dir = |v: Option<&Val>, default: [f64; 3]| match v {
        Some(Val::Ref(d)) => triple(ix, *d, "DIRECTION").is_some_and(|t| near(t, default)),
        Some(Val::Atom(s)) => s == "$",
        _ => false,
    };
    a.get(1)
        .and_then(Val::as_ref)
        .and_then(|p| triple(ix, p, "CARTESIAN_POINT"))
        .is_some_and(|t| near(t, [0.0; 3]))
        && dir(a.get(2), Z)
        && dir(a.get(3), X)
}

fn find_root(file: &StepFile, ix: &Index) -> Result<Root, String> {
    let mut definitions = Vec::new();
    let mut used_as_child = HashSet::new();
    let mut shapes: BTreeMap<u64, u64> = BTreeMap::new(); // PRODUCT_DEFINITION_SHAPE -> definition
    let mut sdrs = Vec::new(); // (definition shape, representation)
    for r in &file.records {
        match p21::type_name(&r.body) {
            t if is_product_definition(t) => definitions.push(r.id),
            "NEXT_ASSEMBLY_USAGE_OCCURRENCE" => {
                let a = p21::parse_entity(&r.body)?.parts.remove(0).1;
                if let Some(child) = a.get(4).and_then(Val::as_ref) {
                    used_as_child.insert(child);
                }
            }
            "PRODUCT_DEFINITION_SHAPE" => {
                let a = p21::parse_entity(&r.body)?.parts.remove(0).1;
                if let Some(d) = a.get(2).and_then(Val::as_ref) {
                    shapes.insert(r.id, d);
                }
            }
            "SHAPE_DEFINITION_REPRESENTATION" => {
                let a = p21::parse_entity(&r.body)?.parts.remove(0).1;
                if let (Some(s), Some(rep)) = (
                    a.first().and_then(Val::as_ref),
                    a.get(1).and_then(Val::as_ref),
                ) {
                    sdrs.push((s, rep));
                }
            }
            _ => {}
        }
    }
    if definitions.is_empty() {
        return Err("component has no PRODUCT_DEFINITION".into());
    }
    let rep_of = |def: u64| {
        sdrs.iter()
            .find(|(s, _)| shapes.get(s) == Some(&def))
            .map(|&(_, rep)| rep)
    };
    let roots: Vec<u64> = definitions
        .iter()
        .copied()
        .filter(|d| !used_as_child.contains(d) && rep_of(*d).is_some())
        .collect();
    if roots.len() != 1 {
        return Err(format!(
            "component has {} root product definitions with a shape (need exactly 1): {}",
            roots.len(),
            ref_list(&roots)
        ));
    }
    let definition = roots[0];
    let representation = rep_of(definition).unwrap();
    let rep = ix.get(representation)?;
    if rep.parts.len() != 1 {
        return Err(format!(
            "root shape representation #{representation} is a complex instance; not supported"
        ));
    }
    let items = rep.parts[0].1.get(1).and_then(Val::as_list).unwrap_or(&[]);
    let origin = items
        .iter()
        .filter_map(Val::as_ref)
        .find(|&i| is_identity_placement(ix, i));

    // PRODUCT_DEFINITION -> formation -> PRODUCT, for the name only.
    let product_name = (|| {
        let d = ix.get(definition).ok()?;
        let formation = d.parts[0].1.get(2)?.as_ref()?;
        let product = ix.get(formation).ok()?.parts[0].1.get(2)?.as_ref()?;
        ix.get(product).ok()?.parts[0].1.first()?.as_str()
    })()
    .unwrap_or_default();
    Ok(Root {
        definition,
        representation,
        origin,
        product_name,
        children: used_as_child.len(),
    })
}

/// Inserts `,#id` at the end of the item list (second parameter) of a representation record.
fn append_item(body: &[u8], id: u64) -> Result<Vec<u8>, String> {
    let err = || format!("cannot find the item list of {}", p21::preview(body));
    let open = body.iter().position(|&c| c == b'(').ok_or_else(err)?;
    let (mut depth, mut in_str, mut list_open, mut i) = (0usize, false, false, open + 1);
    while i < body.len() {
        let c = body[i];
        if in_str {
            in_str = c != b'\'';
        } else if c == b'\'' {
            in_str = true;
        } else if c == b'(' {
            depth += 1;
            list_open = true;
        } else if c == b')' {
            if depth == 1 && list_open {
                let mut out = body[..i].to_vec();
                out.extend_from_slice(format!(",#{id}").as_bytes());
                out.extend_from_slice(&body[i..]);
                return Ok(out);
            }
            depth = depth.checked_sub(1).ok_or_else(err)?;
        }
        i += 1;
    }
    Err(err())
}

pub struct Embedded {
    pub product: ProductIds,
    pub notes: Vec<String>,
}

/// Copies the component's records into `e`, each id shifted by one offset, and returns the ids
/// (after shifting) an assembly needs to instance the component.
pub fn embed_component(
    e: &mut Emitter,
    c: &Contexts,
    file: &StepFile,
    keep_app_context: bool,
) -> Result<Embedded, String> {
    let ix = Index::new(file);
    if ix.by_id.len() != file.records.len() {
        return Err("component defines an instance id twice".into());
    }
    let root = find_root(file, &ix)?;
    let mut notes = Vec::new();

    let offset = e.last_id();
    let max_id = file.records.iter().map(|r| r.id).max().unwrap_or(0);
    // If the root representation has no identity placement, one is written after the component
    // block: a point, two directions, then the placement, so it gets the fourth id after it.
    let extra_origin = offset + max_id + 4;

    // The output declares one schema and one application context. The component's own
    // APPLICATION_CONTEXT and APPLICATION_PROTOCOL_DEFINITION name its source protocol, and
    // every application context must have exactly one protocol definition, so both are dropped
    // and references to them point at the output's instead.
    let mut contexts = HashSet::new();
    let mut protocols = HashSet::new();
    if !keep_app_context {
        for r in &file.records {
            match p21::type_name(&r.body) {
                "APPLICATION_CONTEXT" => drop(contexts.insert(r.id)),
                "APPLICATION_PROTOCOL_DEFINITION" => drop(protocols.insert(r.id)),
                _ => {}
            }
        }
    }
    let mut foreign: BTreeMap<String, usize> = BTreeMap::new();
    let mut inch = false;
    for r in &file.records {
        if contexts.contains(&r.id) || protocols.contains(&r.id) {
            continue;
        }
        let ty = p21::type_name(&r.body);
        if ty.starts_with("CC_DESIGN_") || ty == "DESIGN_CONTEXT" || ty.contains("TESSELLATED") {
            *foreign.entry(ty.to_string()).or_default() += 1;
        }
        inch |= r.body.windows(6).any(|w| w.eq_ignore_ascii_case(b"'INCH'"));
        let mut body = p21::rewrite_refs(&r.body, |n| {
            if contexts.contains(&n) {
                c.application
            } else if protocols.contains(&n) {
                c.protocol
            } else {
                n + offset
            }
        });
        if r.id == root.representation && root.origin.is_none() {
            body = append_item(&body, extra_origin)?;
        }
        e.add_raw(r.id + offset, body);
    }
    let origin = match root.origin {
        Some(o) => o + offset,
        None => {
            let o = e.placement("", [0.0; 3], Z, X);
            assert_eq!(o, extra_origin);
            notes.push(format!(
                "root shape representation #{} had no identity AXIS2_PLACEMENT_3D item; one was appended to its item list",
                root.representation
            ));
            o
        }
    };

    notes.push(format!(
        "embedded {} component records verbatim with ids shifted by {offset}; root product '{}' (definition #{}, representation #{} {}){}",
        file.records.len() - contexts.len() - protocols.len(),
        root.product_name,
        root.definition,
        root.representation,
        ix.type_of(root.representation),
        if root.children > 0 {
            format!("; the component is itself an assembly with {} child definitions, instanced as a whole", root.children)
        } else {
            String::new()
        }
    ));
    if keep_app_context {
        notes.push("component APPLICATION_CONTEXT and APPLICATION_PROTOCOL_DEFINITION kept (--keep-app-context)".into());
    } else {
        notes.push(format!(
            "dropped {} APPLICATION_CONTEXT and {} APPLICATION_PROTOCOL_DEFINITION of the component and pointed their references at the output's",
            contexts.len(),
            protocols.len()
        ));
    }
    if !foreign.is_empty() {
        let list: Vec<String> = foreign.iter().map(|(k, v)| format!("{k} x{v}")).collect();
        notes.push(format!(
            "component entity types specific to its own protocol, copied as they are: {}",
            list.join(", ")
        ));
    }
    let styled = file
        .records
        .iter()
        .filter(|r| p21::type_name(&r.body).ends_with("STYLED_ITEM"))
        .count();
    notes.push(format!(
        "component has {styled} styled items (colours), copied as they are"
    ));
    if inch {
        notes.push(
            "component uses an INCH unit; its units context is kept and the importer must scale it"
                .into(),
        );
    }
    Ok(Embedded {
        product: ProductIds {
            product: 0,
            definition: root.definition + offset,
            representation: root.representation + offset,
            origin,
        },
        notes,
    })
}

/// The `trial-board` assembly product with its placements, written before the children's link
/// entities so they can refer to it.
pub struct Assembly {
    pub definition: u64,
    pub product: u64,
    representation: u64,
    placements: Vec<u64>,
}

pub fn write_assembly(
    e: &mut Emitter,
    c: &Contexts,
    name: &str,
    places: &[([f64; 3], f64)],
) -> Assembly {
    let (product, definition, shape) = write_product(e, c, name);
    let origin = e.placement("", [0.0; 3], Z, X);
    let placements: Vec<u64> = places
        .iter()
        .map(|&(p, deg)| {
            let (cos, sin) = cos_sin(deg);
            e.placement("", p, Z, [cos, sin, 0.0])
        })
        .collect();
    let mut items = vec![origin];
    items.extend(&placements);
    let representation = e.add(format!(
        "SHAPE_REPRESENTATION('{name}',({}),#{})",
        ref_list(&items),
        c.geometric
    ));
    e.add(format!(
        "SHAPE_DEFINITION_REPRESENTATION(#{shape},#{representation})"
    ));
    Assembly {
        definition,
        product,
        representation,
        placements,
    }
}

/// One occurrence of a child under the assembly: NEXT_ASSEMBLY_USAGE_OCCURRENCE, its
/// PRODUCT_DEFINITION_SHAPE, and the transformation from the child's origin placement to
/// placement `slot` of the assembly's representation.
pub fn write_occurrence(
    e: &mut Emitter,
    asm: &Assembly,
    slot: usize,
    name: &str,
    child: &ProductIds,
) {
    let nauo = e.add(format!(
        "NEXT_ASSEMBLY_USAGE_OCCURRENCE('{name}','{name}','',#{},#{},$)",
        asm.definition, child.definition
    ));
    let shape = e.add(format!(
        "PRODUCT_DEFINITION_SHAPE('Placement','Placement of an item',#{nauo})"
    ));
    let transform = e.add(format!(
        "ITEM_DEFINED_TRANSFORMATION('','',#{},#{})",
        child.origin, asm.placements[slot]
    ));
    let relationship = e.add(format!(
        "(REPRESENTATION_RELATIONSHIP('','',#{},#{})REPRESENTATION_RELATIONSHIP_WITH_TRANSFORMATION(#{transform})SHAPE_REPRESENTATION_RELATIONSHIP())",
        child.representation, asm.representation
    ));
    e.add(format!(
        "CONTEXT_DEPENDENT_SHAPE_REPRESENTATION(#{relationship},#{shape})"
    ));
}
