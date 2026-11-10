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
    registry.add(TrackSet);
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
                locked: false,
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
    /// Deletes locked objects too; without it a locked one refuses the whole Command.
    #[serde(default)]
    pub include_locked: bool,
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
        crate::lock::refuse(pcb, deleted.iter().copied(), p.include_locked, "delete")?;
        let tx = cx.tx()?;
        for uuid in &deleted {
            tx.remove(*uuid)?;
        }
        Ok(TrackDeleteResult { deleted })
    }
}

pub struct TrackSet;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TrackSetParams {
    /// The Track segments' UUIDs; each gets every value given.
    pub tracks: Vec<Uuid>,
    /// The new start point.
    #[serde(default)]
    pub start: Option<PointParam>,
    /// The new end point; not with `length`.
    #[serde(default)]
    pub end: Option<PointParam>,
    /// The new length of a straight Track: Start stays and End moves along the line.
    #[serde(default)]
    pub length: Option<Length>,
    #[serde(default)]
    pub width: Option<Length>,
    /// A copper Layer, such as `copper_bottom`.
    #[serde(default)]
    pub layer: Option<LayerRef>,
    /// The Net's UUID, or `null` for no Net; left out, the Net stays.
    #[serde(default, deserialize_with = "super::nullable")]
    #[schemars(with = "Option<Uuid>")]
    pub net: Option<Option<Uuid>>,
    /// Moves the ends of locked Tracks too; without it Start, End or Length on a locked Track is refused.
    #[serde(default)]
    pub include_locked: bool,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct TrackSetResult {
    /// The Tracks that changed, sorted.
    pub tracks: Vec<Uuid>,
}

impl Command for TrackSet {
    const META: Meta = Meta {
        id: "pcb.track.set",
        title: "Set Track",
        description: "Sets values of Track segments: Start and End, Length (Start stays and \
                      End moves along the line), width, copper Layer and Net (`null` for \
                      none). Values left out stay. Returns the Tracks that changed. Start, End and \
                      Length are refused on a locked Track unless `include_locked`.",
        category: "pcb",
        icon: "track",
        kind: Kind::Edit,
    };
    type Params = TrackSetParams;
    type Output = TrackSetResult;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: TrackSetParams) -> Result<TrackSetResult, CommandError> {
        let pcb = pcb(cx)?;
        let document = pcb.uuid;
        if p.length.is_some() && p.end.is_some() {
            return Err(CommandError::invalid_params(
                "/length",
                "give `length` or `end`, not both",
            ));
        }
        let length = p.length.map(|l| positive("/length", l)).transpose()?;
        let width = p.width.map(|w| positive("/width", w)).transpose()?;
        let layer = p
            .layer
            .map(|l| copper_layer(pcb, l, "/layer"))
            .transpose()?;
        let net = match p.net {
            Some(net) => Some(existing_net(pcb, net, "/net")?),
            None => None,
        };
        if p.start.is_some() || p.end.is_some() || length.is_some() {
            crate::lock::refuse(pcb, p.tracks.iter().copied(), p.include_locked, "move")?;
        }
        let mut changed = Vec::new();
        for (i, uuid) in p.tracks.iter().enumerate() {
            let before = pcb.track(*uuid).ok_or_else(|| {
                CommandError::invalid_params(
                    &format!("/tracks/{i}"),
                    format!("no Track has UUID {uuid}"),
                )
            })?;
            let mut track = before.clone();
            track.geometry = moved_ends(&track.geometry, &p, length)?;
            if let Some(width) = width {
                track.width = width;
            }
            if let Some(layer) = layer {
                track.layer = layer;
            }
            if let Some(net) = net {
                track.net = net;
            }
            if track != *before && !changed.iter().any(|t: &Track| t.uuid == *uuid) {
                changed.push(track);
            }
        }
        changed.sort_by_key(|t| t.uuid);
        let tracks = changed.iter().map(|t| t.uuid).collect();
        let tx = cx.tx()?;
        for track in changed {
            tx.replace(Object::Track(InDocument::new(document, track)))?;
        }
        Ok(TrackSetResult { tracks })
    }
}

/// `geometry` with the ends `p` gives, or End moved along the line to `length`.
fn moved_ends(
    geometry: &TrackGeometry,
    p: &TrackSetParams,
    length: Option<reda_model::geom::Nm>,
) -> Result<TrackGeometry, CommandError> {
    let (mut from, mut to) = geometry.ends();
    if let Some(start) = &p.start {
        from = start.point();
    }
    if let Some(end) = &p.end {
        to = end.point();
    }
    if let Some(length) = length {
        let TrackGeometry::Line { .. } = geometry else {
            return Err(CommandError::invalid_params(
                "/length",
                "only a straight Track's length can be set",
            ));
        };
        let (dx, dy) = ((to.x.0 - from.x.0) as f64, (to.y.0 - from.y.0) as f64);
        let now = dx.hypot(dy);
        if now == 0.0 {
            return Err(CommandError::invalid_params(
                "/length",
                "the Track has no direction to lengthen along",
            ));
        }
        let k = length.0 as f64 / now;
        to = reda_model::geom::Point::new(
            reda_model::geom::Nm(from.x.0 + (dx * k).round() as i64),
            reda_model::geom::Nm(from.y.0 + (dy * k).round() as i64),
        );
    }
    if from == to {
        let pointer = if p.end.is_some() { "/end" } else { "/start" };
        return Err(CommandError::invalid_params(
            pointer,
            "a Track's start and end must differ",
        ));
    }
    Ok(match *geometry {
        TrackGeometry::Line { .. } => TrackGeometry::Line { from, to },
        TrackGeometry::Arc { sweep, .. } => TrackGeometry::Arc { from, to, sweep },
    })
}
