//! `pcb.designator_text.set`: moving, turning, resizing or hiding one Footprint's
//! Designator text.

use reda_command::{
    Angle, AvailabilityContext, Command, CommandError, Context, Kind, Length, Meta, Registry,
    Unavailable,
};
use reda_model::{FootprintText, InDocument, Object, TextStyle, Uuid};
use schemars::JsonSchema;
use serde::Deserialize;

use super::Placed;
use crate::commands::outline::PointParam;
use crate::commands::{needs_pcb, pcb, positive};
use crate::placed::{Frame, designator_text};

pub(super) fn register_all(registry: &mut Registry) {
    registry.add(DesignatorTextSet);
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DesignatorTextSetParams {
    /// The Footprint whose Designator text to change.
    pub footprint: Uuid,
    /// Where the text centre goes on the board.
    #[serde(default)]
    pub at: Option<PointParam>,
    /// Its direction on the board, counter-clockwise.
    #[serde(default)]
    pub rotation: Option<Angle>,
    /// The text height; the pen width follows it.
    #[serde(default)]
    pub size: Option<Length>,
    #[serde(default)]
    pub visible: Option<bool>,
    /// Go back to the Footprint's own placement; nothing else may be given with it.
    #[serde(default)]
    pub reset: bool,
}

pub struct DesignatorTextSet;

impl Command for DesignatorTextSet {
    const META: Meta = Meta {
        id: "pcb.designator_text.set",
        title: "Set Designator Text",
        description: "Moves, turns, resizes or hides one Footprint's Designator text. `at` \
                      and `rotation` are on the board; the text keeps its place relative to \
                      the Footprint when the Footprint moves. `reset` goes back to the \
                      Footprint's own placement. Allowed on locked Footprints.",
        category: "pcb",
        icon: "text",
        kind: Kind::Edit,
    };
    type Params = DesignatorTextSetParams;
    type Output = Placed;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: DesignatorTextSetParams,
    ) -> Result<Placed, CommandError> {
        let pcb = pcb(cx)?;
        let Some(instance) = pcb.footprint(p.footprint) else {
            return Err(CommandError::invalid_params(
                "/footprint",
                format!("{} is not a Footprint on the board", p.footprint),
            ));
        };
        let edits =
            p.at.is_some() || p.rotation.is_some() || p.size.is_some() || p.visible.is_some();
        if p.reset && edits {
            return Err(CommandError::invalid_params(
                "/reset",
                "reset takes no other changes",
            ));
        }
        let mut instance = instance.clone();
        if p.reset {
            instance.designator_text = None;
        } else {
            let frame = Frame::of(&instance);
            let Some(current) = designator_text(pcb, &instance) else {
                return Err(CommandError::rejected(format!(
                    "{} has no Designator text: its Footprint draws none",
                    instance.designator
                )));
            };
            let style = match p.size {
                Some(size) => TextStyle {
                    font: current.style.font.clone(),
                    ..TextStyle::of_height(positive("/size", size)?)
                },
                None => current.style.clone(),
            };
            let text = FootprintText {
                at: p.at.map_or(current.at, |at| frame.local(at.point())),
                rotation: p
                    .rotation
                    .map_or(current.rotation, |r| frame.local_angle(r.millideg())),
                style,
                visible: p.visible.unwrap_or(current.visible),
                ..current
            };
            instance.designator_text = Some(text);
        }
        let document = pcb.uuid;
        let uuid = instance.uuid;
        cx.tx()?.replace(Object::FootprintInstance(InDocument::new(
            document, instance,
        )))?;
        Ok(Placed {
            objects: vec![uuid],
        })
    }
}
