//! The copper group displayed in each open Footprint canvas.
use super::FootprintEditorState;
use reda_engine::{
    Engine,
    command::{CommandError, Context},
    model::{Footprint, LibraryElement, Side, Uuid},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CopperLayer {
    #[default]
    Top,
    Middle,
    Bottom,
}

impl CopperLayer {
    pub(super) fn available(self, footprint: &Footprint) -> bool {
        self != Self::Middle || footprint.pads.iter().any(|p| p.middle.is_some())
    }

    pub(crate) fn side(self) -> Option<Side> {
        match self {
            Self::Top => Some(Side::Top),
            Self::Middle => None,
            Self::Bottom => Some(Side::Bottom),
        }
    }
}

pub(crate) fn preview_layer(engine: &Engine, footprint: &Footprint) -> CopperLayer {
    engine
        .view()
        .get::<FootprintEditorState>()
        .and_then(|s| s.preview_layers.get(&footprint.uuid).copied())
        .filter(|layer| layer.available(footprint))
        .unwrap_or_default()
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct Preview {
    footprint: Uuid,
    layer: CopperLayer,
}

impl Preview {
    pub(super) fn apply(&self, cx: &mut Context<'_>) -> Result<(), CommandError> {
        let Some(LibraryElement::Footprint(fp)) = cx.project()?.element(self.footprint) else {
            return Err(CommandError::not_found(format!(
                "no open Footprint has UUID {}",
                self.footprint
            )));
        };
        if !self.layer.available(fp) {
            return Err(CommandError::invalid_params(
                "/preview/layer",
                "the Footprint has no Middle copper group",
            ));
        }
        cx.view::<FootprintEditorState>()?
            .preview_layers
            .insert(self.footprint, self.layer);
        Ok(())
    }
}
