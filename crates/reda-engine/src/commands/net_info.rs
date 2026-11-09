//! `pcb.net.info`: the Net block of Track Properties as a query, with the temperature rise
//! from the `pcb.net_info.delta_t` Preference unless given.

use reda_command::{
    AvailabilityContext, Command, CommandError, Context, Kind, Length, Meta, NO_PROJECT, Registry,
    Unavailable,
};
use reda_model::Uuid;
use reda_pcb::Copper;
use reda_pcb::net_info::{NetInfo, net_info};
use schemars::JsonSchema;
use serde::Deserialize;

pub(crate) fn register_all(registry: &mut Registry) {
    registry.add(NetInfoQuery);
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NetInfoParams {
    pub net: Uuid,
    /// The temperature rise in °C the current estimate allows; left out, the
    /// `pcb.net_info.delta_t` Preference (10 °C unless changed).
    #[serde(default)]
    pub delta_t: Option<f64>,
    /// The Track width to give figures for; left out, the Track's, else the Net Class's
    /// track width.
    #[serde(default)]
    pub width: Option<Length>,
    /// A Track whose Layer, width and resistance to use: "this Track" in the block.
    #[serde(default)]
    pub track: Option<Uuid>,
}

pub struct NetInfoQuery;

impl Command for NetInfoQuery {
    const META: Meta = Meta {
        id: "pcb.net.info",
        title: "Net Information",
        description: "The Net block of Track Properties: the Net's classes, colour, loop \
                      removal, routed length and routed percentage, and for a Track width on \
                      a Layer its resistance (copper at 1.72e-8 Ω·m, the Stackup thickness) and \
                      the IPC-2221 estimate of its maximum current at the temperature rise \
                      `delta_t`. Every default used is returned.",
        category: "pcb",
        icon: "net",
        kind: Kind::Query,
    };
    type Params = NetInfoParams;
    type Output = NetInfo;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        match cx.project {
            None => Err(Unavailable {
                reason: NO_PROJECT.into(),
            }),
            Some(p) if p.pcb().is_none() => Err(Unavailable {
                reason: "the Project has no PCB Document".into(),
            }),
            Some(_) => Ok(()),
        }
    }

    fn run(&self, cx: &mut Context<'_>, p: NetInfoParams) -> Result<NetInfo, CommandError> {
        let delta_t = match p.delta_t {
            Some(d) => d,
            None => {
                crate::preferences::current(cx.user_dirs())
                    .pcb
                    .net_info
                    .delta_t
            }
        };
        if !(delta_t.is_finite() && delta_t > 0.0) {
            return Err(CommandError::invalid_params(
                "/delta_t",
                "the temperature rise must be a positive number of °C",
            ));
        }
        let width = match p.width {
            Some(w) if w.nm().0 <= 0 => {
                return Err(CommandError::invalid_params(
                    "/width",
                    "the width must be positive",
                ));
            }
            w => w.map(|w| w.nm()),
        };
        let project = cx.project()?;
        let pcb = project
            .pcb()
            .ok_or_else(|| CommandError::unavailable("the Project has no PCB Document"))?;
        if pcb.net(p.net).is_none() {
            return Err(CommandError::invalid_params(
                "/net",
                format!("no Net has UUID {}", p.net),
            ));
        }
        if let Some(track) = p.track
            && pcb.track(track).is_none()
        {
            return Err(CommandError::invalid_params(
                "/track",
                format!("no Track has UUID {track}"),
            ));
        }
        let copper = Copper::of(project).expect("the Project has a PCB Document");
        net_info(pcb, &copper, p.net, p.track, width, delta_t).map_err(CommandError::internal)
    }
}
