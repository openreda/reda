//! The Command trait, kinds, errors, results and the registry.
//!
//! Every action in REDA is a Command (ADR-0001). A crate defines its Commands as types
//! implementing [`Command`] and adds them in its own `register_all(&mut Registry)`; the
//! engine assembles the full [`Registry`] from every crate's `register_all()`.

mod command;
mod context;
mod error;
mod history;
mod inputs;
mod registry;

pub use command::{AvailabilityContext, Command, Kind, Meta, NO_PROJECT, NoParams, Unavailable};
pub use context::{Context, Env, Session};
pub use error::{CommandError, ErrorCode};
pub use history::{Entry, History};
pub use inputs::{Clock, FixedClock, IdSource, RandomIds, SeededIds, SystemClock, Timestamp};
pub use registry::{Registry, scope_key};
