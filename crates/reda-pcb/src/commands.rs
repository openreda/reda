//! The PCB Commands. Each one edits the Project's PCB Document and is unavailable while
//! the Project has none.

mod graphic;
mod layer;
mod net_class;
mod outline;
mod rules;
mod text;

use reda_command::{
    AvailabilityContext, CommandError, Context, Length, NO_PROJECT, Registry, Unavailable,
};
use reda_model::geom::Nm;
use reda_model::{PcbDocument, Uuid};
use schemars::JsonSchema;
use serde::Serialize;

/// Adds every PCB Command.
pub fn register_all(registry: &mut Registry) {
    outline::register_all(registry);
    rules::register_all(registry);
    net_class::register_all(registry);
    text::register_all(registry);
    graphic::register_all(registry);
}

/// Why a PCB Command cannot run in a Project without a PCB Document.
const NO_PCB: &str = "the Project has no PCB Document; add one with `document.add {kind: pcb}`";

/// The availability of every PCB Command: an open Project with a PCB Document.
fn needs_pcb(cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
    let reason = match cx.project {
        None => NO_PROJECT,
        Some(project) if project.pcb().is_none() => NO_PCB,
        Some(_) => return Ok(()),
    };
    Err(Unavailable {
        reason: reason.into(),
    })
}

/// The Project's PCB Document, as the edit has left it so far.
fn pcb<'c>(cx: &'c Context<'_>) -> Result<&'c PcbDocument, CommandError> {
    cx.project()?
        .pcb()
        .ok_or_else(|| CommandError::unavailable(NO_PCB))
}

/// What a Command that creates one object returns.
#[derive(Debug, Serialize, JsonSchema)]
pub struct Created {
    /// The new object's UUID.
    pub uuid: Uuid,
}

/// A length param that must not be negative; `pointer` names it in the error.
fn non_negative(pointer: &str, length: Length) -> Result<Nm, CommandError> {
    let nm = length.nm();
    if nm < Nm::ZERO {
        return Err(CommandError::invalid_params(
            pointer,
            format!("{} cannot be negative", &pointer[1..].replace('_', " ")),
        ));
    }
    Ok(nm)
}

/// An optional length param that must not be negative.
fn optional_non_negative(
    pointer: &str,
    length: Option<Length>,
) -> Result<Option<Nm>, CommandError> {
    length.map(|l| non_negative(pointer, l)).transpose()
}

/// A length param that must be positive; `pointer` names it in the error.
fn positive(pointer: &str, length: Length) -> Result<Nm, CommandError> {
    let nm = length.nm();
    if nm <= Nm::ZERO {
        return Err(CommandError::invalid_params(
            pointer,
            format!("the {} must be positive", &pointer[1..].replace('_', " ")),
        ));
    }
    Ok(nm)
}
