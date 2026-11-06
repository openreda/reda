//! Footprint generators: one Command per package family that computes an IPC-7351
//! Nominal Footprint, writes it into a chosen Library and runs the
//! Library check on it.
//!
//! A family is a [`FootprintFamily`]: its Command metadata, its params, and a pure
//! function from params to a [`FootprintPlan`]. [`Generator`] turns any family into a
//! Command; the courtyard, Assembly outline, text and UUIDs are added the same way for
//! all of them. To add a family, write a module with a `FootprintFamily` impl and add one
//! line to [`register_generators`].
//!
//! Writing to disk is the engine's job: it hands [`register_generators`] a
//! [`FootprintStore`], so this crate stays free of file IO.

mod chip;
mod gull_wing;
pub mod ipc;
mod no_lead;
mod pattern;
mod qfp;
mod quad;
mod small_outline;
mod sot;
mod through_hole;

use std::marker::PhantomData;
use std::path::PathBuf;

use reda_command::{Command, CommandError, Context, Length, Meta, Registry, Session};
use reda_model::{Footprint, FootprintGenerator, LibraryElement, Symbol, Uuid, Violation};
use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

pub use chip::{Chip, ChipClass, ChipDimensions, ChipParams, ChipSize};
pub use gull_wing::GullWingDimensions;
pub use no_lead::{Dfn, ExposedPadDimensions, NoLeadDimensions, NoLeadParams, Qfn};
pub use pattern::{Body, FootprintPlan, PadSpec};
pub use qfp::{Qfp, QfpDimensions, QfpParams};
pub use small_outline::{SmallOutlineParams, Soic, SoicParams, Ssop, Tssop};
pub use sot::{Sot, SotPackage, SotParams};
pub use through_hole::{Dip, DipParams, Header, HeaderParams, HeaderPitch};

use crate::{Elements, check, check_element_name};

/// Writes a generated Footprint into the registered Library `library` and returns its
/// file. It refuses a Library that is not registered or is read-only, and a file that
/// already exists.
pub type FootprintStore = fn(
    session: &mut Session,
    library: Uuid,
    footprint: &Footprint,
) -> Result<PathBuf, CommandError>;

/// Adds every generator Command, each writing through `store`.
pub fn register_generators(registry: &mut Registry, store: FootprintStore) {
    registry.add(Generator::<Chip>::new(store));
    registry.add(Generator::<Sot>::new(store));
    registry.add(Generator::<Soic>::new(store));
    registry.add(Generator::<Ssop>::new(store));
    registry.add(Generator::<Tssop>::new(store));
    registry.add(Generator::<Dfn>::new(store));
    registry.add(Generator::<Qfn>::new(store));
    registry.add(Generator::<Qfp>::new(store));
    registry.add(Generator::<Dip>::new(store));
    registry.add(Generator::<Header>::new(store));
}

/// Every generator Command's id, in the order [`register_generators`] adds them.
pub const GENERATORS: [&str; 10] = [
    <Chip as FootprintFamily>::META.id,
    <Sot as FootprintFamily>::META.id,
    <Soic as FootprintFamily>::META.id,
    <Ssop as FootprintFamily>::META.id,
    <Tssop as FootprintFamily>::META.id,
    <Dfn as FootprintFamily>::META.id,
    <Qfn as FootprintFamily>::META.id,
    <Qfp as FootprintFamily>::META.id,
    <Dip as FootprintFamily>::META.id,
    <Header as FootprintFamily>::META.id,
];

type Preview = fn(&serde_json::Value) -> Result<Footprint, CommandError>;

const PREVIEWS: [Preview; 10] = [
    preview_of::<Chip>,
    preview_of::<Sot>,
    preview_of::<Soic>,
    preview_of::<Ssop>,
    preview_of::<Tssop>,
    preview_of::<Dfn>,
    preview_of::<Qfn>,
    preview_of::<Qfp>,
    preview_of::<Dip>,
    preview_of::<Header>,
];

/// The Footprint the generator Command `command` would write with `params`, computed
/// without writing anything (its UUIDs count up from 1), for a generator form's preview. `None`
/// when `command` is not a generator; `invalid_params` when the params do not work.
pub fn preview(
    command: &str,
    params: &serde_json::Value,
) -> Option<Result<Footprint, CommandError>> {
    let i = GENERATORS.iter().position(|id| *id == command)?;
    Some(PREVIEWS[i](params))
}

