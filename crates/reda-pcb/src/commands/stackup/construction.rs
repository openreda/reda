//! Save a board's physical build into a Project copy of a Fab Preset.
use super::needs_pcb;
use reda_command::{AvailabilityContext, Command, CommandError, Context, Kind, Meta, Unavailable};
use reda_model::{Construction, Object, Uuid};
use schemars::JsonSchema;
use serde::Deserialize;

pub(super) struct Save;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SaveParams {
    document: Uuid,
    preset: Uuid,
    name: String,
}

impl Command for Save {
    const META: Meta = Meta {
        id: "pcb.stackup.save_as_construction",
        title: "Save as Construction",
        description: "Saves this board's Stackup and Via types into a named Construction in a Project Fab Preset copy.",
        category: "pcb",
        icon: "save",
        kind: Kind::Edit,
    };
    type Params = SaveParams;
    type Output = ();

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: SaveParams) -> Result<(), CommandError> {
        let name = p.name.trim();
        if name.is_empty() {
            return Err(CommandError::invalid_params(
                "/name",
                "the Construction name is empty",
            ));
        }
        let project = cx.project()?;
        let pcb = project
            .pcb_document(p.document)
            .ok_or_else(|| CommandError::not_found("no such PCB Document"))?;
        let mut preset = project
            .fab_preset(p.preset)
            .ok_or_else(|| CommandError::not_found("no such Project Fab Preset"))?
            .clone();
        let builds = &mut preset.constructions;
        let build = Construction::capture(name.to_owned(), pcb.stackup(), builds);
        if let Some(previous) = builds
            .iter_mut()
            .find(|b| b.name.eq_ignore_ascii_case(name))
        {
            *previous = build;
        } else {
            builds.push(build);
        }
        cx.tx()?.replace(Object::FabPreset(preset))?;
        Ok(())
    }
}
