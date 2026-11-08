//! `pcb.zone.set_name_scheme`: how Zones with no typed name are named, a board setting
//! on Board Setup ▸ Zone Connections.

use reda_command::{AvailabilityContext, Command, CommandError, Context, Kind, Meta, Unavailable};
use reda_model::{BoardSetup, InDocument, Object, ZoneNameScheme};
use schemars::JsonSchema;
use serde::Deserialize;

use super::super::{needs_pcb, pcb};

pub struct ZoneSetNameScheme;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetNameSchemeParams {
    /// `NET_LXX_PXXX` (default), `LXX_NET_PXXX`, `NET_LAYERNAME_PXXX` or
    /// `LAYERNAME_NET_PXXX`.
    pub scheme: ZoneNameScheme,
}

impl Command for ZoneSetNameScheme {
    const META: Meta = Meta {
        id: "pcb.zone.set_name_scheme",
        title: "Set Zone Naming Scheme",
        description: "Sets the board's `zone_name_scheme`, which derives the name of every \
                      Zone with no typed name from its Net (`NONET` for none), its copper \
                      Layer (`LXX`, the copper number, or the Layer name such as \
                      `copper_top`) and `PXXX`, counting the Zones of that Net and Layer in \
                      pour order from `P000`: `NET_LXX_PXXX` (default), `LXX_NET_PXXX`, \
                      `NET_LAYERNAME_PXXX` or `LAYERNAME_NET_PXXX`.",
        category: "pcb",
        icon: "zone",
        kind: Kind::Edit,
    };
    type Params = SetNameSchemeParams;
    type Output = ();

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: SetNameSchemeParams) -> Result<(), CommandError> {
        let pcb = pcb(cx)?;
        let document = pcb.uuid;
        if pcb.setup().zone_name_scheme == p.scheme {
            return Ok(());
        }
        let setup = BoardSetup {
            zone_name_scheme: p.scheme,
            ..pcb.setup().clone()
        };
        cx.tx()?
            .replace(Object::BoardSetup(InDocument::new(document, setup)))?;
        Ok(())
    }
}
