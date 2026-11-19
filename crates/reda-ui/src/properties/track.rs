//! Track Properties (docs/spec/v0.1/pcb.md#track-properties): the selected Tracks' Start
//! and End XY, Layer and Net (with a canvas pick), edited with `pcb.track.set`, and the Net
//! block for selected copper of one Net (`reda_pcb::net_info`). Length and Width are
//! numeric rows ([`super::Numbers`]).

use reda_engine::command::{
    AvailabilityContext, Command, CommandError, Context, Kind, Meta, Registry, Unavailable,
};
use reda_engine::model::geom::Point;
use reda_engine::model::{LayerKind, Object, Project, TrackGeometry, Uuid};
use reda_engine::pcb::Copper;
use reda_engine::pcb::net_info::{self, NetInfo};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::Shared;
use crate::selection::Selection;

/// What the panel shows for selected Tracks.
#[derive(Clone, Debug, PartialEq)]
pub struct TrackProperties {
    /// The selected Track segments, in selection order.
    pub tracks: Vec<Uuid>,
    /// With one straight segment selected, its ends.
    pub ends: Option<(Point, Point)>,
    pub layer: Shared<Uuid>,
    pub net: Shared<Option<Uuid>>,
    /// The copper Layers to choose from, `(uuid, name)`, top to bottom.
    pub layers: Vec<(Uuid, String)>,
    /// The Nets to choose from, `(uuid, name)`, in name order.
    pub nets: Vec<(Uuid, String)>,
}

/// One edit of the Track rows.
#[derive(Clone, Debug, PartialEq)]
pub enum TrackEdit {
    Start(Point),
    End(Point),
    Layer(Uuid),
    Net(Option<Uuid>),
}

impl TrackProperties {
    /// The Track rows for `selection`; `None` unless it has Tracks.
    pub fn of(project: &Project, selection: &Selection) -> Option<TrackProperties> {
        let pcb = project.pcb()?;
        let tracks: Vec<_> = selection
            .objects
            .iter()
            .filter_map(|o| pcb.track(*o))
            .collect();
        if tracks.is_empty() {
            return None;
        }
        let ends = match tracks.as_slice() {
            [one] => match one.geometry {
                TrackGeometry::Line { from, to } => Some((from, to)),
                TrackGeometry::Arc { .. } => None,
            },
            _ => None,
        };
        let layers = pcb
            .layers()
            .layers
            .iter()
            .filter(|l| l.kind == LayerKind::Copper)
            .map(|l| (l.uuid, crate::panels::layers::layer_name(pcb, l)))
            .collect();
        let mut nets: Vec<(Uuid, String)> = pcb.nets().map(|n| (n.uuid, n.name.clone())).collect();
        nets.sort_by(|a, b| a.1.cmp(&b.1));
        Some(TrackProperties {
            tracks: tracks.iter().map(|t| t.uuid).collect(),
            ends,
            layer: Shared::of(tracks.iter().map(|t| t.layer))?,
            net: Shared::of(tracks.iter().map(|t| t.net))?,
            layers,
            nets,
        })
    }

    /// The `pcb.track.set` that makes `edit` to the selected Tracks.
    pub fn command(&self, edit: &TrackEdit) -> (&'static str, Value) {
        let mut params = json!({"tracks": self.tracks});
        let point = |p: &Point| json!({"x": p.x.0, "y": p.y.0});
        match edit {
            TrackEdit::Start(p) => params["start"] = point(p),
            TrackEdit::End(p) => params["end"] = point(p),
            TrackEdit::Layer(layer) => params["layer"] = json!(layer),
            TrackEdit::Net(net) => params["net"] = json!(net),
        }
        ("pcb.track.set", params)
    }

    /// The name of `uuid` among `list`.
    pub fn name_in(list: &[(Uuid, String)], uuid: &Uuid) -> String {
        list.iter()
            .find(|(u, _)| u == uuid)
            .map_or_else(|| uuid.to_string(), |(_, n)| n.clone())
    }
}

