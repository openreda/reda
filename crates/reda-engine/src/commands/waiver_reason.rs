//! `checks.waiver.set_reason`: edits the reason of ERC and DRC Waivers in place, keeping
//! their author and creation time (Violation Details edits it there).

use reda_command::{Command, CommandError, Context, Kind, Meta, Registry};
use reda_model::{Object, Uuid};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub(crate) fn register_all(registry: &mut Registry) {
    registry.add(SetWaiverReason);
}

struct SetWaiverReason;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetWaiverReasonParams {
    /// The Waivers to give the reason, ERC or DRC ones.
    pub waivers: Vec<Uuid>,
    /// The new reason; empty clears it.
    #[serde(default)]
    pub reason: String,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct SetWaiverReasonResult {
    /// The Waivers changed.
    pub waivers: Vec<Uuid>,
}

impl Command for SetWaiverReason {
    const META: Meta = Meta {
        id: "checks.waiver.set_reason",
        title: "Set Waiver Reason",
        description: "Sets the reason of ERC or DRC Waivers, keeping their author and \
                      creation time, as one undo step per Document.",
        category: "checks",
        icon: "waive",
        kind: Kind::Edit,
    };
    type Params = SetWaiverReasonParams;
    type Output = SetWaiverReasonResult;

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: SetWaiverReasonParams,
    ) -> Result<SetWaiverReasonResult, CommandError> {
        if p.waivers.is_empty() {
            return Err(CommandError::invalid_params(
                "/waivers",
                "name at least one Waiver",
            ));
        }
        let mut changed = Vec::new();
        for (i, &uuid) in p.waivers.iter().enumerate() {
            match cx.project()?.object(uuid) {
                Some(Object::Waiver(w)) => {
                    let mut w = w.clone();
                    w.object.reason = p.reason.clone();
                    changed.push(w);
                }
                _ => {
                    return Err(CommandError::invalid_params(
                        &format!("/waivers/{i}"),
                        format!("{uuid} is not a Waiver"),
                    ));
                }
            }
        }
        let tx = cx.tx()?;
        for w in changed {
            tx.replace(Object::Waiver(w))?;
        }
        Ok(SetWaiverReasonResult { waivers: p.waivers })
    }
}
