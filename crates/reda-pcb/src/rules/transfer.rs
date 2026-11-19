//! Moving rules between boards: a board's rules as a [`RuleSet`] (a `.redarules` file),
//! and what importing one would change. Classes are matched by name; Net membership is
//! never carried.

use reda_model::{
    DesignRule, DrcCheck, LayerSpan, NetClass, PcbDocument, PortableClass, PortableLayers,
    PortableRule, PortableSpan, RuleKind, RuleLayers, RuleScope, RuleSet, RuleValues, Severity,
    Uuid,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// The board's rules as a [`RuleSet`]: its Net Classes by name in rank order, the
/// board-wide records in kind order, then each class's records, and the DRC severities.
pub fn export(pcb: &PcbDocument) -> RuleSet {
    let mut classes: Vec<&NetClass> = pcb.net_classes().collect();
    classes.sort_by_key(|c| c.rank);
    let position = |id| {
        pcb.copper_number(id)
            .expect("rules only name existing copper Layers") as u8
    };
    let name = |id| {
        if id == Uuid::NIL {
            crate::DEFAULT_NET_CLASS.to_owned()
        } else {
            pcb.net_class(id)
                .expect("rule names an existing Net Class")
                .name
                .clone()
        }
    };
    let portable = |record: &DesignRule| {
        let net_class_pair = record.scope.net_class_pair.map(|pair| {
            let mut names = pair.map(name);
            names.sort();
            names
        });
        let layers = record.scope.layers.as_ref().map(|layers| match layers {
            RuleLayers::Group(g) => PortableLayers::Group(*g),
            RuleLayers::Named(ids) => {
                let mut numbered: Vec<_> = ids.iter().copied().map(position).collect();
                numbered.sort();
                PortableLayers::Positions(numbered)
            }
        });
        let allowed = record.values.allowed.as_ref().map(|types| {
            types
                .iter()
                .map(|id| {
                    let kind = pcb
                        .stackup()
                        .via_types
                        .iter()
                        .find(|k| k.uuid == *id)
                        .expect("rule names a listed Via type");
                    PortableSpan {
                        from: position(kind.from),
                        to: position(kind.to),
                        micro: kind.micro,
                    }
                })
                .collect()
        });
        PortableRule {
            kind: record.kind,
            net_class: record.scope.net_class.map(name),
            net_class_pair,
            layers,
            values: record.values.clone().with_allowed(allowed),
            enabled: record.enabled,
            comment: record.comment.clone(),
        }
    };
    let mut rules: Vec<_> = RuleKind::ALL
        .iter()
        .map(|kind| portable(pcb.board_rule(*kind)))
        .collect();
    for class in &classes {
        for kind in RuleKind::ALL {
            if let Some(record) = pcb.class_rule(kind, class.uuid) {
                rules.push(portable(record));
            }
        }
    }
    let mut scoped: Vec<_> = pcb
        .rules()
        .filter(|r| r.scope.layers.is_some() || r.scope.net_class_pair.is_some())
        .map(portable)
        .collect();
    scoped.sort_by(|a, b| {
        (a.kind, &a.net_class, &a.net_class_pair, &a.layers).cmp(&(
            b.kind,
            &b.net_class,
            &b.net_class_pair,
            &b.layers,
        ))
    });
    rules.extend(scoped);
    RuleSet {
        copper_layers: pcb.copper_layers().count() as u8,
        net_classes: classes
            .iter()
            .map(|c| PortableClass {
                name: c.name.clone(),
            })
            .collect(),
        rules,
        severities: pcb.setup().severities,
    }
}

/// What is wrong with a rule set as a file, if anything: a record its kind cannot hold,
/// a class the file does not list, or two records of one kind and class.
pub fn problem(set: &RuleSet) -> Option<String> {
    if !(2..=32).contains(&set.copper_layers) {
        return Some("copper_layers must be between 2 and 32".into());
    }
    let mut class_names = BTreeSet::new();
    for class in &set.net_classes {
        let name = &class.name;
        if name.is_empty()
            || name.trim() != name
            || name.eq_ignore_ascii_case(crate::DEFAULT_NET_CLASS)
            || !class_names.insert(name.to_lowercase())
        {
            return Some(format!("invalid or repeated Net Class name {name:?}"));
        }
    }
    let mut seen = BTreeSet::new();
    let valid = |p: u8| (1..=set.copper_layers).contains(&p);
    for (i, rule) in set.rules.iter().enumerate() {
        if rule
            .net_class
            .as_ref()
            .is_some_and(|n| n.eq_ignore_ascii_case(crate::DEFAULT_NET_CLASS))
        {
            return Some(format!(
                "rule {i} must use the board-wide scope for the Default Net Class"
            ));
        }
        for name in rule
            .net_class
            .iter()
            .chain(rule.net_class_pair.iter().flatten())
        {
            if !name.eq_ignore_ascii_case(crate::DEFAULT_NET_CLASS)
                && !class_names.contains(&name.to_lowercase())
            {
                return Some(format!(
                    "rule {i} names the Net Class {name:?}, which is not listed"
                ));
            }
        }
        let pair = rule.net_class_pair.clone().map(|mut pair| {
            pair = pair.map(|name| name.to_lowercase());
            pair.sort();
            pair
        });
        let mut layers = rule.layers.clone();
        if let Some(PortableLayers::Positions(positions)) = &mut layers {
            positions.sort();
            if positions.is_empty()
                || positions.iter().any(|p| !valid(*p))
                || positions.windows(2).any(|w| w[0] == w[1])
            {
                return Some(format!("rule {i} has invalid or repeated copper positions"));
            }
        }
        if !seen.insert((
            rule.kind,
            rule.net_class.as_ref().map(|n| n.to_lowercase()),
            pair,
            layers.clone(),
        )) {
            return Some(format!(
                "rule {i} is a second `{}` rule of its scope",
                rule.kind.id()
            ));
        }
        let mut spans = BTreeSet::new();
        if let Some(allowed) = &rule.values.allowed {
            for span in allowed {
                if !valid(span.from)
                    || !valid(span.to)
                    || span.from == span.to
                    || !spans.insert((span.from.min(span.to), span.from.max(span.to), span.micro))
                {
                    return Some(format!(
                        "rule {i} has invalid or repeated allowed Via spans"
                    ));
                }
            }
        }
        // Structural validation only needs distinct identities for valid portable positions.
        let record = DesignRule {
            uuid: Uuid::NIL,
            kind: rule.kind,
            scope: RuleScope {
                net_class: rule.net_class.as_ref().map(|_| Uuid::NIL),
                net_class_pair: rule.net_class_pair.as_ref().map(|_| [Uuid::NIL; 2]),
                layers: layers.map(|l| match l {
                    PortableLayers::Group(g) => RuleLayers::Group(g),
                    PortableLayers::Positions(ps) => RuleLayers::Named(
                        ps.into_iter().map(|p| Uuid::from_u128(p.into())).collect(),
                    ),
                }),
            },
            values: rule
                .values
                .clone()
                .with_allowed(rule.values.allowed.as_ref().map(|_| Vec::new())),
            enabled: rule.enabled,
            comment: None,
        };
        if let Some(why) = record.problem() {
            return Some(format!("rule {i}: {why}"));
        }
    }
    None
}

/// One record an import writes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Overwrite {
    /// Zero-based source record index, shared with the position-mapping preview.
    pub index: usize,
    pub kind: RuleKind,
    /// The Net Class, by name; left out for the board-wide record.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub net_class: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub net_class_pair: Option<[String; 2]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layers: Option<PortableLayers>,
    /// The record it replaces; left out for a new class record.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rule: Option<Uuid>,
    /// Its values now; left out for a new class record.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before: Option<RuleValues>,
    /// Its values after the import.
    pub after: RuleValues,
    /// Whether it counts after the import.
    pub enabled: bool,
}

