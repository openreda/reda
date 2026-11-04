//! The text geometry query, for outputs, rendering and agents.

use reda_command::{AvailabilityContext, Command, CommandError, Context, Kind, Meta, Unavailable};
use reda_model::Uuid;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{needs_pcb, pcb};
use crate::text::{PlacedText, text_geometry};

pub struct TextGeometry;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TextGeometryParams {
    /// The text objects to lay out; every text on the board when left out.
    #[serde(default)]
    pub texts: Option<Vec<Uuid>>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct TextGeometryResult {
    /// Each text's geometry in board coordinates, in the order asked (sorted by UUID when
    /// every text is returned).
    pub texts: Vec<PlacedText>,
    /// The font families among the returned texts that are missing on this machine, drawn
    /// as Noto Sans instead.
    pub missing_fonts: Vec<String>,
}

impl Command for TextGeometry {
    const META: Meta = Meta {
        id: "pcb.text.geometry",
        title: "Text Geometry",
        description: "Returns the geometry of PCB text in board coordinates, as outputs and \
                      the canvas draw it: stroke-font capsules (with their pen width) and \
                      filled outline-font triangles. Text in a font missing on this machine \
                      is drawn as Noto Sans and listed under `missing_fonts`.",
        category: "pcb",
        icon: "text",
        kind: Kind::Query,
    };
    type Params = TextGeometryParams;
    type Output = TextGeometryResult;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: TextGeometryParams,
    ) -> Result<TextGeometryResult, CommandError> {
        let pcb = pcb(cx)?;
        let fonts = cx.fonts();
        let texts = match p.texts {
            None => pcb.texts().collect(),
            Some(uuids) => uuids
                .into_iter()
                .map(|uuid| {
                    pcb.text(uuid)
                        .ok_or_else(|| CommandError::not_found(format!("no text has UUID {uuid}")))
                })
                .collect::<Result<Vec<_>, _>>()?,
        };
        let texts: Vec<PlacedText> = texts.into_iter().map(|t| text_geometry(t, fonts)).collect();
        let mut missing: Vec<String> = texts
            .iter()
            .filter(|t| t.missing_font)
            .map(|t| t.font.clone())
            .collect();
        missing.sort();
        missing.dedup();
        Ok(TextGeometryResult {
            texts,
            missing_fonts: missing,
        })
    }
}
