//! Tracks: adding a committed route and deleting segments or whole Tracks.

use reda_command::{
    AvailabilityContext, Command, CommandError, Context, Kind, Length, Meta, Registry, Unavailable,
};
use reda_model::{InDocument, LayerKind, Object, PcbDocument, Track, TrackGeometry, Uuid};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::layer::LayerRef;
use super::outline::PointParam;
use super::{needs_pcb, pcb, positive};
use crate::rules::net_rules;

pub(super) fn register_all(registry: &mut Registry) {
    registry.add(TrackAdd);
    registry.add(TrackDelete);
}

/// The copper Layer a param names; `pointer` names the param in errors.
pub(super) fn copper_layer(
    pcb: &PcbDocument,
    layer: LayerRef,
    pointer: &str,
) -> Result<Uuid, CommandError> {
    let layer = layer.resolve(pcb, pointer)?;
    if layer.kind != LayerKind::Copper {
        return Err(CommandError::invalid_params(
            pointer,
            "copper goes on copper Layers only",
        ));
    }
    Ok(layer.uuid)
}

/// A Net param checked to exist.
pub(super) fn existing_net(
    pcb: &PcbDocument,
    net: Option<Uuid>,
    pointer: &str,
) -> Result<Option<Uuid>, CommandError> {
    match net {
        Some(uuid) if pcb.net(uuid).is_none() => Err(CommandError::invalid_params(
            pointer,
            format!("no Net has UUID {uuid}"),
        )),
        net => Ok(net),
    }
}

pub struct TrackAdd;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TrackAddParams {
    /// The copper Layer, such as `copper_top`.
    pub layer: LayerRef,
    /// The route's corners, start to end; each pair of neighbours becomes one Track.
    pub points: Vec<PointParam>,
    /// The Net's UUID. Left out, the route takes the Net of the copper under its first
    /// point on this Layer (a Pad, Track or Via), or else of the copper under its last
    /// point; on neither, it is on no Net.
    #[serde(default)]
    pub net: Option<Uuid>,
    /// The width; left out, the Net Class's track width, else the board's rule.
    #[serde(default)]
    pub width: Option<Length>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct TrackAddResult {
    /// The new Tracks, in route order.
    pub tracks: Vec<Uuid>,
    /// The Net they are on; left out for none.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub net: Option<Uuid>,
}

impl Command for TrackAdd {
    const META: Meta = Meta {
        id: "pcb.track.add",
        title: "Add Track",
        description: "Adds a route on a copper Layer: one straight Track per pair of \
                      neighbouring `points`. The Net and width default from the copper the \
                      route starts on and its Net Class.",
        category: "pcb",
        icon: "track",
        kind: Kind::Edit,
    };
    type Params = TrackAddParams;
    type Output = TrackAddResult;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: TrackAddParams) -> Result<TrackAddResult, CommandError> {
        let pcb = pcb(cx)?;
        let document = pcb.uuid;
        let layer = copper_layer(pcb, p.layer, "/layer")?;
        if p.points.len() < 2 {
            return Err(CommandError::invalid_params(
                "/points",
                "a route needs at least two points",
            ));
        }
        let points: Vec<_> = p.points.iter().map(|q| q.point()).collect();
        if let Some(i) = (1..points.len()).find(|&i| points[i] == points[i - 1]) {
            return Err(CommandError::invalid_params(
                &format!("/points/{i}"),
                "the point repeats the one before it",
            ));
        }
        let net = match existing_net(pcb, p.net, "/net")? {
            Some(net) => Some(net),
            None => {
                let copper = super::board_copper(cx)?;
                let first = points[0];
                let last = points[points.len() - 1];
                copper
                    .net_at(layer, first)
                    .or_else(|| copper.net_at(layer, last))
            }
        };
        let width = match p.width {
            Some(w) => positive("/width", w)?,
            None => net_rules(pcb, net).track_width.default_size(),
        };
        let mut tracks = Vec::new();
        for pair in points.windows(2) {
            let uuid = cx.new_id();
            let track = Track {
                uuid,
                layer,
                net,
                width,
                geometry: TrackGeometry::Line {
                    from: pair[0],
                    to: pair[1],
                },
            };
            cx.tx()?
                .insert(Object::Track(InDocument::new(document, track)))?;
            tracks.push(uuid);
        }
        Ok(TrackAddResult { tracks, net })
    }
}

pub struct TrackDelete;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TrackDeleteParams {
    /// The Track segments' UUIDs.
    pub tracks: Vec<Uuid>,
    /// Deletes each whole Track instead: every segment joined end to end with it on the
    /// same Layer, up to a Pad, a Via or a branch.
    #[serde(default)]
    pub whole: bool,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct TrackDeleteResult {
    /// Every deleted Track, sorted by UUID.
    pub deleted: Vec<Uuid>,
}

impl Command for TrackDelete {
    const META: Meta = Meta {
        id: "pcb.track.delete",
        title: "Delete Track",
        description: "Deletes Track segments, or with `whole` every segment of their Tracks \
                      up to a Pad, Via or branch.",
        category: "pcb",
        icon: "delete",
        kind: Kind::Edit,
    };
    type Params = TrackDeleteParams;
    type Output = TrackDeleteResult;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: TrackDeleteParams,
    ) -> Result<TrackDeleteResult, CommandError> {
        let pcb = pcb(cx)?;
        for (i, uuid) in p.tracks.iter().enumerate() {
            if pcb.track(*uuid).is_none() {
                return Err(CommandError::invalid_params(
                    &format!("/tracks/{i}"),
                    format!("no Track has UUID {uuid}"),
                ));
            }
        }
        let mut deleted: Vec<Uuid> = if p.whole {
            let copper = super::board_copper(cx)?;
            p.tracks
                .iter()
                .flat_map(|t| copper.whole_track(*t))
                .collect()
        } else {
            p.tracks.clone()
        };
        deleted.sort();
        deleted.dedup();
        let tx = cx.tx()?;
        for uuid in &deleted {
            tx.remove(*uuid)?;
        }
        Ok(TrackDeleteResult { deleted })
    }
}
