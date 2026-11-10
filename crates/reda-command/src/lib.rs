//! The Command trait, kinds, errors, results and the registry.
//!
//! Every action in REDA is a Command (ADR-0001). A crate defines its Commands as types
//! implementing [`Command`] and adds them in its own `register_all(&mut Registry)`; the
//! engine assembles the full [`Registry`] from every crate's `register_all()`.

mod batch;
mod command;
mod context;
mod error;
mod fingerprint;
mod fonts;
mod history;
mod inputs;
mod library;
mod pad;
mod registry;
mod schema;
mod signals;
mod trash;
mod units;
mod user;
mod view;
mod waivers;

pub use command::{
    AvailabilityContext, Command, Kind, Meta, NO_ELEMENTS, NO_PROJECT, NoParams, Preset,
    Unavailable,
};
pub use context::{Context, Env, Session};
pub use error::{CommandError, ErrorCode};
pub use fingerprint::Fingerprint;
pub use fonts::{FontFace, Fonts};
pub use history::{Entry, History};
#[cfg(not(target_arch = "wasm32"))]
pub use inputs::SystemClock;
pub use inputs::{Clock, FixedClock, IdSource, RandomIds, SeededIds, Timestamp};
pub use library::{ElementLocation, FootprintCopyData, NoLibraries, PartCopy, PartSource};
pub use pad::{
    HoleParam, HoleToleranceParam, MaskParam, PasteParam, RatioParam, ZoneConnectionParam, mask,
    paste, zone_connection,
};
pub use registry::{Registry, Settle, scope_key};
pub use signals::{CancelToken, Progress, Signals};
pub use trash::Trash;
pub use units::{Angle, AreaValue, CopperThickness, Length};
pub use user::{UserDirs, UserName, os_user_name};
pub use view::ViewState;

pub use waivers::{insert_waivers, select_waivers};