/// The Net block: the figures for the selected copper's one Net, and its Net Classes'
/// names.
#[derive(Clone, Debug, PartialEq)]
pub struct NetBlock {
    pub info: NetInfo,
    /// The names of its Net Classes besides the Default Net Class.
    pub classes: Vec<String>,
}

/// The Net block for `selection` when its Tracks, Vias and Free Pads are all on one Net;
/// the figures are for the first selected Track, warming by `delta_t` °C.
pub fn net_block(project: &Project, selection: &Selection, delta_t: f64) -> Option<NetBlock> {
    let pcb = project.pcb()?;
    let mut nets = Vec::new();
    let mut track = None;
    for o in &selection.objects {
        let net = match project.object(*o)? {
            Object::Track(t) => {
                track = track.or(Some(t.object.uuid));
                t.object.net
            }
            Object::Via(v) => v.object.net,
            Object::FreePad(p) => p.object.net,
            _ => continue,
        };
        nets.push(net);
    }
    let net = match Shared::of(nets)? {
        Shared::Same(Some(net)) => net,
        _ => return None,
    };
    let copper = Copper::of(project)?;
    let info = net_info::net_info(pcb, &copper, net, track, None, delta_t).ok()?;
    let classes = info
        .classes
        .iter()
        .filter_map(|c| pcb.net_classes().find(|n| n.uuid == *c))
        .map(|c| c.name.clone())
        .collect();
    Some(NetBlock { info, classes })
}

/// The Tracks waiting for a click on the canvas to pick their Net (the Net row's pick
/// button); empty when no pick is waiting.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NetPick(pub Vec<Uuid>);

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PickNetParams {
    /// The Tracks to give the picked Net; empty cancels a waiting pick.
    pub tracks: Vec<Uuid>,
}

/// How many Tracks wait for the pick.
#[derive(Serialize, JsonSchema)]
pub struct PickNetResult {
    pub waiting: usize,
}

struct PickNet;

impl Command for PickNet {
    const META: Meta = Meta {
        id: "view.properties.pick_net",
        title: "Pick Net on Canvas",
        description: "The next click on the board picks the Net of the copper under it and \
                      sets it on the Tracks (`pcb.track.set`). Esc cancels.",
        category: "view",
        icon: "net",
        kind: Kind::View,
    };
    type Params = PickNetParams;
    type Output = PickNetResult;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        match cx.project {
            Some(p) if p.pcb().is_some() => Ok(()),
            _ => Err(Unavailable {
                reason: "no PCB is open".into(),
            }),
        }
    }

    fn run(&self, cx: &mut Context<'_>, p: PickNetParams) -> Result<PickNetResult, CommandError> {
        let pcb = cx
            .project()?
            .pcb()
            .ok_or_else(|| CommandError::unavailable("no PCB is open"))?;
        if let Some(bad) = p.tracks.iter().find(|t| pcb.track(**t).is_none()) {
            return Err(CommandError::invalid_params(
                "/tracks",
                format!("no Track has UUID {bad}"),
            ));
        }
        let waiting = p.tracks.len();
        cx.view::<NetPick>()?.0 = p.tracks;
        Ok(PickNetResult { waiting })
    }
}

/// Adds `view.properties.pick_net` to `registry`.
pub fn register_all(registry: &mut Registry) {
    registry.add(PickNet);
}

/// The Net of the copper at `at` on the board, looking on the Tracks' own Layer first and
/// then on every copper Layer.
pub fn net_at(project: &Project, tracks: &[Uuid], at: Point) -> Option<Uuid> {
    let pcb = project.pcb()?;
    let copper = Copper::of(project)?;
    let own = tracks.first().and_then(|t| pcb.track(*t)).map(|t| t.layer);
    own.into_iter()
        .chain(pcb.copper_layers().map(|l| l.uuid))
        .find_map(|layer| {
            copper
                .items_at(layer, at)
                .into_iter()
                .filter(|i| !matches!(i, reda_engine::pcb::Item::Track { uuid } if tracks.contains(uuid)))
                .find_map(|i| copper.net(i))
        })
}
