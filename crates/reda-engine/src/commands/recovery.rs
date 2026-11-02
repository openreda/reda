//! `recovery.restore`: loads Recovery Snapshots left by an unclean exit back into the open
//! Project as unsaved changes.

use reda_command::{
    AvailabilityContext, Command, CommandError, Context, Kind, Meta, NO_PROJECT, Registry,
    Unavailable,
};
use reda_model::Scope;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::recovery;

pub(crate) fn register_all(registry: &mut Registry) {
    registry.add(RecoveryRestore);
}

pub struct RecoveryRestore;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RecoveryRestoreParams {
    /// Snapshot file names in the Project's `.reda/recovery/` folder, as its
    /// `manifest.json` lists them.
    pub snapshots: Vec<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct RecoveryRestoreResult {
    /// What was restored, each now unsaved: `"project"` for the Project file, else the
    /// UUID of a Document or Library element.
    pub restored: Vec<String>,
}

impl Command for RecoveryRestore {
    const META: Meta = Meta {
        id: "recovery.restore",
        title: "Restore Recovery Snapshot",
        description: "Loads Recovery Snapshots from the open Project's `.reda/recovery/` \
                      folder as unsaved changes on top of their files, in one step that \
                      undoes like an edit. A snapshot whose Document is gone recreates it \
                      as an unsaved Document. An unreadable snapshot is refused. The \
                      snapshot files are left in place for the app to delete.",
        category: "project",
        icon: "project_open",
        kind: Kind::File,
    };
    type Params = RecoveryRestoreParams;
    type Output = RecoveryRestoreResult;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        match cx.project {
            Some(_) => Ok(()),
            None => Err(Unavailable {
                reason: NO_PROJECT.into(),
            }),
        }
    }

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: RecoveryRestoreParams,
    ) -> Result<RecoveryRestoreResult, CommandError> {
        let session = cx.session()?;
        let folder = session
            .location
            .clone()
            .ok_or_else(|| CommandError::unavailable("the Project has no folder yet"))?;
        let scopes = recovery::restore(session, &recovery::project_dir(&folder), &p.snapshots)?;
        Ok(RecoveryRestoreResult {
            restored: scopes
                .into_iter()
                .map(|s| match s {
                    Scope::Project => "project".to_owned(),
                    Scope::Document(uuid) | Scope::Element(uuid) => uuid.to_string(),
                })
                .collect(),
        })
    }
}