fn preview_of<F: FootprintFamily>(params: &serde_json::Value) -> Result<Footprint, CommandError> {
    let p: F::Params = serde_json::from_value(params.clone())
        .map_err(|e| CommandError::invalid_params("", e.to_string()))?;
    let target = F::target(&p);
    let pattern = F::plan(&p)?;
    let name = target.name.unwrap_or_else(|| pattern.name.clone());
    let mut next = 0;
    let mut footprint = pattern.into_footprint(name, || {
        next += 1;
        Uuid::from_u128(next)
    });
    footprint.generator = Some(Box::new(generator_of::<F>(&p)?));
    Ok(footprint)
}

/// One package family.
pub trait FootprintFamily: Send + Sync + 'static {
    /// The Command's metadata; its kind must be `Kind::File`, since it writes a file.
    const META: Meta;
    type Params: DeserializeOwned + Serialize + JsonSchema;

    /// Where the Footprint goes and what it is called.
    fn target(params: &Self::Params) -> Target;

    /// The Footprint, or `invalid_params` saying which param does not work.
    fn plan(params: &Self::Params) -> Result<FootprintPlan, CommandError>;
}

/// The Library a generated Footprint is written into, and its name if the caller gave one
/// in place of the IPC-7351 name.
pub struct Target {
    pub library: Uuid,
    pub name: Option<String>,
}

/// The Command for family `F`.
pub struct Generator<F> {
    store: FootprintStore,
    family: PhantomData<fn() -> F>,
}

impl<F> Generator<F> {
    pub fn new(store: FootprintStore) -> Generator<F> {
        Generator {
            store,
            family: PhantomData,
        }
    }
}

/// What a generator returns.
#[derive(Debug, Serialize, JsonSchema)]
pub struct Generated {
    /// The new Footprint's UUID.
    pub footprint: Uuid,
    pub name: String,
    /// The Library it was written into.
    pub library: Uuid,
    /// The file written.
    pub file: PathBuf,
    /// What the Library check found in the new Footprint.
    pub violations: Vec<Violation>,
}

/// A Footprint references no other element, so the check needs no lookups.
struct NoElements;

impl Elements for NoElements {
    fn symbol(&self, _: Uuid) -> Option<&Symbol> {
        None
    }

    fn footprint(&self, _: Uuid) -> Option<&Footprint> {
        None
    }
}

impl<F: FootprintFamily> Command for Generator<F> {
    const META: Meta = F::META;
    type Params = F::Params;
    type Output = Generated;

    fn run(&self, cx: &mut Context<'_>, p: F::Params) -> Result<Generated, CommandError> {
        let target = F::target(&p);
        let pattern = F::plan(&p)?;
        let name = target.name.unwrap_or_else(|| pattern.name.clone());
        check_element_name(&name).map_err(|why| CommandError::invalid_params("/name", why))?;
        let mut footprint = pattern.into_footprint(name, || cx.new_id());
        footprint.generator = Some(Box::new(generator_of::<F>(&p)?));
        let file = (self.store)(cx.session()?, target.library, &footprint)?;
        let violations = check(&NoElements, &LibraryElement::Footprint(footprint.clone()));
        Ok(Generated {
            footprint: footprint.uuid,
            name: footprint.name,
            library: target.library,
            file,
            violations,
        })
    }
}

/// What a Footprint made by family `F` from `params` records about it: the Command and its
/// params, with every default filled in and every length in nanometres.
fn generator_of<F: FootprintFamily>(
    params: &F::Params,
) -> Result<FootprintGenerator, CommandError> {
    let params = serde_json::to_value(params).map_err(|e| CommandError::internal(e.to_string()))?;
    Ok(FootprintGenerator {
        command: F::META.id.to_owned(),
        params,
    })
}

/// A dimension param: its least and greatest value, each an integer in nanometres or a
/// length string.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TolParam {
    pub min: Length,
    pub max: Length,
}

impl TolParam {
    /// The dimension, or `invalid_params` at `pointer` when it is not positive or its
    /// minimum exceeds its maximum.
    pub fn tol(self, pointer: &str) -> Result<ipc::Tol, CommandError> {
        let (min, max) = (self.min.nm(), self.max.nm());
        if min.0 <= 0 {
            return Err(CommandError::invalid_params(
                &format!("{pointer}/min"),
                "the dimension must be greater than zero",
            ));
        }
        if min > max {
            return Err(CommandError::invalid_params(
                &format!("{pointer}/max"),
                "the maximum must not be less than the minimum",
            ));
        }
        Ok(ipc::Tol { min, max })
    }
}

/// `value` in hundredths of a millimetre, rounded, for IPC-7351 names.
pub(crate) fn hundredths(value: reda_model::geom::Nm) -> i64 {
    (value.0 + 5_000) / 10_000
}
