//! `pcb.edge_connector.set`: the board's `edge_connector` property, the card edge's bevel
//! and selective hard gold. Board Setup edits it on the Stackup page, beside the finish.

use reda_command::{
    Angle, AvailabilityContext, Command, CommandError, Context, Kind, Length, Meta, Unavailable,
};
use reda_model::{BoardSetup, EdgeBevel, EdgeConnector, InDocument, Object};
use schemars::JsonSchema;
use serde::Deserialize;

use super::{needs_pcb, pcb};

pub struct EdgeConnectorSet;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(transform = required_key)]
pub struct EdgeConnectorSetParams {
    /// The property; `null` clears it. Required.
    #[serde(default, deserialize_with = "super::nullable")]
    #[schemars(with = "Option<EdgeConnectorParam>")]
    pub edge_connector: Option<Option<EdgeConnectorParam>>,
}

/// Lists `edge_connector` as required in the schema: it must be given, though `null` is a
/// value (it clears the property), which a plain `Option` field would leave optional.
fn required_key(schema: &mut schemars::Schema) {
    if let Some(object) = schema.as_object_mut() {
        let required = object
            .entry("required")
            .or_insert_with(|| serde_json::Value::Array(vec![]));
        if let Some(list) = required.as_array_mut() {
            list.push("edge_connector".into());
        }
    }
}

/// The card edge's bevel and hard gold.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EdgeConnectorParam {
    /// The bevel ground on the card edge; left out for none.
    #[serde(default)]
    pub bevel: Option<BevelParam>,
    /// Selective hard gold on the Edge Connector Pads only; the board finish is unchanged.
    #[serde(default)]
    pub hard_gold: bool,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BevelParam {
    /// The bevel angle, between 0° and 90°, such as `"20deg"`.
    pub angle: Angle,
    /// How far the bevel reaches in from the edge, such as `"1.3mm"`.
    pub depth: Length,
}

impl Command for EdgeConnectorSet {
    const META: Meta = Meta {
        id: "pcb.edge_connector.set",
        title: "Set Edge Connector",
        description: "Sets or clears (`null`) the board's `edge_connector` property: the \
                      bevel (`angle` between 0° and 90°, `depth`) the fab grinds on the card \
                      edge, and `hard_gold` for selective hard gold on the Edge Connector \
                      Pads. The board finish is unchanged. The job file and Fab Notes that \
                      `output.job.run` writes carry both; DRC warns when either is set on a \
                      board with no Edge Connector Pads. One undoable edit.",
        category: "pcb",
        icon: "layers",
        kind: Kind::Edit,
    };
    type Params = EdgeConnectorSetParams;
    type Output = ();

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: EdgeConnectorSetParams) -> Result<(), CommandError> {
        let Some(param) = p.edge_connector else {
            return Err(CommandError::invalid_params(
                "/edge_connector",
                "give the edge connector property, or null to clear it",
            ));
        };
        let edge_connector = param.map(edge_connector).transpose()?;
        let pcb = pcb(cx)?;
        let document = pcb.uuid;
        if pcb.setup().edge_connector == edge_connector {
            return Ok(());
        }
        let setup = BoardSetup {
            edge_connector,
            ..pcb.setup().clone()
        };
        cx.tx()?
            .replace(Object::BoardSetup(InDocument::new(document, setup)))?;
        Ok(())
    }
}

fn edge_connector(p: EdgeConnectorParam) -> Result<EdgeConnector, CommandError> {
    let bevel = p
        .bevel
        .map(|b| {
            let bevel = EdgeBevel {
                angle: b.angle.millideg(),
                depth: b.depth.nm(),
            };
            match bevel.problem() {
                Some((field, why)) => Err(CommandError::invalid_params(
                    &format!("/edge_connector/bevel{field}"),
                    why,
                )),
                None => Ok(bevel),
            }
        })
        .transpose()?;
    Ok(EdgeConnector {
        bevel,
        hard_gold: p.hard_gold,
    })
}
