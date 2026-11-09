//! `view.messages.step`: the next and previous Violation keys. The Command queues the
//! step as view state; the Shell takes it and goes to that Violation.

use reda_engine::command::{Command, CommandError, Context, Kind, Meta, Preset};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;

/// Which way to step.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Step {
    Next,
    Previous,
}

/// The steps asked for and not yet taken: view state the Shell drains.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MessageSteps(pub Vec<Step>);

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StepParams {
    /// Go to the next Violation in the Messages list, or the previous one.
    direction: Step,
}

pub(super) struct StepCommand;

impl Command for StepCommand {
    const META: Meta = Meta {
        id: "view.messages.step",
        title: "Go to Next or Previous Violation",
        description: "Goes to the next or previous Violation in the Messages list, in the \
                      order it shows them: opens its Document, zooms to it and selects \
                      its objects. It wraps around at either end. View state only.",
        category: "view",
        icon: "erc",
        kind: Kind::View,
    };
    type Params = StepParams;
    type Output = ();

    fn presets(&self) -> Vec<Preset> {
        vec![
            Preset::new(
                "next",
                "Next Violation",
                "erc",
                json!({"direction": "next"}),
            ),
            Preset::new(
                "previous",
                "Previous Violation",
                "erc",
                json!({"direction": "previous"}),
            ),
        ]
    }

    fn run(&self, cx: &mut Context<'_>, p: StepParams) -> Result<(), CommandError> {
        cx.view::<MessageSteps>()?.0.push(p.direction);
        Ok(())
    }
}
