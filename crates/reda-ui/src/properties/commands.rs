//! `view.properties.rules`: right-click **Applicable Rules** goes to the Properties
//! panel's Rules region (docs/spec/v0.1/checks.md#explaining-a-rule).

use reda_engine::command::{Command, CommandError, Context, Kind, Meta, NoParams, Registry};
use schemars::JsonSchema;
use serde::Serialize;

use crate::layout::{Layout, PanelId};

/// How many times the Rules region was asked for: view state the panel compares with the
/// count it last revealed, to open the region and scroll to it once per request.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RevealRules(pub u64);

/// Adds `view.properties.rules` to `registry`. The Shell does this.
pub fn register_all(registry: &mut Registry) {
    registry.add(ShowRules);
}

#[derive(Serialize, JsonSchema)]
pub struct ShowRulesResult {}

struct ShowRules;

impl Command for ShowRules {
    const META: Meta = Meta {
        id: "view.properties.rules",
        title: "Applicable Rules",
        description: "Shows the Properties panel on its Rules region: the design rules that \
                      apply to the selected copper (`pcb.rules.applicable`), with the \
                      winning, losing and disabled records.",
        category: "view",
        icon: "rules",
        kind: Kind::View,
    };
    type Params = NoParams;
    type Output = ShowRulesResult;

    fn run(&self, cx: &mut Context<'_>, _: NoParams) -> Result<ShowRulesResult, CommandError> {
        cx.view::<Layout>()?.show_panel(PanelId::Properties)?;
        cx.view::<RevealRules>()?.0 += 1;
        Ok(ShowRulesResult {})
    }
}
