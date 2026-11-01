//! Every persisted design-data type. Data only.
//!
//! A [`Project`] owns per-Document tables keyed by [`Uuid`]. It changes only through a
//! [`Transaction`], which records a [`ChangeSet`] of before/after states per object. The
//! file format is not defined here: the engine maps these types to canonical JSON.

mod change;
mod document;
mod project;
mod transaction;
mod uuid;

pub use change::{Change, ChangeSet, Object, Scope, ScopeSummary};
pub use document::{DocumentEntry, DocumentKind, SchematicSheet};
pub use project::{Project, ProjectProps};
pub use transaction::{Transaction, TxError};
pub use uuid::{ParseUuidError, Uuid};

/// Geometry types, re-exported so model users need no direct `reda-geom` dependency.
pub use reda_geom as geom;
