//! The range of a length parameter. A length's `min` and `max` may name the names choice
//! options bind (#416 decision), so its range depends on the options chosen: the PCIe
//! card's `card_length` starts at the chosen lane width's tab end.

use std::collections::BTreeSet;

use reda_model::geom::Nm;
use reda_model::geom::expr;
use reda_model::{BoardTemplate, ChoiceParameter, LengthParameter, TemplateParameter, When};
use schemars::JsonSchema;
use serde::Serialize;

use super::{Combination, Namespace, choices};

/// A length's range under the options its `when` names.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct LengthRange {
    /// One option key per choice the range depends on; empty for a constant range.
    #[schemars(schema_with = "when_schema")]
    pub when: When,
    pub min: Nm,
    pub max: Nm,
}

fn when_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    schemars::json_schema!({
        "description": "Choice parameter name to the option keys under which this range holds.",
        "type": "object",
        "additionalProperties": { "type": "array", "items": { "type": "string" } }
    })
}

/// The choices whose bound names `length`'s `min` or `max` names, in parameter order.
pub(crate) fn range_choices<'t>(
    template: &'t BoardTemplate,
    length: &LengthParameter,
) -> Vec<&'t ChoiceParameter> {
    let named: BTreeSet<String> = [&length.min, &length.max]
        .into_iter()
        .filter_map(|n| n.expression())
        .filter_map(|text| expr::references(text).ok())
        .flat_map(|r| r.names)
        .collect();
    choices(template)
        .filter(|c| {
            c.options
                .first()
                .is_some_and(|o| o.bind.keys().any(|k| named.contains(k)))
        })
        .collect()
}

/// `length`'s range with `combination`'s options chosen. Every choice its bounds depend
/// on must be in `combination`.
pub(crate) fn range_at(
    template: &BoardTemplate,
    length: &LengthParameter,
    combination: &Combination,
) -> Result<(Nm, Nm), String> {
    let constants = Namespace::empty();
    let mut names = Namespace::empty();
    for choice in range_choices(template, length) {
        let key = combination
            .get(&choice.name)
            .ok_or_else(|| format!("no option of `{}` is chosen", choice.name))?;
        let option = choice
            .options
            .iter()
            .find(|o| &o.key == key)
            .ok_or_else(|| format!("`{key}` is not an option of `{}`", choice.name))?;
        for (name, value) in &option.bind {
            names.insert(name, constants.bound(value)?);
        }
    }
    let min = names.length(&length.min).map_err(|e| format!("min: {e}"))?;
    let max = names.length(&length.max).map_err(|e| format!("max: {e}"))?;
    Ok((min, max))
}

/// The options of `combination` that `length`'s range depends on, as a gate.
pub(crate) fn range_gate(
    template: &BoardTemplate,
    length: &LengthParameter,
    combination: &Combination,
) -> When {
    range_choices(template, length)
        .into_iter()
        .filter_map(|c| {
            combination
                .get(&c.name)
                .map(|key| (c.name.clone(), vec![key.clone()]))
        })
        .collect()
}

/// `" at lanes=x4"` for a gate, empty for none.
pub(crate) fn shown_gate(when: &When) -> String {
    if when.is_empty() {
        return String::new();
    }
    let parts: Vec<String> = when
        .iter()
        .map(|(name, keys)| format!("{name}={}", keys.join("|")))
        .collect();
    format!(" at {}", parts.join(", "))
}

/// Every range of the length parameter `name`: one per combination of the options of the
/// choices its bounds name, the first choice varying slowest, each gated on those options.
/// A length whose bounds name nothing has one range with an empty gate.
pub fn length_ranges(template: &BoardTemplate, name: &str) -> Result<Vec<LengthRange>, String> {
    let length = template
        .parameters
        .iter()
        .find_map(|p| match p {
            TemplateParameter::Length(l) if l.name == name => Some(l),
            _ => None,
        })
        .ok_or_else(|| format!("`{name}` is not a length parameter"))?;
    let mut combos = vec![Combination::new()];
    for choice in range_choices(template, length) {
        combos = combos
            .into_iter()
            .flat_map(|combo| {
                choice.options.iter().map(move |o| {
                    let mut next = combo.clone();
                    next.insert(choice.name.clone(), o.key.clone());
                    next
                })
            })
            .collect();
    }
    combos
        .iter()
        .map(|combo| {
            let (min, max) =
                range_at(template, length, combo).map_err(|e| format!("`{name}` {e}"))?;
            Ok(LengthRange {
                when: range_gate(template, length, combo),
                min,
                max,
            })
        })
        .collect()
}
