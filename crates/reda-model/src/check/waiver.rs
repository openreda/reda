//! Waivers, laying them over a check run, and the fingerprint that tells stale results.
//! ERC and DRC share all of it.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::Violation;
use crate::Uuid;

/// A recorded acceptance that one Violation is intentional, so checks stop reporting it.
///
/// It is stored in the Document that holds the objects involved, and matches a Violation
/// of the same check whose objects are exactly these, in any order. Once those objects no
/// longer break the check together, it matches nothing.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Waiver {
    pub uuid: Uuid,
    /// The check id of the waived Violation, such as `unconnected_pin` or `clearance`.
    pub check: String,
    /// The UUIDs of the objects involved, sorted.
    pub objects: Vec<Uuid>,
    /// Why it is waived. Left out when empty.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub reason: String,
    /// Preference `user.name` at the instant this Waiver was created.
    pub author: String,
    /// Milliseconds since the Unix epoch, UTC, from the injected Clock.
    pub created: i64,
}

impl Waiver {
    /// A Waiver of `violation`'s check and objects.
    pub fn of(
        uuid: Uuid,
        violation: &Violation,
        author: impl Into<String>,
        created: i64,
        reason: impl Into<String>,
    ) -> Waiver {
        Waiver {
            uuid,
            check: violation.check.clone(),
            objects: object_set(&violation.objects),
            reason: reason.into(),
            author: author.into(),
            created,
        }
    }

    /// Whether this Waiver waives `violation`.
    pub fn matches(&self, violation: &Violation) -> bool {
        self.check == violation.check && self.objects == object_set(&violation.objects)
    }
}

fn object_set(objects: &[Uuid]) -> Vec<Uuid> {
    let mut set = objects.to_vec();
    set.sort();
    set.dedup();
    set
}

/// A check run's Violations after its Waivers are laid over them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Waived {
    /// The Violations to show: the unwaived ones, plus the waived ones (each naming its
    /// Waiver) when they were asked for, in the run's order.
    pub violations: Vec<Violation>,
    /// How many Violations Waivers waived.
    pub waived: usize,
    /// The Waivers that matched no Violation, sorted.
    pub unmatched: Vec<Uuid>,
}

/// Lays `waivers` over a run's `violations`. Waived Violations are dropped unless
/// `include_waived`, when they stay with their Waiver named. Only Waivers whose check
/// ran across the whole design can be reported as unmatched. Callers must return false
/// for Off checks, checks omitted from a run, and checks run with a narrowed scope.
pub fn apply_waivers<'a>(
    violations: Vec<Violation>,
    waivers: impl IntoIterator<Item = &'a Waiver>,
    include_waived: bool,
    check_ran_fully: impl Fn(&str) -> bool,
) -> Waived {
    let waivers: Vec<&Waiver> = waivers.into_iter().collect();
    let mut used = vec![false; waivers.len()];
    let mut out = Waived {
        violations: vec![],
        waived: 0,
        unmatched: vec![],
    };
    for mut violation in violations {
        match waivers.iter().position(|w| w.matches(&violation)) {
            Some(i) => {
                used[i] = true;
                out.waived += 1;
                if include_waived {
                    violation.waiver = Some(waivers[i].uuid);
                    out.violations.push(violation);
                }
            }
            None => out.violations.push(violation),
        }
    }
    out.unmatched = (waivers.iter().zip(used))
        .filter(|(w, used)| !used && check_ran_fully(&w.check))
        .map(|(w, _)| w.uuid)
        .collect();
    out.unmatched.sort();
    out
}
