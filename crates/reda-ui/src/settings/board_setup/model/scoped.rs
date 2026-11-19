//! Draft values for Layer and pair records in the shared Board Setup transaction.

use super::{
    BoardSetupDraft, ClassKey, Problem, RuleField, Step, load_text, text_changed, values_param,
};
use std::collections::BTreeSet;

use reda_engine::model::{
    DesignRule, InDocument, Object, PcbDocument, Project, RuleKind, RuleScope, RuleValues, Uuid,
};
use serde_json::json;

impl BoardSetupDraft {
    /// `project`'s PCB Document with the drafted rule records in place of the board's,
    /// so rules resolve as they will after Apply. A value typed invalidly keeps the
    /// board's, and a class deleted in the dialog loses its records.
    pub(crate) fn drafted_pcb(&self, project: &Project) -> Option<PcbDocument> {
        let pcb = project.pcb()?;
        let document = pcb.uuid;
        let mut records: Vec<DesignRule> = pcb.rules().cloned().collect();
        let fresh = |kind: RuleKind, scope: RuleScope| DesignRule {
            uuid: document.child(&format!("drafted/{}/{scope:?}", kind.id())),
            kind,
            scope,
            values: RuleValues::default(),
            enabled: true,
            comment: None,
        };
        for (scope, kind) in self.scoped.keys() {
            if pcb.scoped_rule(*kind, scope).is_none() {
                records.push(fresh(*kind, scope.clone()));
            }
        }
        for row in self.live_classes() {
            let ClassKey::Existing(class) = row.key else {
                continue;
            };
            let kinds: BTreeSet<RuleKind> = row.overrides.keys().map(|f| f.rule().0).collect();
            for kind in kinds {
                if pcb.class_rule(kind, class).is_none() && self.has_record(row.key, kind) {
                    records.push(fresh(kind, RuleScope::class(class)));
                }
            }
        }
        let mut scratch = project.clone();
        let mut tx = scratch.transaction();
        for mut rule in records {
            let scoped = rule.scope.layers.is_some() || rule.scope.net_class_pair.is_some();
            let (drafts, meta) = if scoped {
                (self.scoped.get(&(rule.scope.clone(), rule.kind)), None)
            } else if rule.scope.is_board_wide() {
                (Some(&self.rules), self.board_meta.get(&rule.kind))
            } else {
                let row = rule
                    .scope
                    .net_class
                    .and_then(|c| self.class_row(ClassKey::Existing(c)));
                match row {
                    Some(row) => (Some(&row.overrides), row.meta.get(&rule.kind)),
                    None => {
                        // The class is deleted in the dialog: it resolves to nothing.
                        let _ = tx.remove(rule.uuid);
                        continue;
                    }
                }
            };
            for (field, draft) in drafts.into_iter().flatten() {
                if field.rule().0 != rule.kind {
                    continue;
                }
                if let Ok(value) = field.value(draft.value()) {
                    let _ = rule.values.set(field.rule().1, value);
                }
            }
            if let Some(meta) = meta {
                rule.enabled = *meta.enabled.value();
            }
            let exists = pcb.rule(rule.uuid).is_some();
            let object = Object::DesignRule(InDocument::new(document, rule));
            // A record the model refuses keeps the board's, as Apply would.
            let _ = if exists {
                tx.replace(object)
            } else {
                tx.insert(object)
            };
        }
        tx.project().pcb().cloned()
    }

    pub(super) fn load_scoped(&mut self, pcb: &PcbDocument, reset: bool) {
        if reset {
            self.scoped.clear();
        }
        for r in pcb
            .rules()
            .filter(|r| r.scope.layers.is_some() || r.scope.net_class_pair.is_some())
        {
            self.scoped.entry((r.scope.clone(), r.kind)).or_default();
        }
        for ((scope, kind), fields) in &mut self.scoped {
            let record = pcb.scoped_rule(*kind, scope);
            for field in RuleField::ALL.into_iter().filter(|f| f.rule().0 == *kind) {
                let fresh = record.map_or_else(String::new, |r| field.text_in(&r.values));
                load_text(field, fields.entry(field).or_default(), fresh, reset);
            }
        }
    }

    pub fn scoped_value(&self, scope: &RuleScope, field: RuleField) -> &str {
        self.scoped
            .get(&(scope.clone(), field.rule().0))
            .and_then(|row| row.get(&field))
            .map_or("", |d| d.value().as_str())
    }

    pub fn set_scoped(&mut self, scope: &RuleScope, field: RuleField, text: &str) {
        self.scoped
            .entry((scope.clone(), field.rule().0))
            .or_default()
            .entry(field)
            .or_default()
            .set(text.into());
    }

    pub(crate) fn layer_scopes(&self, owner: &RuleScope, kind: RuleKind) -> Vec<RuleScope> {
        self.scoped
            .keys()
            .filter(|(s, k)| {
                *k == kind
                    && s.layers.is_some()
                    && s.net_class == owner.net_class
                    && s.net_class_pair == owner.net_class_pair
            })
            .map(|(s, _)| s.clone())
            .collect()
    }

    pub(crate) fn scoped_limit(
        &self,
        field: RuleField,
        inner: bool,
    ) -> Option<(
        reda_engine::model::Limit,
        reda_engine::model::geom::Nm,
        &str,
    )> {
        let (kind, f) = field.rule();
        reda_engine::pcb::rules::presets::strictest_limit_on(&self.job_presets, kind, f, inner)
    }

    pub(super) fn scoped_problems(&self) -> Vec<Problem> {
        self.scoped
            .iter()
            .flat_map(|((s, _), row)| {
                row.iter().filter_map(move |(&f, d)| {
                    f.check(d.value(), false)
                        .err()
                        .map(|why| Problem::new(format!("{s:?}/{}", f.id()), why))
                })
            })
            .collect()
    }

    pub(super) fn scoped_steps(&self) -> Vec<Step> {
        let name = |id: Uuid| {
            if id == Uuid::NIL {
                Some(reda_engine::pcb::DEFAULT_NET_CLASS)
            } else {
                self.live_classes()
                    .find(|r| r.key() == ClassKey::Existing(id))
                    .map(|r| r.name.value().as_str().trim())
            }
        };
        self.scoped
            .iter()
            .filter_map(|((scope, kind), row)| {
                if scope.net_class.is_some_and(|c| name(c).is_none()) {
                    return None;
                }
                let changes: Vec<_> = row
                    .iter()
                    .filter_map(|(&f, d)| Some((f.rule().1, text_changed(f, d)?)))
                    .collect();
                if changes.is_empty() {
                    return None;
                }
                let mut param = json!(scope);
                if let Some([a, b]) = scope.net_class_pair {
                    param["net_class_pair"] = json!([name(a)?, name(b)?]);
                }
                Some(Step::new(
                    "pcb.rules.set",
                    json!({"kind":kind,"scope":param,"values":values_param(&changes)}),
                ))
            })
            .collect()
    }
}