/// One DRC severity an import changes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SeverityChange {
    pub check: DrcCheck,
    pub before: Severity,
    pub after: Severity,
}

/// Mapping of a position-sensitive record, including ones that cannot be imported.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PositionRecord {
    /// Zero-based index in the source file.
    pub index: usize,
    pub kind: RuleKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub net_class: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub net_class_pair: Option<[String; 2]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layers: Option<PortableLayers>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed: Option<Vec<PortableSpan>>,
    pub selected: bool,
    pub available: bool,
    pub missing_layers: Vec<u8>,
    pub missing_types: Vec<PortableSpan>,
    pub different_copper_count: bool,
}

/// What importing a rule set changes.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ImportPreview {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub position_records: Vec<PositionRecord>,
    /// Each record written, in the file's order.
    pub overwrites: Vec<Overwrite>,
    /// The Net Classes created empty, because the board has none of that name.
    pub new_classes: Vec<String>,
    /// The DRC severities that change.
    pub severities: Vec<SeverityChange>,
}

/// An import worked out: the preview and the objects to write.
pub struct ImportPlan {
    pub preview: ImportPreview,
    /// New Net Classes, ranked below the board's.
    pub classes: Vec<NetClass>,
    /// The records to write, and whether each replaces one.
    pub records: Vec<(DesignRule, bool)>,
    /// The severities to set, if any change.
    pub severities: Option<reda_model::DrcSeverities>,
}

