//! The batch-waive body that ERC's and DRC's Waiver Commands share.

use reda_model::{InDocument, Object, Project, Severity, Uuid, Violation, Waiver};

use crate::{CommandError, Context, UserNameLookup};

/// What a check domain supplies to [`waive`].
pub trait WaiveChecks {
    /// The current Violations, ignoring Waivers.
    fn violations(&self, cx: &Context<'_>) -> Result<Vec<Violation>, CommandError>;

    /// The Waivers the domain already stores.
    fn waivers<'p>(&self, project: &'p Project) -> impl Iterator<Item = &'p Waiver>;

    /// The Document that stores the Waiver of `violation`.
    fn document(&self, project: &Project, violation: &Violation) -> Result<Uuid, CommandError>;
}

/// Waives the current Violations `targets` names (each a check id and exactly its objects)
/// with one reason, author and creation time, in the caller's transaction, so as one undo
/// step that rolls back whole. Every target is checked against the current results before
/// anything is inserted. Targets that share a fingerprint (such as both ends of a dangling
/// Wire) get one Waiver. Returns the new Waivers, one per distinct fingerprint, in the
/// order each was first selected.
pub fn waive<'t>(
    cx: &mut Context<'_>,
    checks: &impl WaiveChecks,
    targets: impl IntoIterator<Item = (&'t str, &'t [Uuid])>,
    reason: String,
    user_name: UserNameLookup,
) -> Result<Vec<Uuid>, CommandError> {
    let targets: Vec<_> = targets.into_iter().collect();
    if targets.is_empty() {
        return Err(CommandError::invalid_params(
            "/violations",
            "select at least one Violation",
        ));
    }
    let selected = select(cx, checks, targets)?;
    let author = user_name(cx.user_dirs());
    let created = cx.now().0;
    let mut waivers = Vec::with_capacity(selected.len());
    for (document, violation) in selected {
        let waiver = Waiver::of(
            cx.new_id(),
            &violation,
            author.clone(),
            created,
            reason.clone(),
        );
        waivers.push(waiver.uuid);
        cx.tx()?
            .insert(Object::Waiver(InDocument::new(document, waiver)))?;
    }
    Ok(waivers)
}

/// The current Violation each target names, with the Document to store its Waiver in,
/// one per fingerprint.
fn select(
    cx: &Context<'_>,
    checks: &impl WaiveChecks,
    targets: Vec<(&str, &[Uuid])>,
) -> Result<Vec<(Uuid, Violation)>, CommandError> {
    let violations = checks.violations(cx)?;
    let project = cx.project()?;
    let existing: Vec<&Waiver> = checks.waivers(project).collect();
    let mut selected: Vec<(Uuid, Violation)> = Vec::new();
    for (check, objects) in targets {
        let probe = Violation::new(check, Severity::Error).objects(objects.to_vec());
        let candidate = Waiver::of(Uuid::from_u128(0), &probe, "", 0, "");
        let violation = violations
            .iter()
            .find(|v| candidate.matches(v))
            .ok_or_else(|| {
                CommandError::rejected(format!(
                    "no current {check} Violation has exactly these objects"
                ))
            })?;
        if existing.iter().any(|w| w.matches(violation)) {
            return Err(CommandError::rejected(
                "a Waiver already waives this Violation",
            ));
        }
        if selected.iter().any(|(_, v)| candidate.matches(v)) {
            continue;
        }
        selected.push((checks.document(project, violation)?, violation.clone()));
    }
    Ok(selected)
}
