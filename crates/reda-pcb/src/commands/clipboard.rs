//! The board clipboard: `pcb.copy` and `pcb.paste`, with Paste Special's options
//! (docs/spec/v0.1/pcb.md#arranging): keep Net names, and paste on the active Layer.
//!
//! The clipboard is plain data that `copy` returns and `paste` takes back, as the
//! schematic's is. It holds free board objects: Tracks, Vias, Zones, Keepouts, text,
//! graphics, Free Pads and Dimensions. Footprints arrive through Sync only, so they are not
//! copied. Each Net is carried by name, so `keep_nets` finds it again by name on any board.

use std::collections::BTreeMap;

use reda_command::{
    AvailabilityContext, Command, CommandError, Context, Kind, Meta, Registry, Unavailable,
};
use reda_model::geom::{Bounds, Point, Vertex};
use reda_model::{
    Dimension, DimensionTextPosition, FreePad, GraphicShape, InDocument, Keepout, KeepoutShape,
    LayerKind, Object, PcbDocument, PcbGraphic, PcbText, Side, Track, TrackGeometry, Uuid, Via,
    Zone,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::layer::{LayerRef, takes_graphics};
use super::outline::PointParam;
use super::{needs_pcb, pcb};

pub(super) fn register_all(registry: &mut Registry) {
    registry.add(Copy);
    registry.add(Paste);
}

/// Copied board objects, as `pcb.copy` returns them and `pcb.paste` takes them. Treat it
/// as opaque.
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BoardClipboard {
    /// The lower-left corner of the copied objects' extent; pasting puts it at `at`.
    pub origin: Point,
    /// The copied objects' Nets by name, for `keep_nets`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub nets: BTreeMap<Uuid, String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tracks: Vec<Track>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub vias: Vec<Via>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub zones: Vec<Zone>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keepouts: Vec<Keepout>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub texts: Vec<PcbText>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub graphics: Vec<PcbGraphic>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub free_pads: Vec<FreePad>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dimensions: Vec<Dimension>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CopyParams {
    /// The objects to copy, by UUID: Tracks, Vias, Zones, Keepouts, text, graphics, Free
    /// Pads and Dimensions.
    pub objects: Vec<Uuid>,
}

pub struct Copy;

impl Command for Copy {
    const META: Meta = Meta {
        id: "pcb.copy",
        title: "Copy",
        description: "Copies Tracks, Vias, Zones, Keepouts, text, graphics, Free Pads and \
                      Dimensions from the board and returns them as a clipboard for \
                      pcb.paste, with their Nets by name. Footprints come through Sync and \
                      are refused. Changes nothing.",
        category: "pcb",
        icon: "copy",
        kind: Kind::Query,
    };
    type Params = CopyParams;
    type Output = BoardClipboard;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: CopyParams) -> Result<BoardClipboard, CommandError> {
        copy(pcb(cx)?, &p.objects)
    }
}

/// The clipboard for `objects`, or `invalid_params` at `/objects/i` for one that can't be
/// copied.
fn copy(pcb: &PcbDocument, objects: &[Uuid]) -> Result<BoardClipboard, CommandError> {
    if objects.is_empty() {
        return Err(CommandError::invalid_params(
            "/objects",
            "nothing is selected",
        ));
    }
    let mut c = BoardClipboard::default();
    let mut nets: Vec<Option<Uuid>> = Vec::new();
    let mut points: Vec<Point> = Vec::new();
    for (i, &uuid) in objects.iter().enumerate() {
        if let Some(t) = pcb.track(uuid) {
            let (a, b) = t.geometry.ends();
            points.extend([a, b]);
            nets.push(t.net);
            c.tracks.push(t.clone());
        } else if let Some(v) = pcb.via(uuid) {
            points.push(v.at);
            nets.push(v.net);
            c.vias.push(v.clone());
        } else if let Some(z) = pcb.zone(uuid) {
            let mut z = z.clone();
            // A copy is a Zone of its own: its outline in full, no name of its own yet.
            z.vertices = pcb.zone_vertices(&z).to_vec();
            z.follows_outline = false;
            z.name = None;
            points.extend(z.vertices.iter().map(Vertex::point));
            nets.push(z.net);
            c.zones.push(z);
        } else if let Some(k) = pcb.keepout(uuid) {
            points.extend(k.vertices().iter().map(Vertex::point));
            c.keepouts.push(k.clone());
        } else if let Some(t) = pcb.text(uuid) {
            points.push(t.position);
            c.texts.push(t.clone());
        } else if let Some(g) = pcb.graphic(uuid) {
            let (a, b) = match g.shape {
                GraphicShape::Line { from, to }
                | GraphicShape::Arc { from, to, .. }
                | GraphicShape::Rect { from, to, .. } => (from, to),
            };
            points.extend([a, b]);
            c.graphics.push(g.clone());
        } else if let Some(f) = pcb.free_pad(uuid) {
            points.push(f.at);
            nets.push(f.net);
            c.free_pads.push(f.clone());
        } else if let Some(d) = pcb.dimension(uuid) {
            let mut d = d.clone();
            // A copy measures where it lands: its ends no longer follow anything.
            d.a.reference = None;
            d.b.reference = None;
            points.extend([d.a.point, d.b.point]);
            c.dimensions.push(d);
        } else {
            let what = if pcb.footprint(uuid).is_some() {
                "a Footprint: Footprints come through Sync and can't be copied"
            } else {
                "not a Track, Via, Zone, Keepout, text, graphic, Free Pad or Dimension"
            };
            return Err(CommandError::invalid_params(
                &format!("/objects/{i}"),
                format!("{uuid} is {what}"),
            ));
        }
    }
    c.origin = Bounds::of_points(points).map_or(Point::default(), |b| b.min);
    for net in nets.into_iter().flatten() {
        if let Some(n) = pcb.net(net) {
            c.nets.insert(net, n.name.clone());
        }
    }
    unlock(&mut c);
    Ok(c)
}

