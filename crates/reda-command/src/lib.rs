//! The Command trait, kinds, errors, results and the registry.
//!
//! Every action in REDA is a Command (ADR-0001). A crate defines its Commands as types
//! implementing [`Command`] and adds them in its own `register_all(&mut Registry)`; the
//! engine assembles the full [`Registry`] from every crate's `register_all()`.

mod batch;
mod command;
mod context;
mod error;
mod fonts;
mod history;
mod inputs;
mod library;
mod registry;
mod schema;
mod signals;
mod trash;
mod units;
mod user;
mod view;

pub use command::{
    AvailabilityContext, Command, Kind, Meta, NO_PROJECT, NoParams, Preset, Unavailable,
};
pub use context::{Context, Env, Session};
pub use error::{CommandError, ErrorCode};
pub use fonts::{FontFace, Fonts};
pub use history::{Entry, History};
pub use inputs::{Clock, FixedClock, IdSource, RandomIds, SeededIds, SystemClock, Timestamp};
pub use library::{ElementLocation, FootprintCopyData, NoLibraries, PartCopy, PartSource};
pub use registry::{Registry, scope_key};
pub use signals::{CancelToken, Progress, Signals};
pub use trash::Trash;
pub use units::{Angle, Length};
pub use user::UserDirs;
pub use view::ViewState;
