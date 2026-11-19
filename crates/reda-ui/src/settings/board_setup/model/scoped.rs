//! Draft values for Layer and pair records in the shared Board Setup transaction.

use super::{
    BoardSetupDraft, ClassKey, Problem, RuleField, Step, load_text, text_changed, values_param,
};
use reda_engine::model::{PcbDocument, RuleKind, RuleScope, Uuid};
use serde_json::json;

impl BoardSetupDraft {
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
