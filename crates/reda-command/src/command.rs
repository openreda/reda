//! The Command trait, its kinds and its metadata.

use reda_model::Project;
use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::{CommandError, Context};

/// What a Command may touch. Undoability follows from the kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// Mutates Documents in one transaction; undoable. Gets [`Context::tx`].
    Edit,
    /// Read-only; returns data. Gets [`Context::project`].
    Query,
    /// GUI or session state. Never undoable, never dirties, unavailable headless.
    View,
    /// Touches disk: Project new/open/save/close and exports. Never undoable, works
    /// headless. Gets [`Context::session`].
    File,
}

/// A Command's static description, used by menus, the catalogue and MCP.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Meta {
    /// Dotted lowercase snake_case: `domain.noun.verb`, such as `project.save`.
    pub id: &'static str,
    /// English title, such as "Save Project".
    pub title: &'static str,
    /// One or two sentences, written for tooltips and agents alike.
    pub description: &'static str,
    pub category: &'static str,
    /// A semantic icon key, such as `"save"`.
    pub icon: &'static str,
    pub kind: Kind,
}

/// Why a Command cannot run now. Menus grey it out and show the reason.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unavailable {
    pub reason: String,
}

/// What [`Command::available`] can look at. The GUI adds the active Document and the
/// selection in a later ticket.
#[derive(Clone, Copy, Debug)]
pub struct AvailabilityContext<'a> {
    pub project: Option<&'a Project>,
}

/// The reason given when an edit or query runs with no Project open.
pub const NO_PROJECT: &str = "no Project is open";

/// One action. Define a unit struct, implement this, and add it to the crate's
/// `register_all(&mut Registry)`.
pub trait Command: Send + Sync + 'static {
    const META: Meta;
    /// Deserialised from the caller's JSON and described by its JSON Schema. Use
    /// `#[serde(deny_unknown_fields)]`, and [`crate::Length`] / [`crate::Angle`] for lengths
    /// and angles.
    type Params: DeserializeOwned + JsonSchema;
    /// Its schema is the MCP `output_schema`. Lengths and angles are integers here.
    type Output: Serialize + JsonSchema;

    /// Whether the Command can run now. The default asks edits and queries for an open
    /// Project and lets view and file Commands always run.
    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        match Self::META.kind {
            Kind::Edit | Kind::Query if cx.project.is_none() => Err(Unavailable {
                reason: NO_PROJECT.into(),
            }),
            _ => Ok(()),
        }
    }

    /// Named sets of fixed params, run as `<id>:<name>`. Menus, shortcuts and the Palette
    /// bind to them like Commands. Read once, when the Command is registered.
    fn presets(&self) -> Vec<Preset> {
        Vec::new()
    }

    /// Does the work. An edit that returns an error is rolled back by the registry.
    fn run(&self, cx: &mut Context<'_>, params: Self::Params)
    -> Result<Self::Output, CommandError>;
}

/// A named, fixed set of params for a Command, such as `panel.show:properties`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Preset {
    /// Lowercase snake_case; the Preset's id is `<command id>:<name>`.
    pub name: String,
    /// English title, such as "Show Properties".
    pub title: String,
    /// A semantic icon key.
    pub icon: String,
    /// The params the Command runs with. They must fit its params schema.
    pub params: serde_json::Value,
}

impl Preset {
    pub fn new(
        name: impl Into<String>,
        title: impl Into<String>,
        icon: impl Into<String>,
        params: serde_json::Value,
    ) -> Preset {
        Preset {
            name: name.into(),
            title: title.into(),
            icon: icon.into(),
            params,
        }
    }
}

/// Params for a Command that takes none: accepts `{}` (or `null`) and refuses any field.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NoParams {}
