//! A test-only object, behind the `test-objects` feature.

use serde::Serialize;

use crate::Uuid;

/// A test-only object holding one integer. It lives in the Document named by `document`,
/// or in the Project file with `None`, so tests can edit any scope before real object
/// kinds exist. It is never written to disk.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Probe {
    pub uuid: Uuid,
    pub document: Option<Uuid>,
    pub value: i64,
}
