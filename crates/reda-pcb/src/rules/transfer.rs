//! Moving rules between boards: a board's rules as a [`RuleSet`] (a `.redarules` file),
//! and what importing one would change. Classes are matched by name; Net membership is
//! never carried.

use reda_model::{
    DesignRule, DrcCheck, NetClass, PcbDocument, PortableClass, PortableRule, RuleKind, RuleScope,
    RuleSet, RuleValues, Severity, Uuid,
};
use schemars::JsonSchema;
use serde::Serialize;

/// The board's rules as a [`RuleSet`]: its Net Classes by name in rank order, the
/// board-wide records in kind order, then each class's records, and the DRC severities.
pub fn export(pcb: &PcbDocument) -> RuleSet {
    let mut classes: Vec<&NetClass> = pcb.net_classes().collect();
    classes.sort_by_key(|c| c.rank);
    let portable = |record: &DesignRule, class: Option<&NetClass>| PortableRule {
        kind: record.kind,
        net_class: class.map(|c| c.name.clone()),
        values: record.values,
        enabled: record.enabled,
        comment: record.comment.clone(),
    };
    let mut rules: Vec<PortableRule> = RuleKind::ALL
        .iter()
        .map(|kind| portable(pcb.board_rule(*kind), None))
        .collect();
    for class in &classes {
        for kind in RuleKind::ALL {
            if let Some(record) = pcb.class_rule(kind, class.uuid) {
                rules.push(portable(record, Some(class)));
            }
        }
    }
    RuleSet {
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
    let mut seen = Vec::new();
    for (i, rule) in set.rules.iter().enumerate() {
        if let Some(class) = &rule.net_class
            && !set.net_classes.iter().any(|c| &c.name == class)
        {
            return Some(format!(
                "rule {i} names the Net Class {class:?}, which is not listed"
            ));
        }
        let key = (rule.kind, rule.net_class.clone());
        if seen.contains(&key) {
            return Some(format!(
                "rule {i} is a second `{}` rule of its scope",
                rule.kind.id()
            ));
        }
        seen.push(key);
        let record = DesignRule {
            uuid: Uuid::NIL,
            kind: rule.kind,
            scope: if rule.net_class.is_some() {
                RuleScope::class(Uuid::NIL)
            } else {
                RuleScope::BOARD
            },
            values: rule.values,
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
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Overwrite {
    pub kind: RuleKind,
    /// The Net Class, by name; left out for the board-wide record.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub net_class: Option<String>,
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
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct SeverityChange {
    pub check: DrcCheck,
    pub before: Severity,
    pub after: Severity,
}

/// What importing a rule set changes.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, JsonSchema)]
pub struct ImportPreview {
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
    mut new_id: impl FnMut() -> Uuid,
) -> ImportPlan {
    let mut preview = ImportPreview::default();
    let mut classes: Vec<NetClass> = Vec::new();
    let mut records = Vec::new();
    let mut rank = pcb.net_classes().map(|c| c.rank).max().unwrap_or(0);
    for rule in set
        .rules
        .iter()
        .filter(|r| kinds.is_none_or(|k| k.contains(&r.kind)))
    {
        let class = match &rule.net_class {
            None => None,
            Some(name) => {
                let found = pcb
                    .net_classes()
                    .chain(classes.iter())
                    .find(|c| &c.name == name)
                    .map(|c| c.uuid);
                Some(found.unwrap_or_else(|| {
                    rank += 1;
                    let uuid = new_id();
                    preview.new_classes.push(name.clone());
                    classes.push(NetClass {
                        uuid,
                        name: name.clone(),
                        rank,
                    });
                    uuid
                }))
            }
        };
        let scope = class.map_or(RuleScope::BOARD, RuleScope::class);
        let existing = pcb.scoped_rule(rule.kind, scope);
        let record = DesignRule {
            uuid: existing.map_or_else(&mut new_id, |r| r.uuid),
            kind: rule.kind,
            scope,
            values: rule.values,
            enabled: rule.enabled,
            comment: rule.comment.clone(),
        };
        preview.overwrites.push(Overwrite {
            kind: rule.kind,
            net_class: rule.net_class.clone(),
            rule: existing.map(|r| r.uuid),
            before: existing.map(|r| r.values),
            after: rule.values,
            enabled: rule.enabled,
        });
        records.push((record, existing.is_some()));
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
    ImportPlan {
        severities: (!preview.severities.is_empty()).then_some(set.severities),
        preview,
        classes,
        records,
    }
}
