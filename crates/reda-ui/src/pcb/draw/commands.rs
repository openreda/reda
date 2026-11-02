//! `pcb.view.origin.set`: the user origin the cursor position reads from. A view
//! Command, so the user origin Tool, scripts and agents set it the same way.

use reda_engine::command::{
    AvailabilityContext, Command, CommandError, Context, Kind, Length, Meta, Registry, Unavailable,
};
use reda_engine::model::geom::Point;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::pcb::view::BoardView;

/// Adds the drawing Tools' view Commands to `registry`.
pub fn register_all(registry: &mut Registry) {
    registry.add(OriginSet);
}

/// A point on the board.
#[derive(Clone, Copy, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PointParam {
    x: Length,
    y: Length,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OriginSetParams {
    /// Where the cursor position is measured from; left out, the board's own origin.
    #[serde(default)]
    at: Option<PointParam>,
}

/// The user origin now in force.
#[derive(Serialize, JsonSchema)]
pub struct Origin {
    #[serde(skip_serializing_if = "Option::is_none")]
    at: Option<Point>,
}

struct OriginSet;

impl Command for OriginSet {
    const META: Meta = Meta {
        id: "pcb.view.origin.set",
        title: "Set User Origin",
        description: "Sets the user origin that the cursor position on the board is \
                      measured from, or with no point goes back to the board's own origin. \
                      It changes only the view, not the design or the outputs (the \
                      Placement Origin does that).",
        category: "pcb_view",
        icon: "user_origin",
        kind: Kind::View,
    };
    type Params = OriginSetParams;
    type Output = Origin;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        match cx.project {
            Some(p) if p.pcb().is_some() => Ok(()),
            Some(_) => Err(Unavailable {
                reason: "the Project has no PCB Document".into(),
            }),
            None => Err(Unavailable {
                reason: "no Project is open".into(),
            }),
        }
    }

    fn run(&self, cx: &mut Context<'_>, p: OriginSetParams) -> Result<Origin, CommandError> {
        let at = p.at.map(|a| Point::new(a.x.nm(), a.y.nm()));
        cx.view::<BoardView>()?.origin = at;
        Ok(Origin { at })
    }
}
