//! `recovery.restore`: loads Recovery Snapshots left by an unclean exit back into the open
//! Project as unsaved changes.

use reda_command::{Command, CommandError, Context, Kind, Meta, NO_PROJECT, Registry};
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
    /// Snapshot file names in the Project's `.reda/recovery/` folder (with no Project
    /// open, the per-user `recovery/` folder), as its `manifest.json` lists them.
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
                      snapshot files are left in place for the app to delete. With no \
                      Project open it restores Library element snapshots from the per-user \
                      `recovery/` folder, each as an open element with unsaved changes.",
        category: "project",
        icon: "project_open",
        kind: Kind::File,
    };
    type Params = RecoveryRestoreParams;
    type Output = RecoveryRestoreResult;

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: RecoveryRestoreParams,
    ) -> Result<RecoveryRestoreResult, CommandError> {
        let session = cx.session()?;
        let scopes = match (&session.project, &session.user_dirs) {
            (Some(_), _) => {
                let folder = session
                    .location
                    .clone()
                    .ok_or_else(|| CommandError::unavailable("the Project has no folder yet"))?;
                recovery::restore(session, &recovery::project_dir(&folder), &p.snapshots)?
            }
            (None, Some(dirs)) => {
                let dir = recovery::user_dir(dirs);
                recovery::restore_elements(session, &dir, &p.snapshots)?
            }
            (None, None) => return Err(CommandError::unavailable(NO_PROJECT)),
        };
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
