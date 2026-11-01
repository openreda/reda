//! `edit.undo` and `edit.redo`: one step back or forward on one undo stack.

use reda_command::{Command, CommandError, Context, Kind, Meta, Registry};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::DocumentRef;

pub(crate) fn register_all(registry: &mut Registry) {
    registry.add(EditUndo);
    registry.add(EditRedo);
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StackParams {
    /// The stack: a Document's UUID, or `"project"` for the Project file's own stack.
    pub document: DocumentRef,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct StepResult {
    /// The label of the entry undone or redone, such as "Add Wire".
    pub label: String,
}

pub struct EditUndo;

impl Command for EditUndo {
    const META: Meta = Meta {
        id: "edit.undo",
        title: "Undo",
        description: "Undoes the newest edit on one undo stack: a Document's, or the Project \
                      file's with `document: \"project\"`. A cross-Document edit has one \
                      entry in each Document it touched, undone independently.",
        category: "edit",
        icon: "undo",
        kind: Kind::Edit,
    };
    type Params = StackParams;
    type Output = StepResult;

    fn run(&self, cx: &mut Context<'_>, p: StackParams) -> Result<StepResult, CommandError> {
        let label = cx.undo(p.document.0)?;
        Ok(StepResult { label })
    }
}

pub struct EditRedo;

impl Command for EditRedo {
    const META: Meta = Meta {
        id: "edit.redo",
        title: "Redo",
        description: "Redoes the newest undone edit on one undo stack: a Document's, or the \
                      Project file's with `document: \"project\"`. Any new edit on that \
                      stack clears what could be redone.",
        category: "edit",
        icon: "redo",
        kind: Kind::Edit,
    };
    type Params = StackParams;
    type Output = StepResult;

    fn run(&self, cx: &mut Context<'_>, p: StackParams) -> Result<StepResult, CommandError> {
        let label = cx.redo(p.document.0)?;
        Ok(StepResult { label })
    }
}