/// What importing `set` into `pcb` writes: the records of `kinds` (every kind when
/// `None`), the classes they need, and with `severities` the DRC severities. Records of
/// the board the file does not hold are kept.
pub fn plan(
    pcb: &PcbDocument,
    set: &RuleSet,
    kinds: Option<&[RuleKind]>,
    severities: bool,
    position_records: Option<&BTreeSet<usize>>,
    mut new_id: impl FnMut() -> Uuid,
) -> Result<ImportPlan, String> {
    if let Some(rows) = position_records
        && let Some(index) = rows
            .iter()
            .find(|&&i| set.rules.get(i).is_none_or(|r| !r.uses_copper_positions()))
    {
        return Err(format!(
            "no position-dependent source record at index {index}"
        ));
    }
    let mut selected_layers = BTreeMap::new();
    let mut preview = ImportPreview::default();
    let mut missing_classes = BTreeMap::<String, Uuid>::new();
    let mut records = Vec::new();
    let copper: Vec<_> = pcb.copper_layers().map(|l| l.uuid).collect();
    let at = |position: u8| {
        position
            .checked_sub(1)
            .and_then(|i| copper.get(usize::from(i)))
            .copied()
    };
    for (index, rule) in set
        .rules
        .iter()
        .enumerate()
        .filter(|(_, r)| kinds.is_none_or(|k| k.contains(&r.kind)))
    {
        let sensitive = rule.uses_copper_positions();
        let different = copper.len() != usize::from(set.copper_layers);
        let mut mapping = PositionRecord {
            index,
            kind: rule.kind,
            net_class: rule.net_class.clone(),
            net_class_pair: rule.net_class_pair.clone(),
            layers: rule.layers.clone(),
            allowed: rule.values.allowed.clone(),
            available: true,
            selected: !sensitive
                || position_records.map_or(!different, |rows| rows.contains(&index)),
            different_copper_count: different,
            missing_layers: Vec::new(),
            missing_types: Vec::new(),
        };
        let layers = rule.layers.as_ref().map(|filter| match filter {
            PortableLayers::Group(group) => RuleLayers::Group(*group),
            PortableLayers::Positions(positions) => {
                let mut ids = Vec::new();
                for &position in positions {
                    if let Some(id) = at(position) {
                        ids.push(id);
                    } else {
                        mapping.missing_layers.push(position);
                    }
                }
                ids.sort();
                if ids.is_empty() {
                    mapping.available = false;
                }
                RuleLayers::Named(ids)
            }
        });
        let allowed = rule.values.allowed.as_ref().map(|spans| {
            let mut ids = Vec::new();
            for span in spans {
                let kind = at(span.from)
                    .zip(at(span.to))
                    .and_then(|(from, to)| pcb.stackup().type_for_span(LayerSpan { from, to }))
                    .filter(|kind| kind.micro == span.micro);
                if let Some(kind) = kind {
                    ids.push(kind.uuid);
                } else {
                    mapping.missing_types.push(*span);
                }
            }
            if !spans.is_empty() && ids.is_empty() {
                mapping.available = false;
            }
            ids
        });
        mapping.selected &= mapping.available;
        let selected = mapping.selected;
        if sensitive {
            preview.position_records.push(mapping);
        }
        if !selected {
            continue;
        }
        let mut class_id = |name: &str| {
            if name.eq_ignore_ascii_case(crate::DEFAULT_NET_CLASS) {
                return Uuid::NIL;
            }
            if let Some(class) = pcb.net_class_named(name) {
                return class.uuid;
            }
            *missing_classes
                .entry(name.to_lowercase())
                .or_insert_with(&mut new_id)
        };
        let scope = RuleScope {
            net_class: rule.net_class.as_deref().map(&mut class_id),
            net_class_pair: rule.net_class_pair.as_ref().map(|names| {
                let mut pair = [class_id(&names[0]), class_id(&names[1])];
                pair.sort();
                pair
            }),
            layers,
        };
        let existing = pcb.scoped_rule(rule.kind, &scope);
        if let Some((_, why)) = pcb.rule_scope_problem(rule.kind, &scope, existing.map(|r| r.uuid))
        {
            return Err(format!("source rule {index}: {why}"));
        }
        if let Some(RuleLayers::Named(ids)) = &scope.layers {
            for &layer in ids {
                let key = (rule.kind, scope.net_class, scope.net_class_pair, layer);
                if let Some(first) = selected_layers.insert(key, index) {
                    return Err(format!(
                        "source rules {first} and {index} name the same target Layer for one owner; select only one"
                    ));
                }
            }
        }
        let values = rule.values.clone().with_allowed(allowed);
        let record = DesignRule {
            uuid: existing.map_or_else(&mut new_id, |r| r.uuid),
            kind: rule.kind,
            scope,
            values: values.clone(),
            enabled: rule.enabled,
            comment: rule.comment.clone(),
        };
        preview.overwrites.push(Overwrite {
            index,
            kind: rule.kind,
            net_class: rule.net_class.clone(),
            net_class_pair: rule.net_class_pair.clone(),
            layers: rule.layers.clone(),
            rule: existing.map(|r| r.uuid),
            before: existing.map(|r| r.values.clone()),
            after: values,
            enabled: rule.enabled,
        });
        records.push((record, existing.is_some()));
    }
    // Keep the file's relative class ranks even when its records mention them out of order.
    let mut classes = Vec::new();
    let mut rank = pcb.net_classes().map(|c| c.rank).max().unwrap_or(0);
    for class in &set.net_classes {
        if let Some(&uuid) = missing_classes.get(&class.name.to_lowercase()) {
            rank = rank
                .checked_add(1)
                .ok_or("no rank left for an imported Net Class")?;
            preview.new_classes.push(class.name.clone());
            classes.push(NetClass {
                uuid,
                name: class.name.clone(),
                rank,
            });
        }
    }
    let changed: BTreeSet<_> = records.iter().map(|(r, _)| r.uuid).collect();
    let original: Vec<_> = pcb.rules().collect();
    let before = super::validation::problems_for(pcb, &original);
    let combined: Vec<_> = original
        .into_iter()
        .filter(|r| !changed.contains(&r.uuid))
        .chain(records.iter().map(|(r, _)| r))
        .collect();
    for problem in super::validation::problems_for(pcb, &combined) {
        if changed.contains(&problem.rule) || !before.contains(&problem) {
            return Err(format!(
                "importing leaves the `{}` rule invalid: {}",
                problem.kind.id(),
                problem.message
            ));
        }
    }
    let now = pcb.setup().severities;
    if severities {
        preview.severities = DrcCheck::ALL
            .iter()
            .filter(|c| now.get(**c) != set.severities.get(**c))
            .map(|c| SeverityChange {
                check: *c,
                before: now.get(*c),
                after: set.severities.get(*c),
            })
            .collect();
    }
    Ok(ImportPlan {
        severities: (!preview.severities.is_empty()).then_some(set.severities),
        preview,
        classes,
        records,
    })
}