/// Pasted objects start unlocked.
fn unlock(c: &mut BoardClipboard) {
    c.tracks.iter_mut().for_each(|o| o.locked = false);
    c.vias.iter_mut().for_each(|o| o.locked = false);
    c.zones.iter_mut().for_each(|o| o.locked = false);
    c.keepouts.iter_mut().for_each(|o| o.locked = false);
    c.texts.iter_mut().for_each(|o| o.locked = false);
    c.graphics.iter_mut().for_each(|o| o.locked = false);
    c.free_pads.iter_mut().for_each(|o| o.locked = false);
    c.dimensions.iter_mut().for_each(|o| o.locked = false);
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PasteParams {
    /// What pcb.copy returned.
    pub clipboard: BoardClipboard,
    /// Where the clipboard's origin (the lower-left corner of what was copied) goes; where
    /// it was copied from by default.
    #[serde(default)]
    pub at: Option<PointParam>,
    /// Paste Special: copper keeps its Nets, found by name on this board (none when the
    /// board has no Net of that name). Without it pasted copper is on no Net.
    #[serde(default)]
    pub keep_nets: bool,
    /// Paste Special: every object with a Layer goes on this one (Vias and Free Pads keep
    /// their own). Copper kinds need a copper Layer, text, graphics and Dimensions a
    /// drawing Layer.
    #[serde(default)]
    pub layer: Option<LayerRef>,
}

/// The objects a paste made.
#[derive(Debug, Serialize, JsonSchema)]
pub struct PasteResult {
    /// The new objects in clipboard order (Tracks, Vias, Zones, Keepouts, texts, graphics,
    /// Free Pads, Dimensions), ready to select.
    pub objects: Vec<Uuid>,
}

pub struct Paste;

impl Command for Paste {
    const META: Meta = Meta {
        id: "pcb.paste",
        title: "Paste",
        description: "Pastes what pcb.copy returned as new objects, its origin at `at` (where \
                      it was copied from by default), unlocked and on no Net. Paste Special \
                      is `keep_nets` (Nets found again by name) and `layer` (every object \
                      with a Layer goes on that one).",
        category: "pcb",
        icon: "paste",
        kind: Kind::Edit,
    };
    type Params = PasteParams;
    type Output = PasteResult;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: PasteParams) -> Result<PasteResult, CommandError> {
        let pcb = pcb(cx)?;
        let document = pcb.uuid;
        let mut c = p.clipboard;
        let by = p.at.map_or(Point::default(), |at| at.point() - c.origin);

        // Nets: kept by name, or dropped.
        let by_name: BTreeMap<&str, Uuid> = pcb.nets().map(|n| (n.name.as_str(), n.uuid)).collect();
        let net = |old: Option<Uuid>| -> Option<Uuid> {
            if !p.keep_nets {
                return None;
            }
            let name = c.nets.get(&old?)?;
            by_name.get(name.as_str()).copied()
        };
        for t in &mut c.tracks {
            t.net = net(t.net);
        }
        for v in &mut c.vias {
            v.net = net(v.net);
        }
        for z in &mut c.zones {
            z.net = net(z.net);
        }
        for f in &mut c.free_pads {
            f.net = net(f.net);
        }

        if let Some(layer) = p.layer {
            let layer = layer.resolve(pcb, "/layer")?;
            let copper = layer.kind == LayerKind::Copper;
            let copper_kinds =
                !c.tracks.is_empty() || !c.zones.is_empty() || !c.keepouts.is_empty();
            let drawn_kinds =
                !c.texts.is_empty() || !c.graphics.is_empty() || !c.dimensions.is_empty();
            if copper_kinds && !copper {
                return Err(CommandError::rejected(
                    "the Layer is not copper: Tracks, Zones and Keepouts can't go on it",
                ));
            }
            if drawn_kinds && !takes_graphics(layer) {
                return Err(CommandError::rejected(
                    "the Layer takes no text, graphics or Dimensions",
                ));
            }
            let uuid = layer.uuid;
            c.tracks.iter_mut().for_each(|t| t.layer = uuid);
            c.zones.iter_mut().for_each(|z| z.layer = uuid);
            c.keepouts.iter_mut().for_each(|k| k.layer = Some(uuid));
            c.graphics.iter_mut().for_each(|g| g.layer = uuid);
            c.dimensions.iter_mut().for_each(|d| d.layer = uuid);
            let bottom = layer.side == Some(Side::Bottom);
            for t in &mut c.texts {
                t.layer = uuid;
                t.mirrored = bottom;
            }
        } else {
            // Without Paste Special everything keeps its Layer, which must be on this board.
            let missing = c
                .tracks
                .iter()
                .map(|t| t.layer)
                .chain(c.zones.iter().map(|z| z.layer))
                .chain(c.keepouts.iter().filter_map(|k| k.layer))
                .chain(c.texts.iter().map(|t| t.layer))
                .chain(c.graphics.iter().map(|g| g.layer))
                .chain(c.dimensions.iter().map(|d| d.layer))
                .chain(c.vias.iter().flat_map(|v| [v.span.from, v.span.to]))
                .any(|l| pcb.layers().layer(l).is_none());
            if missing {
                return Err(CommandError::rejected(
                    "the clipboard uses a Layer this board doesn't have; paste it on the \
                     active Layer instead",
                ));
            }
        }

        let mut out = Vec::new();
        let mut objects = Vec::new();
        for mut t in c.tracks {
            t.uuid = cx.new_id();
            t.geometry = match t.geometry {
                TrackGeometry::Line { from, to } => TrackGeometry::Line {
                    from: from + by,
                    to: to + by,
                },
                TrackGeometry::Arc { from, to, sweep } => TrackGeometry::Arc {
                    from: from + by,
                    to: to + by,
                    sweep,
                },
            };
            objects.push(t.uuid);
            out.push(Object::Track(InDocument::new(document, t)));
        }
        for mut v in c.vias {
            v.uuid = cx.new_id();
            v.at = v.at + by;
            objects.push(v.uuid);
            out.push(Object::Via(InDocument::new(document, v)));
        }
        for mut z in c.zones {
            z.uuid = cx.new_id();
            shift(&mut z.vertices, by);
            objects.push(z.uuid);
            out.push(Object::Zone(InDocument::new(document, z)));
        }
        for mut k in c.keepouts {
            k.uuid = cx.new_id();
            match &mut k.shape {
                KeepoutShape::Polygon { vertices } | KeepoutShape::Line { vertices, .. } => {
                    shift(vertices, by)
                }
            }
            objects.push(k.uuid);
            out.push(Object::Keepout(InDocument::new(document, k)));
        }
        for mut t in c.texts {
            t.uuid = cx.new_id();
            t.position = t.position + by;
            objects.push(t.uuid);
            out.push(Object::PcbText(InDocument::new(document, t)));
        }
        for mut g in c.graphics {
            g.uuid = cx.new_id();
            g.shape = match g.shape {
                GraphicShape::Line { from, to } => GraphicShape::Line {
                    from: from + by,
                    to: to + by,
                },
                GraphicShape::Arc { from, to, sweep } => GraphicShape::Arc {
                    from: from + by,
                    to: to + by,
                    sweep,
                },
                GraphicShape::Rect { from, to, filled } => GraphicShape::Rect {
                    from: from + by,
                    to: to + by,
                    filled,
                },
            };
            objects.push(g.uuid);
            out.push(Object::PcbGraphic(InDocument::new(document, g)));
        }
        for mut f in c.free_pads {
            f.uuid = cx.new_id();
            f.at = f.at + by;
            objects.push(f.uuid);
            out.push(Object::FreePad(InDocument::new(document, f)));
        }
        for mut d in c.dimensions {
            d.uuid = cx.new_id();
            d.a.point = d.a.point + by;
            d.b.point = d.b.point + by;
            if let DimensionTextPosition::Manual(at) = &mut d.text_position {
                *at = *at + by;
            }
            objects.push(d.uuid);
            out.push(Object::Dimension(InDocument::new(document, d)));
        }
        let tx = cx.tx()?;
        for object in out {
            tx.insert(object)?;
        }
        Ok(PasteResult { objects })
    }
}

fn shift(vertices: &mut [Vertex], by: Point) {
    for v in vertices {
        let p = v.point() + by;
        (v.x, v.y) = (p.x, p.y);
    }
}
