//! Shared preparation and insertion for ERC and DRC's atomic Waiver Commands.

use reda_model::{InDocument, Object, Severity, Uuid, Violation, Waiver};

use crate::{CommandError, Context, UserName};

/// Validate every target against current results before allocating any ids or editing.
/// Domains run their checks and resolve the Document that owns each Violation.
pub fn select_waivers<'a>(
    targets: impl IntoIterator<Item = (&'a str, &'a [Uuid])>,
    violations: &[Violation],
    existing: impl Iterator<Item = &'a Waiver>,
    mut document: impl FnMut(&Violation) -> Result<Uuid, CommandError>,
) -> Result<Vec<(Uuid, Violation)>, CommandError> {
    let existing: Vec<_> = existing.collect();
    let mut selected = Vec::new();
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
        // Several results may have one fingerprint (such as both dangling Wire ends).
        if selected.iter().any(|(_, v)| candidate.matches(v)) {
            continue;
        }
        selected.push((document(violation)?, violation.clone()));
    }
    if selected.is_empty() {
        return Err(CommandError::invalid_params(
            "/violations",
            "select at least one Violation",
        ));
    }
    Ok(selected)
}

/// Insert a fully validated selection in the caller's transaction, with shared metadata.
/// The transaction records one history entry for each affected Document and rolls back
/// all insertions if any step fails.
pub fn insert_waivers(
    cx: &mut Context<'_>,
    selected: Vec<(Uuid, Violation)>,
    reason: String,
    user_name: UserName,
) -> Result<Vec<Uuid>, CommandError> {
    let author = user_name(cx.user_dirs());
    let created = cx.now().0;
    let mut waivers = Vec::new();
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
