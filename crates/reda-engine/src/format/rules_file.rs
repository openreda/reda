//! `<name>.redarules`: a board's design rules made portable between boards.
//!
//! ```text
//! {
//!   "format": 1,
//!   "net_classes": [{"name":"Power"}],
//!   "rules": [
//!     {"kind":"clearance","values":{"value":160000}},
//!     {"kind":"clearance","net_class":"Power","values":{"value":300000}},
//!     …
//!   ],
//!   "severities": {"clearance":"error",…}
//! }
//! ```
//!
//! `net_classes` lists the class names in rank order; a record names its class by name.
//! There is no Net membership and there are no UUIDs, because both differ per board.

use std::path::Path;

use reda_model::{DrcSeverities, PortableClass, PortableRule, RuleSet};
use schemars::JsonSchema;
use serde::Deserialize;

use super::{FORMAT, FileKind, LoadError, parse};
use crate::canonical::{CanonicalError, CanonicalWriter};

/// A REDA rules file (`.redarules`).
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(title = "REDA rules file")]
pub(crate) struct RulesFile {
    /// The file-format version. REDA writes only the current one.
    #[allow(dead_code, reason = "checked by the migration chain")]
    #[schemars(schema_with = "super::schema::format_version")]
    format: u32,
    /// The Net Classes the rules name, by name, highest rank first.
    net_classes: Vec<PortableClass>,
    /// The rule records: board-wide ones, then each class's, which name it.
    rules: Vec<PortableRule>,
    /// The DRC severities.
    severities: DrcSeverities,
}

pub(crate) fn read_rules_file(path: &Path, bytes: &[u8]) -> Result<RuleSet, LoadError> {
    let file: RulesFile = parse(FileKind::Rules, path, bytes)?;
    let set = RuleSet {
        net_classes: file.net_classes,
        rules: file.rules,
        severities: file.severities,
    };
    if let Some(why) = reda_pcb::rules::transfer::problem(&set) {
        return Err(LoadError::Invalid(path.to_owned(), why));
    }
    Ok(set)
}

pub(crate) fn write_rules_file(set: &RuleSet) -> Result<Vec<u8>, CanonicalError> {
    let mut w = CanonicalWriter::new(FORMAT);
    w.field("net_classes", &set.net_classes)?;
    w.list("rules", &set.rules)?;
    w.field("severities", &set.severities)?;
    Ok(w.finish())
}
